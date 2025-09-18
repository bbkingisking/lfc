use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_yaml::Deserializer;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub api_key: String,
    pub model: String,
    pub db_path: PathBuf,
    pub emails: Option<Vec<String>>,
    pub telegram_chat_ids: Option<Vec<String>>,
    pub telegram_bot_token: Option<String>,
    pub email_username: Option<String>,
    pub email_app_password: Option<String>,
}

pub struct EnsureOutcome {
    pub path: PathBuf,
    pub created: bool,
}

impl Config {
    pub fn ensure_user_config() -> Result<EnsureOutcome> {
        let xdg_dirs = xdg::BaseDirectories::with_prefix("lfc");

        if let Some(path) = xdg_dirs.find_config_file("config.yaml") {
            return Ok(EnsureOutcome {
                path,
                created: false,
            });
        } else {
            let config_path = xdg_dirs
                .place_config_file("config.yaml")
                .expect("cannot create configuration directory");
            let mut config_file = File::create(&config_path)?;

            write!(
                &mut config_file,
                r#"# LFC config (YAML)
# All keys are required unless marked optional.

api_key: "<your OpenAI API key>"
model: "gpt-4o-2024-08-06"
db_path: "/path/to/lfc.sqlite3"

# Optional email recipients (can be omitted if using --no-email)
emails:
  - "you@example.com"
  - "friend@example.com"

# Optional telegram recipients (can be omitted if using --no-telegram)
telegram_chat_ids:
  - "chat1_id"
  - "chat2_id"

# Optional telegram/email config (can be omitted if using --no-telegram/--no-email)
telegram_bot_token: "<your bot token>"
email_username: "your email"
email_app_password: "your password"

"#
            )?;

            return Ok(EnsureOutcome {
                path: config_path,
                created: true,
            });
        }
    }

    pub fn get_user_config() -> Result<Config> {
        let xdg_dirs =
            xdg::BaseDirectories::with_prefix("lfc").find_config_file("config.yaml");

        if let Some(existing_config) = &xdg_dirs {
            let raw = fs::read_to_string(existing_config).with_context(|| {
                format!("Failed to read {}", existing_config.display())
            })?;
            let deserialized = Deserializer::from_str(&raw);
            let final_config: Config =
                serde_path_to_error::deserialize(deserialized).map_err(|e| {
                    anyhow!(
                        "Invalid YAML in {} at `{}`: {}",
                        existing_config.display(),
                        e.path(),
                        e.inner()
                    )
                })?;
            Ok(final_config)
        } else {
            Err(anyhow!(
                "Could not read configuration file in config::get_user_config"
            ))
        }
    }
}
