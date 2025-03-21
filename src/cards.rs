use std::sync::{Arc, RwLock};

use alloy::primitives::Address;
use axum::{Json, debug_handler, extract::State, http::StatusCode, response::IntoResponse};
use rs_poker::core::{Card, Hand};
use serde_json::json;
use tracing::{info, instrument};

use crate::{privy::UserSession, state::AppState};

#[debug_handler]
#[instrument]
pub async fn hand(
    session: UserSession,
    State(state): State<Arc<RwLock<AppState>>>,
) -> Result<Json<Hand>, CardsError> {
    info!("endpoint called");
    let state = state.read().expect("state lock should not be poisoned");
    let Some(players) = state.get_players() else {
        return Err(CardsError::GameNotStarted);
    };
    let Some(player) = players.iter().find(|p| p.address == session.wallet) else {
        return Err(CardsError::PlayerNotFound(session.wallet));
    };
    let hand = player.starting_hand.clone();
    drop(state);
    Ok(Json(hand))
}

#[debug_handler]
#[instrument]
pub async fn flop(State(state): State<Arc<RwLock<AppState>>>) -> Result<Json<Hand>, CardsError> {
    info!("endpoint called");
    let state = state.read().expect("state lock should not be poisoned");
    let Some(flop) = state.get_flop() else {
        return Err(CardsError::FlopNotAvailable);
    };
    drop(state);
    Ok(Json(flop))
}

#[debug_handler]
#[instrument]
pub async fn turn(State(state): State<Arc<RwLock<AppState>>>) -> Result<Json<Card>, CardsError> {
    info!("endpoint called");
    let state = state.read().expect("state lock should not be poisoned");
    let Some(turn) = state.get_turn() else {
        return Err(CardsError::TurnNotAvailable);
    };
    drop(state);
    Ok(Json(turn))
}

#[debug_handler]
#[instrument]
pub async fn river(State(state): State<Arc<RwLock<AppState>>>) -> Result<Json<Card>, CardsError> {
    info!("endpoint called");
    let state = state.read().expect("state lock should not be poisoned");
    let Some(river) = state.get_river() else {
        return Err(CardsError::RiverNotAvailable);
    };
    drop(state);
    Ok(Json(river))
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum CardsError {
    #[error("game has not yet started")]
    GameNotStarted,

    #[error("flop is not yet available")]
    FlopNotAvailable,

    #[error("turn card is not yet available")]
    TurnNotAvailable,

    #[error("river card is not yet available")]
    RiverNotAvailable,

    #[error("player not found: {0}")]
    PlayerNotFound(Address),
}

impl IntoResponse for CardsError {
    fn into_response(self) -> axum::response::Response {
        let status = StatusCode::BAD_REQUEST; // for now all errors map to this error code, use a match if this changes
        let body = Json(json!({
            "error": self.to_string(),
        }));
        (status, body).into_response()
    }
}
