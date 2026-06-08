use reqwest;
use serde::Deserialize;
use std::fmt;

pub struct Client {
    http: reqwest::Client,
    base: String,
}

#[derive(Debug)]
pub enum ApiError {
    Http(reqwest::Error),
    NotFound(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Http(e) => write!(f, "{e}"),
            ApiError::NotFound(title) => write!(f, "page not found: \"{title}\""),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<reqwest::Error> for ApiError {
    fn from(e: reqwest::Error) -> Self {
        ApiError::Http(e)
    }
}

// --- Search types ---

#[derive(Debug, Deserialize)]
pub struct SearchResponse {
    pub pages: Vec<SearchPage>,
}

#[derive(Debug, Deserialize)]
pub struct SearchPage {
    #[allow(dead_code)]
    pub id: u64,
    #[allow(dead_code)]
    pub key: String,
    pub title: String,
    pub description: Option<String>,
    pub excerpt: Option<String>,
}

// --- Summary types ---

#[derive(Debug, Deserialize)]
pub struct Summary {
    pub title: String,
    pub description: Option<String>,
    pub extract: Option<String>,
    pub content_urls: Option<ContentUrls>,
}

#[derive(Debug, Deserialize)]
pub struct ContentUrls {
    pub desktop: Option<UrlSet>,
}

#[derive(Debug, Deserialize)]
pub struct UrlSet {
    pub page: Option<String>,
}

impl Client {
    pub fn new(lang: &str, site: &str) -> Self {
        let base = if site.starts_with("http") {
            site.trim_end_matches('/').to_string()
        } else {
            format!("https://{lang}.{site}.org")
        };
        let http = reqwest::Client::builder()
            .user_agent("wiki-cli/0.1 (https://github.com/kernelzeroday/wiki)")
            .build()
            .expect("failed to create HTTP client");
        Self { http, base }
    }

    pub async fn search(&self, query: &str, limit: u32) -> Result<SearchResponse, reqwest::Error> {
        let url = format!("{}/w/rest.php/v1/search/page", self.base);
        self.http
            .get(&url)
            .query(&[
                ("q", query),
                ("limit", &limit.to_string()),
            ])
            .send()
            .await?
            .json()
            .await
    }

    pub async fn summary_direct(&self, title: &str) -> Result<Option<Summary>, reqwest::Error> {
        let encoded = urlencoding::encode(title);
        let url = format!("{}/api/rest_v1/page/summary/{encoded}", self.base);
        let resp = self.http.get(&url).send().await?;
        if resp.status() == 404 {
            return Ok(None);
        }
        resp.json().await.map(Some)
    }

    pub async fn summary(&self, title: &str) -> Result<Summary, ApiError> {
        if let Some(s) = self.summary_direct(title).await? {
            return Ok(s);
        }
        let search = self.search(title, 1).await?;
        if let Some(page) = search.pages.first() {
            if let Some(s) = self.summary_direct(&page.title).await? {
                return Ok(s);
            }
        }
        Err(ApiError::NotFound(title.to_string()))
    }

    pub async fn page_html(&self, title: &str) -> Result<String, reqwest::Error> {
        let encoded = urlencoding::encode(title);
        let url = format!("{}/api/rest_v1/page/html/{encoded}", self.base);
        self.http.get(&url).send().await?.text().await
    }

    pub async fn random_summary(&self) -> Result<Summary, ApiError> {
        let url = format!("{}/api/rest_v1/page/random/summary", self.base);
        Ok(self.http.get(&url).send().await?.json().await?)
    }
}
