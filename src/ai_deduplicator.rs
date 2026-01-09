use async_openai::config::OpenAIConfig;
use anyhow::{Result, Context};
use async_openai::{
    types::{
        ChatCompletionRequestSystemMessage, ChatCompletionRequestUserMessage,
        CreateChatCompletionRequestArgs, ResponseFormat, ResponseFormatJsonSchema,
    },
    Client,
};
use serde::Deserialize;
use serde_json::json;

use crate::config::Config;
use crate::models::{Bullet, Summary};

#[derive(Debug, Deserialize)]
struct DedupResponse {
    results: Vec<bool>,
}

pub async fn ai_deduplicate(cfg: &Config, previous_bullets: &[Bullet], current_summary: &Summary) -> Result<Summary> {
    let api_key = &cfg.api_key;

    let openai_config = OpenAIConfig::default().with_api_key(api_key);
    let client = Client::with_config(openai_config);

    // Extract text content
    let prev_texts: Vec<String> = previous_bullets.iter().map(|b| b.text.clone()).collect();
    let curr_texts: Vec<String> = current_summary.items.iter().map(|b| b.text.clone()).collect();

    let system_prompt = r#"
You are a helpful assistant for summarizing Liverpool FC news.

You are given:
- A list of bullet points that were recently included in previous daily summaries
- A list of new candidate bullet points for today’s summary

Your job is to compare each candidate bullet to all the previous ones and decide:
  - true  → if this bullet is **meaningfully different** and should be included
  - false → if it is **too similar or repetitive**, and should be discarded

Sometimes, today's bullet points are repetitive as well; please also give false to a bullet point if there is another bullet point from today that is making the same point.

The goal is to end up with a list of "true" bullet points that are informative but not repetitive.

Respond only with a structured JSON array of true/false, in the same order as the candidate bullets.
"#;

    let schema = json!({
        "type": "object",
        "properties": {
            "results": {
                "type": "array",
                "items": { "type": "boolean" }
            }
        },
        "required": ["results"],
        "additionalProperties": false
    });

    let user_prompt = format!(
        "PREVIOUS BULLETS:\n{}\n\nCANDIDATE BULLETS:\n{}",
        prev_texts.iter().map(|s| format!("- {}", s)).collect::<Vec<_>>().join("\n"),
        curr_texts.iter().map(|s| format!("- {}", s)).collect::<Vec<_>>().join("\n")
    );

    let response_format = ResponseFormat::JsonSchema {
        json_schema: ResponseFormatJsonSchema {
            description: None,
            name: "dedup_filter".into(),
            schema: Some(schema),
            strict: Some(true),
        },
    };

    let request = CreateChatCompletionRequestArgs::default()
        .model(&cfg.model)
        .messages([
            ChatCompletionRequestSystemMessage::from(system_prompt).into(),
            ChatCompletionRequestUserMessage::from(user_prompt).into(),
        ])
        .response_format(response_format)
        .max_tokens(500u32)
        .service_tier(async_openai::types::ServiceTier::Flex)
        .build()
        .context("Failed to build deduplication request")?;

    let response = client.chat().create(request).await.context("Deduplication call failed")?;

    for choice in response.choices {
        if let Some(content) = choice.message.content {
            let parsed: DedupResponse = serde_json::from_str(&content)
                .context("Failed to parse deduplication JSON response")?;

            if parsed.results.len() != current_summary.items.len() {
                anyhow::bail!(
                    "LLM returned {} results, expected {}",
                    parsed.results.len(),
                    current_summary.items.len()
                );
            }

            // Apply decisions to each bullet
            let updated_bullets: Vec<Bullet> = current_summary
                .items
                .iter()
                .zip(parsed.results.into_iter())
                .map(|(b, accepted)| Bullet {
                    text: b.text.clone(),
                    accepted: Some(accepted),
                })
                .collect();

            let updated_summary = Summary {
                mood: current_summary.mood.clone(),
                date: current_summary.date,
                items: updated_bullets,
            };

            return Ok(updated_summary);
        }
    }

    anyhow::bail!("No valid content in OpenAI response")
}
