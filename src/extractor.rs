use crate::models::NewsArticle; // Assuming Article is a struct with url, metadata, text
use reqwest::Client;
use scraper::{Html, Selector};
use url::{Url};
use anyhow::{Result, anyhow};
use std::collections::{HashSet, HashMap};
use chrono::{DateTime, Utc, Datelike};
use log::{debug, info};
use crate::utils::clean_html_tags;

/// Coordinator function that discovers articles from all sources concurrently
pub async fn discover_all_articles() -> Result<HashSet<Url>> {
    debug!("Starting concurrent article discovery from all sources");

    let (football365_result, thisisanfield_result) = tokio::join!(
        extract_football365_articles(),
        extract_thisisanfield_articles()
    );

    let mut all_urls = football365_result?;
    debug!("Discovered {} URLs from Football365", all_urls.len());

    let thisisanfield_urls = thisisanfield_result?;
    debug!("Discovered {} URLs from This Is Anfield", thisisanfield_urls.len());

    // Merge the URLs from both sources
    all_urls.extend(thisisanfield_urls);
    debug!("Total discovered URLs: {}", all_urls.len());

    Ok(all_urls)
}

async fn extract_football365_articles() -> Result<HashSet<Url>> {
    let base_url = Url::parse("https://www.football365.com").unwrap();
    let full_page_url = base_url.join("liverpool/news")?;

    let client = Client::new();
    let res = client.get(full_page_url.clone()).send().await?.text().await?;
    let document = Html::parse_document(&res);

    let main_selector = Selector::parse("main.w-full.lg\\:w-main").unwrap();
    let a_selector = Selector::parse("a[href]").unwrap();

    let mut links = HashSet::new();

    if let Some(main) = document.select(&main_selector).next() {
        for a in main.select(&a_selector) {
            if let Some(href) = a.value().attr("href") {
                // Convert relative URLs to full URLs
                if let Ok(mut full_url) = base_url.join(href) {
                    full_url.set_fragment(None); // get rid of everything after #
                    let href_str = full_url.to_string();

                    // Apply your filters
                    if !href_str.starts_with("https://www.football365.com/news/") {
                        continue;
                    }
                    if href_str.contains("/news/author/")
                        || href_str.contains("-mediawatch")
                        || href_str.contains("-mailbox")
                    {
                        continue;
                    }

                    links.insert(full_url);
                }
            }
        }
    }

    Ok(links)
}


pub async fn extract_article(client: &Client, url: &Url) -> Result<NewsArticle> {
    let res = client
        .get(url.to_string())
        .send()
        .await?
        .text()
        .await?;

    let document = Html::parse_document(&res);

    // Step 1: Extract metadata
    let mut metadata = HashMap::new();
    let meta_selector = Selector::parse("head meta").unwrap();

    for tag in document.select(&meta_selector) {
        let property = tag.value().attr("property");
        let name = tag.value().attr("name");
        let content = tag.value().attr("content").unwrap_or("").trim();

        match property {
            Some("og:title") => {
                metadata.insert("og:title", content.to_string());
            }
            Some("article:published_time") => {
                metadata.insert("article:published_time", content.to_string());
            }
            Some("og:image") => {
                metadata.insert("og:image", content.to_string());
            }
            _ => {}
        }

        if name == Some("author") {
            metadata.insert("author", content.to_string());
        }
    }

    // Step 2: Extract article body
    let article_selector = Selector::parse("div.ciam-article-f365").unwrap();
    let tag_selector = Selector::parse("p, blockquote").unwrap();
    let exclusion_phrases = vec![
        "READ:",
        "PREMIER LEAGUE FEATURES ON F365",
        "Start the conversation",
        "Go Below The Line",
        "Be the First to Comment",
        "MORE LIVERPOOL COVERAGE ON F365",
        "READ NOW:",
        "READ MORE:",
    ];

    let mut content_parts = vec![];

    if let Some(article) = document.select(&article_selector).next() {
        for tag in article.select(&tag_selector) {
            let text = tag.text().collect::<Vec<_>>().join(" ").trim().to_string();
            if tag.value().attr("style") == Some("text-align: center;") || text.contains("👉") {
                continue;
            }
            if exclusion_phrases.iter().any(|phrase| text.contains(phrase)) {
                continue;
            }
            if !text.is_empty() {
                content_parts.push(text);
            }
        }
    }

    let final_text = content_parts.join("\n\n");

    // Step 3: Build the NewsArticle struct, with safe parsing
    let og_title = metadata
        .remove("og:title")
        .ok_or_else(|| anyhow!("Missing og:title"))?;

    let published_time_str = metadata
        .remove("article:published_time")
        .ok_or_else(|| anyhow!("Missing article:published_time"))?;

    let published_time = published_time_str
        .parse::<DateTime<Utc>>()
        .map_err(|e| anyhow!("Failed to parse published_time: {}", e))?;

    let og_image_str = metadata
        .remove("og:image")
        .ok_or_else(|| anyhow!("Missing og:image"))?;

    let og_image = Url::parse(&og_image_str)
        .map_err(|e| anyhow!("Failed to parse og:image URL: {}", e))?;

    let author = metadata
        .remove("author")
        .unwrap_or_else(|| "Unknown".to_string());

    info!("Successfully scraped football365 article: {}", url.clone());

    Ok(NewsArticle {
        url: url.clone(),
        og_title,
        published_time,
        og_image,
        author,
        text: final_text,
        source: "football365".to_string(),
    })
}

pub async fn extract_thisisanfield_articles() -> Result<HashSet<Url>> {
    let client = Client::new();

    // Fetch the main sitemap index
    let sitemap_url = "https://www.thisisanfield.com/news-sitemap.xml";
    let res = client.get(sitemap_url).send().await?.text().await?;
    let document = Html::parse_document(&res);

    // Select all loc elements (URLs)
    let url_selector = Selector::parse("loc").unwrap();
    let mut links = HashSet::new();

    // Get current year dynamically
    let current_year = Utc::now().year();
    let year_prefix = format!("https://www.thisisanfield.com/{}/", current_year);

    for element in document.select(&url_selector) {
        let url_text = element.text().collect::<String>();
        if let Ok(url) = Url::parse(&url_text) {
            // Filter for Liverpool-related articles from current year
            let url_str = url.to_string();
            if url_str.starts_with(&year_prefix) {
                links.insert(url);
            }
        }
    }

    Ok(links)
}

pub async fn extract_thisisanfield_article(client: &Client, url: &Url) -> Result<NewsArticle> {
    let res = client
        .get(url.to_string())
        .send()
        .await?
        .text()
        .await?;

    let document = Html::parse_document(&res);

    // Step 1: Extract metadata
    let mut metadata = HashMap::new();
    let meta_selector = Selector::parse("head meta").unwrap();

    for tag in document.select(&meta_selector) {
        let property = tag.value().attr("property");
        let name = tag.value().attr("name");
        let content = tag.value().attr("content").unwrap_or("").trim();

        match property {
            Some("og:title") => {
                metadata.insert("og:title", content.to_string());
            }
            Some("article:published_time") => {
                metadata.insert("article:published_time", content.to_string());
            }
            Some("og:image") => {
                metadata.insert("og:image", content.to_string());
            }
            _ => {}
        }

        if name == Some("author") {
            metadata.insert("author", content.to_string());
        }
    }

    // Step 2: Extract article body from main content area
    // This Is Anfield uses different selectors - look for the main content
    let article_selector = Selector::parse("article, .post, .entry, main").unwrap();
    let paragraph_selector = Selector::parse("p").unwrap();

    let mut content_parts = vec![];

    // Try to find the article content using various selectors
    let mut found_content = false;

    // First try to find a specific article container
    for container in document.select(&article_selector) {
        for paragraph in container.select(&paragraph_selector) {
            // Extract text content and also get inner HTML as fallback
            let text = paragraph.text().collect::<Vec<_>>().join(" ").trim().to_string();
            let inner_html = paragraph.inner_html();

            // If text extraction failed but there's HTML content, clean it
            let final_text = if text.is_empty() && !inner_html.trim().is_empty() {
                clean_html_tags(&inner_html)
            } else {
                text
            };

            // Skip empty paragraphs, navigation, and promotional content
            if final_text.is_empty()
                || final_text.len() < 20  // Skip very short paragraphs
                || final_text.contains("READ MORE:")
                || final_text.contains("WATCH:")
                || final_text.contains("Follow us on")
                || final_text.contains("Get our free app")
                || final_text.contains("Click here to get it")
                || final_text.contains("More about:")
                || final_text.contains("Substitutes:")
                || final_text.starts_with("Liverpool:")
                || final_text.starts_with("Burnley:")
                || final_text.contains("© Copyright") {
                continue;
            }

            content_parts.push(final_text);
            found_content = true;
        }
        if found_content {
            break;
        }
    }

    // If no content found in article containers, try broader search
    if !found_content {
        for paragraph in document.select(&paragraph_selector) {
            // Extract text content and also get inner HTML as fallback
            let text = paragraph.text().collect::<Vec<_>>().join(" ").trim().to_string();
            let inner_html = paragraph.inner_html();

            // If text extraction failed but there's HTML content, clean it
            let final_text = if text.is_empty() && !inner_html.trim().is_empty() {
                clean_html_tags(&inner_html)
            } else {
                text
            };

            if final_text.len() >= 30  // Only include substantial paragraphs
                && !final_text.contains("READ MORE:")
                && !final_text.contains("WATCH:")
                && !final_text.contains("Follow us on")
                && !final_text.contains("Get our free app")
                && !final_text.contains("Click here to get it")
                && !final_text.contains("More about:")
                && !final_text.contains("© Copyright")
                && !final_text.starts_with("Liverpool:")
                && !final_text.starts_with("Burnley:") {
                content_parts.push(final_text);
            }
        }
    }

    // Join content parts and apply final HTML cleaning pass
    let joined_text = content_parts.join("\n\n");
    let final_text = clean_html_tags(&joined_text);

    // Step 3: Build the NewsArticle struct
    let og_title = metadata
        .remove("og:title")
        .ok_or_else(|| anyhow!("Missing og:title"))?;

    let published_time_str = metadata
        .remove("article:published_time")
        .ok_or_else(|| anyhow!("Missing article:published_time"))?;

    let published_time = published_time_str
        .parse::<DateTime<Utc>>()
        .map_err(|e| anyhow!("Failed to parse published_time: {}", e))?;

    let og_image_str = metadata
        .remove("og:image")
        .ok_or_else(|| anyhow!("Missing og:image"))?;

    let og_image = Url::parse(&og_image_str)
        .map_err(|e| anyhow!("Failed to parse og:image URL: {}", e))?;

    let author = metadata
        .remove("author")
        .unwrap_or_else(|| "This Is Anfield".to_string());

    info!("Successfully scraped thisisanfield article: {}", url.clone());

    Ok(NewsArticle {
        url: url.clone(),
        og_title,
        published_time,
        og_image,
        author,
        text: final_text,
        source: "thisisanfield".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_extract_thisisanfield_articles() {
        let result = extract_thisisanfield_articles().await;
        assert!(result.is_ok(), "Failed to extract This Is Anfield articles: {:?}", result.err());

        let urls = result.unwrap();
        println!("Found {} articles from This Is Anfield", urls.len());

        for url in urls.iter().take(3) {
            println!("  - {}", url);
        }
    }

    #[tokio::test]
    async fn test_extract_thisisanfield_article() {
        // First get some URLs
        let urls = extract_thisisanfield_articles().await.unwrap();
        if let Some(url) = urls.iter().next() {
            let client = reqwest::Client::new();
            let result = extract_thisisanfield_article(&client, url).await;

            assert!(result.is_ok(), "Failed to extract This Is Anfield article: {:?}", result.err());

            let article = result.unwrap();
            println!("Extracted article:");
            println!("  Title: {}", article.og_title);
            println!("  Author: {}", article.author);
            println!("  Source: {}", article.source);
            println!("  Published: {}", article.published_time);
            println!("  Text length: {} characters", article.text.len());

            assert_eq!(article.source, "thisisanfield");
            assert!(!article.og_title.is_empty());
            assert!(!article.text.is_empty());

            // Verify that no HTML tags remain in the extracted text
            assert!(!article.text.contains("<img"), "HTML img tags should be removed");
            assert!(!article.text.contains("<div"), "HTML div tags should be removed");
            assert!(!article.text.contains("<p>"), "HTML p tags should be removed");
            assert!(!article.text.contains("</p>"), "HTML closing p tags should be removed");
            assert!(!article.text.contains("&nbsp;"), "HTML entities should be cleaned");

            println!("  First 200 chars: {}",
                if article.text.len() > 200 {
                    &article.text[..200]
                } else {
                    &article.text
                });
        } else {
            println!("No articles found to test with");
        }
    }

    #[test]
    fn test_html_cleaning_integration() {
        // Test that HTML cleaning works with typical This Is Anfield content
        let mock_html_content = r#"<img loading="lazy" src="test.jpg" alt="Liverpool" width="600" height="400" />
        <p>Liverpool manager Jürgen Klopp spoke about the team&rsquo;s performance.</p>
        <div class="wp-caption">Some caption text</div>
        [caption id="123"]Image caption[/caption]
        <strong>The Reds</strong> played well in the match."#;

        let cleaned = clean_html_tags(mock_html_content);

        // Verify HTML tags are removed
        assert!(!cleaned.contains("<img"));
        assert!(!cleaned.contains("<p>"));
        assert!(!cleaned.contains("</p>"));
        assert!(!cleaned.contains("<div"));
        assert!(!cleaned.contains("<strong>"));
        assert!(!cleaned.contains("[caption"));

        // Verify content remains
        assert!(cleaned.contains("Liverpool manager"));
        assert!(cleaned.contains("The Reds"));
        assert!(cleaned.contains("played well"));

        // Verify entities are cleaned
        assert!(cleaned.contains("team's")); // &rsquo; should become '

        println!("Cleaned content: {}", cleaned);
    }
}
