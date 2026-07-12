# Agentic RAG Proxy for Ollama — Rust Axum Build Spec

**Purpose:** A transparent reverse-proxy server (Rust + Axum) that augments system prompts using ChromaDB-stored rule chunks, enabling VS Code's native chat to send retrieval-augmented rules instead of a static, ever-growing system prompt.

No auth or rate limiting. All routes are open; port-level security is the operator's responsibility.

---

## 0. Context — Why This Exists

VS Code's native chat (Ask mode and Agent mode) talks directly to Ollama at
`http://localhost:11434`. This server is a **drop-in transparent proxy**: VS Code's Ollama base
URL is pointed here instead (e.g. `http://localhost:8000`) and **every endpoint VS Code uses must
behave identically to real Ollama** — same request/response shapes, same streaming behavior, same
error codes, same headers (minus `Host`).

The only behavioral difference: on `/api/chat` and `/api/generate` requests, the proxy augments the
system prompt using a hybrid retrieval pipeline over ChromaDB before forwarding to Ollama. This
replaces a hand-maintained 1000+ line system prompt with rule chunks retrieved on-demand and ranked
by relevance, always-include priority, and metadata match.

---

## Core Constraints

1. **Transparency** — Only `/api/chat` and `/api/generate` are intercepted; all other routes pass through unchanged.
2. **Independent backends** — Chat and embedding backends are separate (different URLs, configs, models allowed).
3. **Fail-Open** — Any proxy-internal failure forwards the original request unmodified; retrieval never blocks chat.
4. **Tool-Call Fidelity** — On `/api/chat`, only the system message content is modified; `tools`, `tool_calls`, and other fields pass through untouched via `#[serde(flatten)]` catch-all maps.
5. **Human-authored rulesets** — The `rulesets/` directory is maintained outside this application; server writes files only via admin endpoints.
6. **No auth/rate-limiting** — Do not implement these modules.
7. **All limits in `.env`** — No hardcoded constants; every threshold, batch size, timeout, and cap is configurable.

---

## Ollama API Endpoints

Every Ollama endpoint used by this proxy was checked against `https://docs.ollama.com/api/`:

| Endpoint | Status | Notes |
|---|---|---|
| `/api/chat` | ✅ Current | Intercepted; multi-turn `messages[]`, `tools`/tool-calling, `stream` — all fields other than the system message content pass through and return unmodified (Constraint 3a). |
| `/api/generate` | ✅ Current | Intercepted; legacy single-prompt completion, still supported. |
| `/api/embed` | ✅ Current | Current, supports batched `input: string \| string[]`, returns `embeddings: number[][]`, supports `truncate`, `keep_alive`, `options`. **This spec uses `/api/embed` exclusively.** |
| `/api/tags` | ✅ Current | List local models — passthrough only. |
| `/api/ps` | ✅ Current | List running models — used for non-blocking startup ping / health. |
| `/api/show` | ✅ Current | Model info — passthrough only. |
| `/api/pull`, `/api/push`, `/api/create`, `/api/copy`, `/api/delete` | ✅ Current | Model management — passthrough only, never called by proxy logic itself. |

Note: `/api/embeddings` (deprecated, single-input) is not used; `/api/embed` is used exclusively with batching.

chunk batch in one request where feasible, subject to `EMBEDDING_BATCH_SIZE` in `.env`).

---

## Project Structure

Repository root is the project root; `Cargo.toml`, `src/`, and config files sit at the same level as `.gitignore`, `LICENSE`, and `README.md`.

```
.
├── .gitignore
├── LICENSE
├── README.md
├── Cargo.toml
├── .env
├── rulesets.json
├── src/
│   ├── main.rs
│   ├── config.rs
│   ├── error.rs
│   ├── logging.rs
│   ├── ollama/
│   │   ├── chat_client.rs
│   │   ├── embed_client.rs
│   │   └── model_mgmt_client.rs
│   ├── rulesets/
│   │   ├── loader.rs
│   │   ├── watcher.rs
│   │   ├── frontmatter.rs
│   │   └── chunker.rs
│   ├── index/
│   │   ├── index_manager.rs
│   │   └── keyword_index.rs
│   ├── embedding/
│   │   ├── service.rs
│   │   └── cache.rs
│   ├── vectorstore/
│   │   └── chroma_client.rs
│   ├── query/
│   │   └── analyzer.rs
│   ├── retrieval/
│   │   ├── hybrid_engine.rs
│   │   └── ranker.rs
│   ├── prompt/
│   │   └── builder.rs
│   ├── proxy/
│   │   ├── passthrough.rs
│   │   └── intercept.rs
│   ├── api/
│   │   ├── routes_health.rs
│   │   ├── routes_rulesets.rs
│   │   ├── routes_search.rs
│   │   └── routes_admin.rs
│   ├── jobs/
│   │   ├── initial_index.rs
│   │   ├── reindex_queue.rs
│   │   └── retry_failed_embeddings.rs
│   ├── security/
│   │   └── validation.rs
│   └── models/
│       └── schemas.rs
└── logs/
```

---

## `.env`

```env
# --- Server ---
HOST=0.0.0.0
PORT=8000

# --- Ollama backends (SPLIT) ---
# Chat backend: whichever model VS Code selects, forwarded as-is, never validated.
OLLAMA_BASE_URL=http://localhost:11434
# Embedding backend: independent URL, may be a different Ollama instance/host/port entirely.
# Falls back to OLLAMA_BASE_URL if left empty, but is a first-class separate setting.
OLLAMA_EMBEDDING_BASE_URL=http://localhost:11434
EMBEDDING_MODEL=qwen3-embedding:0.6b-q8_0   # REQUIRED — fail startup if empty
EMBEDDING_BATCH_SIZE=16                     # /api/embed supports array input — batch chunks per call
EMBEDDING_MAX_CONCURRENCY=3                 # semaphore size for concurrent embedding requests
EMBEDDING_MAX_RETRIES=1                     # retries on timeout/transient failure before skip+warn

# --- Timeouts (ALL support -1 = disabled) ---
HTTP_CLIENT_TIMEOUT_SECONDS=-1
OLLAMA_CHAT_TIMEOUT_SECONDS=-1              # never interrupt long-running model responses unless set
OLLAMA_EMBEDDING_TIMEOUT_SECONDS=30
STREAMING_IDLE_TIMEOUT_SECONDS=-1
CHROMA_REQUEST_TIMEOUT_SECONDS=15
FILE_OP_TIMEOUT_SECONDS=5
BACKGROUND_JOB_TIMEOUT_SECONDS=-1
HEALTH_CHECK_TIMEOUT_SECONDS=5
CATEGORY_LOCK_READ_TIMEOUT_MS=100           # read-path lock acquisition timeout before fail-open skip

# --- Chroma ---
CHROMA_URL=http://localhost:8001
CHROMA_COLLECTION_PREFIX=rules_

# --- Rulesets ---
RULESET_MAP_FILE=./rulesets.json
RULESETS_DIR=./rulesets/
RELOAD_ON_STARTUP=true
WATCH_RULESETS=true                         # enable fs watcher (Section 6)
WATCHER_DEBOUNCE_MS=500                     # debounce window for filesystem watcher events

# --- Chunking ---
CHUNK_SIZE=400
CHUNK_OVERLAP=50

# --- Retrieval ---
TOP_K=8
SIMILARITY_THRESHOLD=0.25
MAX_INJECTED_CONTEXT_TOKENS=1200
CONTEXT_RESERVED_TOKENS=512
ALWAYS_INCLUDE_SINGLE_CATEGORY_CAP_PCT=30   # % of MAX_INJECTED_CONTEXT_TOKENS
ALWAYS_INCLUDE_ALL_CATEGORIES_CAP_PCT=60    # % of MAX_INJECTED_CONTEXT_TOKENS
GLOBAL_ALWAYS_ON_RETRIEVED_CAP=2            # max retrieved (non always-include) chunks from GLOBAL_ALWAYS_ON
CATEGORY_SELECT_TOP_N=2                     # top-N non-global categories selected by score

# --- Retry / background jobs ---
FAILED_EMBEDDING_RETRY_INTERVAL_SECONDS=300
FAILED_EMBEDDING_MAX_ATTEMPTS=3

# --- Security ---
MAX_RULE_CONTENT_BYTES=131072                # 128 KB cap on admin write `content` field

# --- Logging ---
LOG_DIR=./logs
LOG_LEVEL=info
LOG_RETENTION_DAYS=14
```

`OLLAMA_BASE_URL` drives `ollama/chat_client.rs` (chat/generate passthrough + interception);
`OLLAMA_EMBEDDING_BASE_URL` drives `ollama/embed_client.rs` exclusively. This allows embeddings to
run against a separate host/port/Ollama instance without touching chat traffic. If
`OLLAMA_EMBEDDING_BASE_URL` is unset at startup, `config.rs` defaults it to `OLLAMA_BASE_URL` and
logs an INFO noting the fallback.

---

## Ruleset Frontmatter

Each `.md` begins with YAML frontmatter:

```md
---
id: kotlin_compose_navigation
title: Compose Navigation

applies_to:
  - kotlin
  - android
  - jetpack-compose

scope:
  - compose

type: rule

priority: 90

always_include: false

agent_only: false

examples: true

tags:
  - navigation
  - state
  - deeplink
---

# Compose Navigation

Introduction...

---

## Section 2

...

---

## Section 3

...
```

### Indexing Behavior

- Parse YAML frontmatter (before first `---`).
- Split body on standalone `---` lines; each section is a chunk candidate.
- If section > `CHUNK_SIZE` tokens, apply secondary token-based split (respecting `CHUNK_OVERLAP`), preserving metadata. Never split inside code blocks.
- Each chunk inherits all frontmatter fields.
- Auto-generated metadata: `document_id`, `chunk_index`, `chunk_title`, `total_chunks`, `source_file`, `file_hash`.
- Deterministic chunk ID: `{category}::{relative_path}::{chunk_index}`.

### Dynamic Keyword Vocabulary

At index time, `index/keyword_index.rs` aggregates all `applies_to` tags and `tags` entries across all `.md` files into a lowercased, deduped in-memory set. No hardcoded language/framework lists exist. This set feeds keyword extraction and scoring. Matching is case-insensitive substring (e.g., `"compose"` matches `"Jetpack Compose"`, `"ComposeView"`).

---

## `rulesets.json` (Category Map)

```json
{
  "GLOBAL_ALWAYS_ON": "global/",
  "SWIFT_IOS_RULES": "swift_ios/",
  "KOTLIN_ANDROID_RULES": "kotlin_android/",
  "RUST_AXUM_RULES": "rust_axum/"
}
```

- Fully dynamic; `GLOBAL_ALWAYS_ON` is reserved (always searched, special token budgeting).
- Each key → Chroma collection: `{CHROMA_COLLECTION_PREFIX}{key.to_lowercase()}`.
- Each value → recursively-scanned folder for all `.md` files.
- Add category: update JSON, create folder, call `/admin/reload`.

---

# 1. Project Setup

- `cargo new .` at repo root; `Cargo.toml` and `src/` at the same level as `.gitignore`, `LICENSE`, `README.md`.
- Core dependencies: `axum`, `tokio`, `tower`, `tower-http`, `reqwest`, `serde`, `serde_yaml`, `tracing`, `tracing-appender`, `notify`, `sha2`, `walkdir`, `thiserror`, `async-trait`.
- Load config once at startup into `Arc<Config>` (Axum state); never re-read `.env` per-request. All limits from `Config`, never hardcoded.

---

# 2. Configuration (`.env`)

Every timeout is independently configurable; `-1` disables it. Applies to HTTP client, chat/generate, embeddings, streaming, Chroma, file ops, and background jobs. Never interrupt long-running responses unless explicitly set.

---

# 3. HTTP Server

- Axum `Router`, flat route table; no auth or rate-limiting middleware.
- Middleware: `TraceLayer` → `CompressionLayer` → `CorsLayer`.
- CORS: permissive (localhost dev default).

---

# 4. Ollama Client

- `chat_client` — POST `/api/chat` and `/api/generate` at `OLLAMA_BASE_URL`; streaming passed straight through, only system message modified (via `#[serde(flatten)]` catch-all for forward-compat).
- `embed_client` — POST `/api/embed` at `OLLAMA_EMBEDDING_BASE_URL` (batched by `EMBEDDING_BATCH_SIZE`). Only endpoint used.
- `model_mgmt_client` — Passthrough for `/api/tags`, `/api/ps`, `/api/show` (health only).

---

# 5. Ruleset Loader

- Read `rulesets.json`, validate flat `{String: String}` map.
- Resolve each value against `RULESETS_DIR`; recursively walk for all `.md` files.
- Per file: read bytes → SHA-256 hash → parse frontmatter → chunk on `---` → token split if needed → build records with metadata.
- Missing folders: log WARNING, add to `empty_categories`, continue.

---

# 6. Ruleset Watcher

- Watch `RULESETS_DIR` and `RULESET_MAP_FILE` via `notify` (gated by `WATCH_RULESETS=true`, debounced by `WATCHER_DEBOUNCE_MS`).
- Detect new/modified/deleted `.md` files and category folder changes.
- Emit to `jobs::reindex_queue` for incremental re-indexing (hash-diff, skip unchanged).
- Manual `POST /admin/reload` for non-watcher environments.

---

# 7. Document Processing

See "Ruleset Frontmatter" section for chunk/metadata contract.

---

# 8. Embedding Service

- Batch texts (up to `EMBEDDING_BATCH_SIZE`) into single `/api/embed` calls.
- Concurrency: semaphore sized by `EMBEDDING_MAX_CONCURRENCY`.
- Retry up to `EMBEDDING_MAX_RETRIES`; on failure skip that file's chunks (log WARNING), never abort the run.
- Optional cache (keyed by `file_hash`) to skip re-embedding unchanged content.

---

# 9. ChromaDB

- HTTP client to `CHROMA_URL`.
- Collection: `{CHROMA_COLLECTION_PREFIX}{category_key.to_lowercase()}`.
- Store `EMBEDDING_MODEL` in collection metadata; on mismatch return `409` until reload.
- Ops: create, upsert, delete by ID/filter, similarity search (with metadata filter).

---

# 10. Index Manager

- Sync engine between filesystem and Chroma.
- Initial indexing: full scan + embed + upsert (background, non-blocking; `ingestion_ready` gates retrieval).
- Incremental indexing: hash-diff to skip unchanged, update modified, remove deleted.
- Maintain chunk map (`source_file → [chunk_ids]`) for fast invalidation.

---

# 11. Query Analyzer

Extract from user-only messages (never assistant content):
- Language, framework, tech, topics (vs. dynamic keyword vocabulary; no hardcoded lists).
- Intent (debugging, scaffolding, refactor, etc.).
- Agent mode (Ask vs. Agent).
- Keywords for metadata filtering and keyword-overlap scoring.

---

# 12. Hybrid Retrieval Engine

1. Load `always_include: true` chunks (GLOBAL_ALWAYS_ON always included); cap non-always-include at `GLOBAL_ALWAYS_ON_RETRIEVED_CAP`.
2. Rank non-global categories by relevance; select top `CATEGORY_SELECT_TOP_N`.
3. Filter by metadata (`applies_to`, `scope`, `agent_only`, etc.) vs. query.
4. Search Chroma (embedding similarity, `n_results=TOP_K`, threshold `SIMILARITY_THRESHOLD`).
5. Rank by: similarity + metadata match + keyword-overlap boost (case-insensitive substring match) + priority.
6. Select final docs, cap injected tokens at `MAX_INJECTED_CONTEXT_TOKENS`.

---

# 13. Prompt Builder

Assemble augmented system message in deterministic order:
1. Always-include rules (GLOBAL_ALWAYS_ON first, then alphabetical).
2. High-priority rules (priority descending).
3. Retrieved rules (relevance-ranked).
4. Examples (if `examples: true` and within budget).
5. Original system message (appended).

Format: `## {category} › {chunk_title}` with optional `[score: X.XX]`.

If no original system message, omit leading section. Non-system messages pass through verbatim.

---

# 14. API Endpoints

| Method | Path | Purpose |
|---|---|---|
| `*` | `/api/*` | Passthrough (except chat/generate). |
| `POST` | `/api/chat` | Intercept, augment system message, stream. |
| `POST` | `/api/generate` | Intercept, augment system, stream. |
| `GET` | `/admin/health` | Status, reachability, active categories, ingestion_ready. |
| `GET` | `/admin/rulesets` | Counts per category. |
| `POST` | `/admin/rulesets/{category}/{filename}` | Re-index or write+index. |
| `DELETE` | `/admin/rulesets/{category}/{filename}` | Remove chunks (`?delete_from_disk=true` deletes file). |
| `DELETE` | `/admin/rulesets/{category}` | Drop collection (requires confirm). |
| `POST` | `/admin/reload` | Full re-scan + index. |
| `POST` | `/admin/reset` | Drop all, rebuild (requires confirm). |
| `GET` | `/admin/index-status` | Job status, last error. |
| `POST` | `/search` | Ad-hoc retrieval debug query. |

No auth.

---

# 15. Background Jobs

- `initial_index` — Startup task (non-blocking); sets `ingestion_ready = true` on completion.
- `reindex_queue` — Consume watcher events, incremental re-index (debounced).
- `retry_failed_embeddings` — Periodic sweep retry failed chunks (bounded by `FAILED_EMBEDDING_MAX_ATTEMPTS`).

---

# 16. Storage

- File hashes (sidecar): idempotent re-index across restarts.
- Metadata/keyword/chunk maps: rebuilt on full index, incrementally updated by watcher.
- Embedding cache (optional): keyed by `file_hash` + chunk index.
- Index state: `ingestion_ready`, `_embedding_model_dirty`, `empty_categories`, last error per category.

---

# 17. Security

Input validation only (no auth, no rate-limiting):
- `{category}`: `^[a-zA-Z0-9_\-]+$` → `422`.
- `{filename}`: `^[a-zA-Z0-9_\-\.]+\.md$` → `422`.
- `content` field: ≤ `MAX_RULE_CONTENT_BYTES` → `413`.

No subprocess invocations.

---

# 18. Observability

Structured JSON logs via `tracing` + `tracing-appender` (daily rotation):
- **access.log**: `{timestamp, method, path, status_code, duration_ms, intercepted, request_id}`.
- **app.log**: startup, config, loading, events, indexing, embedding calls, Chroma ops, query analysis, search, ranking, errors, timings.

Use `tracing` macros only (no `println!`). Thread `request_id` (UUID v4) through all spans for end-to-end tracing. Respect `LOG_LEVEL`.

---

## Token Budget

Estimate: `max(len / 4, words * 1.33)` or use tiktoken-rs if available.

**Always-include caps (hard limits):**
- Single category: ≤ `ALWAYS_INCLUDE_SINGLE_CATEGORY_CAP_PCT`% of `MAX_INJECTED_CONTEXT_TOKENS`.
- All categories: ≤ `ALWAYS_INCLUDE_ALL_CATEGORIES_CAP_PCT`% of `MAX_INJECTED_CONTEXT_TOKENS`.
- If exceeded: inject shortest-fitting chunks (GLOBAL_ALWAYS_ON first, then alphabetical) until cap.

---

## Concurrency

- One lock per category (lazy-created `DashMap`).
- Write ops acquire lock for full operation.
- Read ops attempt acquire with `CATEGORY_LOCK_READ_TIMEOUT_MS` timeout; on timeout, skip and fail open.
- `/admin/reset` acquires all locks before dropping collections.

---

## Fail-Open Scenarios

| Scenario | Behavior |
|---|---|
| Embedding model mismatch | `409` on admin/interception; passthrough unaffected. |
| Ingestion in progress | Retrieval fails open. |
| Category lock timeout | Skip category, fail open. |
| Ollama unreachable | Forward original request unmodified. |
| Embedding/retrieval error | Log ERROR, forward original request. |

---

## Code Quality

- Strict module separation: proxy, retrieval, vectorstore, embedding are decoupled (separate configs/retry policies).
- All public functions documented and instrumented with `#[tracing::instrument]`.
- Core modules (`chunker`, `ranker`, `builder`, `keyword_index`) unit-testable with in-memory fixtures and mocked Chroma (trait-based DI).

---

## Non-Goals

- Web UI, auth, rate-limiting, model whitelist.
- Cloud-hosted vector DB, agentic tool-calling loops.
- Hand-modeled Ollama endpoint structs (only model actual endpoints used).
- Coupling embedding model to chat model selection.

---

## No Hardcoded Constants

All limits, thresholds, batch sizes, retry counts, timeouts, and percentages must be in `.env` (read into `Config` at startup), never inlined as source literals.

Examples: `EMBEDDING_BATCH_SIZE`, `TOP_K`, `SIMILARITY_THRESHOLD`, `CHUNK_SIZE`, `CHUNK_OVERLAP`, `CATEGORY_LOCK_READ_TIMEOUT_MS`, `WATCHER_DEBOUNCE_MS`, `FAILED_EMBEDDING_RETRY_INTERVAL_SECONDS`, `FAILED_EMBEDDING_MAX_ATTEMPTS`, `MAX_RULE_CONTENT_BYTES`, all timeouts.