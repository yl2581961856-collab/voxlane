use axum::{routing::get, Router};
use tower_http::trace::TraceLayer;

use crate::config::Config;
use super::ws::ws_handler;

pub fn routes(_cfg: Config) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/ws", get(ws_handler))
        .layer(TraceLayer::new_for_http())
}
