mod config;
mod core;
mod error;
mod integration;
mod protocol;
mod server;

use crate::config::Config;
use crate::server::build_app;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "voxlane=debug,tower_http=info,axum=info".into()),
        )
        .init();

    let cfg = Config::from_env()?;
    let app = build_app(cfg.clone());

    let listener = tokio::net::TcpListener::bind(&cfg.bind_addr).await?;
    tracing::info!("listening on {}", cfg.bind_addr);

    axum::serve(listener, app).await?;
    Ok(())
}
