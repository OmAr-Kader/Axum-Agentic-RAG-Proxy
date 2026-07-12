use std::collections::HashMap;

use tokio::sync::RwLock;

/// Cache embeddings keyed by file_hash + chunk_index
#[derive(Debug, Default)]
pub struct EmbeddingCache {
    cache: RwLock<HashMap<String, Vec<f32>>>,
}

impl EmbeddingCache {
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Get cached embedding
    pub async fn get(&self, key: &str) -> Option<Vec<f32>> {
        let cache = self.cache.read().await;
        cache.get(key).cloned()
    }

    /// Store embedding in cache
    pub async fn put(&self, key: String, embedding: Vec<f32>) {
        let mut cache = self.cache.write().await;
        cache.insert(key, embedding);
    }

    /// Generate cache key from file hash and chunk index
    pub fn cache_key(file_hash: &str, chunk_index: usize) -> String {
        format!("{file_hash}::{chunk_index}")
    }

    /// Clear all cached embeddings
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}
