use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::RwLock;
use tracing::warn;

use crate::config::Config;
use crate::embedding::service::EmbeddingService;
use crate::error::AppError;
use crate::index::index_manager::IndexManager;
use crate::models::schemas::{ChatMessage, ChromaQueryRequest, RetrievedChunk, RuleChunk};
use crate::query::analyzer::analyze_query;
use crate::retrieval::ranker::rank_chunks;
use crate::rulesets::chunker::estimate_tokens;
use crate::vectorstore::chroma_client::ChromaClient;

pub struct HybridEngine {
    config: Arc<Config>,
    index_manager: Arc<IndexManager>,
    embedding_service: Arc<EmbeddingService>,
    chroma: Arc<ChromaClient>,
    category_locks: DashMap<String, Arc<RwLock<()>>>,
    all_chunks: Arc<RwLock<HashMap<String, Vec<RuleChunk>>>>,
}

impl HybridEngine {
    pub fn new(
        config: Arc<Config>,
        index_manager: Arc<IndexManager>,
        embedding_service: Arc<EmbeddingService>,
        chroma: Arc<ChromaClient>,
    ) -> Self {
        Self {
            config,
            index_manager,
            embedding_service,
            chroma,
            category_locks: DashMap::new(),
            all_chunks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn load_chunks(&self, all_chunks: HashMap<String, Vec<RuleChunk>>) {
        let flattened: Vec<RuleChunk> = all_chunks.values().flatten().cloned().collect();
        for category in all_chunks.keys() {
            self.category_locks
                .entry(category.clone())
                .or_insert_with(|| Arc::new(RwLock::new(())));
        }

        let mut guard = self.all_chunks.write().await;
        *guard = all_chunks;
        drop(guard);

        self.index_manager.rebuild_keyword_index(&flattened).await;
    }

    pub async fn categories(&self) -> Vec<String> {
        let mut categories: Vec<String> = self.all_chunks.read().await.keys().cloned().collect();
        categories.sort();
        categories
    }


    pub async fn retrieve_old(&self, messages: &[ChatMessage]) -> Vec<RetrievedChunk> {
        let all_chunks = self.all_chunks.read().await.clone();
        if all_chunks.is_empty() {
            return Vec::new();
        }

        let keyword_index = self.index_manager.keyword_index.read().await;
        let analysis = analyze_query(messages, &keyword_index);
        drop(keyword_index);
        tracing::info!(?analysis, "Query analysis");

        let user_text = messages
            .iter()
            .filter(|message| message.role.eq_ignore_ascii_case("user"))
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        tracing::info!(%user_text, "User text extracted from messages");

        let mut candidates: Vec<(RuleChunk, f32)> = all_chunks
            .values()
            .flat_map(|chunks| {
                chunks
                    .iter()
                    .filter(|chunk| chunk.frontmatter.always_include)
                    .cloned()
                    .map(|chunk| (chunk, 1.0))
                    .collect::<Vec<_>>()
            })
            .collect();

        if !user_text.trim().is_empty() {
            match self.embedding_service.embed_query(&user_text).await {
                Ok(query_embedding) => {
                    for (category, chunks) in &all_chunks {
                        if chunks.is_empty() {
                            continue;
                        }

                        let lock = self
                            .category_locks
                            .entry(category.clone())
                            .or_insert_with(|| Arc::new(RwLock::new(())))
                            .clone();
                        let Ok(_guard) = tokio::time::timeout(
                            Duration::from_millis(self.config.category_lock_read_timeout_ms),
                            lock.read(),
                        )
                        .await
                        else {
                            warn!(category = %category, "Timed out waiting for category lock");
                            continue;
                        };

                        let collection = match self
                            .chroma
                            .get_or_create_collection(category, &self.config.embedding_model)
                            .await
                        {
                            Ok(collection) => collection,
                            Err(error) => {
                                warn!(category = %category, error = %error, "Failed to open Chroma collection");
                                continue;
                            }
                        };

                        let request = ChromaQueryRequest {
                            query_embeddings: vec![query_embedding.clone()],
                            n_results: self.config.category_select_top_n.max(self.config.top_k),
                            where_filter: None,
                            include: Some(vec!["distances".to_string()]),
                        };

                        let response = match self.chroma.query(&collection.id, &request).await {
                            Ok(response) => response,
                            Err(error) => {
                                warn!(category = %category, error = %error, "Chroma query failed");
                                continue;
                            }
                        };

                        let ids = response.ids.into_iter().next().unwrap_or_default();
                        let distances = response.distances.unwrap_or_default().into_iter().next().unwrap_or_default();

                        for (index, chunk_id) in ids.iter().enumerate() {
                            if let Some(chunk) = chunks.iter().find(|chunk| &chunk.id == chunk_id) {
                                let distance = *distances.get(index).unwrap_or(&0.0);
                                let similarity = 1.0 / (1.0 + distance.max(0.0));
                                if similarity >= self.config.similarity_threshold {
                                    candidates.push((chunk.clone(), similarity));
                                }
                            }
                        }
                    }
                }
                Err(error) => warn!(error = %error, "Query embedding failed; returning always-include context only"),
            }
        }

        tracing::info!(candidates = candidates.len(), "Total candidate chunks before ranking");
        let ranked = rank_chunks(candidates, &analysis);
        self.apply_budget(ranked)
    }

    pub fn embedding_service(&self) -> &Arc<EmbeddingService> {
        &self.embedding_service
    }

    fn apply_budget(&self, ranked: Vec<RetrievedChunk>) -> Vec<RetrievedChunk> {
        let token_budget = self
            .config
            .max_injected_context_tokens
            .saturating_sub(self.config.context_reserved_tokens);
        if token_budget == 0 {
            return Vec::new();
        }

        let global_token_cap = self.config.global_always_on_retrieved_cap.min(token_budget);
        let always_total_token_cap = token_budget
            * self.config.always_include_all_categories_cap_pct.min(100)
            / 100;
        let per_category_token_cap = token_budget
            * self.config.always_include_single_category_cap_pct.min(100)
            / 100;

        let mut chosen = Vec::new();
        let mut seen = HashSet::new();
        let mut used_tokens = 0usize;
        let mut global_tokens = 0usize;
        let mut always_tokens = 0usize;
        let mut per_category_tokens: HashMap<String, usize> = HashMap::new();

        let mut global_always = Vec::new();
        let mut category_always = Vec::new();
        let mut retrieved = Vec::new();

        for chunk in ranked {
            if chunk.chunk.frontmatter.always_include && chunk.chunk.category.eq_ignore_ascii_case("global") {
                global_always.push(chunk);
            } else if chunk.chunk.frontmatter.always_include {
                category_always.push(chunk);
            } else {
                retrieved.push(chunk);
            }
        }

        global_always.sort_by(|left, right| right.chunk.frontmatter.priority.cmp(&left.chunk.frontmatter.priority));
        category_always.sort_by(|left, right| right.chunk.frontmatter.priority.cmp(&left.chunk.frontmatter.priority));

        for chunk in global_always {
            let tokens = estimate_tokens(&chunk.chunk.content);
            if seen.insert(chunk.chunk.id.clone())
                && used_tokens + tokens <= token_budget
                && global_tokens + tokens <= global_token_cap
            {
                global_tokens += tokens;
                used_tokens += tokens;
                chosen.push(chunk);
            }
        }

        for chunk in category_always {
            let tokens = estimate_tokens(&chunk.chunk.content);
            let category_used = per_category_tokens
                .entry(chunk.chunk.category.clone())
                .or_insert(0);
            if seen.insert(chunk.chunk.id.clone())
                && used_tokens + tokens <= token_budget
                && always_tokens + tokens <= always_total_token_cap.max(tokens)
                && *category_used + tokens <= per_category_token_cap.max(tokens)
            {
                *category_used += tokens;
                always_tokens += tokens;
                used_tokens += tokens;
                chosen.push(chunk);
            }
        }

        for chunk in retrieved {
            let tokens = estimate_tokens(&chunk.chunk.content);
            if seen.insert(chunk.chunk.id.clone()) && used_tokens + tokens <= token_budget {
                used_tokens += tokens;
                chosen.push(chunk);
            }
        }

        tracing::info!(chosen = chosen.len(), "Total chunks selected after applying budget");
        chosen
    }


    //////////////////////////////////////////////////////////////////////////////////////////
    
    pub async fn retrieve(&self, messages: &[ChatMessage]) -> Result<Vec<RetrievedChunk>, AppError> {
        let all_chunks: HashMap<String, Vec<RuleChunk>> = self.all_chunks.read().await.clone();
        if all_chunks.is_empty() {
            return Err(AppError::NotFound("No chunks loaded".into()));
        }

        for msg in messages {
            tracing::info!(role = %msg.role, content = %msg.content, "Processing message");
        }
        // Extract identifiers from messages
        let identifiers = self.extract_file_ids_v2(messages)?;
        tracing::info!(?identifiers, "Identifiers extracted from messages");

        if identifiers.is_empty() {
            tracing::warn!("No identifiers found in messages; expected format: #proxy_ollama:file_id1,file_id2 or #proxy_ollama:CATEGORY_NAME");
            /*return Err(AppError::Validation(
                "No identifiers found; expected format: #proxy_ollama:file_id1,file_id2 or #proxy_ollama:CATEGORY_NAME".into(),
            ));*/
        }

        // Parse identifiers and determine if they're categories or file IDs
        let mut candidates: Vec<(RuleChunk, f32)> = Vec::new();
        let mut not_found = Vec::new();

        //for identifier in &identifiers {
            //let parsed = self.parse_file_id_v2(identifier)?;
            for id_or_category in identifiers.clone() {
                tracing::info!(?id_or_category, "Processing identifier from message");
                // First, check if it's a category name
                if let Some(chunks) = all_chunks.get(&id_or_category) {
                    tracing::info!(category = %id_or_category, "Found as category name");
                    for chunk in chunks {
                        candidates.push((chunk.clone(), 1.0));
                    }
                } else {
                    // Otherwise, search for it as a file ID in frontmatter across all chunks
                    let mut found = false;
                    tracing::info!(all_chunks = all_chunks.len(), "Searching for file ID in all chunks");
                    for (category, chunks) in &all_chunks {
                        tracing::info!(category = %category, chunks = chunks.len(), "Searching in category");
                        for chunk in chunks {
                            if chunk.frontmatter.id == id_or_category {
                                tracing::info!(file_id = %id_or_category, category = %category, "Found as file ID");
                                candidates.push((chunk.clone(), 1.0));
                                found = true;
                            }
                        }
                    }
                    if !found {
                        tracing::warn!(id_or_category = %id_or_category, "Not found as category or file ID");
                        not_found.push(id_or_category);
                    }
                }
            }
        //}

        if candidates.is_empty() {
            return Err(AppError::NotFound(format!(
                "No chunks found for identifiers: {:?}",
                not_found
            )));
        }

        if !not_found.is_empty() {
            tracing::warn!(?not_found, "Some identifiers were not found, but returning matching ones");
        }

        tracing::info!(candidates = candidates.len(), "Total candidate chunks before ranking");
        
        // Use default query analysis since we're not embedding
        let ranked = rank_chunks(candidates, &Default::default());
        Ok(self.apply_budget_v2(ranked))
    }

    /// Extract identifiers from messages that start with #proxy_ollama:
    /// Identifiers can be file IDs or category names.
    fn extract_file_ids_v2(&self, messages: &[ChatMessage]) -> Result<Vec<String>, AppError> {
        const PREFIX: &str = "#proxy_ollama:";
        let mut file_ids = Vec::new();

        for message in messages {
            if message.role.eq_ignore_ascii_case("user") {
                for word in message.content.split_whitespace() {
                    if let Some(rest) = word.strip_prefix(PREFIX) {
                        // rest == "kotlin_swiftui_to_compose_conversion,global_rules"
                        for id in rest.split(',') {
                            let trimmed = id.trim();
                            if !trimmed.is_empty() {
                                file_ids.push(trimmed.to_string());
                            }
                        }
                    }
                }
            }
        }
        tracing::info!(file_ids = ?file_ids, "Extracted file IDs from messages");
        Ok(file_ids)
    }

    /// Parse an identifier to extract file IDs or category names.
    /// Expected format: #proxy_ollama:id1,id2 or #proxy_ollama:CATEGORY1,CATEGORY2
    fn parse_file_id_v2(&self, identifier: &str) -> Result<Vec<String>, AppError> {
        if !identifier.starts_with("#proxy_ollama:") {
            return Err(AppError::Validation(format!(
                "Invalid identifier format: {}; expected #proxy_ollama:id1,id2",
                identifier
            )));
        }

        let ids_str = &identifier["#proxy_ollama:".len()..];
        if ids_str.is_empty() {
            return Err(AppError::Validation(
                "Identifier has no IDs/categories after #proxy_ollama:".into(),
            ));
        }

        let ids: Vec<String> = ids_str
            .split(',')
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
            .collect();

        if ids.is_empty() {
            return Err(AppError::Validation(
                "No valid identifiers found in #proxy_ollama: format".into(),
            ));
        }

        Ok(ids)
    }

    /// Clone of apply_budget for v2 (same logic)
    fn apply_budget_v2(&self, ranked: Vec<RetrievedChunk>) -> Vec<RetrievedChunk> {
        let token_budget = self
            .config
            .max_injected_context_tokens
            .saturating_sub(self.config.context_reserved_tokens);
        if token_budget == 0 {
            return Vec::new();
        }

        let global_token_cap = self.config.global_always_on_retrieved_cap.min(token_budget);
        let always_total_token_cap = token_budget
            * self.config.always_include_all_categories_cap_pct.min(100)
            / 100;
        let per_category_token_cap = token_budget
            * self.config.always_include_single_category_cap_pct.min(100)
            / 100;

        let mut chosen = Vec::new();
        let mut seen = HashSet::new();
        let mut used_tokens = 0usize;
        let mut global_tokens = 0usize;
        let mut always_tokens = 0usize;
        let mut per_category_tokens: HashMap<String, usize> = HashMap::new();

        let mut global_always = Vec::new();
        let mut category_always = Vec::new();
        let mut retrieved = Vec::new();

        for chunk in ranked {
            if chunk.chunk.frontmatter.always_include && chunk.chunk.category.eq_ignore_ascii_case("global") {
                global_always.push(chunk);
            } else if chunk.chunk.frontmatter.always_include {
                category_always.push(chunk);
            } else {
                retrieved.push(chunk);
            }
        }

        global_always.sort_by(|left, right| right.chunk.frontmatter.priority.cmp(&left.chunk.frontmatter.priority));
        category_always.sort_by(|left, right| right.chunk.frontmatter.priority.cmp(&left.chunk.frontmatter.priority));

        for chunk in global_always {
            let tokens = estimate_tokens(&chunk.chunk.content);
            if seen.insert(chunk.chunk.id.clone())
                && used_tokens + tokens <= token_budget
                && global_tokens + tokens <= global_token_cap
            {
                global_tokens += tokens;
                used_tokens += tokens;
                chosen.push(chunk);
            }
        }

        for chunk in category_always {
            let tokens = estimate_tokens(&chunk.chunk.content);
            let category_used = per_category_tokens
                .entry(chunk.chunk.category.clone())
                .or_insert(0);
            if seen.insert(chunk.chunk.id.clone())
                && used_tokens + tokens <= token_budget
                && always_tokens + tokens <= always_total_token_cap.max(tokens)
                && *category_used + tokens <= per_category_token_cap.max(tokens)
            {
                *category_used += tokens;
                always_tokens += tokens;
                used_tokens += tokens;
                chosen.push(chunk);
            }
        }

        for chunk in retrieved {
            let tokens = estimate_tokens(&chunk.chunk.content);
            if seen.insert(chunk.chunk.id.clone()) && used_tokens + tokens <= token_budget {
                used_tokens += tokens;
                chosen.push(chunk);
            }
        }
        tracing::info!(chosen = chosen.len(), "Total chunks selected after applying budget");
        chosen
    }
}
