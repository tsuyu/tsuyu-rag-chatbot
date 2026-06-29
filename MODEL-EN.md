# Models Used — TSUYU RAG Chatbot

This document describes each AI model in the chatbot stack: why it was chosen,
its role in the RAG (Retrieval-Augmented Generation) pipeline, and its configuration.

All models run **on-premise** (no external APIs). Data does not leave the TSUYU server —
important for sensitive government documents.

---

## Stack summary

| Role | Model | Size | Served by | Endpoint |
|---|---|---|---|---|
| Answer generation | **Qwen3 14B** | ~14B params | Ollama | `POST /api/generate` |
| Embedding (vector) | **bge-m3** | ~567M params, 1024-dim | Ollama | `POST /api/embed` |
| Reranker (cross-encoder) | **bge-reranker-v2-m3** | ~568M params | TEI (separate service) | `POST /rerank` |
| Tokenizer (chunking) | **cl100k_base** (tiktoken) | — (BPE, not a neural model) | Embedded in binary | — |

> Configuration via `.env` — see [.env.example](.env.example). Model names can be
> changed (`GEN_MODEL`, `EMBED_MODEL`, `RERANKER_MODEL`) without code changes.

---

## Data flow: where each model fits in

```
                    INGEST (once, during document upload)
  Document ──► [cl100k_base] split into chunks ──► [bge-m3] embed ──► store in pgvector
                  tokenizer                           embedding          (PostgreSQL)

                    QUERY (each user question)
  Question ──► [bge-m3] embed ──► hybrid search (vector + keyword)
                                        │
                                        ▼  retrieve RETRIEVE_N candidates
                                [bge-reranker-v2-m3] reorder
                                        │
                                        ▼  take TOP_K best
                                relevance guardrail (reject if not relevant enough)
                                        │
                                        ▼
                                [Qwen3 14B] generate answer from context
```

---

## 1. Qwen3 14B — answer generation (LLM)

**Role:** Reads retrieved document chunks + user question, then writes an answer
in Bahasa Malaysia. This is the "brain" that composes the final sentence.

**Why this model:**
- **Strong multilingual support**, including Bahasa Malaysia — important because all
  documents and interactions are in BM.
- **14B params** is a good balance: smart enough to reason over TSUYU's technical
  agricultural/administrative context, but still runnable on a single server GPU
  (vs. 70B models which need much more expensive hardware).
- Runs locally via Ollama — no data sent outside.

**Important configuration:**
- `GEN_MODEL=qwen3:14b`
- `GEN_THINK=false` — Qwen3 has a *"thinking"* (reasoning) mode that generates
  `<think>...</think>` blocks before the answer. For RAG, we **disable** this because
  it slows responses without much benefit for the "answer from context" task. The app also
  has a filter (`strip_thinking` / `ThinkFilter` in [src/services/generate.rs](src/services/generate.rs))
  that strips `<think>` blocks from streaming output in case the model still produces them.

**Security mitigation:** Document context is treated as **DATA, not instructions**. The
system prompt directs the model to ignore any "instructions" embedded in documents, and
untrusted input is sanitized (`sanitize_untrusted`) to prevent *prompt injection*.

---

## 2. bge-m3 — embedding model (semantic search)

**Role:** Converts text (document chunks during ingest, and questions during query) into
**1024-dimensional vectors**. Vectors with similar meaning will be close together,
enabling "meaning-based" search rather than just keyword matching.

**Why this model:**
- **Multilingual** (BGE-M3 = *Multi-Linguality, Multi-Functionality, Multi-Granularity*) —
  handles BM text mixed with English terms/numbers well without pre-normalization.
- **1024-dim** output — good detail without being too heavy for pgvector.
- Can handle long text (up to ~8192 tokens), suitable for document chunks.

**Important configuration:**
- `EMBED_MODEL=bge-m3`
- `EMBED_DIM=1024` — **MUST match** model output. If changing embedding model,
  update this value; the DB schema will be reconciled automatically and old chunks cleared
  (requires `POST /ingest?force=true` again).
- `EMBED_BATCH_SIZE=16` — during ingest, chunks are embedded in batches via
  Ollama's `/api/embed` (much faster than one at a time).

**Stored in:** PostgreSQL + pgvector, as type `vector(1024)`. Similarity search
uses cosine distance (operator `<=>`).

---

## 3. bge-reranker-v2-m3 — reranker (cross-encoder)

**Role:** After the initial search returns ~`RETRIEVE_N` candidates, the reranker
evaluates each **(question, chunk)** pair directly and gives a more accurate relevance
score. Only the best `TOP_K` chunks are sent to Qwen3.

**Why a reranker is needed:**
- Embedding (bge-m3) is fast but *coarse* — it compares two separately computed vectors.
  The reranker (cross-encoder) reads the question **and** chunk together, so it is far
  more accurate at judging actual relevance, albeit slower.
- The **retrieve-many → rerank → take-few** pattern gives high accuracy without needing
  to send too much context to the LLM (saves tokens + less noise).

**Important configuration:**
- `RERANK_ENABLED=true` — can be disabled if no reranker service is available; the system
  will continue using the hybrid search order only.
- `RERANKER_URL=http://localhost:8081` — served by a **separate** service
  (HuggingFace TEI — *text-embeddings-inference*), not Ollama. Endpoint `/rerank`.
- `RERANKER_MODEL=bge-reranker-v2-m3`
- `RETRIEVE_N=30`, `TOP_K=5` — retrieve 30 candidates, keep 5 best after reranking.

**Relation to guardrail:** The reranker score is also used by the anti-hallucination
guardrail (`RELEVANCE_MIN_RERANK`) to reject questions that have no sufficiently relevant
context — see [README-EN.md](README-EN.md) guardrail section.

---

## 4. cl100k_base (tiktoken) — tokenizer for chunking

**Role:** Not a neural model — this is a BPE (Byte-Pair Encoding) *tokenizer* that
counts text size in **tokens** when splitting documents into chunks. This ensures each
chunk fits within model context limits precisely (not estimated by character/word count).

**Why this tokenizer:**
- **Embedded in binary** (`tiktoken-rs`) — no external files need to be downloaded at
  runtime, suitable for strict on-premise deployments.
- `cl100k_base` is a mature BPE tokenizer that estimates token size reasonably closely
  for most modern models (including Qwen3/bge-m3 for chunk sizing purposes).

**Important configuration:**
- `CHUNK_TOKENS=700` — target size for each chunk (tokens).
- `CHUNK_OVERLAP=100` — tokens overlapping between neighboring chunks, so context across
  chunk boundaries is not lost.

See [src/services/chunk.rs](src/services/chunk.rs).

---

## Approximate memory requirements (rough guide)

| Model | Estimated VRAM/RAM (typical quantization) |
|---|---|
| Qwen3 14B (Q4_K_M) | ~9–10 GB |
| bge-m3 | ~2–3 GB |
| bge-reranker-v2-m3 | ~2–3 GB |
| **Total** | **~14–16 GB** (GPU recommended) |

> Actual numbers depend on quantization level, context length, and batch size.
> See the *hardware recommendations* section in [README-EN.md](README-EN.md).

---

## Switching models

All model names are controlled via `.env`, so you can swap without code changes:

- **Switch LLM:** change `GEN_MODEL` (e.g. `qwen3:32b` if hardware allows, or
  a smaller model for limited servers). Ensure the model exists in Ollama
  (`ollama pull <name>`). `/health` will verify its existence.
- **Switch embedding:** change `EMBED_MODEL` **and** `EMBED_DIM` to match, then
  `POST /ingest?force=true` to re-embed all documents.
- **Switch/disable reranker:** change `RERANKER_MODEL`/`RERANKER_URL`, or set
  `RERANK_ENABLED=false`.
