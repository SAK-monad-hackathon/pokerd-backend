use std::{
    fmt::Write,
    sync::{Arc, RwLock},
    time::Duration,
};

use IPokerTable::{currentPhaseReturn, playerIndicesReturn};
use alloy::{
    contract::{CallBuilder, CallDecoder},
    eips::eip1559::Eip1559Estimation,
    network::Ethereum,
    primitives::{Address, U256},
    providers::{Provider, ProviderBuilder},
    rpc::{
        self,
        types::{Filter, Log, TransactionReceipt},
    },
    sol_types::SolEvent as _,
    transports::{http::Http, layers::RetryBackoffLayer},
};
use anyhow::{Context as _, Result};
use rs_poker::core::{Card, Hand};
use tokio::time::MissedTickBehavior;
use tracing::{debug, info, trace, warn};

use crate::state::{GamePhase, MAX_PLAYERS, TablePlayer};
use crate::{bindings::IPokerTable, state::AppState};

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
    let (rpc_url, signer, table_address, mut last_processed_block) = {
        let state = state.read().unwrap();
        (
            state.rpc_url.clone(),
            state.signer.clone(),
            state.table_address,
            state.last_processed_block,
        )
    };
    let wallet = signer.default_signer().address();
    let transport = Http::with_client(
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("reqwest client should be built successfully"),
        rpc_url.parse()?,
    );
    let provider = ProviderBuilder::new().wallet(signer).on_client(
        rpc::client::ClientBuilder::default()
            // retry requests max 5 times, with 1 second of initial backoff. Rate limit of 100'000 CU
            // per second. If the error is an HTTP 429 with backoff information, those parameters are
            // used automatically
            .layer(RetryBackoffLayer::new(5, 1000, 100_000))
            .transport(transport, false),
    );

    let table = IPokerTable::IPokerTableInstance::new(table_address, &provider);

    let currentPhaseReturn { phase } = table
        .currentPhase()
        .call()
        .await
        .context("getting current phase")?;
    if !matches!(phase, IPokerTable::GamePhases::WaitingForPlayers) {
        warn!("a round is already ongoing, need to cancel");
        let tx = table.cancelCurrentRound();
        let receipt = submit_tx_with_retry(&provider, wallet, tx)
            .await
            .context("sending round cancellation tx")?;
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
        let playerIndicesReturn { player } = table
            .playerIndices(U256::from(seat))
            .call()
            .await
            .context("getting player seat")?;
        if player == Address::ZERO {
            debug!("no player for seat {seat}");
        } else {
            info!(?player, seat, "found player");
            state.write().unwrap().table_players.push(TablePlayer {
                address: player,
                seat: seat.into(),
            });
        }
    }

    // let filter = Filter::new()
    //     .address(table_address)
    //     .events(ALL_EVENTS)
    //     .from_block(BlockNumberOrTag::Latest);

    // let poller = provider.watch_blocks().await?;

    // let poller = provider
    //     .watch_logs(&filter)
    //     .await
    //     .context("registering log filter")?;
    // let mut stream = poller.into_stream().flat_map(stream::iter);
    if last_processed_block == 0 {
        last_processed_block = provider
            .get_block_number()
            .await
            .context("getting latest block number")?
            - 1;
        debug!("processing logs from latest block {last_processed_block}");
    }

    let mut interval = tokio::time::interval(Duration::from_secs(2));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        interval.tick().await;
        let latest_block = provider
            .get_block_number()
            .await
            .context("getting latest block number")?;
        if latest_block <= last_processed_block {
            continue;
        }
        trace!(latest_block);
        let filter = Filter::new()
            .address(table_address)
            .events(ALL_EVENTS)
            .from_block(last_processed_block + 1)
            .to_block(latest_block);
        let logs = provider.get_logs(&filter).await.with_context(|| {
            format!(
                "getting logs for block range {} - {latest_block}",
                last_processed_block + 1
            )
        })?;
        let mut logs: Vec<_> = logs
            .into_iter()
            .filter(|l| l.block_number.is_some() && l.log_index.is_some())
            .collect();
        // make sure they are sorted
        logs.sort_by(|a, b| {
            a.block_number
                .unwrap()
                .cmp(&b.block_number.unwrap())
                .then(a.log_index.unwrap().cmp(&b.log_index.unwrap()))
        });
        if logs.is_empty() {
            trace!(
                start = last_processed_block + 1,
                end = latest_block,
                "no logs"
            );
        } else {
            debug!(
                start = last_processed_block + 1,
                end = latest_block,
                logs = logs.len(),
                "got logs"
            );
        }
        for log in logs {
            handle_event(&provider, Arc::clone(&state), &table, wallet, log)
                .await
                .context("processing log")?;
        }
        last_processed_block = latest_block;
        state.write().unwrap().last_processed_block = last_processed_block;
    }

    // while let Some(block_hash) = stream.next().await {
    //     debug!("new block {block_hash}");
    //     let filter = Filter::new()
    //         .address(table_address)
    //         .events(ALL_EVENTS)
    //         .at_block_hash(block_hash);
    //     let logs = provider
    //         .get_logs(&filter)
    //         .await
    //         .with_context(|| format!("getting logs for block {block_hash}"))?;
    //     for log in logs {
    //         handle_event(&provider, Arc::clone(&state), &table, wallet, log).await?;
    //     }
    // }
    // warn!("stream finished");
    // Ok(())
}

#[allow(clippy::too_many_lines)]
pub async fn handle_event<P: Provider>(
    provider: P,
    state: Arc<RwLock<AppState>>,
    table: &IPokerTable::IPokerTableInstance<(), P>,
    wallet: Address,
    log: Log,
) -> Result<()> {
    let Some(topic) = log.topic0() else {
        return Ok(());
    };
    match *topic {
        IPokerTable::PlayerJoined::SIGNATURE_HASH => {
            let log = IPokerTable::PlayerJoined::decode_log(&log.inner, true)
                .context("decoding log for PlayerJoined")?;
            let num_players = {
                let mut state = state.write().unwrap();
                let seat = log.indexOnTable.try_into()?;
                state.table_players.push(TablePlayer {
                    address: log.address,
                    seat,
                });
                info!(player = ?log.address, seat = seat.to_string(), "new player joined");
                state.table_players.len()
            };
            if num_players > 1 {
                info!("we have {num_players} players, round starting");
                let tx =
                    table.setCurrentPhase(IPokerTable::GamePhases::WaitingForDealer, String::new());
                let receipt = submit_tx_with_retry(&provider, wallet, tx)
                    .await
                    .context("submitting tx")?;
                let hash = receipt.transaction_hash;
                if receipt.status() {
                    info!("transaction {hash} succeeded");
                } else {
                    warn!("transaction {hash} reverted");
                }
            }
        }
        IPokerTable::PlayerLeft::SIGNATURE_HASH => {
            let log = IPokerTable::PlayerLeft::decode_log(&log.inner, true)
                .context("decoding log for PlayerLeft")?;
            {
                let mut state = state.write().unwrap();
                state.table_players.retain(|p| p.address != log.address);
                let seat = log.indexOnTable.try_into()?;
                state
                    .remove_player(seat)
                    .context("removing player from round because they left")?;
                info!(player = ?log.address, seat = seat.to_string(), "player left");
            }
        }
        IPokerTable::PhaseChanged::SIGNATURE_HASH => {
            let log = IPokerTable::PhaseChanged::decode_log(&log.inner, true)
                .context("decoding log for PhaseChanged")?;
            debug!(new_phase = ?log.newPhase, "phase changed");
            match log.newPhase {
                IPokerTable::GamePhases::WaitingForPlayers => {
                    info!("entered waiting for players phase");
                    let num_players = {
                        let state = state.read().unwrap();
                        state.table_players.len()
                    };
                    if num_players > 1 {
                        info!("we have {num_players} players, round starting");
                        let tx = table.setCurrentPhase(
                            IPokerTable::GamePhases::WaitingForDealer,
                            String::new(),
                        );
                        let receipt = submit_tx_with_retry(&provider, wallet, tx)
                            .await
                            .context("submitting tx")?;
                        let hash = receipt.transaction_hash;
                        if receipt.status() {
                            info!("transaction {hash} succeeded");
                        } else {
                            warn!("transaction {hash} reverted");
                        }
                    }
                }
                IPokerTable::GamePhases::WaitingForDealer => {
                    {
                        state.write().unwrap().set_ready();
                    }
                    info!("starting pre-flop phase");
                    let tx = table.setCurrentPhase(IPokerTable::GamePhases::PreFlop, String::new());
                    let receipt = submit_tx_with_retry(&provider, wallet, tx)
                        .await
                        .context("submitting tx")?;
                    let hash = receipt.transaction_hash;
                    if receipt.status() {
                        info!("transaction {hash} succeeded");
                    } else {
                        warn!("transaction {hash} reverted");
                    }
                }
                IPokerTable::GamePhases::PreFlop => {
                    info!("started pre-flop phase");
                    // TODO: start timeout and then kick players which haven't bet
                }
                IPokerTable::GamePhases::WaitingForFlop => {
                    let flop = {
                        let mut state = state.write().unwrap();
                        state
                            .set_waiting_for_flop()
                            .context("setting WaitingForFlop phase")?;
                        state.reveal_flop().context("revealing flop")?
                    };
                    info!(?flop, "starting flop phase");
                    let tx =
                        table.setCurrentPhase(IPokerTable::GamePhases::Flop, hand_to_string(&flop));
                    let receipt = submit_tx_with_retry(&provider, wallet, tx)
                        .await
                        .context("submitting tx")?;
                    let hash = receipt.transaction_hash;
                    if receipt.status() {
                        info!("transaction {hash} succeeded");
                    } else {
                        warn!("transaction {hash} reverted");
                    }
                }
                IPokerTable::GamePhases::Flop => {
                    info!("started flop phase");
                    // TODO: start timeout and then kick players which haven't bet
                }
                IPokerTable::GamePhases::WaitingForTurn => {
                    let turn = {
                        let mut state = state.write().unwrap();
                        state
                            .set_waiting_for_turn()
                            .context("setting WaitingForTurn phase")?;
                        state.reveal_turn().context("revealing turn card")?
                    };
                    info!(?turn, "starting turn phase");
                    let tx =
                        table.setCurrentPhase(IPokerTable::GamePhases::Turn, card_to_string(turn));
                    let receipt = submit_tx_with_retry(&provider, wallet, tx)
                        .await
                        .context("submitting tx")?;
                    let hash = receipt.transaction_hash;
                    if receipt.status() {
                        info!("transaction {hash} succeeded");
                    } else {
                        warn!("transaction {hash} reverted");
                    }
                }
                IPokerTable::GamePhases::Turn => {
                    info!("started turn phase");
                    // TODO: start timeout and then kick players which haven't bet
                }
                IPokerTable::GamePhases::WaitingForRiver => {
                    let river = {
                        let mut state = state.write().unwrap();
                        state
                            .set_waiting_for_river()
                            .context("setting WaitingForRiver phase")?;
                        state.reveal_river().context("revealing river card")?
                    };
                    info!(?river, "starting river phase");
                    let tx = table
                        .setCurrentPhase(IPokerTable::GamePhases::River, card_to_string(river));
                    let receipt = submit_tx_with_retry(&provider, wallet, tx)
                        .await
                        .context("submitting tx")?;
                    let hash = receipt.transaction_hash;
                    if receipt.status() {
                        info!("transaction {hash} succeeded");
                    } else {
                        warn!("transaction {hash} reverted");
                    }
                }
                IPokerTable::GamePhases::River => {
                    info!("started river phase");
                    // TODO: start timeout and then kick players which haven't bet
                }
                IPokerTable::GamePhases::WaitingForResult => {
                    let (hands, winners) = {
                        let mut state = state.write().unwrap();
                        state
                            .set_waiting_for_result()
                            .context("setting WaitingForResult phase")?;
                        state.reveal_winner().context("revealing winners")?
                    };
                    info!(?winners, ?hands, "announcing winners");
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
                    let receipt = submit_tx_with_retry(&provider, wallet, tx)
                        .await
                        .context("submitting tx")?;
                    let hash = receipt.transaction_hash;
                    if receipt.status() {
                        info!("transaction {hash} succeeded");
                    } else {
                        warn!("transaction {hash} reverted");
                    }
                }
                IPokerTable::GamePhases::__Invalid => {
                    return Ok(());
                }
            }
        }
        IPokerTable::PlayerBet::SIGNATURE_HASH => {}
        IPokerTable::PlayerFolded::SIGNATURE_HASH => {
            let log = IPokerTable::PlayerFolded::decode_log(&log.inner, true)
                .context("decoding log for PlayerFolded")?;
            {
                let mut state = state.write().unwrap();
                let seat = log.indexOnTable.try_into()?;
                state
                    .remove_player(seat)
                    .context("removing player from round because they folded")?;
                info!(player = ?log.address, seat = seat.to_string(), "player folded");
            }
        }
        IPokerTable::PlayerWonWithoutShowdown::SIGNATURE_HASH
        | IPokerTable::ShowdownEnded::SIGNATURE_HASH => {
            info!("game ended, resetting for new round");
            state.write().unwrap().phase = GamePhase::default();
        }
        _ => {
            return Ok(());
        }
    }
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
