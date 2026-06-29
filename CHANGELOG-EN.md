# CHANGELOG — TSUYU RAG Chatbot

All notable changes to this project are recorded here. Format based on
[Keep a Changelog](https://keepachangelog.com/).

Change types: **Added** (new features), **Changed** (existing behavior), **Fixed** (bug fixes),
**Security** (security-related).

---

## [Unreleased]

- **Security** Config secrets (`DATABASE_URL`, `API_KEY`, `ADMIN_API_KEY`) wrapped in
  `secrecy::SecretString` — not printed by `Debug`, memory zeroed when dropped, and
  accessed only via `.expose_secret()` (at boundaries: `init_pool` & constant-time auth
  key comparison). Reduces risk of secret leakage via logs/dumps.
- **Fixed** Timezone — connection pool now sets session timezone (via parameterized
  `set_config`) on every connection, so `TIMESTAMPTZ` (`ingested_at`, `created_at`)
  displays in the chosen zone regardless of server OS/PG configuration (production
  servers are typically UTC). Values stored as UTC internally. Export timestamps on
  the frontend are "when exported" (user browser local time).
- **Added** Configurable timezone — `APP_TIMEZONE` (IANA zone name, default
  `Asia/Kuala_Lumpur`). Applied to DB session and verified switchable (e.g. `UTC`).
- **Added** Character Card — admin-tunable assistant persona
  (name, role, tone, language, answer length, emoji, special rules). Stored as
  JSON file (`CHARACTER_CARD_PATH`), injected into the *system prompt*, and editable
  via the `/admin` UI (takes effect immediately without restart via `RwLock`).
  Endpoints: `GET /admin/character` (user), `PUT /admin/character` (admin). Hard
  anti-hallucination/anti-injection rules are chained AFTER the persona so they
  cannot be overridden. Sensible defaults used if file is absent.
- **Fixed** Conversation memory ordering — `load_recent` now sorts by `id` (BIGSERIAL),
  not `created_at`. Both messages in one turn are saved in a single transaction &
  share the same `now()`, so `created_at` cannot guarantee user→assistant ordering.
  Exposed by real DB integration tests (first run).
- **Added** Partial compile-time SQL checks _(#22)_ — safe CRUD queries
  (documents, memory, sessions, stats) converted to `sqlx::query!` macros (SQL + types
  validated against schema at build time). Offline cache `.sqlx/` generated
  (`cargo sqlx prepare`) & committed so `cargo build` doesn't require a DB. Retrieval
  queries (vector/keyword/hybrid) remain runtime `query()` because they're dynamically
  built + use `vector`/`tsvector` columns. Offline build: `SQLX_OFFLINE=true cargo build`.
- **Changed** Project structure to library + binary: `src/main.rs` is now a thin
  wrapper (`#[tokio::main]` → `tsuyu_rag_chatbot::run()`); all logic (arg parsing,
  setup, router, command dispatch) moved to `src/lib.rs` with `pub async fn run()`.
  Modules made `pub` so integration tests can access the API surface via the lib crate.
- **Changed** DB integration tests moved from in-source `#[cfg(test)]` modules
  (`src/testutil.rs`, `src/integration_tests.rs`) to a proper test crate
  [tests/integration.rs](tests/integration.rs) using `use tsuyu_rag_chatbot::…`. Run
  with `cargo test --test integration` (still gated by `TEST_DATABASE_URL`).
- **Added** Answer export/print on chat frontend — **Copy** button (📋) on each answer,
  **Print** (🖨️, with clean print CSS), and **Export** conversation to `.md` or `.txt`
  file (questions + answers + references). All client-side (JavaScript) —
  no backend changes, no data leaves.
- **Added** Document management UI at `GET /admin` — server-rendered with **Askama**
  templates (new `askama` dependency, embedded in binary, no external files/CDN).
  Shows document list + metadata + chunk counts, triggers ingest (normal/force),
  and deletes documents. Page is open; actions send `ADMIN_API_KEY` as Bearer header.
  HTML support paths: `GET /admin/documents`, `POST /admin/ingest`,
  `DELETE /admin/documents/:id`. Link added on main chat page.
- **Added** CLI commands — binary now supports several commands beyond `serve`
  (default), all reading the same `.env` & running once without server/API key:
  - `ingest [--force]` — same ingest pipeline as `POST /ingest`; prints summary,
    non-zero exit code if any files failed.
  - `check` — pre-flight check (DB, Ollama, models, reranker); reuses `/health` logic.
    Non-zero exit code if unhealthy.
  - `stats` — document/chunk/message counts + DB size.
  - `prune-sessions [--older-than N]` — delete conversation memory > N days (default 90;
    enforces PDPA retention).
  - `ask "<question>"` — one-shot RAG query; prints answer + sources.
  - `--help` shows usage.
- **Added** `tsuyu-rag-ingest.service` + `.timer` systemd units for scheduled ingest
  (see [deploy/](deploy/) & [RUNBOOK-EN.md](RUNBOOK-EN.md) §4b).
- **Changed** `/health` logic extracted to `gather_health` so it's shared between the
  HTTP handler & the `check` CLI command. Added `memory::prune_older_than` &
  `chat::jawab_soalan`.

Documentation reorganized & completed:
- **Added** [MODEL-EN.md](MODEL-EN.md) — description of each model (Qwen3 14B, bge-m3,
  reranker, tokenizer) & model switching guide.
- **Added** [RUNBOOK-EN.md](RUNBOOK-EN.md) — operations guide: logs, monitoring, backup &
  recovery (DR), failure scenarios.
- **Added** [SECURITY-EN.md](SECURITY-EN.md) — threat model, data classification,
  hardening checklist, data retention (PDPA), incident response.
- **Added** [USER-GUIDE-EN.md](USER-GUIDE-EN.md) — end user guide.
- **Added** [DOCUMENT-GUIDE-EN.md](DOCUMENT-GUIDE-EN.md) — document preparation & upload.
- **Added** "Related documents" section in [README-EN.md](README-EN.md).

---

## Development history (by feature)

Project built incrementally. Below is a summary of major features by development milestone
(refer to [ROADMAP-EN.md](ROADMAP-EN.md) for proposed `(#n)` numbers).

### Anti-hallucination guardrail
- **Added** pre-LLM relevance check: if context isn't relevant enough (reranker score
  or cosine distance), system rejects without calling LLM — reduces fabricated answers.
  Config thresholds: `RELEVANCE_ENABLED`, `RELEVANCE_MIN_RERANK`,
  `RELEVANCE_MAX_DISTANCE` (defaults intentionally loose, to tune after real data).

### Conversation frontend
- **Added** bubble-style chat interface, typing indicator, Enter to send, document
  management & metadata filtering. _(#24)_

### Security & resilience
- **Security** Tiered roles — `API_KEY` (user) vs `ADMIN_API_KEY` (admin),
  constant-time comparison. _(#14)_
- **Security** Prompt injection mitigation — neutralize fake delimiters + "DATA not
  instructions" directive. _(#15)_
- **Added** Per-IP rate limit (`RATE_LIMIT_RPM`) + body size limit (`MAX_BODY_BYTES`). _(#13)_
- **Added** Retry + backoff for Ollama calls (embed/generate/rerank). _(#11)_
- **Added** Graceful shutdown (handle systemd SIGTERM & Ctrl-C). _(#17)_

### Observability & operations
- **Added** Model-specific health check — `/health` verifies `GEN_MODEL` & `EMBED_MODEL`
  exist in Ollama. _(#20)_
- **Added** Prometheus metrics `/metrics` — chat/ingest counts, retrieval/generation times. _(#18)_
- **Added** Structured DB migrations (embedded sqlx) + runtime dim/FTS reconciliation. _(#16)_
- **Added** Integration tests gated by `TEST_DATABASE_URL`. _(#21)_

### Retrieval quality
- **Added** Cross-encoder reranker (`bge-reranker-v2-m3` via TEI): retrieve-N →
  rerank → top-k. _(#2)_
- **Added** Hybrid search — vector (pgvector) + keyword (`tsvector`/GIN) combined with
  Reciprocal Rank Fusion (RRF). _(#3)_
- **Added** Chunk metadata + filtering — sidecar `.meta.json`
  (category/department/year/security), filter searches via `filter`, display in `sources[].meta`.
- **Added** Real tokenizer for chunking — BPE `cl100k_base` (tiktoken-rs), embedded. _(#4)_
- **Added** Richer citations — `sources[]` include `page` (PDF per-page) + `snippet`. _(#6)_
- **Added** Conversation memory multi-turn — session history (`session_id`) in
  PostgreSQL. _(#5)_

### Model stack upgrade
- **Changed** Model stack to **Qwen3 14B** (generate) + **bge-m3** (embed, 1024-dim) +
  reranker, with Qwen3 *thinking* mode filter.
- **Added** Configurable embedding dimensions (`EMBED_DIM`), schema auto-reconciled. _(#23)_

### Ingest
- **Added** Recursive ingest — `DOCS_DIR` traversed including subfolders. _(#9)_
- **Added** Incremental ingest — skip unchanged files (size + mtime), `?force=true` to force. _(#8)_
- **Added** Batch embedding during ingest (`/api/embed`, `EMBED_BATCH_SIZE`). _(#7)_
- **Added** Document management — `GET /documents`, `DELETE /documents/:id`. _(#19)_

### Foundation
- **Added** Token-by-token answer streaming (SSE) — `/chat/stream`. _(#1)_
- **Added** Basic API key authentication (`Authorization: Bearer`, middleware). _(#12)_
- **Added** Initial version: Axum + tokio + sqlx + PostgreSQL/pgvector + Ollama, basic
  RAG pipeline (ingest → embed → retrieve → generate), Bahasa Malaysia, systemd deploy.

---

> Starting from the first official release, use semantic version numbers (e.g. `## [1.0.0] - 2026-xx-xx`)
> and move "Unreleased" entries below it.
