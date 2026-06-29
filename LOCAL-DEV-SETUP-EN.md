# Local Dev Setup — Reference Notes

Step-by-step reference for running TSUYU RAG Chatbot on a **CPU-only development
machine**. Prepared & verified running on **2026-06-02**.

> For full production installation, see [README-EN.md](README-EN.md). This document is
> specifically for lightweight dev setup (small models, no capable GPU).

---

## 1. Tested machine

| Component | Spec | Notes |
|---|---|---|
| CPU | Intel i7-3770 (2012, 4C/8T) | Old, **no AVX2** |
| GPU | NVIDIA GT 1030, **2 GB VRAM** | Too small for LLM → use CPU |
| RAM | 31 GB | Sufficient |
| OS | Ubuntu 24.04, PostgreSQL 16, Ollama 0.21.1 | |

**Verdict:** the production stack (qwen3:14b + bge-m3 + reranker) is too slow here.
Use **small models on CPU** — one RAG answer takes ~**5–6 seconds**.

---

## 2. Setup steps (one-time)

### 2.1 Install pgvector
```bash
sudo apt-get install -y postgresql-16-pgvector
```

### 2.2 Create database + extension
```bash
createdb tsuyu_rag
psql -d tsuyu_rag -c "CREATE EXTENSION IF NOT EXISTS vector;"
```

### 2.3 Create Postgres role
TCP `localhost` requires a password (pg_hba uses scram), so create a dedicated role:
```bash
psql -d postgres -c "CREATE ROLE tsuyu LOGIN SUPERUSER PASSWORD 'password';"
```
> `SUPERUSER` is for local dev only (avoid permission issues with schema/migration). Do not
> use in production.

### 2.4 Download small models (Ollama)
```bash
ollama pull qwen3:1.7b          # generator (~1.4 GB)
ollama pull nomic-embed-text    # 768-dim embedding (~274 MB)
```

### 2.5 Create `.env` file
Copy the following values to `.env` (this file is git-ignored):
```ini
DATABASE_URL=postgres://tsuyu:password@localhost/tsuyu_rag
OLLAMA_URL=http://localhost:11434

GEN_MODEL=qwen3:1.7b
EMBED_MODEL=nomic-embed-text
EMBED_DIM=768
GEN_THINK=false
RERANK_ENABLED=false

DOCS_DIR=/home/tsuyu/rust_project/tsuyu-rag-chatbot/docs
BIND_ADDR=127.0.0.1:8080
API_KEY=
ADMIN_API_KEY=
RATE_LIMIT_RPM=0
RUST_LOG=info
```
> This profile is also documented as comments in [.env.example](.env.example).
> **Important:** `EMBED_DIM=768` for nomic (not 1024 for bge-m3). If you change the
> embed model, run `ingest --force` again.

### 2.6 Build binary
```bash
cargo build --release
```

---

## 3. Validation (actual results from this session)

```bash
# 1. Pre-flight check
./target/release/tsuyu-rag-chatbot check
# → ✓ Database  ✓ Ollama  ✓ Generation model  ✓ Embedding model  → Status: ok

# 2. Prepare sample document & ingest
mkdir -p docs   # place .pdf/.docx/.txt/.md files here
./target/release/tsuyu-rag-chatbot ingest
# → Ingest complete: 1 document processed, 1 chunk saved, 0 failed.

# 3. Ask (full RAG pipeline)
./target/release/tsuyu-rag-chatbot ask "How many days annual leave for Grade 41?"
# → "...Grade 41 gets 30 days annual leave per year."  (5.6s on CPU)
#   Source: example-leave.txt (chunk 0)
```

| Test | Result |
|---|---|
| `check` (DB + Ollama + models) | ✅ all green |
| `ingest` | ✅ 1 document, 1 chunk |
| `ask` (RAG CLI) | ✅ accurate answer, ~5.6 seconds |
| `serve` → `GET /health` | ✅ `{"status":"ok",...}` |
| `GET /` (chat frontend) | ✅ |
| `GET /admin` (Askama UI) | ✅ renders |
| `POST /chat` (RAG HTTP) | ✅ answer + correct sources |

---

## 4. Daily usage

```bash
# Ensure services are up
systemctl is-active postgresql            # should be "active"
curl -s http://localhost:11434/api/tags   # Ollama up?

# Start server + open browser
./target/release/tsuyu-rag-chatbot serve
#   Chat   : http://127.0.0.1:8080/
#   Admin  : http://127.0.0.1:8080/admin

# Or use CLI directly
./target/release/tsuyu-rag-chatbot stats
./target/release/tsuyu-rag-chatbot ask "<question>"
```

---

## 5. Upgrading to production (GPU machine)

On a server with GPU ≥16 GB, change only `.env` (no code changes):
```ini
GEN_MODEL=qwen3:14b
EMBED_MODEL=bge-m3
EMBED_DIM=1024
RERANK_ENABLED=true
```
Then `ingest --force` to re-embed with the new model. See
[README-EN.md](README-EN.md) §Hardware recommendations and [MODEL-EN.md](MODEL-EN.md).

---

## 6. Quick troubleshooting

| Symptom | Common cause | Action |
|---|---|---|
| `check` fails on DB | Postgres down / wrong role | `systemctl status postgresql`; verify `tsuyu` role |
| `check` fails on model | Model not pulled | `ollama pull qwen3:1.7b nomic-embed-text` |
| "Information not found" answer | Not ingested / wrong EMBED_DIM | `ingest --force`; ensure `EMBED_DIM=768` |
| Very slow answers | Model too large for CPU | Use qwen3:1.7b, not 14b |
