use alloy::primitives::Address;
use anyhow::{Result, bail};
use derive_more::{Deref, From, Into, IsVariant};
use itertools::Itertools as _;
use rs_poker::core::{Card, FlatDeck, Hand, Rankable as _};

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
        // TODO: send tx to change phase to `waitingForDealer`
        self.phase = GamePhase::WaitingForDealer;
    }

    pub fn start_game(&mut self, participants: &[TablePlayer]) -> Result<()> {
        // triggered when the phase changed to `WaitingForDealer` according to contract events
        // need to make sure that we accounted for all the participants which entered before the phase change
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
                starting_hand: Hand::new_with_cards((0..2).map(|_| deck.deal().unwrap()).collect()),
            });
        }
        self.phase = GamePhase::PreFlop { deck, players };
        // TODO: send tx to change phase to `PreFlop`
        Ok(())
    }

    pub fn reveal_flop(&mut self) -> Result<()> {
        let (mut deck, players) = match &self.phase {
            GamePhase::WaitingForFlop { deck, players } => (deck.clone(), players.clone()),
            GamePhase::WaitingForPlayers
            | GamePhase::WaitingForDealer
            | GamePhase::PreFlop { .. } => bail!("too soon"),
            _ => bail!("too late"),
        };
        let flop = Hand::new_with_cards((0..3).map(|_| deck.deal().unwrap()).collect());
        self.phase = GamePhase::Flop {
            deck,
            players,
            flop,
        };
        // TODO: send tx to change phase to `Flop`
        Ok(())
    }

    pub fn reveal_turn(&mut self) -> Result<()> {
        let (mut deck, players, flop) = match &self.phase {
            GamePhase::WaitingForTurn {
                deck,
                players,
                flop,
            } => (deck.clone(), players.clone(), flop.clone()),
            GamePhase::WaitingForPlayers
            | GamePhase::WaitingForDealer
            | GamePhase::PreFlop { .. }
            | GamePhase::WaitingForFlop { .. }
            | GamePhase::Flop { .. } => bail!("too soon"),
            _ => bail!("too late"),
        };
        let turn = deck.deal().unwrap();
        self.phase = GamePhase::Turn {
            deck,
            players,
            flop,
            turn,
        };
        // TODO: send tx to change phase to `Turn`
        Ok(())
    }

    pub fn reveal_river(&mut self) -> Result<()> {
        let (mut deck, players, flop, turn) = match &self.phase {
            GamePhase::WaitingForRiver {
                deck,
                players,
                flop,
                turn,
            } => (deck.clone(), players.clone(), flop.clone(), *turn),
            GamePhase::WaitingForPlayers
            | GamePhase::WaitingForDealer
            | GamePhase::PreFlop { .. }
            | GamePhase::WaitingForFlop { .. }
            | GamePhase::Flop { .. }
            | GamePhase::WaitingForTurn { .. }
            | GamePhase::Turn { .. } => bail!("too soon"),
            _ => bail!("too late"),
        };
        let river = deck.deal().unwrap();
        self.phase = GamePhase::River {
            deck,
            players,
            flop,
            turn,
            river,
        };
        // TODO: send tx to change phase to `River`
        Ok(())
    }

    pub fn announce_winner(&mut self) -> Result<()> {
        let GamePhase::WaitingForResult {
            players,
            flop,
            turn,
            river,
            ..
        } = &self.phase
        else {
            bail!("too soon");
        };
        // players and hands
        let _: Vec<_> = players
            .iter()
            .map(|p| (p.seat, p.starting_hand.clone()))
            .collect();
        // winner(s)
        let _: Vec<_> = players
            .iter()
            .map(|p| {
                let mut hand = p.starting_hand.clone();
                hand.extend(flop.iter());
                hand.insert(*turn);
                hand.insert(*river);
                (p, hand.rank_five())
            })
            .max_set_by_key(|(_, h)| *h)
            .into_iter()
            .map(|(p, _)| p.seat)
            .collect();
        // TODO: send tx to reveal winner(s) and cards of all players
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
