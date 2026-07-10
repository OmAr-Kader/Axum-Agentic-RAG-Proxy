use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    response::Response,
};
use std::sync::Arc;

use crate::error::AppError;
use crate::AppState;

#[tracing::instrument(skip(state, req))]
pub async fn passthrough_handler(
    State(state): State<Arc<AppState>>,
    req: Request,
) -> Result<Response, AppError> {
    let (parts, body) = req.into_parts();
    let method = parts.method.clone();
    let path = parts.uri.path().to_string();
    let query = parts
        .uri
        .query()
        .map(|value| format!("?{value}"))
        .unwrap_or_default();

    let body_bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .map_err(|error| AppError::Internal(format!("Failed to read request body: {error}")))?;

    let response = state
        .model_mgmt_client
        .forward(
            reqwest::Method::from_bytes(method.as_str().as_bytes())
                .unwrap_or(reqwest::Method::GET),
            &format!("{path}{query}"),
            if body_bytes.is_empty() {
                None
            } else {
                Some(body_bytes)
            },
            state.config.http_client_timeout,
        )
        .await?;

    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let headers = response.headers().clone();
    let body_bytes = response
        .bytes()
        .await
        .map_err(|error| AppError::Internal(error.to_string()))?;

    let mut builder = Response::builder().status(status);
    for (key, value) in &headers {
        if key.as_str() != "host" && key.as_str() != "transfer-encoding" {
            builder = builder.header(key, value);
        }
    }

    builder
        .body(Body::from(body_bytes))
        .map_err(|error| AppError::Internal(error.to_string()))
}
