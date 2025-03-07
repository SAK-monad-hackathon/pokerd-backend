use std::sync::{Arc, RwLock};

use alloy::primitives::Address;
use anyhow::anyhow;
use axum::{Json, debug_handler, extract::State, http::StatusCode, response::IntoResponse};
use derive_more::IsVariant;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    AppError,
    auth::Claims,
    state::{AppState, GamePhase},
};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IsVariant, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum Card {
    Ace = 1,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Jack,
    Queen,
    King,
}

impl From<Card> for u8 {
    fn from(value: Card) -> Self {
        value as u8
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hand([Card; 2]);

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum CardsError {
    #[error("match has not yet started")]
    MatchNotStarted,

    #[error("player not found: {0}")]
    PlayerNotFound(Address),
}

impl IntoResponse for CardsError {
    fn into_response(self) -> axum::response::Response {
        let status = match self {
            CardsError::MatchNotStarted | CardsError::PlayerNotFound(_) => StatusCode::BAD_REQUEST,
        };
        let body = Json(json!({
            "error": self.to_string(),
        }));
        (status, body).into_response()
    }
}

#[debug_handler]
pub async fn hand(
    claims: Claims,
    State(state): State<Arc<RwLock<AppState>>>,
) -> Result<Json<Hand>, CardsError> {
    let state = state.read().expect("state lock should not be poisoned");
    if matches!(state.phase, GamePhase::WaitingForPlayers) {
        return Err(CardsError::MatchNotStarted);
    }
    let Some(player) = state.players.iter().find(|p| p.address == claims.address) else {
        return Err(CardsError::PlayerNotFound(claims.address));
    };
    Ok(Json(player.hand.clone()))
}
