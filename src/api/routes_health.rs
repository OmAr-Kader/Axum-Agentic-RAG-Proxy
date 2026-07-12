use axum::{extract::State, Json};
use std::sync::Arc;

use crate::models::schemas::HealthResponse;
use crate::AppState;

#[tracing::instrument(skip(state))]
pub async fn health_handler(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let ollama_reachable = state
        .model_mgmt_client
        .ping(state.config.health_check_timeout)
        .await;
    let chroma_reachable = state.chroma.ping().await;
    let ingestion_ready = state.index_manager.is_ready();
    let active_categories = state.hybrid_engine.categories().await;

    Json(HealthResponse {
        status: "ok".to_string(),
        ollama_reachable,
        chroma_reachable,
        ingestion_ready,
        active_categories,
    })
}
