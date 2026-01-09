use std::collections::HashSet;
use url::Url;

use crate::calendar::check_today_fixture;
use crate::config::{Config, EnsureOutcome};
use crate::db::{Db, load_existing_urls_from_db};
use crate::logger::init_logger;
use crate::models::NewsArticle;
use crate::extractor::{extract_article, discover_all_articles, extract_thisisanfield_article};
use crate::ai_summarizer::summarize_articles;
use crate::ai_deduplicator::ai_deduplicate;
use crate::utils::format_summary_plain_text;
use crate::email::send_email;
use crate::telegram::send_telegram_message;

use anyhow::Result;
use log::{debug, info, error};

pub async fn run_scraper(no_ai: bool, no_email: bool, no_telegram: bool) -> Result<()> {
    // 0) Initialize logger
    init_logger()?;
    debug!("Logger initialized");

    // 1) Ensure config exists
    let config_outcome: EnsureOutcome = Config::ensure_user_config()?;
    if config_outcome.created {
        info!(
            "Config file created at {}. Please edit it and restart the app.",
            config_outcome.path.display()
        );
        return Ok(());
    }

    let cfg = Config::get_user_config()?;
    debug!("User config loaded");

    // 1a) Validate config based on enabled features
    if !no_email && (cfg.email_username.is_none() || cfg.email_app_password.is_none()) {
        error!("Email configuration is missing but email notifications are enabled. Use --no-email to skip email notifications or configure email settings.");
        return Ok(());
    }

    if !no_telegram && cfg.telegram_bot_token.is_none() {
        error!("Telegram bot token is missing but telegram notifications are enabled. Use --no-telegram to skip telegram notifications or configure telegram_bot_token.");
        return Ok(());
    }

    // 2) Open DB
    let db = Db::open(&cfg)?;
    debug!("Database opened");

    // 3) Load existing articles
    let existing_urls: HashSet<Url> = load_existing_urls_from_db(&db)?;
    debug!("Loaded {} existing article URLs from DB", existing_urls.len());

    // 4) Discover new URLs from multiple sources concurrently
    let mut new_urls: HashSet<Url> = discover_all_articles().await?;

    new_urls.retain(|url| !existing_urls.contains(url));
    debug!("Retained {} new URLs after deduplication", new_urls.len());

    if new_urls.is_empty() {
        info!("No new articles found. Everything is up to date.");
        return Ok(());
    } else {
        info!("Found {} new articles, starting scrape…", new_urls.len());
    }

    // 5) Get a new fetch ID
    let fetch_id = db.create_fetch()?;
    debug!("Created new fetch ID: {}", fetch_id);

    // 6) Create MPSC channel
    let (tx, mut rx) = tokio::sync::mpsc::channel::<NewsArticle>(200);
    debug!("Channel created for article transmission");

    // 7) Spawn DB writer
    let db_writer = db;
    let writer_handle = tokio::spawn(async move {
        while let Some(article) = rx.recv().await {
            if let Err(e) = db_writer.insert_article(fetch_id, &article) {
                error!("DB insert failed: {:?}", e);
            } else {
                debug!("Inserted article: {}", article.url);
            }
        }
        info!("All articles inserted for fetch_id {}", fetch_id);
    });

    // 8) Create HTTP client
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 13.5; rv:116.0) Gecko/20100101 Firefox/116.0")
        .build()?;
    debug!("HTTP client created");

    // 9) Scrape each URL
    for url in new_urls {
        let tx = tx.clone();
        let client = client.clone();
        let url_clone = url.clone();

        tokio::spawn(async move {
            // Determine which extractor to use based on the URL
            let extract_result = if url_clone.host_str() == Some("www.thisisanfield.com") {
                extract_thisisanfield_article(&client, &url_clone).await
            } else {
                extract_article(&client, &url_clone).await
            };

            match extract_result {
                Ok(article) => {
                    if let Err(_) = tx.send(article).await {
                        error!("Failed to send article — receiver closed");
                    } else {
                        debug!("Article sent from {}", url_clone);
                    }
                }
                Err(err) => {
                    error!("Failed to scrape {}: {:?}", url_clone, err);
                }
            }
        });
    }

    drop(tx); // signal completion
    debug!("Dropped sender, waiting for writer to finish");

    writer_handle.await?;
    info!("Writing new articles to the DB finished.");

    if no_ai {
        info!("--no-ai flag set, skipping AI processing and summary sending");
        return Ok(());
    }

    // 10) Summarize
    let mut db = Db::open(&cfg)?;
    let previous_articles = db.load_articles_for_latest_fetch(&fetch_id)?;
    let today_fixture = check_today_fixture().await?;
    let summary = summarize_articles(&cfg, &previous_articles, &today_fixture).await?;

    // 11) Deduplication sources
    let published_bullets   = db.fetch_latest_published_bullets()?;          // suppressors
    info!("These are yesterday's bullet points that will be deduplicated against: {:#?}", published_bullets.iter().map(|b| b.text.clone()).collect::<Vec<String>>());
    let carryover_bullets   = db.fetch_unpublished_accepted_bullets_since_last_published()?;

    // merge today's candidates with carryover BEFORE dedup
    let mut merged = summary.clone();

    // dedupe texts before pushing (prevents duplicates if today already has same text)
    let mut seen: HashSet<String> = summary.items.iter().map(|b| b.text.clone()).collect();

    for mut b in carryover_bullets {
        if !seen.contains(&b.text) {
            seen.insert(b.text.clone());      // keep a copy in the set
            b.accepted = None;                // re-evaluate today
            merged.items.push(b);             // move the whole Bullet (including text)
        }
    }
    info!("These are today's bullet candidates: {:#?}", merged.items.iter().map(|b| b.text.clone()).collect::<Vec<String>>());

    // run dedup USING published bullets as the "previous" set
    let processed_summary = ai_deduplicate(&cfg, &published_bullets, &merged).await?;

    info!("The deduplicator accepted {} bullet points.",
        processed_summary
            .items
            .iter()
            .filter(|i| i.accepted == Some(true))
            .count());

    info!("The deduplicator rejected {} bullet points. To check what was rejected, query the database.",
        processed_summary
            .items
            .iter()
            .filter(|i| i.accepted == Some(false))
            .count());

    debug!("This is what the deduplicator returned {:#?}", processed_summary.items);

    // persist summary (do not flip accepted flags)
    db.insert_summary(fetch_id, &processed_summary)?;

    // send notifications…
    let plain_text = format_summary_plain_text(&processed_summary);

    let email_task = if no_email {
        info!("--no-email flag set, skipping email notifications");
        tokio::spawn(async { Ok(()) })
    } else {
        let cfg_clone = cfg.clone();
        let text_clone = plain_text.clone();
        tokio::spawn(async move { send_email(&cfg_clone, &text_clone).await })
    };

    let telegram_task = if no_telegram {
        info!("--no-telegram flag set, skipping telegram notifications");
        tokio::spawn(async { Ok(()) })
    } else {
        let cfg_clone = cfg.clone();
        let text_clone = plain_text.clone();
        tokio::spawn(async move { send_telegram_message(&cfg_clone, &text_clone).await })
    };

    let (email_res, telegram_res) = tokio::join!(email_task, telegram_task);

    match email_res.unwrap() {
        Ok(_) if !no_email => info!("Email(s) sent."),
        Err(e) if !no_email => error!("Email(s) failed: {e:?}"),
        _ => {}
    }

    match telegram_res.unwrap() {
        Ok(_) if !no_telegram => info!("Telegram(s) sent."),
        Err(e) if !no_telegram => error!("Telegram(s) failed: {e:?}"),
        _ => {}
    }

    db.mark_summary_sent(fetch_id)?;

    Ok(())
}
