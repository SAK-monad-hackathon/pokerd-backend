//! Backend service for
use std::{
    env,
    sync::{Arc, RwLock},
};

use anyhow::Result;
use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::json;
use tracing::{debug, level_filters::LevelFilter};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

use cards::hand;
use privy::{Privy, PrivyConfig};
use state::{AppState, GamePhase};

pub mod cards;
pub mod privy;
pub mod state;

#[tokio::main]
async fn main() -> Result<()> {
    // read .env if present
    let _ = dotenvy::dotenv();

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
    let state = Arc::new(RwLock::new(AppState {
        phase: GamePhase::default(),
        privy: Privy::new(PrivyConfig::from_env()?),
    }));

    // routes
    let app = Router::new()
        .route("/", get(healthcheck))
        .route("/hand", get(hand))
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

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum AppError {
    #[error("internal server error: {0}")]
    Internal(#[from] anyhow::Error),

    #[error("auth error: {0}")]
    Auth(#[from] privy::PrivyError),

    #[error("cards endpoint error: {0}")]
    Cards(#[from] cards::CardsError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::Internal(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
            AppError::Auth(err) => {
                return err.into_response();
            }
            AppError::Cards(err) => {
                return err.into_response();
            }
        };

        let body = Json(json!({
            "error": error_message
        }));
        (status, body).into_response()
    }
}
