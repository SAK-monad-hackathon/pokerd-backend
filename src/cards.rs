use std::sync::{Arc, RwLock};

use anyhow::anyhow;
use axum::{Json, debug_handler, extract::State};
use derive_more::IsVariant;
use serde::{Deserialize, Serialize};

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

#[debug_handler]
pub async fn hand(
    claims: Claims,
    State(state): State<Arc<RwLock<AppState>>>,
) -> Result<Json<Hand>, AppError> {
    let state = state.read().expect("state lock should not be poisoned");
    if matches!(state.phase, GamePhase::WaitingForPlayers) {
        return Err(anyhow!("match has not yet started").into());
    }
    let Some(player) = state.players.iter().find(|p| p.address == claims.address) else {
        return Err(anyhow!("player not found in current game").into());
    };
    Ok(Json(player.hand.clone()))
}
