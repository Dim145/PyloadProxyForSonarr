use crate::config::Config;
use crate::pyload::Client;

pub struct AppState {
    pub config: Config,
    pub pyload: Client,
}

impl AppState {
    pub fn new(config: Config, pyload: Client) -> Self {
        Self { config, pyload }
    }
}
