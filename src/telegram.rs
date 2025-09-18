use anyhow::Result;
use teloxide::{prelude::*, types::ChatId};
use crate::config::Config;

pub async fn send_telegram_message(cfg: &Config, text: &str) -> Result<()> {
    let Some(recipients) = &cfg.telegram_chat_ids else { return Ok(()); };
    if recipients.is_empty() { return Ok(()); }
    
    let Some(telegram_bot_token) = &cfg.telegram_bot_token else { return Ok(()); };

    let bot = Bot::new(telegram_bot_token);

    for recipient in recipients {
        let chat_id: i64 = recipient.parse()?;
        bot.send_message(ChatId(chat_id), text).await?;
    }

    Ok(())
}
