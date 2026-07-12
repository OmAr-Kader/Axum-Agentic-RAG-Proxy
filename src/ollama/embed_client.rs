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
        tracing::info!(url = %url, model = %self.model, inputs = texts.len(), "Preparing embed request");
        let payload = EmbedRequestBody {
            model: self.model.clone(),
            input: texts,
        };

        tracing::info!(
            url = %url,
            model = %payload.model,
            inputs = payload.input.len(),
            "Sending embed request"
        );

        let mut request = self.client.post(&url).json(&payload);

        if let Some(timeout) = self.timeout {
            request = request.timeout(timeout);
        }

        let response = request.send().await?;

        let status = response.status();
        let body = response.text().await?;

        tracing::info!(
            status = %status,
            body = %body,
            "Received embed response"
        );
        if !status.is_success() {
            return Err(AppError::Embedding(format!(
                "Ollama returned HTTP {}: {}",
                status,
                body
            )));
        }

        let parsed: EmbedResponse = serde_json::from_str(&body).map_err(|err| {
            AppError::Embedding(format!(
                "Failed to deserialize Ollama response.\nError: {}\nBody: {}",
                err, body
            ))
        })?;

        Ok(parsed.embeddings)
    }
}
