use std::sync::Arc;

use tokio::sync::Semaphore;
use tracing::warn;

use crate::config::Config;
use crate::embedding::cache::EmbeddingCache;
use crate::error::AppError;
use crate::models::schemas::RuleChunk;
use crate::ollama::embed_client::EmbedClient;

/// Embedding service with batching and concurrency control
pub struct EmbeddingService {
    client: Arc<EmbedClient>,
    config: Arc<Config>,
    semaphore: Arc<Semaphore>,
    cache: Arc<EmbeddingCache>,
}

impl EmbeddingService {
    pub fn new(client: Arc<EmbedClient>, config: Arc<Config>) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.embedding_max_concurrency));
        Self {
            client,
            config,
            semaphore,
            cache: Arc::new(EmbeddingCache::new()),
        }
    }

    /// Embed a batch of chunks, returning (chunk_id, embedding) pairs.
    /// Respects EMBEDDING_BATCH_SIZE and EMBEDDING_MAX_CONCURRENCY.
    /// On failure after retries, skips the batch (logs WARNING).
    #[tracing::instrument(skip(self, chunks), fields(chunk_count = chunks.len()))]
    pub async fn embed_chunks(&self, chunks: &[RuleChunk]) -> Vec<(String, Vec<f32>)> {
        let mut results = Vec::new();

        for batch in chunks.chunks(self.config.embedding_batch_size) {
            let texts: Vec<String> = batch.iter().map(|c| c.content.clone()).collect();
            let ids: Vec<String> = batch.iter().map(|c| c.id.clone()).collect();

            let mut uncached_indices = Vec::new();
            let mut uncached_texts = Vec::new();
            for (i, chunk) in batch.iter().enumerate() {
                let key = EmbeddingCache::cache_key(&chunk.file_hash, chunk.chunk_index);
                if let Some(cached) = self.cache.get(&key).await {
                    results.push((ids[i].clone(), cached));
                } else {
                    uncached_indices.push(i);
                    uncached_texts.push(texts[i].clone());
                }
            }

            if uncached_texts.is_empty() {
                continue;
            }

            let _permit = self.semaphore.acquire().await.expect("Semaphore closed");

            let mut attempts = 0;
            let max_retries = self.config.embedding_max_retries;
            let embeddings = loop {
                match self.client.embed(uncached_texts.clone()).await {
                    Ok(embs) => break Some(embs),
                    Err(e) => {
                        attempts += 1;
                        if attempts > max_retries {
                            warn!(
                                error = %e,
                                batch_size = uncached_texts.len(),
                                "Embedding batch failed after retries, skipping"
                            );
                            break None;
                        }
                        warn!(error = %e, attempt = attempts, "Embedding batch failed, retrying");
                    }
                }
            };

            if let Some(embeddings) = embeddings {
                for (idx, embedding) in uncached_indices.iter().zip(embeddings.iter()) {
                    let chunk = &batch[*idx];
                    let key = EmbeddingCache::cache_key(&chunk.file_hash, chunk.chunk_index);
                    self.cache.put(key, embedding.clone()).await;
                    results.push((ids[*idx].clone(), embedding.clone()));
                }
            }
        }

        results
    }

    /// Embed a single query string
    #[tracing::instrument(skip(self, text))]
    pub async fn embed_query(&self, text: &str) -> Result<Vec<f32>, AppError> {
        let _permit = self.semaphore.acquire().await.expect("Semaphore closed");
        let embeddings = self.client.embed(vec![text.to_string()]).await?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Embedding("No embedding returned for query".into()))
    }

    /// Clear the embedding cache
    pub async fn clear_cache(&self) {
        self.cache.clear().await;
    }
}
