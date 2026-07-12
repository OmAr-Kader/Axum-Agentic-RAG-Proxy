mod api;
mod config;
mod embedding;
mod error;
mod index;
mod jobs;
mod logging;
mod models;
mod ollama;
mod prompt;
mod proxy;
mod query;
mod retrieval;
mod rulesets;
mod security;
mod vectorstore;

use std::sync::Arc;

use axum::{routing::{any, delete, get, post}, Router};
use reqwest::Client;
use tokio::net::TcpListener;
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};
use tracing::info;

use crate::config::Config;
use crate::embedding::service::EmbeddingService;
use crate::error::AppError;
use crate::index::index_manager::IndexManager;
use crate::logging::init_logging;
use crate::ollama::{chat_client::ChatClient, embed_client::EmbedClient, model_mgmt_client::ModelMgmtClient};
use crate::retrieval::hybrid_engine::HybridEngine;
use crate::vectorstore::chroma_client::ChromaClient;

pub struct AppState {
    pub config: Arc<Config>,
    pub chat_client: Arc<ChatClient>,
    pub model_mgmt_client: Arc<ModelMgmtClient>,
    pub hybrid_engine: Arc<HybridEngine>,
    pub index_manager: Arc<IndexManager>,
    pub chroma: Arc<ChromaClient>,
}

#[tokio::main]
async fn main() -> Result<(), AppError> {
    let config = Arc::new(Config::from_env()?);
    init_logging(&config);

    let http_client = build_http_client(config.http_client_timeout)?;
    let chat_client = Arc::new(ChatClient {
        client: http_client.clone(),
        base_url: config.ollama_base_url.clone(),
    });
    let embed_client = Arc::new(EmbedClient {
        client: http_client.clone(),
        base_url: config.ollama_embedding_base_url.clone(),
        model: config.embedding_model.clone(),
        timeout: config.ollama_embedding_timeout,
    });
    let model_mgmt_client = Arc::new(ModelMgmtClient {
        client: http_client,
        base_url: config.ollama_base_url.clone(),
    });
    let index_manager = Arc::new(IndexManager::new(config.clone()));
    let chroma = Arc::new(ChromaClient::new(&config)?);
    let embedding_service = Arc::new(EmbeddingService::new(embed_client, config.clone()));
    let hybrid_engine = Arc::new(HybridEngine::new(
        config.clone(),
        index_manager.clone(),
        embedding_service,
        chroma.clone(),
    ));

    let state = Arc::new(AppState {
        config: config.clone(),
        chat_client,
        model_mgmt_client,
        hybrid_engine,
        index_manager,
        chroma,
    });

    tokio::spawn(jobs::initial_index::run_initial_index(state.clone()));
    tokio::spawn(jobs::retry_failed_embeddings::retry_failed_embeddings_loop(state.clone()));

    if config.watch_rulesets {
        let rx = rulesets::watcher::start_watcher(
            config.rulesets_dir.clone(),
            config.ruleset_map_file.clone(),
            config.watcher_debounce_ms,
        )
        .await?;
        tokio::spawn(jobs::reindex_queue::process_reindex_queue(state.clone(), rx));
    }

    let app = Router::new()
        .route("/api/chat", post(proxy::intercept::intercept_chat))
        .route("/api/generate", post(proxy::intercept::intercept_generate))
        .route("/admin/health", get(api::routes_health::health_handler))
        .route("/admin/rulesets", get(api::routes_rulesets::list_rulesets))
        .route(
            "/admin/rulesets/{category}/{filename}",
            post(api::routes_rulesets::write_ruleset)
                .delete(api::routes_rulesets::delete_ruleset_file),
        )
        .route(
            "/admin/rulesets/{category}",
            delete(api::routes_rulesets::delete_category),
        )
        .route("/admin/reload", post(api::routes_admin::reload_handler))
        .route("/admin/reset", post(api::routes_admin::reset_handler))
        .route("/admin/index-status", get(api::routes_admin::index_status_handler))
        .route("/search", post(api::routes_search::search_handler))
        .route("/api/{*rest}", any(proxy::passthrough::passthrough_handler))
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let bind_address = format!("{}:{}", config.host, config.port);
    let listener = TcpListener::bind(&bind_address)
        .await
        .map_err(|error| AppError::Internal(format!("Failed to bind {bind_address}: {error}")))?;
    info!(address = %bind_address, "Listening");

    axum::serve(listener, app)
        .await
        .map_err(|error| AppError::Internal(format!("Server error: {error}")))
}

fn build_http_client(timeout: Option<std::time::Duration>) -> Result<Client, AppError> {
    let mut builder = Client::builder();
    if let Some(timeout) = timeout {
        builder = builder.timeout(timeout);
    }
    builder
        .build()
        .map_err(|error| AppError::Internal(format!("Failed to build HTTP client: {error}")))
}
