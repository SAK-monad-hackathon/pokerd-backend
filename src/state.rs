use alloy::primitives::Address;
use derive_more::IsVariant;
use rs_poker::core::{Card, FlatDeck, Hand};

#[derive(Debug, Clone, Default, IsVariant)]
pub enum GamePhase {
    #[default]
    WaitingForPlayers,
    WaitingForDealer,
    PreFlop {
        deck: FlatDeck,
        players: Vec<Player>,
    },
    WaitingForFlop {
        deck: FlatDeck,
        players: Vec<Player>,
    },
    Flop {
        deck: FlatDeck,
        players: Vec<Player>,
        flop: Hand,
    },
    WaitingForTurn {
        deck: FlatDeck,
        players: Vec<Player>,
        flop: Hand,
    },
    Turn {
        deck: FlatDeck,
        players: Vec<Player>,
        flop: Hand,
        turn: Card,
    },
    WaitingForRiver {
        deck: FlatDeck,
        players: Vec<Player>,
        flop: Hand,
        turn: Card,
    },
    River {
        deck: FlatDeck,
        players: Vec<Player>,
        flop: Hand,
        turn: Card,
        river: Card,
    },
    WaitingForResult {
        deck: FlatDeck,
        players: Vec<Player>,
        flop: Hand,
        turn: Card,
        river: Card,
    },
}

impl GamePhase {
    #[must_use]
    pub fn get_players(&self) -> Option<&Vec<Player>> {
        match self {
            GamePhase::WaitingForPlayers | GamePhase::WaitingForDealer => None,
            GamePhase::PreFlop { players, .. }
            | GamePhase::WaitingForFlop { players, .. }
            | GamePhase::Flop { players, .. }
            | GamePhase::WaitingForTurn { players, .. }
            | GamePhase::Turn { players, .. }
            | GamePhase::WaitingForRiver { players, .. }
            | GamePhase::River { players, .. }
            | GamePhase::WaitingForResult { players, .. } => Some(players),
        }
    }

    pub fn get_players_mut(&mut self) -> Option<&mut Vec<Player>> {
        match self {
            GamePhase::WaitingForPlayers | GamePhase::WaitingForDealer => None,
            GamePhase::PreFlop { players, .. }
            | GamePhase::WaitingForFlop { players, .. }
            | GamePhase::Flop { players, .. }
            | GamePhase::WaitingForTurn { players, .. }
            | GamePhase::Turn { players, .. }
            | GamePhase::WaitingForRiver { players, .. }
            | GamePhase::River { players, .. }
            | GamePhase::WaitingForResult { players, .. } => Some(players),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Player {
    /// The wallet address of the player
    pub address: Address,

    /// The starting hand of the player
    pub starting_hand: Hand,
}

#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub phase: GamePhase,
}
