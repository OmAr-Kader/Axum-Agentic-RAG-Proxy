use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::info;

use crate::rulesets::watcher::WatchEvent;
use crate::AppState;

pub async fn process_reindex_queue(state: Arc<AppState>, mut rx: mpsc::Receiver<WatchEvent>) {
    while let Some(event) = rx.recv().await {
        info!(event = ?event, "Processing watch event");
        while rx.try_recv().is_ok() {}
        crate::jobs::initial_index::run_initial_index(state.clone()).await;
    }
}
