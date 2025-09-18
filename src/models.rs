use chrono::{NaiveDate, Utc};
use chrono::DateTime;
use url::Url;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Summary {
    pub mood: String,
    pub items: Vec<Bullet>,
    pub date: NaiveDate,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Bullet {
    pub text: String,
    pub accepted: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct NewsArticle {
    pub url: Url, // from the 'url' crate
    pub og_title: String,
    pub published_time: DateTime<Utc>,
    pub og_image: Url, // url pointing to an image
    pub author: String,
    pub text: String,
    pub source: String,
}
