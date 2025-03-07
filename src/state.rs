use alloy::primitives::Address;
use derive_more::IsVariant;

use crate::cards::Hand;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, IsVariant)]
pub enum GamePhase {
    #[default]
    WaitingForPlayers,
    WaitingForDealer,
    PreFlop,
    WaitingForFlop,
    Flop,
    WaitingForTurn,
    Turn,
    WaitingForRiver,
    River,
    WaitingForResult,
}

#[derive(Debug, Clone, Hash)]
pub struct Player {
    pub address: Address,
    pub hand: Hand,
}

#[derive(Debug, Clone, Default, Hash)]
pub struct AppState {
    pub phase: GamePhase,
    pub players: Vec<Player>,
}
