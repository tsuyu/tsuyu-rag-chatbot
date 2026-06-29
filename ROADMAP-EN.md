# ROADMAP — TSUYU RAG Chatbot Improvements

Improvement checklist. **Done** items are checked; **Not done** items are checkboxes
to implement later. Organized by category, with estimated impact/effort.

---

## ✅ Done

- [x] **Answer streaming (SSE)** — `/chat/stream`, token by token. _(#1)_
- [x] **Batch embedding during ingest** — `/api/embed`, `EMBED_BATCH_SIZE`. _(#7)_
- [x] **Incremental ingest** — skip unchanged files (size + mtime), `?force=true`. _(#8)_
- [x] **Basic authentication (API key)** — `Authorization: Bearer`, middleware, constant-time. _(#12)_
- [x] **Document management** — `GET /documents`, `DELETE /documents/:id`. _(#19)_
- [x] **Reranking (cross-encoder)** — `bge-reranker-v2-m3`, retrieve-N → rerank → top-k. _(#2)_
- [x] **Configurable embedding dimensions** — `EMBED_DIM`, schema auto-reconciled. _(#23)_
- [x] **Model stack upgrade** — Qwen3 14B + bge-m3 + thinking mode filter.
- [x] **Hybrid search** — vector + keyword (`tsvector`/GIN) combined with RRF, single DB. _(#3)_
- [x] **Conversation memory (multi-turn)** — session history (`session_id`) in PostgreSQL. _(#5)_
- [x] **Chunk metadata + filtering** — sidecar `.meta.json` (category/department/year/security),
  filter searches via `filter`, display in `sources[].meta`.
- [x] **Real tokenizer for chunking** — BPE tokens (`cl100k_base`/tiktoken-rs), embedded. _(#4)_
- [x] **Richer citations** — `sources[]` include `page` (PDF per-page) + `snippet`. _(#6)_
- [x] **Recursive ingest** — `DOCS_DIR` traversed including subfolders (explicit stack). _(#9)_
- [x] **Retry + backoff for Ollama** — retry transient errors (embed/generate/rerank). _(#11)_
- [x] **Rate limiting & request size** — fixed-window per-IP (`RATE_LIMIT_RPM`) + `DefaultBodyLimit`. _(#13)_
- [x] **Graceful shutdown** — handle SIGTERM (systemd) & Ctrl-C. _(#17)_
- [x] **Model-specific health check** — `/health` verifies `GEN_MODEL` & `EMBED_MODEL` exist. _(#20)_
- [x] **Integration tests** — memory/documents/schema, gated by `TEST_DATABASE_URL`. _(#21)_
- [x] **Structured DB migrations** — embedded sqlx migrations + runtime dim/FTS reconciliation. _(#16)_
- [x] **Tiered roles/access** — user key (`API_KEY`) vs admin (`ADMIN_API_KEY`). _(#14)_
- [x] **Prompt injection mitigation** — neutralize fake delimiters + "DATA not instructions" directive. _(#15)_
- [x] **Metrics & observability** — `/metrics` Prometheus (chat/ingest counts, retrieval/generation times). _(#18)_
- [x] **Fuller frontend** — conversation history (bubbles), typing indicator, Enter-to-send. _(#24)_
- [x] **Anti-hallucination guardrail** — pre-LLM relevance threshold (rerank/cosine), early reject without LLM.
- [x] **Partial compile-time queries** _(#22)_ — safe CRUD queries (documents/memory/sessions/stats)
      converted to `sqlx::query!` macros (SQL + type checks at compile time). Offline `.sqlx` cache
      committed so build works without DB. Retrieval queries (vector/keyword/hybrid) remain
      runtime `query()` because they're dynamically built (`format!`) + use `vector`/`tsvector` columns.

---

## ⬜ Not done

### ⚡ Performance & scale

- [ ] **HNSW index tuning** _(#10)_ — set `m`/`ef_construction` & `ef_search` based on
      data size for speed/accuracy balance. _(impact: medium · effort: low)_

---

## Suggested next sequence

1. **HNSW index tuning (#10)** — when data grows.

Remaining items are optional/scale-dependent — the project core is complete
for production use.

> Numbers `(#n)` refer to the original proposal list discussed in development sessions.
