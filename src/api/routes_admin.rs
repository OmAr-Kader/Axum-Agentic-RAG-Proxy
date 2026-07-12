use axum::{
    extract::{Query, State},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::info;

use crate::error::AppError;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ConfirmQuery {
    #[serde(default)]
    pub confirm: bool,
}

#[tracing::instrument(skip(state))]
pub async fn reload_handler(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    info!("Manual reload triggered");
    state
        .index_manager
        .ingestion_ready
        .store(false, std::sync::atomic::Ordering::Relaxed);

    let state_clone = state.clone();
    tokio::spawn(async move {
        crate::jobs::initial_index::run_initial_index(state_clone).await;
    });

    Ok(Json(serde_json::json!({"status": "reload_started"})))
}

#[tracing::instrument(skip(state))]
pub async fn reset_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ConfirmQuery>,
) -> Result<impl IntoResponse, AppError> {
    if !query.confirm {
        return Err(AppError::Validation(
            "Must pass ?confirm=true to reset".into(),
        ));
    }

    info!("Full reset triggered");
    state
        .index_manager
        .ingestion_ready
        .store(false, std::sync::atomic::Ordering::Relaxed);

    {
        let mut chunk_map = state.index_manager.chunk_map.write().await;
        chunk_map.clear();
    }
    {
        let mut hashes = state.index_manager.file_hashes.write().await;
        hashes.clear();
    }

    let state_clone = state.clone();
    tokio::spawn(async move {
        crate::jobs::initial_index::run_initial_index(state_clone).await;
    });

    Ok(Json(serde_json::json!({"status": "reset_started"})))
}

#[tracing::instrument(skip(state))]
pub async fn index_status_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let ready = state.index_manager.is_ready();
    let last_error = state.index_manager.last_error.read().await.clone();
    let empty_cats = state.index_manager.empty_categories.read().await.clone();

    Json(serde_json::json!({
        "ingestion_ready": ready,
        "last_error": last_error,
        "empty_categories": empty_cats,
    }))
}
