use axum::{body::Body, extract::State, http::StatusCode, response::Response};
use bytes::Bytes;
use std::sync::Arc;
use tracing::warn;

use crate::error::AppError;
use crate::models::schemas::{ChatMessage, ChatRequest, GenerateRequest};
use crate::prompt::builder::build_system_prompt;
use crate::AppState;

#[tracing::instrument(skip(state, body))]
pub async fn intercept_chat(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> Result<Response, AppError> {
    let mut chat_req: ChatRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            warn!(error = %error, "Failed to parse chat request, passing through");
            return forward_streaming(&state, "/api/chat", body, state.config.ollama_chat_timeout).await;
        }
    };

    let chunks = state.hybrid_engine.retrieve(&chat_req.messages).await?;
    tracing::info!(retrieved_chunks = chunks.len(), "Retrieved chunks for chat request");
    if !chunks.is_empty() {
        let original_system = chat_req
            .messages
            .iter()
            .find(|message| message.role.eq_ignore_ascii_case("system"))
            .map(|message| message.content.clone());
        let augmented = build_system_prompt(&chunks, original_system.as_deref(), &state.config);

        if let Some(system_message) = chat_req
            .messages
            .iter_mut()
            .find(|message| message.role.eq_ignore_ascii_case("system"))
        {
            system_message.content = augmented;
        } else {
            chat_req.messages.insert(
                0,
                ChatMessage {
                    role: "system".to_string(),
                    content: augmented,
                    extra: std::collections::HashMap::new(),
                },
            );
        }
    }

    let new_body = serde_json::to_vec(&chat_req).map_err(|error| AppError::Internal(error.to_string()))?;
    forward_streaming(
        &state,
        "/api/chat",
        Bytes::from(new_body),
        state.config.ollama_chat_timeout,
    )
    .await
}

#[tracing::instrument(skip(state, body))]
pub async fn intercept_generate(
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> Result<Response, AppError> {
    let mut generate_req: GenerateRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            warn!(error = %error, "Failed to parse generate request, passing through");
            return forward_streaming(
                &state,
                "/api/generate",
                body,
                state.config.ollama_chat_timeout,
            )
            .await;
        }
    };

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: generate_req.prompt.clone().unwrap_or_default(),
        extra: std::collections::HashMap::new(),
    }];
    let chunks = state.hybrid_engine.retrieve(&messages).await?;
    tracing::info!(retrieved_chunks = chunks.len(), "Retrieved chunks for generate request");
    if !chunks.is_empty() {
        generate_req.system = Some(build_system_prompt(
            &chunks,
            generate_req.system.as_deref(),
            &state.config,
        ));
    }

    let new_body = serde_json::to_vec(&generate_req)
        .map_err(|error| AppError::Internal(error.to_string()))?;
    forward_streaming(
        &state,
        "/api/generate",
        Bytes::from(new_body),
        state.config.ollama_chat_timeout,
    )
    .await
}

async fn forward_streaming(
    state: &Arc<AppState>,
    path: &str,
    body: Bytes,
    timeout: Option<std::time::Duration>,
) -> Result<Response, AppError> {
    let response = if path == "/api/chat" {
        state.chat_client.forward_chat(body, timeout).await?
    } else {
        state.chat_client.forward_generate(body, timeout).await?
    };

    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let headers = response.headers().clone();
    let body = Body::from_stream(response.bytes_stream());

    let mut builder = Response::builder().status(status);
    for (key, value) in &headers {
        if key.as_str() != "host" && key.as_str() != "transfer-encoding" {
            builder = builder.header(key, value);
        }
    }

    builder
        .body(body)
        .map_err(|error| AppError::Internal(error.to_string()))
}
