use std::{
    env,
    fs::{self, File},
    io::Write,
    path::PathBuf,
};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_yaml::Deserializer;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub model: String,
    #[serde(default = "default_db_path")]
    pub db_path: PathBuf,
    #[serde(skip)]
    pub api_key: String,
    #[serde(skip)]
    pub emails: Option<Vec<String>>,
    #[serde(skip)]
    pub telegram_chat_ids: Option<Vec<String>>,
    #[serde(skip)]
    pub telegram_bot_token: Option<String>,
    #[serde(skip)]
    pub email_username: Option<String>,
    #[serde(skip)]
    pub email_app_password: Option<String>,
}

fn default_db_path() -> PathBuf {
    let data_dir = dirs::data_dir().expect("Could not determine data directory");
    data_dir.join("lfc").join("articles.db")
}

fn env_csv(key: &str) -> Option<Vec<String>> {
    let val = env::var(key).ok()?;
    let items: Vec<String> = val
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if items.is_empty() { None } else { Some(items) }
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
# Only model and db_path are configured here.
# All secrets are read from environment variables:
#   LFC_API_KEY               - OpenAI API key
#   LFC_EMAILS                - comma-separated recipient email addresses
#   LFC_TELEGRAM_CHAT_IDS     - comma-separated Telegram chat IDs
#   LFC_TELEGRAM_BOT_TOKEN    - Telegram bot token
#   LFC_EMAIL_USERNAME        - SMTP email username
#   LFC_EMAIL_APP_PASSWORD    - SMTP email app password

model: "gpt-4o-2024-08-06"
# db_path: "/custom/path/to/articles.db"   # optional, defaults to XDG data dir
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

        let Some(existing_config) = xdg_dirs else {
            return Err(anyhow!(
                "Could not read configuration file in config::get_user_config"
            ));
        };

        let raw = fs::read_to_string(&existing_config).with_context(|| {
            format!("Failed to read {}", existing_config.display())
        })?;
        let deserialized = Deserializer::from_str(&raw);
        let mut cfg: Config =
            serde_path_to_error::deserialize(deserialized).map_err(|e| {
                anyhow!(
                    "Invalid YAML in {} at `{}`: {}",
                    existing_config.display(),
                    e.path(),
                    e.inner()
                )
            })?;

        // Populate secrets from environment variables
        cfg.api_key = env::var("LFC_API_KEY")
            .map_err(|_| anyhow!("LFC_API_KEY environment variable is not set"))?;
        cfg.emails = env_csv("LFC_EMAILS");
        cfg.telegram_chat_ids = env_csv("LFC_TELEGRAM_CHAT_IDS");
        cfg.telegram_bot_token = env::var("LFC_TELEGRAM_BOT_TOKEN").ok();
        cfg.email_username = env::var("LFC_EMAIL_USERNAME").ok();
        cfg.email_app_password = env::var("LFC_EMAIL_APP_PASSWORD").ok();

        Ok(cfg)
    }
}
