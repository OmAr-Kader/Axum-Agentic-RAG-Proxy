use std::collections::HashMap;
use std::sync::Arc;

use tracing::{error, info, warn};

use crate::error::AppError;
use crate::models::schemas::{ChromaAddRequest, RuleChunk};
use crate::rulesets::loader::{load_all_rulesets, load_category_map};
use crate::AppState;

pub async fn run_initial_index(state: Arc<AppState>) {
    state
        .index_manager
        .ingestion_ready
        .store(false, std::sync::atomic::Ordering::Relaxed);

    if let Err(error) = run(state.clone()).await {
        error!(error = %error, "Initial indexing failed");
        state
            .index_manager
            .set_last_error(Some(error.to_string()))
            .await;
        return;
    }

    state.index_manager.set_last_error(None).await;
    state.index_manager.set_ready();
    info!("Initial indexing completed");
}

async fn run(state: Arc<AppState>) -> Result<(), AppError> {
    let category_map = load_category_map(&state.config.ruleset_map_file)?;
    let (chunks_by_category, empty_categories) = load_all_rulesets(
        &category_map,
        &state.config.rulesets_dir,
        state.config.chunk_size,
        state.config.chunk_overlap,
    )?;

    state.hybrid_engine.load_chunks(chunks_by_category.clone()).await;

    {
        let mut chunk_map = state.index_manager.chunk_map.write().await;
        chunk_map.clear();
        for (category, chunks) in &chunks_by_category {
            let grouped = group_source_chunks(category, chunks);
            for (source, ids) in grouped {
                chunk_map.insert(source, ids);
            }
        }
    }

    {
        let mut file_hashes = state.index_manager.file_hashes.write().await;
        file_hashes.clear();
        for (category, chunks) in &chunks_by_category {
            for chunk in chunks {
                file_hashes.insert(
                    format!("{}/{}", category, chunk.source_file),
                    chunk.file_hash.clone(),
                );
            }
        }
    }

    {
        let mut empty = state.index_manager.empty_categories.write().await;
        *empty = empty_categories;
    }

    for category in category_map.keys() {
        let collection_name = state.chroma.collection_name(category);
        if let Err(error) = state.chroma.delete_collection(&collection_name).await {
            warn!(category = %category, error = %error, "Failed to reset collection before reindex");
        }
    }

    for (category, chunks) in &chunks_by_category {
        if chunks.is_empty() {
            continue;
        }

        let collection = state
            .chroma
            .get_or_create_collection(category, &state.config.embedding_model)
            .await?;
        let embeddings = state.hybrid_engine.embedding_service().embed_chunks(chunks).await;
        if embeddings.is_empty() {
            warn!(category = %category, "No embeddings generated for category");
            continue;
        }

        let embedding_map: HashMap<String, Vec<f32>> = embeddings.into_iter().collect();
        let mut ids = Vec::new();
        let mut vectors = Vec::new();
        let mut documents = Vec::new();
        let mut metadatas = Vec::new();

        for chunk in chunks {
            let Some(embedding) = embedding_map.get(&chunk.id) else {
                continue;
            };
            ids.push(chunk.id.clone());
            vectors.push(embedding.clone());
            documents.push(chunk.content.clone());
            metadatas.push(chunk_metadata(chunk));
        }

        if ids.is_empty() {
            continue;
        }

        let request = ChromaAddRequest {
            ids,
            embeddings: vectors,
            documents: Some(documents),
            metadatas: Some(metadatas),
        };
        state.chroma.upsert(&collection.id, &request).await?;
    }

    Ok(())
}

fn group_source_chunks(category: &str, chunks: &[RuleChunk]) -> HashMap<String, Vec<String>> {
    let mut grouped = HashMap::new();
    for chunk in chunks {
        grouped
            .entry(format!("{}/{}", category, chunk.source_file))
            .or_insert_with(Vec::new)
            .push(chunk.id.clone());
    }
    grouped
}

fn chunk_metadata(chunk: &RuleChunk) -> HashMap<String, serde_json::Value> {
    HashMap::from([
        ("category".to_string(), serde_json::Value::String(chunk.category.clone())),
        (
            "source_file".to_string(),
            serde_json::Value::String(chunk.source_file.clone()),
        ),
        (
            "document_id".to_string(),
            serde_json::Value::String(chunk.document_id.clone()),
        ),
        (
            "chunk_index".to_string(),
            serde_json::Value::from(chunk.chunk_index),
        ),
        (
            "total_chunks".to_string(),
            serde_json::Value::from(chunk.total_chunks),
        ),
        (
            "chunk_title".to_string(),
            serde_json::Value::String(chunk.chunk_title.clone()),
        ),
        (
            "priority".to_string(),
            serde_json::Value::from(chunk.frontmatter.priority),
        ),
        (
            "always_include".to_string(),
            serde_json::Value::from(chunk.frontmatter.always_include),
        ),
        (
            "agent_only".to_string(),
            serde_json::Value::from(chunk.frontmatter.agent_only),
        ),
        (
            "applies_to".to_string(),
            serde_json::Value::Array(
                chunk
                    .frontmatter
                    .applies_to
                    .iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        ),
        (
            "tags".to_string(),
            serde_json::Value::Array(
                chunk
                    .frontmatter
                    .tags
                    .iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        ),
    ])
}
