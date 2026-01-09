mod app;
mod config;
mod db;
mod models;
mod extractor;
mod ai_summarizer;
mod ai_deduplicator;
mod utils;
mod email;
mod telegram;
mod calendar;
mod logger;

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(name = "lfc")]
#[command(about = "Liverpool FC News Aggregator")]
struct Cli {
    /// Skip AI processing and summary sending (for debugging)
    #[arg(long)]
    no_ai: bool,

    /// Skip email notifications
    #[arg(long)]
    no_email: bool,

    /// Skip telegram notifications
    #[arg(long)]
    no_telegram: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    app::run_scraper(cli.no_ai, cli.no_email, cli.no_telegram).await
}
