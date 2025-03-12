use std::{
    fmt::Write,
    sync::{Arc, RwLock},
    time::Duration,
};

use IPokerTable::{currentPhaseReturn, playerIndicesReturn};
use alloy::{
    contract::{CallBuilder, CallDecoder},
    eips::{BlockNumberOrTag, eip1559::Eip1559Estimation},
    network::Ethereum,
    primitives::{Address, U256},
    providers::{Provider, ProviderBuilder},
    rpc::types::{Filter, TransactionReceipt},
    sol_types::SolEvent as _,
};
use anyhow::{Context as _, Result};
use futures_util::{StreamExt as _, stream};
use rs_poker::core::{Card, Hand};
use tracing::{info, warn};

use crate::state::{GamePhase, MAX_PLAYERS, TablePlayer};
#[allow(clippy::wildcard_imports)]
use crate::{bindings::*, state::AppState};

const ALL_EVENTS: [&str; 7] = [
    IPokerTable::PlayerJoined::SIGNATURE,
    IPokerTable::PlayerLeft::SIGNATURE,
    IPokerTable::PhaseChanged::SIGNATURE,
    IPokerTable::PlayerBet::SIGNATURE,
    IPokerTable::PlayerFolded::SIGNATURE,
    IPokerTable::PlayerWonWithoutShowdown::SIGNATURE,
    IPokerTable::ShowdownEnded::SIGNATURE,
];

#[allow(clippy::too_many_lines)]
pub async fn listen(state: Arc<RwLock<AppState>>) -> Result<()> {
    let (rpc_url, signer, table_address) = {
        let state = state.read().unwrap();
        (
            state.rpc_url.clone(),
            state.signer.clone(),
            state.table_address,
        )
    };
    let wallet = signer.default_signer().address();
    let provider = ProviderBuilder::new()
        .wallet(signer)
        .on_http(rpc_url.parse()?);

    let table = IPokerTable::IPokerTableInstance::new(table_address, &provider);

    let currentPhaseReturn { phase } = table.currentPhase().call().await?;
    if !matches!(phase, IPokerTable::GamePhases::WaitingForPlayers) {
        warn!("a round is already ongoing, need to cancel");
        let tx = table.cancelCurrentRound();
        let receipt = submit_tx_with_retry(&provider, wallet, tx).await?;
        let hash = receipt.transaction_hash;
        if receipt.status() {
            info!("transaction {hash} succeeded");
        } else {
            warn!("transaction {hash} reverted");
        }
        info!("cancelled current round");
    }

    // retrieve existing players
    for seat in 0..MAX_PLAYERS {
        let playerIndicesReturn { player } = table.playerIndices(U256::from(seat)).call().await?;
        if player != Address::ZERO {
            state.write().unwrap().table_players.push(TablePlayer {
                address: player,
                seat: seat.into(),
            });
        }
    }

    let filter = Filter::new()
        .address(table_address)
        .events(ALL_EVENTS)
        .from_block(BlockNumberOrTag::Latest);

    let poller = provider.watch_logs(&filter).await?;
    let mut stream = poller.into_stream().flat_map(stream::iter);

    while let Some(log) = stream.next().await {
        let Some(topic) = log.topic0() else {
            continue;
        };
        match *topic {
            IPokerTable::PlayerJoined::SIGNATURE_HASH => {
                let log = IPokerTable::PlayerJoined::decode_log(&log.inner, true)?;
                let num_players = {
                    let mut state = state.write().unwrap();
                    state.table_players.push(TablePlayer {
                        address: log.address,
                        seat: log.indexOnTable.try_into()?,
                    });
                    state.table_players.len()
                };
                if num_players > 1 {
                    let tx = table
                        .setCurrentPhase(IPokerTable::GamePhases::WaitingForDealer, String::new());
                    let receipt = submit_tx_with_retry(&provider, wallet, tx).await?;
                    let hash = receipt.transaction_hash;
                    if receipt.status() {
                        info!("transaction {hash} succeeded");
                    } else {
                        warn!("transaction {hash} reverted");
                    }
                }
            }
            IPokerTable::PlayerLeft::SIGNATURE_HASH => {
                let log = IPokerTable::PlayerLeft::decode_log(&log.inner, true)?;
                {
                    let mut state = state.write().unwrap();
                    state.table_players.retain(|p| p.address != log.address);
                    state.remove_player(log.indexOnTable.try_into()?)?;
                }
            }
            IPokerTable::PhaseChanged::SIGNATURE_HASH => {
                let log = IPokerTable::PhaseChanged::decode_log(&log.inner, true)?;
                match log.newPhase {
                    IPokerTable::GamePhases::WaitingForPlayers => {}
                    IPokerTable::GamePhases::WaitingForDealer => {
                        {
                            state.write().unwrap().set_ready();
                        }
                        let tx =
                            table.setCurrentPhase(IPokerTable::GamePhases::PreFlop, String::new());
                        let receipt = submit_tx_with_retry(&provider, wallet, tx).await?;
                        let hash = receipt.transaction_hash;
                        if receipt.status() {
                            info!("transaction {hash} succeeded");
                        } else {
                            warn!("transaction {hash} reverted");
                        }
                    }
                    IPokerTable::GamePhases::PreFlop => {
                        // TODO: start timeout and then kick players which haven't bet
                    }
                    IPokerTable::GamePhases::WaitingForFlop => {
                        let flop = {
                            let mut state = state.write().unwrap();
                            state.set_waiting_for_flop()?;
                            state.reveal_flop()?
                        };
                        let tx = table
                            .setCurrentPhase(IPokerTable::GamePhases::Flop, hand_to_string(&flop));
                        let receipt = submit_tx_with_retry(&provider, wallet, tx).await?;
                        let hash = receipt.transaction_hash;
                        if receipt.status() {
                            info!("transaction {hash} succeeded");
                        } else {
                            warn!("transaction {hash} reverted");
                        }
                    }
                    IPokerTable::GamePhases::Flop => {
                        // TODO: start timeout and then kick players which haven't bet
                    }
                    IPokerTable::GamePhases::WaitingForTurn => {
                        let turn = {
                            let mut state = state.write().unwrap();
                            state.set_waiting_for_turn()?;
                            state.reveal_turn()?
                        };
                        let tx = table
                            .setCurrentPhase(IPokerTable::GamePhases::Turn, card_to_string(turn));
                        let receipt = submit_tx_with_retry(&provider, wallet, tx).await?;
                        let hash = receipt.transaction_hash;
                        if receipt.status() {
                            info!("transaction {hash} succeeded");
                        } else {
                            warn!("transaction {hash} reverted");
                        }
                    }
                    IPokerTable::GamePhases::Turn => {
                        // TODO: start timeout and then kick players which haven't bet
                    }
                    IPokerTable::GamePhases::WaitingForRiver => {
                        let river = {
                            let mut state = state.write().unwrap();
                            state.set_waiting_for_river()?;
                            state.reveal_river()?
                        };
                        let tx = table
                            .setCurrentPhase(IPokerTable::GamePhases::River, card_to_string(river));
                        let receipt = submit_tx_with_retry(&provider, wallet, tx).await?;
                        let hash = receipt.transaction_hash;
                        if receipt.status() {
                            info!("transaction {hash} succeeded");
                        } else {
                            warn!("transaction {hash} reverted");
                        }
                    }
                    IPokerTable::GamePhases::River => {
                        // TODO: start timeout and then kick players which haven't bet
                    }
                    IPokerTable::GamePhases::WaitingForResult => {
                        let (hands, winners) = {
                            let mut state = state.write().unwrap();
                            state.set_waiting_for_result()?;
                            state.announce_winner()?
                        };
                        let tx = table.revealShowdownResult(
                            (0..MAX_PLAYERS)
                                .map(|seat| {
                                    hands
                                        .iter()
                                        .find(|(s, _)| **s == seat)
                                        .map_or(String::new(), |(_, h)| hand_to_string(h))
                                })
                                .collect(),
                            winners.into_iter().map(Into::into).collect(),
                        );
                        let receipt = submit_tx_with_retry(&provider, wallet, tx).await?;
                        let hash = receipt.transaction_hash;
                        if receipt.status() {
                            info!("transaction {hash} succeeded");
                        } else {
                            warn!("transaction {hash} reverted");
                        }
                    }
                    IPokerTable::GamePhases::__Invalid => continue,
                }
            }
            IPokerTable::PlayerBet::SIGNATURE_HASH => {}
            IPokerTable::PlayerFolded::SIGNATURE_HASH => {
                let log = IPokerTable::PlayerFolded::decode_log(&log.inner, true)?;
                {
                    let mut state = state.write().unwrap();
                    state.remove_player(log.indexOnTable.try_into()?)?;
                }
            }
            IPokerTable::PlayerWonWithoutShowdown::SIGNATURE_HASH
            | IPokerTable::ShowdownEnded::SIGNATURE_HASH => {
                state.write().unwrap().phase = GamePhase::default();
            }
            _ => {
                continue;
            }
        }
    }
    warn!("stream finished");

    Ok(())
}

pub async fn submit_tx_with_retry<T: Clone, P: Provider + Clone, D: CallDecoder + Clone>(
    provider: impl Provider,
    wallet: Address,
    tx: CallBuilder<T, P, D, Ethereum>,
) -> Result<TransactionReceipt> {
    // set a fixed nonce so we can re-submit with more gas
    let mut nonce = provider
        .get_transaction_count(wallet)
        .pending()
        .await
        .context("failed to get nonce for the wallet")?;
    let mut gas = Eip1559Estimation {
        max_fee_per_gas: 0,
        max_priority_fee_per_gas: 0,
    };
    let mut tries = 0usize;
    loop {
        // if the new gas is not enough to re-submit the transaction, increase it, otherwise use the new gas estimate
        update_gas(
            &mut gas,
            provider
                .estimate_eip1559_fees()
                .await
                .context("failed to estimate gas")?,
        );

        let pending = tx
            .clone()
            .nonce(nonce)
            .max_fee_per_gas(gas.max_fee_per_gas)
            .max_priority_fee_per_gas(gas.max_priority_fee_per_gas)
            .send()
            .await?;
        let hash = pending.tx_hash().to_string();
        match pending
            .with_timeout(Some(Duration::from_secs(30)))
            .get_receipt()
            .await
        {
            Ok(receipt) => {
                return Ok(receipt);
            }
            Err(e) => {
                match tries {
                    0..=5 => {} // try again with more gas
                    6..=8 => {
                        // try to replace a "blocked" transaction by getting the latest transaction count, ignoring
                        // any tx in the mempool
                        warn!(
                            "retried {hash} with the same nonce for 5 times which didn't work, now retrying with the earliest pending nonce"
                        );
                        nonce = provider
                            .get_transaction_count(wallet)
                            .latest()
                            .await
                            .context("failed to get nonce for the bot wallet")?;
                    }
                    9.. => {
                        return Err(e).context("could not get receipt after many tries");
                    }
                }
                tries += 1;
                warn!(tries, err = ?e, "transaction {hash} was not mined after timeout, retrying with more gas");
            }
        }
    }
}

/// Update the gas estimate with a new estimate, making sure that the new values are at least 10% higher than the old
/// ones.
fn update_gas(old: &mut Eip1559Estimation, new_estimate: Eip1559Estimation) {
    old.max_fee_per_gas = new_estimate
        .max_fee_per_gas
        .max((old.max_fee_per_gas * 110).div_ceil(100));
    old.max_priority_fee_per_gas = new_estimate
        .max_priority_fee_per_gas
        .max((old.max_priority_fee_per_gas * 110).div_ceil(100));
}

#[must_use]
pub fn card_to_string(card: Card) -> String {
    format!("{}{}", card.value.to_char(), card.suit.to_char())
}

#[must_use]
pub fn hand_to_string(hand: &Hand) -> String {
    hand.iter().fold(String::new(), |mut output, c| {
        let _ = write!(output, "{}", c.value.to_char());
        let _ = write!(output, "{}", c.suit.to_char());
        output
    })
}
