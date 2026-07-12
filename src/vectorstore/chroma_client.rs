use std::collections::HashMap;

use chroma::client::{ChromaAuthMethod, ChromaHttpClient, ChromaHttpClientOptions};
use chroma::types::{IncludeList, Metadata, MetadataValue, QueryResponse, UpdateMetadata};
use crate::config::Config;
use crate::error::AppError;
use crate::models::schemas::{
    ChromaAddRequest, ChromaCollection, ChromaQueryRequest, ChromaQueryResponse,
};

/// ChromaDB HTTP client
pub struct ChromaClient {
    client: ChromaHttpClient,
    collection_prefix: String,
}

impl ChromaClient {
    pub fn new(config: &Config) -> Result<Self, AppError> {
        let endpoint = config
            .chroma_url
            .parse()
            .map_err(|error| AppError::Config(format!("invalid CHROMA_URL `{}`: {error}", config.chroma_url)))?;

        let options = ChromaHttpClientOptions {
            endpoint,
            endpoints: Vec::new(),
            auth_method: ChromaAuthMethod::None,
            retry_options: Default::default(),
            tenant_id: None,
            database_name: None,
        };

        Ok(Self {
            client: ChromaHttpClient::new(options),
            collection_prefix: config.chroma_collection_prefix.clone(),
        })
    }

    /// Get collection name for a category
    pub fn collection_name(&self, category: &str) -> String {
        format!("{}{}", self.collection_prefix, category.to_lowercase())
    }

    /// Check if Chroma is reachable
    pub async fn ping(&self) -> bool {
        self.client.heartbeat().await.is_ok()
    }

    /// Get or create a collection
    #[tracing::instrument(skip(self))]
    pub async fn get_or_create_collection(
        &self,
        category: &str,
        embedding_model: &str,
    ) -> Result<ChromaCollection, AppError> {
        let name = self.collection_name(category);
        let mut metadata = Metadata::new();
        metadata.insert("embedding_model".to_string(), MetadataValue::from(embedding_model.to_string()));

        let collection = self
            .client
            .get_or_create_collection(&name, None, Some(metadata))
            .await
            .map_err(|error| AppError::Chroma(error.to_string()))?;

        let collection_model = collection.to_collection_model();
        let mapped_metadata = collection_model
            .metadata
            .map(convert_metadata_to_json);

        if let Some(meta) = &mapped_metadata {
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

        Ok(ChromaCollection {
            id: collection.id().to_string(),
            name: collection.name().to_string(),
            metadata: mapped_metadata,
        })
    }

    /// Upsert documents into a collection
    #[tracing::instrument(skip(self, request), fields(collection = %collection_id, count = request.ids.len()))]
    pub async fn upsert(
        &self,
        collection_id: &str,
        request: &ChromaAddRequest,
    ) -> Result<(), AppError> {
        let collection = self
            .client
            .get_collection_by_id(collection_id)
            .await
            .map_err(|error| AppError::Chroma(error.to_string()))?;

        let documents = request.documents.as_ref().map(|docs| {
            docs.iter().cloned().map(Some).collect::<Vec<Option<String>>>()
        });

        let metadatas = request
            .metadatas
            .as_ref()
            .map(|items| items.iter().map(convert_update_metadata).collect::<Vec<Option<UpdateMetadata>>>());

        collection
            .upsert(
                request.ids.clone(),
                Some(request.embeddings.clone()),
                documents,
                None,
                metadatas,
            )
            .await
            .map_err(|error| AppError::Chroma(error.to_string()))?;

        Ok(())
    }

    /// Query a collection for similar embeddings
    #[tracing::instrument(skip(self, request), fields(collection = %collection_id))]
    pub async fn query(
        &self,
        collection_id: &str,
        request: &ChromaQueryRequest,
    ) -> Result<ChromaQueryResponse, AppError> {
        let collection = self
            .client
            .get_collection_by_id(collection_id)
            .await
            .map_err(|error| AppError::Chroma(error.to_string()))?;

        let include = match &request.include {
            Some(include) => Some(
                IncludeList::try_from(include.clone())
                    .map_err(|error| AppError::Validation(format!("invalid Chroma include list: {error}")))?,
            ),
            None => Some(IncludeList::default_query()),
        };

        let response: QueryResponse = collection
            .query(
                request.query_embeddings.clone(),
                Some(request.n_results as u32),
                None,
                None,
                include,
            )
            .await
            .map_err(|error| AppError::Chroma(error.to_string()))?;

        Ok(convert_query_response(response))
    }

    /// Delete documents by IDs from a collection
    #[tracing::instrument(skip(self, ids), fields(collection = %collection_id, count = ids.len()))]
    pub async fn delete_by_ids(&self, collection_id: &str, ids: &[String]) -> Result<(), AppError> {
        let collection = self
            .client
            .get_collection_by_id(collection_id)
            .await
            .map_err(|error| AppError::Chroma(error.to_string()))?;

        collection
            .delete(Some(ids.to_vec()), None, None)
            .await
            .map_err(|error| AppError::Chroma(error.to_string()))?;

        Ok(())
    }

    /// Delete an entire collection
    #[tracing::instrument(skip(self))]
    pub async fn delete_collection(&self, collection_name: &str) -> Result<(), AppError> {
        let result = self
            .client
            .delete_collection(collection_name)
            .await
            .map_err(|error| AppError::Chroma(error.to_string()));

        if let Err(error) = result {
            let message = error.to_string().to_lowercase();
            if message.contains("404") || message.contains("not found") {
                return Ok(());
            }
            return Err(error);
        }

        Ok(())
    }
}

fn convert_update_metadata(
    metadata: &HashMap<String, serde_json::Value>,
) -> Option<UpdateMetadata> {
    let mut out = UpdateMetadata::new();
    for (key, value) in metadata {
        let converted = match value {
            serde_json::Value::Bool(v) => MetadataValue::from(*v),
            serde_json::Value::Number(v) if v.is_i64() => MetadataValue::from(v.as_i64().unwrap_or_default()),
            serde_json::Value::Number(v) => MetadataValue::from(v.as_f64().unwrap_or_default()),
            serde_json::Value::String(v) => MetadataValue::from(v.clone()),
            serde_json::Value::Array(values) => {
                if values.iter().all(|item| item.is_string()) {
                    let strings = values
                        .iter()
                        .filter_map(|item| item.as_str().map(ToString::to_string))
                        .collect::<Vec<_>>();
                    MetadataValue::from(strings)
                } else if values.iter().all(|item| item.is_boolean()) {
                    let bools = values
                        .iter()
                        .filter_map(serde_json::Value::as_bool)
                        .collect::<Vec<_>>();
                    MetadataValue::from(bools)
                } else if values.iter().all(|item| item.is_i64()) {
                    let ints = values
                        .iter()
                        .filter_map(serde_json::Value::as_i64)
                        .collect::<Vec<_>>();
                    MetadataValue::from(ints)
                } else if values.iter().all(|item| item.is_number()) {
                    let floats = values
                        .iter()
                        .filter_map(serde_json::Value::as_f64)
                        .collect::<Vec<_>>();
                    MetadataValue::from(floats)
                } else {
                    continue;
                }
            }
            _ => continue,
        };
        out.insert(key.clone(), converted.into());
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn convert_metadata_to_json(metadata: Metadata) -> HashMap<String, serde_json::Value> {
    metadata
        .into_iter()
        .map(|(key, value)| (key, metadata_value_to_json(value)))
        .collect()
}

fn metadata_value_to_json(value: MetadataValue) -> serde_json::Value {
    match value {
        MetadataValue::Bool(v) => serde_json::Value::Bool(v),
        MetadataValue::Int(v) => serde_json::json!(v),
        MetadataValue::Float(v) => serde_json::json!(v),
        MetadataValue::Str(v) => serde_json::Value::String(v),
        MetadataValue::BoolArray(v) => serde_json::json!(v),
        MetadataValue::IntArray(v) => serde_json::json!(v),
        MetadataValue::FloatArray(v) => serde_json::json!(v),
        MetadataValue::StringArray(v) => serde_json::json!(v),
        MetadataValue::SparseVector(v) => serde_json::json!({
            "indices": v.indices,
            "values": v.values,
        }),
    }
}

fn convert_query_response(response: QueryResponse) -> ChromaQueryResponse {
    let embeddings = response.embeddings.map(|outer| {
        outer
            .into_iter()
            .map(|inner| {
                inner
                    .into_iter()
                    .map(|item| item.unwrap_or_default())
                    .collect::<Vec<Vec<f32>>>()
            })
            .collect::<Vec<Vec<Vec<f32>>>>()
    });

    let documents = response.documents.map(|outer| {
        outer
            .into_iter()
            .map(|inner| {
                inner
                    .into_iter()
                    .map(|item| item.unwrap_or_default())
                    .collect::<Vec<String>>()
            })
            .collect::<Vec<Vec<String>>>()
    });

    let metadatas = response.metadatas.map(|outer| {
        outer
            .into_iter()
            .map(|inner| {
                inner
                    .into_iter()
                    .map(|item| item.map(convert_metadata_to_json).unwrap_or_default())
                    .collect::<Vec<HashMap<String, serde_json::Value>>>()
            })
            .collect::<Vec<Vec<HashMap<String, serde_json::Value>>>>()
    });

    let distances = response.distances.map(|outer| {
        outer
            .into_iter()
            .map(|inner| {
                inner
                    .into_iter()
                    .map(|item| item.unwrap_or_default())
                    .collect::<Vec<f32>>()
            })
            .collect::<Vec<Vec<f32>>>()
    });

    ChromaQueryResponse {
        ids: response.ids,
        embeddings,
        documents,
        metadatas,
        distances,
    }
}
