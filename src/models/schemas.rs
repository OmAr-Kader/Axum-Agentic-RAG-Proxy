use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// --- Ollama Chat ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: Option<bool>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// --- Ollama Generate ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateRequest {
    pub model: String,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub system: Option<String>,
    #[serde(default)]
    pub stream: Option<bool>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// --- Ollama Embed ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedRequest {
    pub model: String,
    pub input: EmbedInput,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EmbedInput {
    Single(String),
    Batch(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedResponse {
    pub model: String,
    pub embeddings: Vec<Vec<f32>>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

// --- Chroma ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromaCollection {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromaAddRequest {
    pub ids: Vec<String>,
    pub embeddings: Vec<Vec<f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documents: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadatas: Option<Vec<HashMap<String, serde_json::Value>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromaQueryRequest {
    pub query_embeddings: Vec<Vec<f32>>,
    pub n_results: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub where_filter: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromaQueryResponse {
    pub ids: Vec<Vec<String>>,
    #[serde(default)]
    pub embeddings: Option<Vec<Vec<Vec<f32>>>>,
    #[serde(default)]
    pub documents: Option<Vec<Vec<String>>>,
    #[serde(default)]
    pub metadatas: Option<Vec<Vec<HashMap<String, serde_json::Value>>>>,
    #[serde(default)]
    pub distances: Option<Vec<Vec<f32>>>,
}

// --- Ruleset / Chunks ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesetFrontmatter {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub applies_to: Vec<String>,
    #[serde(default)]
    pub scope: Vec<String>,
    #[serde(default = "default_rule_type")]
    pub r#type: String,
    #[serde(default)]
    pub priority: u32,
    #[serde(default)]
    pub always_include: bool,
    #[serde(default)]
    pub agent_only: bool,
    #[serde(default)]
    pub examples: bool,
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_rule_type() -> String {
    "rule".to_string()
}

#[derive(Debug, Clone)]
pub struct RuleChunk {
    pub id: String,
    pub document_id: String,
    pub chunk_index: usize,
    pub chunk_title: String,
    pub total_chunks: usize,
    pub content: String,
    pub source_file: String,
    pub file_hash: String,
    pub category: String,
    pub frontmatter: RulesetFrontmatter,
}

// --- Query Analysis ---

#[derive(Debug, Clone, Default)]
pub struct QueryAnalysis {
    pub keywords: Vec<String>,
    pub languages: Vec<String>,
    pub frameworks: Vec<String>,
    pub topics: Vec<String>,
    pub intent: Option<String>,
    pub is_agent_mode: bool,
}

// --- Retrieval Result ---

#[derive(Debug, Clone)]
pub struct RetrievedChunk {
    pub chunk: RuleChunk,
    pub similarity_score: f32,
    pub keyword_boost: f32,
    pub final_score: f32,
}

// --- Admin API ---

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub ollama_reachable: bool,
    pub chroma_reachable: bool,
    pub ingestion_ready: bool,
    pub active_categories: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RulesetWriteRequest {
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub top_k: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub id: String,
    pub content: String,
    pub score: f32,
    pub category: String,
    pub metadata: HashMap<String, serde_json::Value>,
}
