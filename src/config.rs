use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub api_key: String,
    pub pyload_url: String,
    pub pyload_api_key: String,
    pub download_dir: String,
    pub pyload_dest: u8,
    pub default_category: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            port: env_or("PORT", "8080").parse()?,
            api_key: env_required("SABNZBD_API_KEY")?,
            pyload_url: env_required("PYLOAD_URL")?.trim_end_matches('/').to_string(),
            pyload_api_key: env_required("PYLOAD_API_KEY")?,
            download_dir: env_or("DOWNLOAD_DIR", "/downloads"),
            pyload_dest: env_or("PYLOAD_DEST", "1").parse()?,
            default_category: env_or("DEFAULT_CATEGORY", "sonarr"),
        })
    }
}

fn env_required(key: &str) -> anyhow::Result<String> {
    env::var(key).map_err(|_| anyhow::anyhow!("missing required env var: {key}"))
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}
