use std::time::Duration;

use crate::config::Config;
use crate::error::AppError;
use crate::models::schemas::{
    ChromaAddRequest, ChromaCollection, ChromaQueryRequest, ChromaQueryResponse,
};

/// ChromaDB HTTP client
pub struct ChromaClient {
    client: reqwest::Client,
    base_url: String,
    collection_prefix: String,
    timeout: Option<Duration>,
}

impl ChromaClient {
    pub fn new(config: &Config) -> Self {
        let client = reqwest::Client::new();
        Self {
            client,
            base_url: config.chroma_url.clone(),
            collection_prefix: config.chroma_collection_prefix.clone(),
            timeout: config.chroma_request_timeout,
        }
    }

    /// Get collection name for a category
    pub fn collection_name(&self, category: &str) -> String {
        format!("{}{}", self.collection_prefix, category.to_lowercase())
    }

    /// Check if Chroma is reachable
    pub async fn ping(&self) -> bool {
        let url = format!("{}/api/v1/heartbeat", self.base_url);
        let req = self.client.get(&url);
        let req = if let Some(t) = self.timeout {
            req.timeout(t)
        } else {
            req
        };
        req.send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// Get or create a collection
    #[tracing::instrument(skip(self))]
    pub async fn get_or_create_collection(
        &self,
        category: &str,
        embedding_model: &str,
    ) -> Result<ChromaCollection, AppError> {
        let name = self.collection_name(category);
        let url = format!("{}/api/v1/collections", self.base_url);
        let body = serde_json::json!({
            "name": name,
            "metadata": {"embedding_model": embedding_model},
            "get_or_create": true
        });
        let req = self.client.post(&url).json(&body);
        let req = if let Some(t) = self.timeout {
            req.timeout(t)
        } else {
            req
        };
        let resp = req
            .send()
            .await
            .map_err(|e| AppError::Chroma(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Chroma(format!(
                "Create collection failed ({}): {}",
                status, text
            )));
        }
        let col: ChromaCollection = resp
            .json()
            .await
            .map_err(|e| AppError::Chroma(e.to_string()))?;

        if let Some(meta) = &col.metadata {
            if let Some(model_val) = meta.get("embedding_model") {
                if let Some(model_str) = model_val.as_str() {
                    if model_str != embedding_model {
                        return Err(AppError::Conflict(format!(
                            "Collection '{}' uses model '{}' but current model is '{}'",
                            name, model_str, embedding_model
                        )));
                    }
                }
            }
        }

        Ok(col)
    }

    /// Upsert documents into a collection
    #[tracing::instrument(skip(self, request), fields(collection = %collection_id, count = request.ids.len()))]
    pub async fn upsert(
        &self,
        collection_id: &str,
        request: &ChromaAddRequest,
    ) -> Result<(), AppError> {
        let url = format!(
            "{}/api/v1/collections/{}/upsert",
            self.base_url, collection_id
        );
        let req = self.client.post(&url).json(request);
        let req = if let Some(t) = self.timeout {
            req.timeout(t)
        } else {
            req
        };
        let resp = req
            .send()
            .await
            .map_err(|e| AppError::Chroma(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Chroma(format!(
                "Upsert failed ({}): {}",
                status, text
            )));
        }
        Ok(())
    }

    /// Query a collection for similar embeddings
    #[tracing::instrument(skip(self, request), fields(collection = %collection_id))]
    pub async fn query(
        &self,
        collection_id: &str,
        request: &ChromaQueryRequest,
    ) -> Result<ChromaQueryResponse, AppError> {
        let url = format!(
            "{}/api/v1/collections/{}/query",
            self.base_url, collection_id
        );
        let req = self.client.post(&url).json(request);
        let req = if let Some(t) = self.timeout {
            req.timeout(t)
        } else {
            req
        };
        let resp = req
            .send()
            .await
            .map_err(|e| AppError::Chroma(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Chroma(format!(
                "Query failed ({}): {}",
                status, text
            )));
        }
        resp.json()
            .await
            .map_err(|e| AppError::Chroma(e.to_string()))
    }

    /// Delete documents by IDs from a collection
    #[tracing::instrument(skip(self, ids), fields(collection = %collection_id, count = ids.len()))]
    pub async fn delete_by_ids(&self, collection_id: &str, ids: &[String]) -> Result<(), AppError> {
        let url = format!(
            "{}/api/v1/collections/{}/delete",
            self.base_url, collection_id
        );
        let body = serde_json::json!({ "ids": ids });
        let req = self.client.post(&url).json(&body);
        let req = if let Some(t) = self.timeout {
            req.timeout(t)
        } else {
            req
        };
        let resp = req
            .send()
            .await
            .map_err(|e| AppError::Chroma(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Chroma(format!(
                "Delete failed ({}): {}",
                status, text
            )));
        }
        Ok(())
    }

    /// Delete an entire collection
    #[tracing::instrument(skip(self))]
    pub async fn delete_collection(&self, collection_name: &str) -> Result<(), AppError> {
        let url = format!("{}/api/v1/collections/{}", self.base_url, collection_name);
        let req = self.client.delete(&url);
        let req = if let Some(t) = self.timeout {
            req.timeout(t)
        } else {
            req
        };
        let resp = req
            .send()
            .await
            .map_err(|e| AppError::Chroma(e.to_string()))?;
        if !resp.status().is_success() && resp.status().as_u16() != 404 {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Chroma(format!(
                "Delete collection failed ({}): {}",
                status, text
            )));
        }
        Ok(())
    }
}
