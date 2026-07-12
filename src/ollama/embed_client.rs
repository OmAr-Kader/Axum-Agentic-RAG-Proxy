use crate::error::AppError;
use crate::models::schemas::EmbedResponse;
use reqwest::Client;
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct EmbedClient {
    pub client: Client,
    pub base_url: String,
    pub model: String,
    pub timeout: Option<Duration>,
}

#[derive(Debug, Serialize)]
struct EmbedRequestBody {
    model: String,
    input: Vec<String>,
}

impl EmbedClient {
    #[tracing::instrument(skip(self, texts), fields(batch_size = texts.len()))]
    pub async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, AppError> {
        let url = format!("{}/api/embed", self.base_url.trim_end_matches('/'));
        let payload = EmbedRequestBody {
            model: self.model.clone(),
            input: texts,
        };

        let mut request = self.client.post(url).json(&payload);
        if let Some(timeout) = self.timeout {
            request = request.timeout(timeout);
        }

        let response = request.send().await?.error_for_status()?;
        let body: EmbedResponse = response.json().await?;

        Ok(body.embeddings)
    }
}
