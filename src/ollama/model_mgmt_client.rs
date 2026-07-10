use crate::error::AppError;
use bytes::Bytes;
use reqwest::{Client, Method, Response};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ModelMgmtClient {
    pub client: Client,
    pub base_url: String,
}

impl ModelMgmtClient {
    pub async fn ping(&self, timeout: Option<Duration>) -> bool {
        let url = format!("{}/api/ps", self.base_url.trim_end_matches('/'));
        let mut request = self.client.get(url);

        if let Some(timeout) = timeout {
            request = request.timeout(timeout);
        }

        match request.send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    pub async fn forward(
        &self,
        method: Method,
        path: &str,
        body: Option<Bytes>,
        timeout: Option<Duration>,
    ) -> Result<Response, AppError> {
        let path = path.trim_start_matches('/');
        let url = format!("{}/{}", self.base_url.trim_end_matches('/'), path);
        let mut request = self.client.request(method, url);

        if let Some(body) = body {
            request = request.body(body);
        }

        if let Some(timeout) = timeout {
            request = request.timeout(timeout);
        }

        Ok(request.send().await?)
    }
}
