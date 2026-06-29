# TSUYU RAG Chatbot

> Bahasa Malaysia: [README-BM.md](README-BM.md)

Internal RAG (Retrieval-Augmented Generation) chatbot for **TSUYU**. Built so that
**all data stays on-premise** (no external APIs) and interacts primarily in **Bahasa Malaysia**.

User asks a question → system finds the most relevant document excerpts → builds a prompt
with that context → sends to a local LLM (Ollama) → returns an answer with a list of
reference documents.

---

## Contents

- [Architecture](#architecture)
- [Technology stack](#technology-stack)
- [System requirements](#system-requirements)
- [Hardware recommendations](#hardware-recommendations)
- [Installation & setup](#installation--setup)
- [Configuration (.env)](#configuration-env)
- [Running the application](#running-the-application)
- [Authentication](#authentication)
- [API endpoints](#api-endpoints)
- [Common workflow](#common-workflow)
- [Metadata](#metadata)
- [Anti-hallucination guardrails](#anti-hallucination-guardrails)
- [Character Card (persona)](#character-card-persona)
- [Project structure](#project-structure)
- [Tests](#tests)
- [Deploy to Ubuntu (systemd)](#deploy-to-ubuntu-systemd)
- [Troubleshooting](#troubleshooting)
- [Design notes](#design-notes)
- [Related documents](#related-documents)

---

## Related documents

| Document | Audience | Contents |
|---|---|---|
| [MODEL-EN.md](MODEL-EN.md) | Developers / evaluators | Description of each model (Qwen3 14B, bge-m3, reranker, tokenizer) & how to swap |
| [RUNBOOK-EN.md](RUNBOOK-EN.md) | IT staff / operations | Daily operations, logs, monitoring, backup & recovery, failure scenarios |
| [SECURITY-EN.md](SECURITY-EN.md) | ICT security / audit | Threat model, data classification, hardening, data retention (PDPA), incidents |
| [USER-GUIDE-EN.md](USER-GUIDE-EN.md) | End users | How to ask questions, read answers & sources, system limits, FAQ |
| [DOCUMENT-GUIDE-EN.md](DOCUMENT-GUIDE-EN.md) | Document upload staff | Supported formats, PDF/OCR quality, sidecar metadata, how to ingest |
| [LOCAL-DEV-SETUP-EN.md](LOCAL-DEV-SETUP-EN.md) | Developers | CPU-only dev setup (small models) — install steps + validation |
| [CHANGELOG-EN.md](CHANGELOG-EN.md) | Everyone | Change history & features by release |
| [ROADMAP-EN.md](ROADMAP-EN.md) | Developers | Feature status & proposed improvements |

---

## Architecture

```
                    ┌──────────────┐
   User ───────────▶│  Frontend    │  (lightweight htmx, GET /)
                    │  HTML        │
                    └──────┬───────┘
                           │ POST /chat { question }
                           ▼
   ┌───────────────────────────────────────────────────────────┐
   │                  Rust + Axum (API)                        │
   │                                                           │
   │   /chat ─▶ embed question ─▶ HYBRID retrieve N:           │
   │                              vector + keyword → RRF       │
   │                                │                          │
   │                                ▼                          │
   │                          RERANK (top-N → top-k)           │
   │                                │                          │
   │                                ▼                          │
   │                       build prompt → generate             │
   │   /ingest ─▶ read docs ─▶ chunk ─▶ embed (batch) ─▶ DB   │
   │   /health ─▶ ping DB + Ollama + reranker                  │
   └──────┬──────────────────┬─────────────────────┬───────────┘
          │                  │                     │
          ▼                  ▼                     ▼
   ┌───────────────┐  ┌───────────────┐   ┌────────────────┐
   │  PostgreSQL   │  │    Ollama     │   │   Reranker     │
   │ pgvector+tsv  │  │ Qwen3 + bge-m3│   │ bge-reranker   │
   └───────────────┘  └───────────────┘   └────────────────┘
```

**RAG flow (POST /chat):**
1. Generate embedding for the user's question (Ollama `bge-m3`, 1024 dimensions).
2. **Hybrid search** — retrieve `RETRIEVE_N` candidates in parallel:
   - **Vector**: pgvector cosine distance (`<=>`) — semantic relevance.
   - **Keyword**: PostgreSQL full-text (`tsvector`/`ts_rank`) — exact term matching.
   - Combine both rankings with **Reciprocal Rank Fusion (RRF)**.
3. **Rerank** candidates with cross-encoder (`bge-reranker-v2-m3`) → take the best `TOP_K`.
4. Build a prompt injecting the chunk context.
5. Send prompt to the generation model (`qwen3:14b`) via Ollama (thinking mode disabled).
6. Return `{ answer, sources[] }` (token-by-token if using `/chat/stream`).

> **Individually disableable:** `HYBRID_ENABLED=false` (vector only, no keyword) and
> `RERANK_ENABLED=false` (skip rerank step). Both can be disabled for the simplest pipeline
> (vector → generate).

---

## Technology stack

| Layer           | Choice                           | Notes |
|-----------------|----------------------------------|-------|
| API             | Rust + Axum + tokio              | async, systemd service |
| DB driver       | sqlx 0.8 (PostgreSQL)            | connection pool |
| Vector store    | PostgreSQL 16 + pgvector         | single DB for metadata + vectors |
| LLM runtime     | Ollama                           | port 11434 |
| Generation model| `qwen3:14b`                      | Q4_K_M, thinking mode disabled |
| Embedding model | `bge-m3`                         | 1024 dimensions, multilingual |
| Reranker        | `bge-reranker-v2-m3` (TEI/Infinity)| cross-encoder, `/rerank` endpoint |
| Document reading| `pdf-extract`, `zip`, `quick-xml`| PDF / DOCX / TXT / MD |
| Frontend        | HTML + JS (vanilla)              | chat bubbles, streaming, typing indicator |

---

## System requirements

- **Rust** (stable toolchain; project tested on rustc 1.92)
- **PostgreSQL 16** with **pgvector** extension
- **Ollama** with embedding & generation models pulled

> Version note: this project does **not** use the `pgvector` crate. Embeddings are stored
> as text literals `'[...]'::vector` to remain compatible with sqlx 0.8 and rustc 1.92.
> (The latest `pgvector` crate pulls in sqlx 0.9 which requires rustc 1.94.)

---

## Hardware recommendations

The primary determining factor is the **Ollama LLM model** — it consumes the most RAM/VRAM.
The Rust (Axum) application and PostgreSQL are relatively lightweight compared to the models.

### Recommendations by scale

The current stack (Qwen3 14B + bge-m3 + reranker) runs **three models** concurrently,
so VRAM requirements are higher than a minimal stack.

| Scale                          | CPU            | RAM     | GPU (recommended)                   | Model stack | Storage (SSD) |
|-------------------------------|----------------|---------|--------------------------------------|-------------|---------------|
| **Minimum** (testing / demo)  | 8 cores        | 16 GB   | NVIDIA 12 GB VRAM                    | `qwen3:8b` + bge-m3 (rerank off) | 40 GB |
| **Recommended** (small office)| 12 cores       | 32 GB   | NVIDIA 16 GB VRAM                    | `qwen3:14b` + bge-m3 + reranker  | 80 GB |
| **Optimal** (many users)      | 16+ cores      | 64 GB   | NVIDIA 24 GB VRAM (e.g. RTX 4090/A5000) | `qwen3:14b` + bge-m3 + reranker | 150 GB+ |

> **Tip:** without a GPU, models can still run on CPU but responses will be **much slower**
> (especially Qwen3 14B). An NVIDIA GPU with sufficient VRAM gives the biggest speed boost.
> If VRAM is limited, keep **bge-m3** (lightweight, big BM benefit) but downgrade the
> generation model to `qwen3:8b` and consider `RERANK_ENABLED=false`.

### Approximate model memory usage (Q4_K_M quantization)

| Model                  | File size | Min RAM/VRAM at runtime |
|------------------------|-----------|-------------------------|
| `qwen3:8b`             | ~5 GB     | ~7–9 GB                 |
| `qwen3:14b`            | ~9 GB     | ~12–16 GB               |
| `bge-m3` (embedding)   | ~1.2 GB   | ~2 GB                   |
| `bge-reranker-v2-m3`   | ~1.1 GB   | ~2 GB (separate service)|

Required RAM/VRAM = total size of all active models + space for context (KV cache).
For the full stack (Qwen3 14B + bge-m3 + reranker), target **≥16 GB VRAM**.

### GPU notes

- **NVIDIA (CUDA)** is the most mature support for Ollama. Cards like RTX 3060 12GB,
  RTX 4060 Ti 16GB, or RTX 4090 are suitable depending on budget.
- **AMD (ROCm)** is supported on certain GPUs, but compatibility verification is more complex.
- **Apple Silicon (Mac)** works well for development, but TSUYU's production server is expected
  to be Ubuntu — prioritize NVIDIA GPU for production.
- Ensure **VRAM ≥ model size**. If the model is larger than VRAM, Ollama will "offload"
  part to RAM/CPU and become much slower.

### Storage

- **Model storage**: each model is stored in `~/.ollama/models` (see sizes above).
- **Database**: 1024-dimension embeddings (bge-m3) ≈ **4 KB per chunk**. Rough guide:
  ~1 million chunks ≈ a few GB (vectors + text + HNSW index). Original source document
  size is not stored in the DB (only chunk text + metadata).
- Use **SSD/NVMe** for PostgreSQL for good vector search performance.

### Quick summary

> For the full TSUYU stack, a balanced target:
> **12-core CPU, 32 GB RAM, NVIDIA 16 GB VRAM GPU, 80 GB SSD**, running
> `qwen3:14b` + `bge-m3` + `bge-reranker-v2-m3`. If VRAM ≤12 GB, use `qwen3:8b`
> and/or disable the reranker.

---

## Installation & setup

### 1. Install system dependencies

```bash
# PostgreSQL + pgvector
sudo apt update
sudo apt install -y postgresql postgresql-16-pgvector

# Ollama
curl -fsSL https://ollama.com/install.sh | sh
```

### 2. Set up Ollama models

```bash
ollama pull qwen3:14b
ollama pull bge-m3
sudo systemctl status ollama   # Ollama becomes a systemd service after install
```

### 3. Set up reranker service

The reranker is a cross-encoder that is **not** served by Ollama, so it runs as a separate
service with a `/rerank` endpoint (compatible with HuggingFace TEI). Example using Docker
(`text-embeddings-inference`):

```bash
docker run --gpus all -p 8081:80 \
  ghcr.io/huggingface/text-embeddings-inference:latest \
  --model-id BAAI/bge-reranker-v2-m3
```

This service should expose `POST /rerank` with body
`{ "query": "...", "texts": ["...", ...] }`. Set `RERANKER_URL` to its address
(default `http://localhost:8081`).

> No GPU/reranker service? Set `RERANK_ENABLED=false` in `.env` — the system will use
> vector search only (slightly lower quality but still functional).

### 4. Set up database

```bash
sudo -u postgres psql <<'SQL'
CREATE DATABASE tsuyu_rag;
CREATE USER tsuyu WITH PASSWORD 'password';
GRANT ALL PRIVILEGES ON DATABASE tsuyu_rag TO tsuyu;
\c tsuyu_rag
CREATE EXTENSION IF NOT EXISTS vector;
SQL
```

> Schema is managed via **sqlx migrations** in [migrations/](migrations/), run
> **automatically** at startup and tracked in the `_sqlx_migrations` table (idempotent).
> The [schema.sql](schema.sql) file is reference only. To add schema changes, create a
> new migration file (e.g. `migrations/0002_xxx.sql`) — do not modify committed migrations.

### 5. Configure the application

```bash
cp .env.example .env
# Edit .env for your environment (see next section)
```

---

## Configuration (.env)

| Variable          | Required | Default                   | Description |
|-------------------|:--------:|---------------------------|-------------|
| `DATABASE_URL`    | ✅       | —                         | PostgreSQL connection URL |
| `OLLAMA_URL`      | ❌       | `http://localhost:11434`  | Ollama address |
| `GEN_MODEL`       | ❌       | `qwen3:14b`               | Answer generation model |
| `EMBED_MODEL`     | ❌       | `bge-m3`                  | Embedding model |
| `EMBED_DIM`       | ❌       | `1024`                    | Vector dimensions — must match model (bge-m3=1024, nomic=768) |
| `GEN_THINK`       | ❌       | `false`                   | Qwen3 thinking mode: `false`/`true`/`default` |
| `DOCS_DIR`        | ❌       | `./docs`                  | Document folder for ingestion |
| `CHARACTER_CARD_PATH` | ❌  | `character.json`          | Persona JSON file (character card); default if absent |
| `APP_TIMEZONE`    | ❌       | `Asia/Kuala_Lumpur`       | Timezone (IANA) for TIMESTAMPTZ display |
| `BIND_ADDR`       | ❌       | `127.0.0.1:8080`          | Server bind address |
| `API_KEY`         | ❌       | _(empty)_                 | User key: `/chat`, `/chat/stream`, `GET /documents` |
| `ADMIN_API_KEY`   | ❌       | _(empty)_                 | Admin key: `/ingest`, `DELETE /documents/:id`, `DELETE /sessions/:id` |
| `RERANK_ENABLED`  | ❌       | `true`                    | Enable reranking after vector search |
| `RERANKER_URL`    | ❌       | `http://localhost:8081`   | Reranker service address (`/rerank` endpoint) |
| `RERANKER_MODEL`  | ❌       | `bge-reranker-v2-m3`      | Reranker model name |
| `TOP_K`           | ❌       | `5`                       | Final chunk count sent to LLM |
| `RETRIEVE_N`      | ❌       | `30`                      | Candidate count from pgvector before rerank (> `TOP_K`) |
| `HYBRID_ENABLED`  | ❌       | `true`                    | Combine vector + keyword (BM25) search via RRF |
| `RRF_K`           | ❌       | `60`                      | Constant k in Reciprocal Rank Fusion |
| `FTS_CONFIG`      | ❌       | `simple`                  | PostgreSQL full-text config (`simple`/`english`) |
| `MEMORY_ENABLED`  | ❌       | `true`                    | Remember conversation history for requests with `session_id` |
| `MEMORY_TURNS`    | ❌       | `6`                       | Number of recent messages loaded as conversation context |
| `RELEVANCE_ENABLED`| ❌      | `true`                    | Guardrail: reject questions without LLM if context isn't relevant enough |
| `RELEVANCE_MIN_RERANK`| ❌   | `0.0`                     | Minimum reranker score threshold (when rerank enabled) |
| `RELEVANCE_MAX_DISTANCE`| ❌ | `1.0`                    | Maximum cosine distance threshold (when rerank disabled) |
| `CHUNK_TOKENS`    | ❌       | `700`                     | Target size per chunk (actual tokens, BPE) |
| `CHUNK_OVERLAP`   | ❌       | `100`                     | Overlap between chunks (tokens) |
| `EMBED_BATCH_SIZE`| ❌       | `16`                      | Chunks per embedding call during ingest |
| `OLLAMA_MAX_RETRIES`| ❌     | `2`                       | Retry attempts for failed Ollama/reranker calls |
| `OLLAMA_RETRY_BASE_MS`| ❌   | `500`                     | Base backoff duration (ms), doubled each attempt |
| `RATE_LIMIT_RPM`  | ❌       | `120`                     | Requests allowed per IP per minute. 0 = disabled |
| `MAX_BODY_BYTES`  | ❌       | `2097152`                 | Request body size limit (bytes, default 2 MiB) |
| `RUST_LOG`        | ❌       | `info`                    | Log level (e.g. `debug`, `tsuyu_rag_chatbot=debug`) |

Example `.env`:

```env
DATABASE_URL=postgres://tsuyu:password@localhost/tsuyu_rag
OLLAMA_URL=http://localhost:11434
GEN_MODEL=qwen3:14b
EMBED_MODEL=bge-m3
EMBED_DIM=1024
GEN_THINK=false
DOCS_DIR=/opt/tsuyu-rag/docs
BIND_ADDR=127.0.0.1:8080
API_KEY=
ADMIN_API_KEY=
RERANK_ENABLED=true
RERANKER_URL=http://localhost:8081
RERANKER_MODEL=bge-reranker-v2-m3
TOP_K=5
RETRIEVE_N=30
HYBRID_ENABLED=true
RRF_K=60
FTS_CONFIG=simple
MEMORY_ENABLED=true
MEMORY_TURNS=6
RELEVANCE_ENABLED=true
RELEVANCE_MIN_RERANK=0.0
RELEVANCE_MAX_DISTANCE=1.0
CHUNK_TOKENS=700
CHUNK_OVERLAP=100
EMBED_BATCH_SIZE=16
OLLAMA_MAX_RETRIES=2
OLLAMA_RETRY_BASE_MS=500
RATE_LIMIT_RPM=120
MAX_BODY_BYTES=2097152
RUST_LOG=info
```

---

## Running the application

```bash
# Development mode
cargo run

# Optimized build (for deployment)
cargo build --release
./target/release/tsuyu-rag-chatbot
```

> **Build note (compile-time queries):** some queries (documents/memory/sessions/stats)
> use the `sqlx::query!` macro validated at compile time. Builds use the committed
> **`.sqlx/`** cache, so **no DB is needed for normal builds**. If you modify any `query!`
> queries, regenerate the cache: `cargo sqlx prepare` (with `DATABASE_URL` set), then
> commit the `.sqlx/` folder. To force offline mode: `SQLX_OFFLINE=true cargo build`.
> (Install tool: `cargo install sqlx-cli --no-default-features --features postgres`.)

After starting, open a browser to `http://127.0.0.1:8080` for the chat frontend. Each
answer has a **Copy** button (📋); conversations can be **printed** (🖨️) or **exported**
to a `.md`/`.txt` file (questions + answers + references) — all client-side, no data leaves.
The **document management UI** (view list, trigger ingest, delete documents) is at
`http://127.0.0.1:8080/admin` — server-rendered with Askama templates. Enter the
`ADMIN_API_KEY` on that page to authorize ingest/delete actions.

### CLI commands

The same binary supports several commands beyond starting the server. All commands
read the same `.env` config (`DATABASE_URL`, `DOCS_DIR`, etc.) and run once then exit —
**no server or API key needed** — ideal for cron, deploy scripts, and troubleshooting.

```bash
tsuyu-rag-chatbot                  # (default) start HTTP server — same as `serve`
tsuyu-rag-chatbot serve            # start HTTP server explicitly
tsuyu-rag-chatbot ingest           # ingest documents once (incremental)
tsuyu-rag-chatbot ingest --force   # re-ingest all files even if unchanged
tsuyu-rag-chatbot check            # pre-flight check: DB, Ollama, models, reranker
tsuyu-rag-chatbot stats            # DB overview: document/chunk/message counts + size
tsuyu-rag-chatbot prune-sessions --older-than 30   # delete conversation memory > 30 days
tsuyu-rag-chatbot ask "What is the annual leave policy?"   # one-shot RAG query
tsuyu-rag-chatbot --help           # show help
```

| Command | Use | Exit code |
|---|---|---|
| `ingest [--force]` | Same ingest pipeline as `POST /ingest`. Prints summary. | `1` if any files failed |
| `check` | Validate DB + Ollama + models + reranker are reachable (before deploy). | `1` if unhealthy |
| `stats` | Document/chunk/message counts + DB size. Read-only. | `0` |
| `prune-sessions [--older-than N]` | Delete conversation memory > N days (default 90; PDPA policy). | `0` |
| `ask "<question>"` | Full RAG pipeline; print answer + sources. Smoke test. | `0` |

See [DOCUMENT-GUIDE-EN.md](DOCUMENT-GUIDE-EN.md) §5 and [RUNBOOK-EN.md](RUNBOOK-EN.md) for
automated scheduling & operational use.

---

## Authentication

**Two-tier** authentication using API keys via the `Authorization: Bearer <key>` header.

| Role  | Variable      | Endpoints |
|-------|---------------|-----------|
| **User**  | `API_KEY`       | `POST /chat`, `POST /chat/stream`, `GET /documents` |
| **Admin** | `ADMIN_API_KEY` | `POST /ingest`, `DELETE /documents/:id`, `DELETE /sessions/:id` |

Rules:
- The **admin key also grants access** to user endpoints (admin ⊇ user).
- If `ADMIN_API_KEY` is **not set**, admin endpoints **fall back** to `API_KEY`
  (single-key mode — same as earlier versions).
- If `API_KEY` is **empty**, user authentication is disabled; if both are empty, all
  endpoints are open (local development) — a warning is logged at startup.
- `GET /health`, frontend `GET /`, and UI page `GET /admin` are always **open** —
  but `/admin` is just a page shell; its data & actions
  (`/admin/documents`, `/admin/ingest`, …) still require a valid key.
- Wrong/missing key → **401 Unauthorized**.

Example:

```bash
# Regular user can ask questions
curl -X POST http://localhost:8080/chat \
  -H 'Authorization: Bearer <API_KEY>' \
  -H 'Content-Type: application/json' \
  -d '{ "question": "Question?" }'

# Only admin can trigger ingest / delete
curl -X POST http://localhost:8080/ingest \
  -H 'Authorization: Bearer <ADMIN_API_KEY>'
```

**Frontend**: an "API key" field is provided on the main page; the value is stored in
browser `localStorage` and sent automatically with every request. (For admin operations
via UI, enter the `ADMIN_API_KEY` in that field.)

> **Best practices:**
> - Use long & random keys (e.g. `openssl rand -hex 32`), different for user vs admin.
> - Send over HTTPS only (put Nginx/TLS in front — see deploy section).
> - Key comparison is done in **constant time** to prevent timing attacks.
> - For full per-user or SSO, this can be upgraded in the future.

---

## API endpoints

> If keys are set, the `curl` examples below need the header
> `-H 'Authorization: Bearer <key>'` — use `ADMIN_API_KEY` for admin endpoints
> (`/ingest`, `DELETE …`) and `API_KEY` for the rest.

### `GET /health`
Check health of DB, Ollama, model availability, and reranker (if enabled).

```bash
curl http://localhost:8080/health
```
```json
{
  "status": "ok",
  "database": true,
  "ollama": true,
  "reranker": true,
  "models": { "gen": true, "embed": true }
}
```
Returns **200** if all relevant components are up, **503** if any fail
(`status: "degraded"`). Fields:
- `models.gen` / `models.embed` — whether `GEN_MODEL` / `EMBED_MODEL` actually exist
  in Ollama (checked from `/api/tags`; matching ignores `:latest` tag). If `false`, run
  `ollama pull <model>`.
- `reranker` — absent if `RERANK_ENABLED=false`.

---

### `GET /metrics`
Metrics in **Prometheus** text format (open, for internal scraping).

```bash
curl http://localhost:8080/metrics
```
```
tsuyu_chat_requests_total 128
tsuyu_chat_errors_total 3
tsuyu_ingest_runs_total 5
tsuyu_retrieval_duration_ms_sum 9100
tsuyu_retrieval_duration_ms_count 125
tsuyu_generate_duration_ms_sum 412000
tsuyu_generate_duration_ms_count 125
```
Average time computed in Prometheus: `…_duration_ms_sum / …_duration_ms_count`. Suitable
for connecting to Grafana for retrieval/generation latency dashboards & error rate tracking.

---

### `POST /ingest`
Trigger ingestion of all supported files (PDF, DOCX, TXT, MD) in `DOCS_DIR`
**recursively** (including subfolders; hidden `.` folders are skipped).
Runs as a **background task** — response returns immediately; check logs for progress.

```bash
curl -X POST http://localhost:8080/ingest
```
```json
{ "status": "accepted", "message": "Ingest started in background. Unchanged files will be skipped. Check logs for progress." }
```

**Incremental ingest:** by default, files **unchanged** since the last ingest are
**skipped** without being read or re-embedded. Change detection is based on
**file size + modification time (mtime)** stored in the `documents` table
(`size_bytes`, `mtime_unix`). This makes repeated ingest very fast when only a small
fraction of documents change.

To **force** re-ingest of all files (e.g. after changing embedding model or chunk size),
use `?force=true`:

```bash
curl -X POST 'http://localhost:8080/ingest?force=true'
```

Logs will show a summary, e.g.:
```
ingest complete: 3 documents, 57 chunks, 12 unchanged, 0 skipped (errors)
```

> Re-ingesting a changed file is safe: old chunks for that document are deleted first
> (keyed on file path), so there are no duplicates.

**Batch embedding:** during ingest, chunks are processed in batches (`EMBED_BATCH_SIZE`,
default 16) — each batch generates its embeddings in **one** call to Ollama's `/api/embed`
endpoint, and is inserted into the DB with **one** multi-row INSERT. This drastically
reduces HTTP round-trips and DB overhead compared to processing chunks one by one. Increase
`EMBED_BATCH_SIZE` for faster ingest if RAM/Ollama allows; decrease if large batches timeout.

> Requires an Ollama version supporting the `/api/embed` endpoint (most current installs).
> The question path (`/chat`) still uses `/api/embeddings` for single embeddings.

---

### `POST /chat`
Ask a question and get an answer + references.

```bash
curl -X POST http://localhost:8080/chat \
  -H 'Content-Type: application/json' \
  -d '{ "question": "How many days of annual leave does staff get?" }'
```
```json
{
  "answer": "According to the documents, annual leave is 20 days ...",
  "sources": [
    {
      "document_id": 3, "filename": "leave-policy.pdf", "chunk_index": 5,
      "page": 4,
      "snippet": "Permanent staff are entitled to 20 days annual leave per year …",
      "distance": 0.18,
      "meta": { "category": "hr", "department": "Human Resources", "year": 2024, "security": "internal" }
    }
  ]
}
```
`sources[]` fields:
- `page` — page number (1-based) for PDFs; null for TXT/MD/DOCX.
- `snippet` — brief excerpt of chunk content (whitespace compressed, ~240 characters).
- `distance` — cosine distance, **smaller** = more relevant.
- `meta` — source document metadata (see [Metadata](#metadata)).

**Conversation memory (optional):** include `session_id` to enable follow-up questions.
The system will load the last few turns of that session as context, and save the new turn
after answering.

```bash
# First question
curl -X POST http://localhost:8080/chat -H 'Content-Type: application/json' \
  -d '{ "question": "How many days annual leave?", "session_id": "session-ali-123" }'
# Follow-up — understands "contract" refers to annual leave
curl -X POST http://localhost:8080/chat -H 'Content-Type: application/json' \
  -d '{ "question": "What about for contract staff?", "session_id": "session-ali-123" }'
```

> Without `session_id`, each question is handled without past context. Memory can be
> disabled globally with `MEMORY_ENABLED=false`.

**Metadata filtering (optional):** restrict search to matching documents. Only set fields
filter; others are ignored.

```bash
curl -X POST http://localhost:8080/chat -H 'Content-Type: application/json' \
  -d '{
        "question": "What are the procurement requirements?",
        "filter": { "category": "procurement", "year": 2024 }
      }'
```

---

### `POST /chat/stream`
Same as `/chat`, but **streams the answer token by token** using
Server-Sent Events (SSE) — the answer displays as the model generates it, without
waiting for the full response. This is the endpoint used by the frontend.

```bash
curl -N -X POST http://localhost:8080/chat/stream \
  -H 'Content-Type: application/json' \
  -d '{ "question": "How many days of annual leave does staff get?" }'
```

SSE event sequence returned:

| Event     | `data`                          | Description |
|-----------|---------------------------------|-------------|
| `sources` | JSON array `Source[]`           | Sent first, as soon as retrieval completes |
| `token`   | JSON-quoted string, e.g. `"cu"` | Many events; each is one text chunk |
| `done`    | `[DONE]`                        | End marker |
| `error`   | JSON-quoted string              | If an error occurs during generation |

Raw stream example:
```
event: sources
data: [{"document_id":3,"filename":"leave-policy.pdf","chunk_index":5,"distance":0.18}]

event: token
data: "According to "

event: token
data: "the documents, leave "

event: done
data: [DONE]
```

> **Client note:** `token` (and `error`) are JSON-quoted so newline characters in
> answers are safely sent in a single `data:` field. Clients must `JSON.parse()` the
> value before displaying (see [static/index.html](static/index.html)).
> Since a request body is needed (POST), use `fetch()` + stream reader,
> not native `EventSource` (which only supports GET).

---

### `GET /documents`
List all ingested documents, with chunk count for each.

```bash
curl http://localhost:8080/documents \
  -H 'Authorization: Bearer <API_KEY>'   # if API_KEY is set
```
```json
{
  "count": 2,
  "documents": [
    {
      "id": 3,
      "filename": "leave-policy.pdf",
      "path": "/opt/tsuyu-rag/docs/leave-policy.pdf",
      "size_bytes": 184320,
      "mtime_unix": 1748764800,
      "chunk_count": 12,
      "ingested_at": "2026-06-01 08:09:00+00"
    }
  ]
}
```

---

### `DELETE /documents/:id`
Delete one document and **all its chunks** (via `ON DELETE CASCADE`).

```bash
curl -X DELETE http://localhost:8080/documents/3 \
  -H 'Authorization: Bearer <API_KEY>'   # if API_KEY is set
```
```json
{ "deleted": true, "id": 3 }
```
Returns **404 Not Found** if a document with that id doesn't exist.

> To remove a document from the system entirely, also delete the original file from
> `DOCS_DIR` — otherwise the next ingest will re-add it.

---

### `DELETE /sessions/:id`
Clear conversation memory for one session (delete all messages for that session).

```bash
curl -X DELETE http://localhost:8080/sessions/session-ali-123 \
  -H 'Authorization: Bearer <API_KEY>'   # if API_KEY is set
```
```json
{ "cleared": true, "messages_deleted": 4 }
```

---

### Document management UI (`GET /admin`)

A server-rendered web page (**Askama** templates) to simplify ingest without `curl`:
view document list + metadata + chunk counts, trigger ingest (normal/force), and delete
documents. Page is open (like `/`); ingest/delete actions send the `ADMIN_API_KEY`
entered on the page as an `Authorization: Bearer` header.

Supporting paths returning HTML (used by the page itself):
`GET /admin/documents` (table fragment), `POST /admin/ingest`, `DELETE /admin/documents/:id`.
JSON endpoints `/documents`, `/ingest` remain for automation.

---

## Common workflow

```bash
# 1. Ensure infra is up
curl http://localhost:8080/health

# 2. Place documents in DOCS_DIR
cp ~/tsuyu-docs/*.pdf /opt/tsuyu-rag/docs/

# 3. Ingest
curl -X POST http://localhost:8080/ingest
#    (monitor: journalctl -u tsuyu-rag -f  OR  cargo run terminal log)

# 4. Chat
curl -X POST http://localhost:8080/chat \
  -H 'Content-Type: application/json' \
  -d '{ "question": "Your question here" }'
```

---

## Anti-hallucination guardrails

To ensure the LLM **only answers from TSUYU documents** (and doesn't fabricate answers),
the system uses several layers of defense:

1. **Relevance threshold (pre-LLM)** — the strongest layer. Before calling the LLM, the system
   checks the best context's relevance score:
   - When reranker is on: best chunk `rerank_score` must be ≥ `RELEVANCE_MIN_RERANK`.
   - When reranker is off: smallest cosine distance must be ≤ `RELEVANCE_MAX_DISTANCE`.
   - If it fails → directly return **"information not found in TSUYU documents"** without
     calling Ollama. This blocks hallucinations **deterministically** (model gets no chance
     to answer) and saves time/resources.
2. **Prompt grounding** — firm instructions: *"Use ONLY information in the CONTEXT… If none,
   say not found. Do not fabricate answers."*
3. **Data vs instructions separation** — see [prompt injection mitigation](#design-notes).

**Tuning thresholds:**
- Too many valid questions rejected? → **loosen**: lower `RELEVANCE_MIN_RERANK`
  (e.g. `-2.0`) or raise `RELEVANCE_MAX_DISTANCE` (e.g. `1.3`).
- LLM still answers outside documents? → **tighten**: raise `RELEVANCE_MIN_RERANK`
  (e.g. `1.0`) or lower `RELEVANCE_MAX_DISTANCE` (e.g. `0.7`).
- Disable entirely with `RELEVANCE_ENABLED=false` (LLM always called; relies on prompt
  grounding only).

> **Calibration tip:** observe the `distance` field in `sources[]` responses from `/chat` for
> questions you *know* are answered correctly, then set the threshold slightly looser than
> that value. With the reranker, scores are more meaningful than raw cosine distance.

---

## Metadata

Each document can include **metadata** via a **sidecar** file named
`<document-name>.meta.json` in the same folder. For example, for `leave-policy.pdf`,
create `leave-policy.pdf.meta.json`:

```json
{
  "category": "hr",
  "department": "Human Resources Division",
  "year": 2024,
  "security": "internal"
}
```

| Field        | Type   | Example | Use |
|--------------|--------|---------|-----|
| `category`   | text   | `contract`, `policy`, `procurement`, `hr` | Document type |
| `department` | text   | `Human Resources Division` | Owning department |
| `year`       | number | `2024` | Document year |
| `security`   | text   | `public`, `internal`, `confidential` | Security level |

- **All fields optional** — documents without a sidecar are still ingested (empty metadata).
- Metadata is stored at the **document** level (shared by all its chunks).
- Metadata is returned in `sources[].meta` in every answer, and can **filter**
  searches via the `filter` field in `/chat` (see [POST /chat](#post-chat)).
- **Updating a sidecar triggers re-ingest**: incremental ingest checks the sidecar's
  mtime, so updating only the `.meta.json` is enough for new metadata to take effect
  (no need for `?force=true`).

---

## Character Card (persona)

The assistant persona can be **tuned by an admin** without changing code — name, role,
tone, language, answer length, emoji, and special rules. It is injected into the system prompt.

Stored as a JSON file at `CHARACTER_CARD_PATH` (default `character.json`). If the file
is absent, a sensible default persona is used. Example:

```json
{
  "name": "Ayu",
  "role": "TSUYU officer assistant",
  "tone": "Formal but friendly",
  "language": "Bahasa Malaysia",
  "verbosity": "medium",
  "emoji": false,
  "special_rules": [
    "Always use official government terminology",
    "Provide document references when available"
  ]
}
```

- `verbosity`: `short` | `medium` | `long`. Missing fields use defaults.
- **How to edit:** UI `/admin` (Character Card section) — changes take **effect immediately**
  for subsequent questions; or edit the file directly & restart.
- **API:** `GET /admin/character` (read), `PUT /admin/character` (update, admin auth).
- **Security:** persona is trusted admin input, but hard rules
  (answer from context only + anti-injection) are chained **after** the persona in the prompt,
  so the character card cannot weaken guardrails.

---

## Project structure

```
tsuyu-rag-chatbot/
├── Cargo.toml
├── .env.example
├── migrations/                 # sqlx schema migrations (actual schema source)
│   └── 0001_initial.sql
├── .sqlx/                      # compile-time query cache (committed; offline build without DB)
├── schema.sql                  # schema snapshot (reference only)
├── README.md
├── deploy/
│   └── tsuyu-rag.service        # example systemd unit
├── templates/                 # Askama templates (admin UI, server-rendered)
│   ├── admin.html
│   └── documents_table.html
├── static/
│   └── index.html              # lightweight chat frontend
├── tests/
│   └── integration.rs          # DB integration tests via lib crate (gated by TEST_DATABASE_URL)
└── src/
    ├── main.rs                 # THIN entry point: #[tokio::main] → tsuyu_rag_chatbot::run()
    ├── lib.rs                  # actual entry point: run(), arg parsing, setup, command dispatch
    ├── cli.rs                  # CLI commands: ingest/check/stats/prune-sessions/ask
    ├── config.rs               # read config from environment
    ├── auth.rs                 # API key auth middleware (Bearer)
    ├── ratelimit.rs            # per-IP rate limiting middleware (+ unit tests)
    ├── metrics.rs              # atomic counters + Prometheus render (+ unit tests)
    ├── error.rs                # AppError → HTTP response (no unwraps)
    ├── state.rs                # shared AppState (config, pool, http client)
    ├── db.rs                   # pool, run_migrations + reconcile dim/fts, vector_literal()
    ├── models.rs               # request/response structs
    ├── handlers/
    │   ├── mod.rs              # router (open + protected) + frontend
    │   ├── health.rs           # GET /health (DB + Ollama + reranker)
    │   ├── metrics.rs          # GET /metrics (Prometheus)
    │   ├── ingest.rs           # POST /ingest
    │   ├── chat.rs             # POST /chat, POST /chat/stream (retrieve→rerank→generate)
    │   ├── documents.rs        # GET /documents, DELETE /documents/:id
    │   ├── admin.rs            # document management UI (Askama): GET /admin + HTML actions
    │   └── sessions.rs         # DELETE /sessions/:id (clear memory)
    └── services/
        ├── mod.rs
        ├── character.rs        # character card (persona) — admin-tunable (+ unit tests)
        ├── chunk.rs            # BPE token-based text splitting (+ unit tests)
        ├── embed.rs            # Ollama embedding calls (single + batch)
        ├── retrieve.rs         # vector + keyword + hybrid RRF search (+ unit tests)
        ├── rerank.rs           # cross-encoder reranking (external service)
        ├── retry.rs            # retry + backoff for Ollama/reranker calls
        ├── generate.rs         # prompt building + generation + <think> filter (+ unit tests)
        ├── ingest.rs           # ingest pipeline (read → chunk → embed → save)
        ├── memory.rs           # conversation memory (load/save session)
        ├── metadata.rs         # read .meta.json sidecar (+ unit tests)
        └── documents.rs        # list & delete documents
```

---

## Tests

```bash
cargo test
```

**Unit tests** (no external dependencies) cover:
- **Chunking logic** ([src/services/chunk.rs](src/services/chunk.rs)) — empty text,
  single chunk, long text splitting, token overlap, and overlap limits.
- **Prompt building + thinking filter** ([src/services/generate.rs](src/services/generate.rs)).
- **Hybrid RRF fusion** ([src/services/retrieve.rs](src/services/retrieve.rs)).
- **Brief snippet** ([src/handlers/chat.rs](src/handlers/chat.rs)).
- **Rate limiting** ([src/ratelimit.rs](src/ratelimit.rs)) & **model matching** ([src/handlers/health.rs](src/handlers/health.rs)).

**Integration tests** ([tests/integration.rs](tests/integration.rs)) — a separate test
crate that accesses the API via the **lib crate** (`tsuyu_rag_chatbot`), requires real
PostgreSQL + pgvector, **gated** by `TEST_DATABASE_URL`. Without that env var, tests are
cleanly skipped (not failed). To run them:

```bash
# Set up test DB (once) with pgvector:
createdb tsuyu_rag_test && psql -d tsuyu_rag_test -c 'CREATE EXTENSION IF NOT EXISTS vector'

# Run integration tests (MUST be serial — each test clears shared tables):
TEST_DATABASE_URL=postgres://tsuyu:password@localhost/tsuyu_rag_test \
    cargo test --test integration -- --test-threads=1
```

> Structure: `src/main.rs` is a thin wrapper (`#[tokio::main]` → `run()`); all logic
> lives in `src/lib.rs` + submodules, so integration tests can access it via the library.
> See [Project structure](#project-structure).

Integration coverage: session memory (save/load/limit/clear), document management
(list/delete/chunk cascade), and schema idempotency. The test schema is cleared before
each case, so use a **separate database** (not the production DB).

---

## Deploy to Ubuntu (systemd)

Ubuntu uses **systemd** (not NSSM). Example unit file: [deploy/tsuyu-rag.service](deploy/tsuyu-rag.service).

```bash
# 1. Build & place binary
cargo build --release
sudo mkdir -p /opt/tsuyu-rag/docs
sudo cp target/release/tsuyu-rag-chatbot /opt/tsuyu-rag/
sudo cp .env /opt/tsuyu-rag/.env

# 2. Install systemd unit
sudo cp deploy/tsuyu-rag.service /etc/systemd/system/tsuyu-rag.service
sudo systemctl daemon-reload
sudo systemctl enable --now tsuyu-rag
sudo systemctl status tsuyu-rag

# 3. View logs
journalctl -u tsuyu-rag -f
```

> **Important:** the systemd service **does not** inherit the shell environment. The unit
> uses `EnvironmentFile=/opt/tsuyu-rag/.env` to load config from an absolute path.

### Reranker service

If `RERANK_ENABLED=true`, the reranker service must be up before the application can answer.
The easiest way is Docker (see setup section). Ensure it auto-starts — either via
`--restart unless-stopped` on the Docker container, or its own systemd unit.
If you run the reranker on a different machine/port, update `RERANKER_URL` in `.env`.

### (Optional) Reverse proxy + internal security

Put Nginx in front for TLS, and restrict access with `ufw` so only TSUYU's internal
subnet can connect. `proxy_pass` to `127.0.0.1:8080`.

### Windows → Ubuntu concept mapping

| Windows (existing)        | Ubuntu (new)                   |
|---------------------------|--------------------------------|
| NSSM service              | systemd unit                   |
| `nssm set AppDirectory`   | `WorkingDirectory=` in unit    |
| `.env` via dotenv path    | `EnvironmentFile=` in unit     |
| `tsuyu-log` (tail log)    | `journalctl -u tsuyu-rag -f`   |
| manual service restart    | `Restart=on-failure` (auto)    |

---

## Troubleshooting

| Symptom | Possible cause & action |
|---------|------------------------|
| `/health` returns `database: false` | PostgreSQL not running, wrong `DATABASE_URL`, or `vector` extension not installed. |
| `/health` returns `ollama: false` | Ollama not running (`systemctl status ollama`) or wrong `OLLAMA_URL`. |
| `/health` returns `reranker: false` | Reranker service not running or wrong `RERANKER_URL`. Disable with `RERANK_ENABLED=false` if not in use. |
| `/health` returns `models.gen`/`models.embed`: false | Model not pulled. Run `ollama pull <GEN_MODEL>` / `ollama pull <EMBED_MODEL>`. |
| Ingest skips all files | Check `DOCS_DIR` is correct & has `.pdf/.docx/.txt/.md` files. Check `journalctl` logs. |
| Empty answers / "information not found" | Documents not ingested, or question is out of scope of documents. |
| Vector dimension error | Ensure `EMBED_DIM` matches model (bge-m3=1024). After changing model/dimension, run `POST /ingest?force=true`. |
| Answer contains `<think>` text | Set `GEN_THINK=false` (default). The filter also strips these blocks automatically. |
| "Empty" embeddings from Ollama | Embedding model not pulled or wrong model name. |

For more detailed logs:
```bash
RUST_LOG=tsuyu_rag_chatbot=debug cargo run
```

---

## Design notes

- **No `unwrap()`/`expect()`** in production code — all errors handled via
  `?`, `match`, and `AppError` types (thiserror for domain, anyhow for upper layers).
- **All I/O async** (tokio): DB, HTTP to Ollama, and file reads. Blocking operations
  (PDF/DOCX reading) run in `spawn_blocking`.
- **Runtime sqlx queries** (not compile-time `query!` macro) so it can compile without
  a live DB. Can be switched to compile-time macros if strict checking is wanted.
- **Real-token chunking**: chunk size counted in BPE tokens (`cl100k_base` via
  tiktoken-rs), not word count — more consistent with model context limits and more
  accurate for mixed text (BM, punctuation). Tokenizer is embedded in the binary (no
  external files) and loaded once into `AppState`. See [src/services/chunk.rs](src/services/chunk.rs).
- **Rich citations**: each chunk records page number (PDF extracted per-page
  via `extract_text_by_pages`); references in `sources[]` include `page` + actual `snippet`
  text for verifiability. Non-page formats (TXT/MD/DOCX) → `page` null.
- **Idempotent ingest**: documents identified by file path (`UNIQUE`); re-ingest
  deletes old chunks in one transaction before inserting new ones.
- **Batch ingest**: embeddings & INSERTs done in batches (`EMBED_BATCH_SIZE`)
  to reduce HTTP round-trips to Ollama and DB overhead.
- **Incremental ingest**: unchanged files skipped based on size + mtime
  (`size_bytes`, `mtime_unix`); use `?force=true` to force re-ingest.
- **Recursive ingest**: `DOCS_DIR` traversed including subfolders (explicit stack, not
  async recursion); hidden folders skipped. See `list_supported_files`.
- **Retry + backoff for Ollama**: calls to Ollama & reranker are retried for transient
  errors (timeout/connect/5xx/429) with exponential backoff (`OLLAMA_MAX_RETRIES`,
  `OLLAMA_RETRY_BASE_MS`); 4xx errors are not retried. For `/chat/stream`, only the
  initial call is retried. See [src/services/retry.rs](src/services/retry.rs).
- **Two-tier auth**: API key middleware (`Authorization: Bearer`) with user role
  (`API_KEY`) vs admin (`ADMIN_API_KEY`); admin ⊇ user; admin falls back to
  `API_KEY` if not set. Constant-time comparison; `/health` & frontend are open.
  See [src/auth.rs](src/auth.rs).
- **Rate limiting & body size**: fixed-window per-IP middleware (`RATE_LIMIT_RPM`, → 429 if
  exceeded) + `DefaultBodyLimit` (`MAX_BODY_BYTES`). No external dependency; fail-safe
  (allow) if the key is poisoned. See [src/ratelimit.rs](src/ratelimit.rs).
- **Graceful shutdown**: server handles **SIGTERM** (systemd) & **Ctrl-C**, completing
  in-flight requests before exiting (`axum ... with_graceful_shutdown`). See `main::shutdown_signal`.
- **Prompt injection mitigation**: untrusted input (document content, questions, history)
  is sanitized — fake delimiter lines (`=== ... ===`) are stripped so they can't spoof
  prompt structure — and system instructions assert content is **DATA, not instructions**.
  See `generate::sanitize_untrusted` (+ tests).
- **Relevance guardrail (anti-hallucination)**: before calling the LLM, system checks whether
  retrieved context is relevant enough — using reranker score (`RELEVANCE_MIN_RERANK`) or cosine
  distance (`RELEVANCE_MAX_DISTANCE`). If below threshold, immediately returns "not found in
  TSUYU documents" **without calling LLM** — deterministically prevents out-of-context answers.
  Combined with prompt instruction "use ONLY context". See `chat::nilai_relevan` (+ tests)
  & [Anti-hallucination guardrails](#anti-hallucination-guardrails).
- **Metrics & observability**: atomic counters (`AtomicU64`) in `AppState` instrumented on
  the chat/ingest pipeline (counts, errors, retrieval/generation times); exposed as
  Prometheus text at `GET /metrics` — no Prometheus crate. See [src/metrics.rs](src/metrics.rs).
- **Document management**: `GET /documents` (list + chunk count) and
  `DELETE /documents/:id` (delete + cascade chunks). Frontend has a simple panel for this.
- **Embeddings as text literals** `'[...]'::vector` — see `db::vector_literal()`.
- **Hybrid search**: combines vector (pgvector) + keyword (`tsvector`/GIN, auto-generated)
  search using **Reciprocal Rank Fusion** — no separate BM25 engine, stays single DB.
  RRF logic is a pure function with tests. See [src/services/retrieve.rs](src/services/retrieve.rs).
- **Two-stage reranking**: broad retrieval (`RETRIEVE_N`) → cross-encoder
  (`bge-reranker-v2-m3`) → best `TOP_K`. Reranker is an external service (Ollama has no
  rerank endpoint); can be disabled. See [src/services/rerank.rs](src/services/rerank.rs).
- **Conversation memory**: session history stored in the `messages` PostgreSQL table (same DB).
  Requests with `session_id` load the last `MEMORY_TURNS` turns into the prompt,
  enabling follow-up questions. See [src/services/memory.rs](src/services/memory.rs).
- **Sidecar metadata**: document metadata read from `<document>.meta.json` files, stored
  in the `documents` table, and filters searches via the SQL pattern
  `($n IS NULL OR col = $n)` (no dynamic SQL). See [src/services/metadata.rs](src/services/metadata.rs)
  & [Metadata](#metadata).
- **Structured DB migrations**: schema managed by embedded sqlx migrations ([migrations/](migrations/)),
  tracked in `_sqlx_migrations`. Hybrid: static migrations for core structure +
  runtime reconciliation for non-default `EMBED_DIM`/`FTS_CONFIG` (since static migrations
  can't take parameters). See `db::run_migrations`.
- **Configurable embedding dimensions & FTS** (`EMBED_DIM`/`FTS_CONFIG`): schema is
  reconciled automatically after migration; changing dimension clears old chunks (re-ingest needed).
  See `db::reconcile_embedding_dim` & `db::reconcile_fts_config`.
- **Thinking filter**: Qwen3's `<think>...</think>` blocks are stripped from answers (including
  during streaming, across token boundaries). See `generate::strip_thinking` & `ThinkFilter`.
