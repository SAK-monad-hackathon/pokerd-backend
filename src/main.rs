use std::env;

use anyhow::Result;
use axum::{Router, routing::get};
use tracing::{debug, level_filters::LevelFilter};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

#[tokio::main]
async fn main() -> Result<()> {
    // initialize tracing
    let env_filter = env::var("RUST_LOG")
        .map(|log_level| {
            EnvFilter::builder()
                .with_default_directive(LevelFilter::ERROR.into())
                .parse_lossy(log_level)
        })
        .unwrap_or(EnvFilter::new("debug"));
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(env_filter)
        .init();

    // routes
    let app = Router::new().route("/", get(root));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    debug!("serving on port 3000");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn root() -> &'static str {
    "Hello, World!"
}
