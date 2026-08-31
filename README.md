# Axum Agentic RAG Proxy

[![Rust](https://img.shields.io/badge/Rust-2021-orange?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Axum](https://img.shields.io/badge/Framework-Axum-blue?style=flat-square&logo=rust)](https://github.com/tokio-rs/axum)
[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg?style=flat-square)](LICENSE)
[![LLM Backends](https://img.shields.io/badge/LLM_Backends-Ollama%20%7C%20LM%20Studio%20%7C%20oMLX-brightgreen?style=flat-square)](https://lmstudio.ai/)
[![Vector DB](https://img.shields.io/badge/Vector_DB-ChromaDB-red?style=flat-square&logo=docker)](https://www.trychroma.com)
[![API Collection](https://img.shields.io/badge/API_Collection-Postman%2FBruno-FF6C37?style=flat-square&logo=postman)](Agentic-RAG-Proxy.collection.json)

A transparent reverse proxy for Ollama, written in Rust with Axum. It sits in front of `http://localhost:11434`, intercepts `/api/chat` and `/api/generate`, injects retrieved rule chunks into the system prompt, and forwards everything else — including every other Ollama endpoint — unmodified.

> **NOTE:**  
> While designed around Ollama endpoints by default, this proxy **is NOT restricted to Ollama**. It can run using [LM Studio](https://lmstudio.ai/), [oMLX](https://omlx.ai/), or **any local LLM server** that supports or mirrors standard inference and embedding routes (`/api/chat`, `/api/generate`, `/api/embed`). Simply adjust `OLLAMA_BASE_URL` and `OLLAMA_EMBEDDING_BASE_URL` in your `.env` to point to your server of choice!

## Features
* **Transparent Ollama proxy**: `/api/chat` and `/api/generate` are intercepted and augmented; all other paths (`/api/tags`, `/api/ps`, `/api/show`, `/api/pull`, etc.) are passed through byte-for-byte via a catch-all handler.
* **Tag-based rule retrieval**: user messages containing `#proxy_ollama:<id_or_category>` (comma-separated) are parsed to pull matching rule chunks by frontmatter `id` or by category name (e.g. `GLOBAL_ALWAYS_ON`, `RUST_AXUM_RULES`).
* **Fail-open by design**: if the request body can't be parsed as a chat/generate payload, it's forwarded unchanged instead of erroring; unresolved identifiers log a warning rather than blocking the request.
* **Token-budgeted context injection**: retrieved chunks are ranked (priority, `always_include`, keyword/metadata scoring) and packed into the system prompt under a configurable token budget, with separate caps for global always-on rules, per-category always-on rules, and freely retrieved chunks.
* **Markdown ruleset ingestion**: rules live as YAML-frontmatter Markdown files under a configurable directory, split into sections/chunks by heading and token size (`CHUNK_SIZE` / `CHUNK_OVERLAP`), with SHA-256 file hashing for change detection.
* **Live filesystem watcher**: the `rulesets/` directory and the category map JSON file are watched (via `notify`/`notify-debouncer-mini`); changes trigger a debounced reindex.
* **Embedding pipeline into ChromaDB**: on reindex, chunks are embedded through Ollama's `/api/embed` (batched, concurrency-limited, retried, in-memory cached by file hash + chunk index) and upserted into per-category ChromaDB collections. Note: the live chat/generate retrieval path (`HybridEngine::retrieve`) selects chunks by tag/category match, not by vector similarity — the vector-similarity retrieval code (`HybridEngine::retrieve_old`) exists in the codebase but is not called by any route.
* **Admin API** for ruleset management: list ruleset chunk counts, write/delete individual `.md` rule files, delete a whole category (with `?confirm=true`), trigger a full reindex, and reset all in-memory + ChromaDB state.
* **`/search` endpoint**: runs the same retrieval path as chat interception against an arbitrary query string, with optional category filter and `top_k` override.
* **Health endpoint** reporting Ollama reachability, ChromaDB reachability, ingestion readiness, and active rule categories.
* **Ready-to-use API Collection**: includes `Agentic-RAG-Proxy.collection.json` for fast endpoint testing via Postman, Bruno, or Insomnia.
* **Filename/category/content-size validation** on all ruleset write/delete admin routes.
* **Structured logging** via `tracing` with file rotation (`tracing-appender`) and configurable retention/level.
* **Independent Ollama backends**: chat/generate traffic and embedding traffic can point at different Ollama instances/models via separate base URLs.
* **Configurable timeouts everywhere**, each independently settable and individually disable-able with `-1`.

## Installation

```bash
git clone <repository-url>
cd axum-ollama-agentic-rag-proxy

# Copy and edit the environment file (see Configuration below)
cp .env .env.local   # or edit .env directly

# Build the project
cargo build --release

# Pull the required Ollama models
ollama pull <your-chat-model>
ollama pull qwen3-embedding:0.6b-q8_0   # or whatever EMBEDDING_MODEL is set to

# Start ChromaDB container using official Docker image
docker run -d \
  --name chroma \
  --restart unless-stopped \
  -p 8001:8000 \
  -v chroma_data:/data \
  -e CHROMA_PERSIST_PATH=/data \
  -e CHROMA_ALLOW_RESET=true \
  chromadb/chroma:latest

```

## Rulesets

Rules are plain Markdown files with YAML frontmatter, organized into categories on disk. The server needs two things to find them:

1. **`RULESETS_DIR`** — the root folder containing one subfolder per category.
2. **`RULESET_MAP_FILE`** — a JSON file mapping each category name to its subfolder (relative to `RULESETS_DIR`).

### Category map

`rulesets.json` (path set by `RULESET_MAP_FILE`):

```json
{
  "GLOBAL_ALWAYS_ON": "global/",
  "RUST_AXUM_RULES": "rust_axum/"
}
```

The key (e.g. `GLOBAL_ALWAYS_ON`) is the category name used in `#proxy_ollama:` tags and in `/search`'s `category` field. The value is the subfolder under `RULESETS_DIR` that's walked recursively for `.md` files.

### Directory layout

With `RULESETS_DIR=./rulesets/` and the map above, the folder structure looks like:

```
rulesets/
├── global/
│   └── rules.md
└── rust_axum/
    └── ...
```

Any `.md` file placed under a mapped category folder (nested subfolders are fine — the loader walks recursively) is picked up on the next index/reindex.

### Ruleset file format

Every `.md` file must start with a YAML frontmatter block (`---` delimited) followed by the rule content in Markdown. Fields map directly to `RulesetFrontmatter`:

| Field | Type | Meaning |
|---|---|---|
| `id` | string | Unique identifier; usable directly in `#proxy_ollama:<id>` |
| `title` | string | Fallback chunk title if a section has no heading |
| `applies_to` | list of strings | Languages/platforms this rule targets (matched against query keywords for ranking) |
| `scope` | list of strings | Free-form scope tags |
| `type` | string | Rule type, defaults to `rule` if omitted |
| `priority` | integer | Higher priority chunks are preferred when ranking and when the token budget is tight |
| `always_include` | bool | If true, this chunk is treated as always-on (subject to the `ALWAYS_INCLUDE_*` budget caps) rather than only retrieved on match |
| `agent_only` | bool | Boosted when the query is detected as agent-mode (keywords like "agent", "tool", "workflow") |
| `examples` | bool | Marks the file as containing examples |
| `tags` | list of strings | Free-form tags used in keyword/metadata scoring |

The body after the closing `---` is split into chunks by `##`-style Markdown sections (falling back to the whole body if there are none), then further split by `CHUNK_SIZE`/`CHUNK_OVERLAP` if a section is too long.

[rulesets/global/rules.md](rulesets/global/rules.md), following this format exactly:

With `always_include: true` and `category: global` (from its `GLOBAL_ALWAYS_ON` folder), this file's chunks are eligible for the always-on budget (`GLOBAL_ALWAYS_ON_RETRIEVED_CAP`) and get pulled in via `#proxy_ollama:GLOBAL_ALWAYS_ON` or by referencing `id: global_development` directly.

Adding or editing files under `RULESETS_DIR` is picked up automatically if `WATCH_RULESETS=true`, or on the next `POST /admin/reload` / server restart with `RELOAD_ON_STARTUP=true`. Files can also be written through the admin API (`POST /admin/rulesets/{category}/{filename}`), which validates the category/filename and triggers a reindex.

## Configuration

All configuration is read from environment variables (loaded via `dotenvy` from `.env` at startup). Every variable below is **required** — the process fails fast at startup if any is missing or unparseable. Timeout variables accept `-1` to disable that timeout entirely.

| Variable | Example | Description |
|---|---|---|
| `HOST` | `0.0.0.0` | Bind address |
| `PORT` | `8000` | Bind port |
| `OLLAMA_BASE_URL` | `http://localhost:11434` | Ollama backend used for `/api/chat`, `/api/generate`, and all passthrough routes |
| `OLLAMA_EMBEDDING_BASE_URL` | `http://10.0.0.11:11434` | Ollama backend used for embeddings; falls back to `OLLAMA_BASE_URL` if unset/empty |
| `EMBEDDING_MODEL` | `qwen-speed-embed` | Model name passed to `/api/embed` |
| `EMBEDDING_BATCH_SIZE` | `16` | Chunks per embedding request |
| `EMBEDDING_MAX_CONCURRENCY` | `1` | Max concurrent embedding requests (semaphore-limited) |
| `EMBEDDING_MAX_RETRIES` | `1` | Retries per failed embedding batch before it's skipped |
| `HTTP_CLIENT_TIMEOUT_SECONDS` | `-1` | Default HTTP client timeout |
| `OLLAMA_CHAT_TIMEOUT_SECONDS` | `-1` | Timeout for `/api/chat` and `/api/generate` forwarding |
| `OLLAMA_EMBEDDING_TIMEOUT_SECONDS` | `-1` | Timeout for embedding requests |
| `STREAMING_IDLE_TIMEOUT_SECONDS` | `-1` | Idle timeout for streamed responses |
| `CHROMA_REQUEST_TIMEOUT_SECONDS` | `-1` | Timeout for ChromaDB requests |
| `FILE_OP_TIMEOUT_SECONDS` | `-1` | Timeout for file operations |
| `BACKGROUND_JOB_TIMEOUT_SECONDS` | `-1` | Timeout for background jobs |
| `HEALTH_CHECK_TIMEOUT_SECONDS` | `20` | Timeout for the Ollama ping in `/admin/health` |
| `CATEGORY_LOCK_READ_TIMEOUT_MS` | `1000` | Timeout (ms) for acquiring a per-category read lock |
| `CHROMA_URL` | `http://10.0.0.11:8001` | ChromaDB server URL |
| `CHROMA_COLLECTION_PREFIX` | `rules_` | Prefix applied to per-category collection names |
| `RULESET_MAP_FILE` | `./rulesets.json` | JSON file mapping category name → ruleset subdirectory |
| `RULESETS_DIR` | `./rulesets/` | Root directory containing category subfolders of `.md` rule files |
| `RELOAD_ON_STARTUP` | `false` | Whether to re-embed and re-upsert all chunks into ChromaDB on startup |
| `WATCH_RULESETS` | `true` | Whether to watch `RULESETS_DIR` and `RULESET_MAP_FILE` for changes |
| `WATCHER_DEBOUNCE_MS` | `500` | Debounce interval for filesystem watch events |
| `CHUNK_SIZE` | `400` | Target chunk size in estimated tokens |
| `CHUNK_OVERLAP` | `50` | Token overlap between adjacent chunks when splitting |
| `TOP_K` | `8` | Default number of results returned by `/search` |
| `SIMILARITY_THRESHOLD` | `0.25` | Minimum similarity score (used by the unused vector-retrieval code path) |
| `MAX_INJECTED_CONTEXT_TOKENS` | `1200` | Total token budget for injected rule context |
| `CONTEXT_RESERVED_TOKENS` | `512` | Tokens reserved (subtracted from the budget) for the rest of the conversation |
| `ALWAYS_INCLUDE_SINGLE_CATEGORY_CAP_PCT` | `30` | Max % of the token budget one category's always-include chunks may consume |
| `ALWAYS_INCLUDE_ALL_CATEGORIES_CAP_PCT` | `60` | Max % of the token budget all always-include chunks combined may consume |
| `GLOBAL_ALWAYS_ON_RETRIEVED_CAP` | `2` | Token cap for global-category always-include chunks |
| `CATEGORY_SELECT_TOP_N` | `2` | Result count requested per category (used by the unused vector-retrieval code path) |
| `FAILED_EMBEDDING_RETRY_INTERVAL_SECONDS` | `300` | Interval for the failed-embedding retry loop |
| `FAILED_EMBEDDING_MAX_ATTEMPTS` | `3` | Max retry attempts (field is loaded; the retry loop itself currently only sleeps and logs) |
| `MAX_RULE_CONTENT_BYTES` | `131072` | Max size of a ruleset file body accepted by the admin write endpoint |
| `LOG_DIR` | `./logs` | Directory for rotated log files |
| `LOG_LEVEL` | `info` | `tracing` log level |
| `LOG_RETENTION_DAYS` | `14` | Log file retention in days |

## Usage

Build and run:

```bash
cargo run --release
```

The server listens on `HOST:PORT` (default `0.0.0.0:8000`) and exposes:

| Method | Path | Description |
|---|---|---|
| `POST` | `/api/chat` | Ollama chat proxy; injects retrieved rule chunks into the system message before forwarding |
| `POST` | `/api/generate` | Ollama generate proxy; injects retrieved rule chunks into `system` before forwarding |
| `GET` | `/admin/health` | Ollama/ChromaDB reachability, ingestion readiness, active categories |
| `GET` | `/admin/rulesets` | Chunk counts per category |
| `POST` | `/admin/rulesets/{category}/{filename}` | Write a `.md` ruleset file (triggers reindex) |
| `DELETE` | `/admin/rulesets/{category}/{filename}` | Remove a ruleset file's chunks from the index; `?delete_from_disk=true` also deletes the file |
| `DELETE` | `/admin/rulesets/{category}` | Delete a category's ChromaDB collection; requires `?confirm=true`, optionally `&delete_from_disk=true` |
| `POST` | `/admin/reload` | Trigger a full reindex |
| `POST` | `/admin/reset?confirm=true` | Clear in-memory index state and delete all ChromaDB collections, then reindex |
| `GET` | `/admin/index-status` | Ingestion readiness, last error, empty categories |
| `POST` | `/search` | Run retrieval against an arbitrary `{"query": "...", "category": "...", "top_k": N}` body |
| `ANY` | `/api/*` (other paths) | Transparent passthrough to `OLLAMA_BASE_URL` |

### API Collection

An API collection file [Agentic-RAG-Proxy.collection.json](Agentic-RAG-Proxy.collection.json) is included in the root directory. You can import this file directly into [Postman](https://www.postman.com/), [Bruno](https://www.usebruno.com/), or [Insomnia](https://insomnia.rest/) to immediately test all chat, generate, search, and admin endpoints.

### Example Request

To retrieve rules in a chat request, tag a user message with `#proxy_ollama:` followed by a comma-separated list of rule frontmatter `id`s and/or category names from `rulesets.json`:

```bash
curl http://localhost:8000/api/chat \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.1",
    "messages": [
      {"role": "user", "content": "#proxy_ollama:GLOBAL_ALWAYS_ON,RUST_AXUM_RULES How do I structure an Axum handler?"}
    ]
  }'
```

Point any Ollama-compatible client (e.g. an IDE's chat integration) at `http://localhost:8000` instead of `http://localhost:11434` to use the proxy transparently for all other Ollama traffic.

## License

Apache License 2.0. See `LICENSE`.