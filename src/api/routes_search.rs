use axum::{extract::State, Json};
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::AppError;
use crate::models::schemas::{ChatMessage, SearchRequest, SearchResultItem};
use crate::AppState;

#[tracing::instrument(skip(state, body))]
pub async fn search_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SearchRequest>,
) -> Result<Json<Vec<SearchResultItem>>, AppError> {
    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: body.query.clone(),
        extra: HashMap::new(),
    }];

    let results: Vec<SearchResultItem> = state
        .hybrid_engine
        .retrieve(&messages)
        .await
        .into_iter()
        .filter(|chunk| {
            body.category
                .as_ref()
                .map(|category| category.eq_ignore_ascii_case(&chunk.chunk.category))
                .unwrap_or(true)
        })
        .take(body.top_k.unwrap_or(state.config.top_k))
        .map(|chunk| SearchResultItem {
            id: chunk.chunk.id,
            content: chunk.chunk.content,
            score: chunk.final_score,
            category: chunk.chunk.category,
            metadata: HashMap::from([
                ("chunk_title".to_string(), serde_json::Value::String(chunk.chunk.chunk_title)),
                (
                    "source_file".to_string(),
                    serde_json::Value::String(chunk.chunk.source_file),
                ),
                (
                    "similarity_score".to_string(),
                    serde_json::Value::from(chunk.similarity_score),
                ),
            ]),
        })
        .collect();

    Ok(Json(results))
}
