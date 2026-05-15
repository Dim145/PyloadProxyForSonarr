#![allow(dead_code)]

use crate::config::Config;
use crate::error::ProxyError;
use reqwest::Client as HttpClient;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

#[derive(Clone)]
pub struct Client {
    http: HttpClient,
    base_url: String,
}

#[derive(Debug, Deserialize)]
pub struct Package {
    pub pid: i64,
    pub name: String,
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub linksdone: Option<i64>,
    #[serde(default)]
    pub sizedone: Option<i64>,
    #[serde(default)]
    pub linkstotal: Option<i64>,
    #[serde(default)]
    pub sizetotal: Option<i64>,
    #[serde(default)]
    pub links: Option<Vec<FileData>>,
}

#[derive(Debug, Deserialize)]
pub struct FileData {
    #[serde(default)]
    pub fid: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub status: i64,
    #[serde(default)]
    pub statusmsg: String,
    #[serde(default)]
    pub error: String,
}

#[derive(Debug, Deserialize)]
pub struct ServerStatus {
    pub pause: bool,
    pub active: i64,
    pub queue: i64,
    pub total: i64,
    pub speed: i64,
}

#[derive(Debug, Deserialize)]
pub struct DownloadInfo {
    #[serde(default)]
    pub fid: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub speed: i64,
    #[serde(default)]
    pub eta: i64,
    #[serde(default)]
    pub bleft: i64,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub percent: i32,
    #[serde(default)]
    pub status: i64,
    #[serde(default)]
    pub wait_until: f64,
    #[serde(default)]
    pub package_id: i64,
}

impl Client {
    pub fn new(cfg: &Config) -> Result<Self, ProxyError> {
        let mut headers = HeaderMap::new();
        let mut key = HeaderValue::from_str(&cfg.pyload_api_key)
            .map_err(|e| ProxyError::PyLoad(format!("invalid PYLOAD_API_KEY: {e}")))?;
        key.set_sensitive(true);
        headers.insert("X-API-Key", key);
        let http = HttpClient::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(30))
            .user_agent("pyload-proxy-for-sonarr/0.1")
            .build()?;
        Ok(Self {
            http,
            base_url: cfg.pyload_url.clone(),
        })
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, ProxyError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http.get(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ProxyError::PyLoad(format!("GET {path} → {status}: {body}")));
        }
        Ok(resp.json().await?)
    }

    pub async fn version(&self) -> Result<String, ProxyError> {
        self.get_json("/api/get_server_version").await
    }

    pub async fn status(&self) -> Result<ServerStatus, ProxyError> {
        self.get_json("/api/status_server").await
    }

    pub async fn add_package(&self, name: &str, urls: &[String], dest: u8) -> Result<i64, ProxyError> {
        let url = format!("{}/api/add_package", self.base_url);
        let resp = self
            .http
            .post(&url)
            .json(&json!({
                "name": name,
                "links": urls,
                "dest": dest,
            }))
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ProxyError::PyLoad(format!("add_package → {status}: {body}")));
        }
        Ok(resp.json::<i64>().await?)
    }

    pub async fn queue(&self) -> Result<Vec<Package>, ProxyError> {
        self.get_json("/api/get_queue_data").await
    }

    pub async fn collector(&self) -> Result<Vec<Package>, ProxyError> {
        self.get_json("/api/get_collector_data").await
    }

    pub async fn package_data(&self, pid: i64) -> Result<Package, ProxyError> {
        self.get_json(&format!("/api/get_package_data?package_id={pid}"))
            .await
    }

    pub async fn downloads(&self) -> Result<Vec<DownloadInfo>, ProxyError> {
        self.get_json("/api/status_downloads").await
    }

    pub async fn delete_packages(&self, pids: &[i64]) -> Result<(), ProxyError> {
        let url = format!("{}/api/delete_packages", self.base_url);
        let resp = self
            .http
            .post(&url)
            .json(&json!({ "package_ids": pids }))
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ProxyError::PyLoad(format!("delete_packages → {status}: {body}")));
        }
        Ok(())
    }
}
