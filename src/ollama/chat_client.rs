use crate::error::AppError;
use bytes::Bytes;
use reqwest::{Client, Response};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ChatClient {
    pub client: Client,
    pub base_url: String,
}

impl ChatClient {
    #[tracing::instrument(skip(self, body))]
    pub async fn forward_chat(
        &self,
        body: Bytes,
        timeout: Option<Duration>,
    ) -> Result<Response, AppError> {
        self.forward_json("/api/chat", body, timeout).await
    }

    #[tracing::instrument(skip(self, body))]
    pub async fn forward_generate(
        &self,
        body: Bytes,
        timeout: Option<Duration>,
    ) -> Result<Response, AppError> {
        self.forward_json("/api/generate", body, timeout).await
    }

    async fn forward_json(
        &self,
        path: &str,
        body: Bytes,
        timeout: Option<Duration>,
    ) -> Result<Response, AppError> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let mut request = self
            .client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body);

        if let Some(timeout) = timeout {
            request = request.timeout(timeout);
        }
        tracing::info!(path = path, "Forwarding request to Ollama API");
        Ok(request.send().await?)
    }
}
