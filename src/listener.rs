use std::sync::{Arc, RwLock};

use alloy::{
    eips::BlockNumberOrTag, providers::ProviderBuilder, rpc::types::Filter,
    sol_types::SolEvent as _,
};
use anyhow::Result;

use crate::{bindings::*, state::AppState};

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
        .events([
            IPokerTable::PhaseChanged::SIGNATURE,
            IPokerTable::PlayerJoined::SIGNATURE,
            IPokerTable::PlayerLeft::SIGNATURE,
        ])
        .from_block(BlockNumberOrTag::Latest);

    Ok(())
}
