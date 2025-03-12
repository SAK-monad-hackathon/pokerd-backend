use std::sync::{Arc, RwLock};

use anyhow::Result;

use crate::state::AppState;

pub async fn listen(state: Arc<RwLock<AppState>>) -> Result<()> {
    Ok(())
}
