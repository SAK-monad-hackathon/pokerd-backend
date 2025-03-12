use std::sync::{Arc, RwLock};

use alloy::{
    eips::BlockNumberOrTag,
    primitives::B256,
    providers::{Provider, ProviderBuilder},
    rpc::types::Filter,
    sol_types::SolEvent as _,
};
use anyhow::Result;
use futures_util::{StreamExt as _, stream};
use tracing::warn;

use crate::state::TablePlayer;
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

const ALL_TOPICS: [B256; 7] = [
    IPokerTable::PlayerJoined::SIGNATURE_HASH,
    IPokerTable::PlayerLeft::SIGNATURE_HASH,
    IPokerTable::PhaseChanged::SIGNATURE_HASH,
    IPokerTable::PlayerBet::SIGNATURE_HASH,
    IPokerTable::PlayerFolded::SIGNATURE_HASH,
    IPokerTable::PlayerWonWithoutShowdown::SIGNATURE_HASH,
    IPokerTable::ShowdownEnded::SIGNATURE_HASH,
];

pub async fn listen(state: Arc<RwLock<AppState>>) -> Result<()> {
    let (rpc_url, signer, table_address) = {
        let state = state.read().unwrap();
        (
            state.rpc_url.clone(),
            state.signer.clone(),
            state.table_address,
        )
    };
    let provider = ProviderBuilder::new()
        .wallet(signer)
        .on_http(rpc_url.parse()?);

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
                {
                    state.write().unwrap().table_players.push(TablePlayer {
                        address: log.address,
                        seat: log.indexOnTable.try_into()?,
                    });
                }
            }
            IPokerTable::PlayerLeft::SIGNATURE_HASH => {
                let log = IPokerTable::PlayerLeft::decode_log(&log.inner, true)?;
            }
            IPokerTable::PhaseChanged::SIGNATURE_HASH => {}
            IPokerTable::PlayerBet::SIGNATURE_HASH => {}
            IPokerTable::PlayerFolded::SIGNATURE_HASH => {}
            IPokerTable::PlayerWonWithoutShowdown::SIGNATURE_HASH => {}
            IPokerTable::ShowdownEnded::SIGNATURE_HASH => {}
            _ => {
                continue;
            }
        }
    }
    warn!("stream finished");

    Ok(())
}
