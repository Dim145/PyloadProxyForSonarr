mod models;
mod routes;

use crate::state::AppState;
use axum::{
    Router,
    routing::{any, get},
};
use std::sync::Arc;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api", any(routes::api_endpoint))
        .route("/sabnzbd/api", any(routes::api_endpoint))
        .route("/health", get(routes::health))
        .with_state(state)
}
