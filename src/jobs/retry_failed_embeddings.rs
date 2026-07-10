use std::sync::Arc;

use tokio::time::{sleep, Duration};
use tracing::debug;

use crate::AppState;

pub async fn retry_failed_embeddings_loop(state: Arc<AppState>) {
    let interval = Duration::from_secs(state.config.failed_embedding_retry_interval_seconds.max(1));
    loop {
        sleep(interval).await;
        debug!("retry_failed_embeddings_loop tick");
    }
}
