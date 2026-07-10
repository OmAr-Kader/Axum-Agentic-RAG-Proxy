use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::config::Config;
use crate::index::keyword_index::KeywordIndex;
use crate::models::schemas::RuleChunk;

/// State for the index manager
#[derive(Debug)]
pub struct IndexManager {
    pub config: Arc<Config>,
    pub ingestion_ready: Arc<std::sync::atomic::AtomicBool>,
    pub chunk_map: Arc<RwLock<HashMap<String, Vec<String>>>>, // source_file -> [chunk_ids]
    pub keyword_index: Arc<RwLock<KeywordIndex>>,
    pub file_hashes: Arc<RwLock<HashMap<String, String>>>, // source_file -> hash
    pub empty_categories: Arc<RwLock<Vec<String>>>,
    pub last_error: Arc<RwLock<Option<String>>>,
}

impl IndexManager {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            ingestion_ready: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            chunk_map: Arc::new(RwLock::new(HashMap::new())),
            keyword_index: Arc::new(RwLock::new(KeywordIndex::new())),
            file_hashes: Arc::new(RwLock::new(HashMap::new())),
            empty_categories: Arc::new(RwLock::new(Vec::new())),
            last_error: Arc::new(RwLock::new(None)),
        }
    }

    /// Check if ingestion is complete
    pub fn is_ready(&self) -> bool {
        self.ingestion_ready
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Mark ingestion as complete
    pub fn set_ready(&self) {
        self.ingestion_ready
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Update the chunk map for a source file
    pub async fn update_chunk_map(&self, source_file: &str, chunk_ids: Vec<String>) {
        let mut map = self.chunk_map.write().await;
        map.insert(source_file.to_string(), chunk_ids);
    }

    /// Remove a source file from the chunk map, return its chunk IDs
    pub async fn remove_from_chunk_map(&self, source_file: &str) -> Vec<String> {
        let mut map = self.chunk_map.write().await;
        map.remove(source_file).unwrap_or_default()
    }

    /// Update keyword index from chunks
    pub async fn rebuild_keyword_index(&self, all_chunks: &[RuleChunk]) {
        let mut kw_index = self.keyword_index.write().await;
        kw_index.build_from_chunks(all_chunks);
    }

    /// Check if a file hash has changed
    pub async fn has_file_changed(&self, source_file: &str, new_hash: &str) -> bool {
        let hashes = self.file_hashes.read().await;
        match hashes.get(source_file) {
            Some(existing) => existing != new_hash,
            None => true,
        }
    }

    /// Update stored file hash
    pub async fn update_file_hash(&self, source_file: &str, hash: &str) {
        let mut hashes = self.file_hashes.write().await;
        hashes.insert(source_file.to_string(), hash.to_string());
    }

    /// Set last error
    pub async fn set_last_error(&self, error: Option<String>) {
        let mut last = self.last_error.write().await;
        *last = error;
    }
}
