use std::sync::{Arc, RwLock};

use alloy::primitives::Address;
use axum::{Json, debug_handler, extract::State, http::StatusCode, response::IntoResponse};
use rs_poker::core::Hand;
use serde_json::json;

use crate::{auth::Claims, state::AppState};

#[debug_handler]
pub async fn hand(
    claims: Claims,
    State(state): State<Arc<RwLock<AppState>>>,
) -> Result<Json<Hand>, CardsError> {
    let state = state.read().expect("state lock should not be poisoned");
    let Some(players) = state.phase.get_players() else {
        return Err(CardsError::GameNotStarted);
    };
    let Some(player) = players.iter().find(|p| p.address == claims.address) else {
        return Err(CardsError::PlayerNotFound(claims.address));
    };
    Ok(Json(player.starting_hand.clone()))
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum CardsError {
    #[error("game has not yet started")]
    GameNotStarted,

    #[error("player not found: {0}")]
    PlayerNotFound(Address),
}

impl IntoResponse for CardsError {
    fn into_response(self) -> axum::response::Response {
        let status = match self {
            CardsError::GameNotStarted | CardsError::PlayerNotFound(_) => StatusCode::BAD_REQUEST,
        };
        let body = Json(json!({
            "error": self.to_string(),
        }));
        (status, body).into_response()
    }
}
