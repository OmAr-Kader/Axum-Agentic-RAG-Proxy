#![allow(dead_code)]

use std::{env, str::FromStr, time::Duration};

use tracing::info;

use crate::error::AppError;

/// All config from .env, loaded once at startup into Arc<Config>.
/// Every timeout supports -1 = disabled (returns None).
#[derive(Debug, Clone)]
pub struct Config {
    // Server
    pub host: String,
    pub port: u16,
    // Ollama
    pub ollama_base_url: String,
    pub ollama_embedding_base_url: String,
    pub embedding_model: String,
    pub embedding_batch_size: usize,
    pub embedding_max_concurrency: usize,
    pub embedding_max_retries: u32,
    // Timeouts
    pub http_client_timeout: Option<Duration>,
    pub ollama_chat_timeout: Option<Duration>,
    pub ollama_embedding_timeout: Option<Duration>,
    pub streaming_idle_timeout: Option<Duration>,
    pub chroma_request_timeout: Option<Duration>,
    pub file_op_timeout: Option<Duration>,
    pub background_job_timeout: Option<Duration>,
    pub health_check_timeout: Option<Duration>,
    pub category_lock_read_timeout_ms: u64,
    // Chroma
    pub chroma_url: String,
    pub chroma_collection_prefix: String,
    // Rulesets
    pub ruleset_map_file: String,
    pub rulesets_dir: String,
    pub reload_on_startup: bool,
    pub watch_rulesets: bool,
    pub watcher_debounce_ms: u64,
    // Chunking
    pub chunk_size: usize,
    pub chunk_overlap: usize,
    // Retrieval
    pub top_k: usize,
    pub similarity_threshold: f32,
    pub max_injected_context_tokens: usize,
    pub context_reserved_tokens: usize,
    pub always_include_single_category_cap_pct: usize,
    pub always_include_all_categories_cap_pct: usize,
    pub global_always_on_retrieved_cap: usize,
    pub category_select_top_n: usize,
    // Retry
    pub failed_embedding_retry_interval_seconds: u64,
    pub failed_embedding_max_attempts: u32,
    // Security
    pub max_rule_content_bytes: usize,
    // Logging
    pub log_dir: String,
    pub log_level: String,
    pub log_retention_days: u64,
}

impl Config {
    pub fn from_env() -> Result<Self, AppError> {
        dotenvy::dotenv().ok();

        let host = env_var("HOST")?;
        let port = parse_required("PORT")?;

        let ollama_base_url = env_var("OLLAMA_BASE_URL")?;
        let ollama_embedding_base_url = match env::var("OLLAMA_EMBEDDING_BASE_URL") {
            Ok(value) if !value.trim().is_empty() => value,
            _ => {
                info!("OLLAMA_EMBEDDING_BASE_URL unset or empty; defaulting to OLLAMA_BASE_URL");
                ollama_base_url.clone()
            }
        };

        let embedding_model = non_empty_env_var("EMBEDDING_MODEL")?;

        Ok(Self {
            host,
            port,
            ollama_base_url,
            ollama_embedding_base_url,
            embedding_model,
            embedding_batch_size: parse_required("EMBEDDING_BATCH_SIZE")?,
            embedding_max_concurrency: parse_required("EMBEDDING_MAX_CONCURRENCY")?,
            embedding_max_retries: parse_required("EMBEDDING_MAX_RETRIES")?,
            http_client_timeout: parse_timeout("HTTP_CLIENT_TIMEOUT_SECONDS")?,
            ollama_chat_timeout: parse_timeout("OLLAMA_CHAT_TIMEOUT_SECONDS")?,
            ollama_embedding_timeout: parse_timeout("OLLAMA_EMBEDDING_TIMEOUT_SECONDS")?,
            streaming_idle_timeout: parse_timeout("STREAMING_IDLE_TIMEOUT_SECONDS")?,
            chroma_request_timeout: parse_timeout("CHROMA_REQUEST_TIMEOUT_SECONDS")?,
            file_op_timeout: parse_timeout("FILE_OP_TIMEOUT_SECONDS")?,
            background_job_timeout: parse_timeout("BACKGROUND_JOB_TIMEOUT_SECONDS")?,
            health_check_timeout: parse_timeout("HEALTH_CHECK_TIMEOUT_SECONDS")?,
            category_lock_read_timeout_ms: parse_required("CATEGORY_LOCK_READ_TIMEOUT_MS")?,
            chroma_url: env_var("CHROMA_URL")?,
            chroma_collection_prefix: env_var("CHROMA_COLLECTION_PREFIX")?,
            ruleset_map_file: env_var("RULESET_MAP_FILE")?,
            rulesets_dir: env_var("RULESETS_DIR")?,
            reload_on_startup: parse_required("RELOAD_ON_STARTUP")?,
            watch_rulesets: parse_required("WATCH_RULESETS")?,
            watcher_debounce_ms: parse_required("WATCHER_DEBOUNCE_MS")?,
            chunk_size: parse_required("CHUNK_SIZE")?,
            chunk_overlap: parse_required("CHUNK_OVERLAP")?,
            top_k: parse_required("TOP_K")?,
            similarity_threshold: parse_required("SIMILARITY_THRESHOLD")?,
            max_injected_context_tokens: parse_required("MAX_INJECTED_CONTEXT_TOKENS")?,
            context_reserved_tokens: parse_required("CONTEXT_RESERVED_TOKENS")?,
            always_include_single_category_cap_pct: parse_required(
                "ALWAYS_INCLUDE_SINGLE_CATEGORY_CAP_PCT",
            )?,
            always_include_all_categories_cap_pct: parse_required(
                "ALWAYS_INCLUDE_ALL_CATEGORIES_CAP_PCT",
            )?,
            global_always_on_retrieved_cap: parse_required("GLOBAL_ALWAYS_ON_RETRIEVED_CAP")?,
            category_select_top_n: parse_required("CATEGORY_SELECT_TOP_N")?,
            failed_embedding_retry_interval_seconds: parse_required(
                "FAILED_EMBEDDING_RETRY_INTERVAL_SECONDS",
            )?,
            failed_embedding_max_attempts: parse_required("FAILED_EMBEDDING_MAX_ATTEMPTS")?,
            max_rule_content_bytes: parse_required("MAX_RULE_CONTENT_BYTES")?,
            log_dir: env_var("LOG_DIR")?,
            log_level: env_var("LOG_LEVEL")?,
            log_retention_days: parse_required("LOG_RETENTION_DAYS")?,
        })
    }
}

fn env_var(key: &str) -> Result<String, AppError> {
    env::var(key).map_err(|err| AppError::Config(format!("missing {key}: {err}")))
}

fn non_empty_env_var(key: &str) -> Result<String, AppError> {
    let value = env_var(key)?;
    if value.trim().is_empty() {
        return Err(AppError::Config(format!("{key} must be non-empty")));
    }

    Ok(value)
}

fn parse_required<T>(key: &str) -> Result<T, AppError>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    let value = env_var(key)?;
    value
        .parse::<T>()
        .map_err(|err| AppError::Config(format!("invalid {key} value `{value}`: {err}")))
}

fn parse_timeout(key: &str) -> Result<Option<Duration>, AppError> {
    let value = env_var(key)?;
    let seconds = value
        .parse::<i64>()
        .map_err(|err| AppError::Config(format!("invalid {key} value `{value}`: {err}")))?;

    if seconds < 0 {
        Ok(None)
    } else {
        Ok(Some(Duration::from_secs(seconds as u64)))
    }
}
