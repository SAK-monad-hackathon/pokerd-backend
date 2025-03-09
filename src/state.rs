use alloy::primitives::Address;
use anyhow::{Result, bail};
use derive_more::{Deref, From, Into, IsVariant};
use rs_poker::core::{Card, FlatDeck, Hand};

use crate::privy::Privy;

pub const MAX_PLAYERS: usize = 5;

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

#[derive(Debug, Copy, Clone, From, Into, Deref)]
pub struct Seat(usize);

#[derive(Debug, Clone)]
pub struct TablePlayer {
    pub address: Address,
    pub seat: Seat,
}

#[derive(Debug, Clone)]
pub struct Player {
    /// The wallet address of the player
    pub address: Address,

    /// The seat ID of the player
    pub seat: Seat,

    /// The starting hand of the player
    pub starting_hand: Hand,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub privy: Privy,
    pub table_players: Vec<TablePlayer>,
    pub phase: GamePhase,
}

impl AppState {
    pub fn set_ready(&mut self) {
        self.phase = GamePhase::WaitingForDealer;
        // TODO send tx to contract to set state
    }

    pub fn start_game(&mut self, participants: &[TablePlayer]) -> Result<()> {
        match self.phase {
            GamePhase::WaitingForDealer => {}
            GamePhase::WaitingForPlayers => bail!("still waiting for players"),
            _ => bail!("game has already started"),
        }
        if participants.len() < 2 {
            bail!("not enough players");
        }
        if participants.len() > MAX_PLAYERS {
            bail!("too many players");
        }
        let mut players = vec![];
        let mut deck = FlatDeck::default(); // already shuffled
        for player in participants {
            players.push(Player {
                address: player.address,
                seat: player.seat,
                starting_hand: Hand::new_with_cards(vec![
                    deck.deal().expect("should have enough cards"),
                    deck.deal().expect("should have enough cards"),
                ]),
            });
        }
        self.phase = GamePhase::PreFlop { deck, players };
        // TODO send tx to contract to set state
        Ok(())
    }

    #[must_use]
    pub fn get_players(&self) -> Option<&Vec<Player>> {
        match &self.phase {
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

    #[must_use]
    pub fn get_flop(&self) -> Option<Hand> {
        match &self.phase {
            GamePhase::WaitingForPlayers
            | GamePhase::WaitingForDealer
            | GamePhase::PreFlop { .. }
            | GamePhase::WaitingForFlop { .. } => None,
            GamePhase::Flop { flop, .. }
            | GamePhase::WaitingForTurn { flop, .. }
            | GamePhase::Turn { flop, .. }
            | GamePhase::WaitingForRiver { flop, .. }
            | GamePhase::River { flop, .. }
            | GamePhase::WaitingForResult { flop, .. } => Some(flop.clone()),
        }
    }

    #[must_use]
    pub fn get_turn(&self) -> Option<Card> {
        match self.phase {
            GamePhase::WaitingForPlayers
            | GamePhase::WaitingForDealer
            | GamePhase::PreFlop { .. }
            | GamePhase::WaitingForFlop { .. }
            | GamePhase::Flop { .. }
            | GamePhase::WaitingForTurn { .. } => None,
            GamePhase::Turn { turn, .. }
            | GamePhase::WaitingForRiver { turn, .. }
            | GamePhase::River { turn, .. }
            | GamePhase::WaitingForResult { turn, .. } => Some(turn),
        }
    }

    #[must_use]
    pub fn get_river(&self) -> Option<Card> {
        match self.phase {
            GamePhase::WaitingForPlayers
            | GamePhase::WaitingForDealer
            | GamePhase::PreFlop { .. }
            | GamePhase::WaitingForFlop { .. }
            | GamePhase::Flop { .. }
            | GamePhase::WaitingForTurn { .. }
            | GamePhase::Turn { .. }
            | GamePhase::WaitingForRiver { .. } => None,
            GamePhase::River { river, .. } | GamePhase::WaitingForResult { river, .. } => {
                Some(river)
            }
        }
    }
}
