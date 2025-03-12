//! Backend service for
use std::{
    env,
    sync::{Arc, RwLock},
};

use alloy::{
    hex::FromHex as _,
    primitives::{Address, B256, address},
    signers::local::PrivateKeySigner,
};
use anyhow::{Context as _, Result};
use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::json;
use tracing::{debug, info, instrument, level_filters::LevelFilter};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt as _};

use cards::{flop, hand, river, turn};
use privy::{Privy, PrivyConfig};
use state::{AppState, GamePhase, TablePlayer};

pub mod bindings;
pub mod cards;
pub mod listener;
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
        .unwrap_or(EnvFilter::new("error,pokerd_backend=debug"));
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(env_filter)
        .init();

    // init app state
    let state = Arc::new(RwLock::new(AppState {
        privy: Privy::new(PrivyConfig::from_env()?),
        rpc_url: env::var("RPC_URL").context("RPC_URL environment variable")?,
        signer: PrivateKeySigner::from_bytes(&B256::from_hex(
            env::var("PRIVATE_KEY").context("PRIVATE_KEY environment variable")?,
        )?)?
        .into(),
        table_address: env::var("TABLE_ADDRESS")
            .context("TABLE_ADDRESS environment variable")?
            .parse()?,
        table_players: vec![],
        phase: GamePhase::default(),
    }));

    // start listener task
    tokio::spawn({
        let state = Arc::clone(&state);
        listener::listen(state)
    });

    // routes
    let app = Router::new()
        .route("/", get(healthcheck))
        .route("/hand", get(hand))
        .route("/flop", get(flop))
        .route("/turn", get(turn))
        .route("/river", get(river))
        .with_state(state);

    // start server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    debug!("serving on port 3000");
    axum::serve(listener, app).await?;
    Ok(())
}

#[instrument]
async fn healthcheck() -> &'static str {
    info!("endpoint called");
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
