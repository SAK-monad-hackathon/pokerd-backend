//! Backend service for
use std::{
    env,
    sync::{Arc, RwLock},
};

use anyhow::Result;
use auth::authorize;
use axum::{
    Router,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use tracing::{debug, level_filters::LevelFilter};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

use crate::state::AppState;

pub mod auth;
pub mod cards;
pub mod state;

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

    // init app state
    let state = Arc::new(RwLock::new(AppState::default()));

    // routes
    let app = Router::new()
        .route("/", get(healthcheck))
        .route("/authorize", post(authorize))
        .with_state(state);

    // start server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    debug!("serving on port 3000");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn healthcheck() -> &'static str {
    "Server is running"
}

pub struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}
