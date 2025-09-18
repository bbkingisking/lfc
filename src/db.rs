use rusqlite::OptionalExtension;
use crate::models::{Summary, Bullet};
use std::collections::HashSet;

use crate::models::NewsArticle;
use crate::config::Config;

use rusqlite::{Connection, params};
use anyhow::{Result, Context};
use url::{Url};

const SCHEMA_SQL: &str = include_str!("../schema.sql");

pub struct Db {
    conn: Connection
}

impl Db {
    pub fn open(cfg: &Config) -> Result<Self> {
        let path = &cfg.db_path;
        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open DB at {}", path.display()))?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        conn.execute_batch(SCHEMA_SQL).context("Failed to initialize schema")?;

        Ok(Db { conn })
    }

    pub fn create_fetch(&self) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO fetches DEFAULT VALUES;",
            [],
        )?;

        let fetch_id = self.conn.last_insert_rowid();
        Ok(fetch_id)
    }

    pub fn insert_article(&self, fetch_id: i64, article: &NewsArticle) -> Result<()> {
        self.conn.execute(
            "INSERT INTO articles (
                fetch_id, url, og_title, published_time, og_image, author, text, source
            ) VALUES (
                ?, ?, ?, ?, ?, ?, ?, ?
            )",
            params![
                &fetch_id,
                &article.url.to_string(),
                &article.og_title,
                &article.published_time.to_rfc3339(),
                &article.og_image.to_string(),
                &article.author,
                &article.text,
                &article.source,
            ],
        )?;

        Ok(())
    }

    pub fn load_articles_for_latest_fetch(&self, fetch_id: &i64) -> Result<Vec<NewsArticle>> {
        let mut stmt = self.conn.prepare(
            "SELECT url, og_title, published_time, og_image, author, text, source FROM articles WHERE fetch_id = ?",
        )?;

        let iter = stmt.query_and_then([fetch_id], |row| {
            let url: Url = row.get::<_, String>(0)?.parse()?;
            let published_time = row.get::<_, String>(2)?.parse()?;
            let og_image: Url = row.get::<_, String>(3)?.parse()?;
            Ok(NewsArticle {
                url,
                og_title: row.get(1)?,
                published_time,
                og_image,
                author: row.get(4)?,
                text: row.get(5)?,
                source: row.get(6)?,
            })
        })?;

        Ok(iter.collect::<Result<Vec<_>>>()?)
    }

    pub fn insert_summary(&mut self, fetch_id: i64, summary: &Summary) -> Result<()> {
        let tx = self.conn.transaction()?; // transactional insert

        // 📝 Insert into summaries table (just the mood now)
        tx.execute(
            "INSERT INTO summaries (fetch_id, mood_text) VALUES (?, ?)",
            params![fetch_id, &summary.mood],
        ).context("Failed to insert into summaries table")?;

        // ➕ Insert each bullet (with accepted flag) into bullets table
        let mut stmt = tx.prepare(
            "INSERT INTO bullets (fetch_id, text, accepted) VALUES (?, ?, ?)"
        )?;

        for bullet in &summary.items {
            stmt.execute(params![
                fetch_id,
                bullet.text,
                bullet.accepted,
            ])?;
        }

        drop(stmt);
        tx.commit().context("Failed to commit summary + bullets")?;

        Ok(())
    }
    // bullets from the most recent *published* (sent=1) summary
    pub fn fetch_latest_published_bullets(&self) -> anyhow::Result<Vec<Bullet>> {
        let mut stmt = self.conn.prepare(r#"
            SELECT s.fetch_id
            FROM summaries s
            WHERE s.sent = 1
            ORDER BY s.generated_at DESC
            LIMIT 1
        "#)?;
        let fetch_id_opt = stmt.query_row([], |row| row.get::<_, i64>(0)).optional()?;
        let Some(fetch_id) = fetch_id_opt else { return Ok(vec![]) };

        let mut stmt = self.conn.prepare(
            "SELECT text, accepted FROM bullets WHERE fetch_id = ? AND accepted = 1"
        )?;
        let iter = stmt.query_map([fetch_id], |row| {
            Ok(Bullet {
                text: row.get(0)?,
                accepted: row.get::<_, Option<bool>>(1)?,
            })
        })?;
        Ok(iter.filter_map(|r| r.ok()).collect())
    }

    // accepted bullets from the most recent *unpublished* (sent=0) summary
    pub fn fetch_unpublished_accepted_bullets_since_last_published(&self) -> anyhow::Result<Vec<Bullet>> {
        let mut stmt = self.conn.prepare(r#"
            SELECT DISTINCT b.text, b.accepted
            FROM bullets b
            JOIN summaries s ON s.fetch_id = b.fetch_id
            WHERE s.sent = 0
              AND b.accepted = 1
              AND s.generated_at >
                  COALESCE((SELECT MAX(generated_at) FROM summaries WHERE sent = 1),
                           '0001-01-01T00:00:00Z')
            ORDER BY s.generated_at DESC, b.id DESC
        "#)?;
        let iter = stmt.query_map([], |row| {
            Ok(Bullet {
                text: row.get(0)?,
                accepted: row.get::<_, Option<bool>>(1)?,
            })
        })?;
        Ok(iter.filter_map(|r| r.ok()).collect())
    }

    // mark a summary as sent after notifications succeed
    pub fn mark_summary_sent(&self, fetch_id: i64) -> anyhow::Result<()> {
        self.conn.execute(
            "UPDATE summaries SET sent = 1 WHERE fetch_id = ?",
            rusqlite::params![fetch_id],
        )?;
        Ok(())
    }
}

pub fn load_existing_urls_from_db(db: &Db) -> Result<HashSet<Url>> {
    let mut stmt = db.conn.prepare(
        "SELECT url FROM articles",
    )?;

    let url_iter = stmt.query_and_then([], |row| {
        let url_str: String = row.get(0)?;
        let url = url_str.parse::<Url>()?;
        Ok(url)
    })?;

    let urls: HashSet<Url> = url_iter.collect::<Result<HashSet<_>>>()?;
    Ok(urls)
}
