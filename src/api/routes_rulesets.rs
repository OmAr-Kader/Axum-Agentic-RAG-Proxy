use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

use crate::error::AppError;
use crate::models::schemas::RulesetWriteRequest;
use crate::security::validation::{validate_category, validate_content_size, validate_filename};
use crate::AppState;

#[tracing::instrument(skip(state))]
pub async fn list_rulesets(
    State(state): State<Arc<AppState>>,
) -> Result<Json<HashMap<String, usize>>, AppError> {
    let all_chunks = state.hybrid_engine.categories().await;
    let chunk_store = state.index_manager.chunk_map.read().await;
    let mut counts: HashMap<String, usize> = all_chunks.into_iter().map(|category| (category, 0)).collect();
    for ids in chunk_store.values() {
        if let Some(category) = ids.first().and_then(|id| id.split("::").next()) {
            *counts.entry(category.to_string()).or_insert(0) += ids.len();
        }
    }
    Ok(Json(counts))
}

#[tracing::instrument(skip(state, body))]
pub async fn write_ruleset(
    State(state): State<Arc<AppState>>,
    Path((category, filename)): Path<(String, String)>,
    Json(body): Json<RulesetWriteRequest>,
) -> Result<impl IntoResponse, AppError> {
    validate_category(&category)?;
    validate_filename(&filename)?;
    validate_content_size(body.content.as_bytes(), state.config.max_rule_content_bytes)?;

    let dir = std::path::Path::new(&state.config.rulesets_dir);
    let category_dir = dir.join(&category);
    std::fs::create_dir_all(&category_dir)
        .map_err(|error| AppError::Internal(format!("Failed to create directory: {error}")))?;
    let file_path = category_dir.join(&filename);
    std::fs::write(&file_path, &body.content)
        .map_err(|error| AppError::Internal(format!("Failed to write file: {error}")))?;

    info!(category = %category, filename = %filename, "Ruleset file written");
    tokio::spawn(crate::jobs::initial_index::run_initial_index(state.clone()));

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"status": "created", "file": filename})),
    ))
}

#[derive(Debug, Deserialize)]
pub struct DeleteQuery {
    #[serde(default)]
    pub delete_from_disk: bool,
    #[serde(default)]
    pub confirm: bool,
}

#[tracing::instrument(skip(state))]
pub async fn delete_ruleset_file(
    State(state): State<Arc<AppState>>,
    Path((category, filename)): Path<(String, String)>,
    Query(query): Query<DeleteQuery>,
) -> Result<impl IntoResponse, AppError> {
    validate_category(&category)?;
    validate_filename(&filename)?;

    let source_key = format!("{category}/{filename}");
    let chunk_ids = state.index_manager.remove_from_chunk_map(&source_key).await;
    state.index_manager.file_hashes.write().await.remove(&source_key);

    if !chunk_ids.is_empty() {
        let collection = state
            .chroma
            .get_or_create_collection(&category, &state.config.embedding_model)
            .await?;
        state.chroma.delete_by_ids(&collection.id, &chunk_ids).await?;
    }

    if query.delete_from_disk {
        let file_path = std::path::Path::new(&state.config.rulesets_dir)
            .join(&category)
            .join(&filename);
        if file_path.exists() {
            std::fs::remove_file(&file_path)
                .map_err(|error| AppError::Internal(format!("Failed to delete file: {error}")))?;
        }
        tokio::spawn(crate::jobs::initial_index::run_initial_index(state.clone()));
    }

    info!(category = %category, filename = %filename, chunks_removed = chunk_ids.len(), "Removed ruleset file");
    Ok(Json(
        serde_json::json!({"status": "deleted", "chunks_removed": chunk_ids.len()}),
    ))
}

#[tracing::instrument(skip(state))]
pub async fn delete_category(
    State(state): State<Arc<AppState>>,
    Path(category): Path<String>,
    Query(query): Query<DeleteQuery>,
) -> Result<impl IntoResponse, AppError> {
    validate_category(&category)?;

    if !query.confirm {
        return Err(AppError::Validation(
            "Must pass ?confirm=true to delete a category".into(),
        ));
    }

    let collection_name = state.chroma.collection_name(&category);
    state.chroma.delete_collection(&collection_name).await?;

    if query.delete_from_disk {
        let category_path = std::path::Path::new(&state.config.rulesets_dir).join(&category);
        if category_path.exists() {
            std::fs::remove_dir_all(&category_path)
                .map_err(|error| AppError::Internal(format!("Failed to delete category: {error}")))?;
        }
        tokio::spawn(crate::jobs::initial_index::run_initial_index(state.clone()));
    }

    info!(category = %category, "Category collection deleted");
    Ok(Json(serde_json::json!({"status": "deleted", "category": category})))
}
