use alloy::primitives::Address;
use derive_more::IsVariant;

use crate::cards::Card;

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
    address: Address,
    hand: [Card; 2],
}

#[derive(Debug, Clone, Default, Hash)]
pub struct AppState {
    phase: GamePhase,
    players: Vec<Player>,
}
