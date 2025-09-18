use anyhow::{Context, Result};
use async_openai::{
    types::{
        ChatCompletionRequestSystemMessage, ChatCompletionRequestUserMessage,
        CreateChatCompletionRequestArgs, ResponseFormat, ResponseFormatJsonSchema,
    },
    Client,
    config::OpenAIConfig,
};
use serde::{Deserialize};
use serde_json::json;
use tokio::time::Duration;
use tiktoken_rs::{o200k_base, CoreBPE};
use chrono::Local;
use log::debug;

use crate::config::Config;
use crate::models::{NewsArticle, Summary, Bullet};

#[derive(Debug, Deserialize)]
struct RawAiSummary {
    mood: String,
    items: Vec<String>,
}
const MAX_TOKENS: usize = 100_000;
const MIN_BODY_TOKENS: usize = 40; // don't over-trim tiny bodies
const SEP_TOKENS_PER_ARTICLE: usize = 6; // rough buffer for "\n\n" joins

pub async fn summarize_articles(cfg: &Config, articles: &[NewsArticle]) -> Result<Summary> {
    debug!("Starting summarize_articles with {} articles", articles.len());
    let api_key = &cfg.api_key;

    let openai_config = OpenAIConfig::default().with_api_key(api_key);
    let client = Client::with_config(openai_config);
    debug!("Created OpenAI client");

    debug!("Starting content truncation for {} articles", articles.len());
    let combined_text: String = truncate_content(articles)?;
    debug!("Content truncated, final length: {} characters", combined_text.len());

    let mut system_prompt = String::new();

    let prompt = r#"
        You are a Liverpool (LFC) fan and supporter. You have access to some news published about the club from the last 24 hours.

        Analyze all the provided articles and create a summary of the key developments and trends from the past 24 hours.

        Return only a JSON object with this structure:
        {
          "mood": string,
          "items": [string, string, ...]
        }

        The "mood" string should be a ONE-SENTENCE summary stating whether the news is mostly positive, mostly negative, or mixed, and very briefly why.

        Each item in the "items" array of string is a bullet point summarizing some news/development. Feel free to end the bullet point text with an appropriate emoji. Don't repeat the same story across multiple bullet points, even if there are multiple articles talking about it.

        Feel free to be biased towards our beloved club. Use casual language and emojis.
        Feel free to ignore articles that are not relevant or that seem to be ads.
        Please do not use clickbait titles, summaries, or language. Be concise. Do not include live streaming information.
        The most important areas that fans would care about are potential transfers, injuries, player/team stats, and match summaries/previews.
        "#;

    system_prompt.push_str(prompt);
    system_prompt.push_str(&format!("Today's date is {}. Even though articles are published either today or yesterday, they may be referencing events and news that happened a long time ago. Don't summarize those, as they have likely been covered by previous summaries.", Local::now().date_naive().format("%Y-%m-%d").to_string()));

    let schema = json!({
      "type": "object",
      "properties": {
        "mood": { "type": "string" },
        "items": {
          "type": "array",
          "items": { "type": "string" }
        }
      },
      "required": ["mood", "items"],
      "additionalProperties": false
    });

    let response_format = ResponseFormat::JsonSchema {
        json_schema: ResponseFormatJsonSchema {
            description: None,
            name: "lfc_summary".to_string(),
            schema: Some(schema),
            strict: Some(true),
        },
    };

    debug!("Building OpenAI request with model: {}", cfg.model);
    let request = CreateChatCompletionRequestArgs::default()
        .model(&cfg.model)
        .messages([
            ChatCompletionRequestSystemMessage::from(system_prompt).into(),
            ChatCompletionRequestUserMessage::from(combined_text).into(),
        ])
        .response_format(response_format)
        .max_tokens(1000u32)
        .build()
        .context("Failed to build OpenAI request")?;

    debug!("Making OpenAI API call with 60s timeout...");
    let start_time = std::time::Instant::now();
    let response = match tokio::time::timeout(
        Duration::from_secs(60), // adjust as needed
        client.chat().create(request)
    ).await {
        Ok(api_result) => {
            let elapsed = start_time.elapsed();
            debug!("OpenAI API call completed in {:?}", elapsed);
            match api_result {
                Ok(response) => response,
                Err(api_error) => {
                    debug!("OpenAI API returned error: {:?}", api_error);
                    return Err(anyhow::anyhow!("OpenAI API error: {}", api_error));
                }
            }
        }
        Err(_timeout_error) => {
            let elapsed = start_time.elapsed();
            debug!("OpenAI API call timed out after {:?}", elapsed);
            return Err(anyhow::anyhow!("OpenAI API call timed out after 60 seconds"));
        }
    };

    debug!("Processing {} response choices", response.choices.len());
    for choice in response.choices {
        if let Some(content) = choice.message.content {
            debug!("Received response content, length: {} chars", content.len());
            debug!("Response content: {}", content);
            let raw: RawAiSummary = serde_json::from_str(&content)
                .context("Failed to parse AI JSON summary")?;

            let items: Vec<Bullet> = raw.items.into_iter()
                .map(|text| Bullet {
                    text,
                    accepted: None,
                })
                .collect();

            debug!("Successfully created summary with {} items", items.len());
            return Ok(Summary {
                mood: raw.mood,
                items,
                date: chrono::Utc::now().date_naive(), // fills in today's date
            });
        }
    }

    debug!("No valid content found in OpenAI response");
    anyhow::bail!("No valid content in OpenAI response");
}

fn decode_first_n_tokens(bpe: &CoreBPE, s: &str, n: usize) -> String {
    if n == 0 || s.is_empty() {
        return String::new();
    }
    let ids = bpe.encode_with_special_tokens(s);
    let keep = ids.len().min(n);
    bpe.decode(ids[..keep].to_vec()).unwrap_or_default()
}

fn truncate_content(articles: &[NewsArticle]) -> Result<String> {
    debug!("Starting truncate_content with {} articles", articles.len());
    let bpe = o200k_base().unwrap();
    debug!("Initialized BPE tokenizer");

    // token counts
    let mut title_tok = Vec::with_capacity(articles.len());
    let mut body_tok  = Vec::with_capacity(articles.len());
    for (i, a) in articles.iter().enumerate() {
        let title_tokens = bpe.encode_with_special_tokens(&a.og_title).len();
        let body_tokens = bpe.encode_with_special_tokens(&a.text).len();
        title_tok.push(title_tokens);
        body_tok.push(body_tokens);
        debug!("Article {}: title={} tokens, body={} tokens", i, title_tokens, body_tokens);
    }

    // we "keep" full bodies initially, then level them down
    let mut keep_body = body_tok.clone();

    // total estimate (titles + kept bodies + separators)
    let mut total = title_tok.iter().sum::<usize>()
        + keep_body.iter().sum::<usize>()
        + SEP_TOKENS_PER_ARTICLE * articles.len();

    debug!("Initial token count: {} (max allowed: {})", total, MAX_TOKENS);

    if total <= MAX_TOKENS {
        debug!("Content fits within token limit, no truncation needed");
        let combined = articles
            .iter()
            .map(|a| format!("{}\n\n{}", a.og_title, a.text))
            .collect::<Vec<_>>()
            .join("\n\n");
        return Ok(combined);
    }

    // helper to find indices of the (trimmable) max and second max bodies
    let find_two_largest = |keep: &Vec<usize>| -> Option<(usize, usize)> {
        // consider only entries strictly above MIN_BODY_TOKENS
        let mut max_i: Option<usize> = None;
        let mut second_i: Option<usize> = None;

        for i in 0..keep.len() {
            if keep[i] <= MIN_BODY_TOKENS { continue; }
            match max_i {
                None => max_i = Some(i),
                Some(mi) => {
                    if keep[i] > keep[mi] {
                        second_i = max_i;
                        max_i = Some(i);
                    } else if second_i.map_or(true, |si| keep[i] > keep[si]) && i != mi {
                        second_i = Some(i);
                    }
                }
            }
        }
        max_i.map(|mi| (mi, second_i.unwrap_or(mi)))
    };

    // level the tallest bodies until we fit
    let mut iteration = 0;
    while total > MAX_TOKENS {
        iteration += 1;
        let needed = total - MAX_TOKENS;
        debug!("Truncation iteration {}: need to reduce by {} tokens", iteration, needed);

        let some = find_two_largest(&keep_body);
        if some.is_none() {
            break; // nothing left we’re allowed to trim
        }
        let (imax, isecond) = some.unwrap();
        let max_len = keep_body[imax];
        let second_len = keep_body[isecond];

        // target is second longest (or MIN if that’s higher)
        let target_len = std::cmp::max(second_len, MIN_BODY_TOKENS);
        let mut diff = max_len.saturating_sub(target_len);

        // if all trimmable bodies are equal length (diff == 0), still shave at least 1
        // (bounded by MIN and by how much we need)
        if diff == 0 {
            diff = (max_len.saturating_sub(MIN_BODY_TOKENS)).min(needed).max(1);
        }

        let shave = diff.min(needed);
        if shave == 0 {
            debug!("No more tokens can be shaved, breaking");
            break;
        }

        debug!("Shaving {} tokens from article {}", shave, imax);
        keep_body[imax] = keep_body[imax].saturating_sub(shave);
        total = total.saturating_sub(shave);
        debug!("New total after shaving: {}", total);
    }

    // rebuild final text
    debug!("Rebuilding final text with {} articles", articles.len());
    let mut out = Vec::with_capacity(articles.len());
    for (i, a) in articles.iter().enumerate() {
        let title = &a.og_title; // titles intact
        let body  = decode_first_n_tokens(&bpe, &a.text, keep_body[i]);
        debug!("Article {}: keeping {} body tokens", i, keep_body[i]);
        out.push(format!("{title}\n\n{body}"));
    }
    let final_text = out.join("\n\n");
    debug!("Final combined text length: {} characters", final_text.len());
    Ok(final_text)
}
