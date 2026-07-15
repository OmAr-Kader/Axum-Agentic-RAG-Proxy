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
        tracing::info!(?self.timeout, "Embed timeout");
        let response = match request.send().await {
            Ok(response) => response,
            Err(err) => {
                tracing::error!(
                    error = %err,
                    debug = ?err,
                    timeout = err.is_timeout(),
                    connect = err.is_connect(),
                    status = ?err.status(),
                    url = ?err.url(),
                    "Failed to send embed request"
                );

                return Err(err.into());
            }
        };

        let status = response.status();

        let body = match response.text().await {
            Ok(body) => body,
            Err(err) => {
                tracing::error!(
                    error = %err,
                    debug = ?err,
                    "Failed to read response body"
                );

                return Err(err.into());
            }
        };

        tracing::info!(
            status = %status,
            "Received embed response"
        );

        if !status.is_success() {
            return Err(AppError::Embedding(format!(
                "Ollama returned HTTP {}: {}",
                status, body
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
