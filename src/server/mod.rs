pub mod router;
pub mod ws;

use axum::Router;
use crate::config::Config;

pub fn build_app(cfg: Config) -> Router {
    router::routes(cfg)
}
