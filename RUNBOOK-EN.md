# RUNBOOK — TSUYU RAG Chatbot Operations

Daily operations guide for IT staff **maintaining** this system on the server
(not a development guide). For initial setup & API, see [README-EN.md](README-EN.md).

> **Default paths** (per [deploy/tsuyu-rag.service](deploy/tsuyu-rag.service)):
> application at `/opt/tsuyu-rag/`, running as user `tsuyu`, systemd unit
> `tsuyu-rag`, config at `/opt/tsuyu-rag/.env`. Adjust if your environment differs.

---

## 0. Quick checklist (when something breaks)

| Symptom | Check first | Section |
|---|---|---|
| Chatbot not responding at all | `systemctl status tsuyu-rag` | [§2](#2-service-control) |
| Replies "information not found" for valid questions | Ollama/embedding up? Documents ingested? | [§6](#6-failure-scenarios--recovery) |
| Slow answers | Ollama/GPU load, retrieval time at `/metrics` | [§5](#5-monitoring) |
| 500 errors on `/chat` | App logs, DB & Ollama up? | [§6](#6-failure-scenarios--recovery) |
| `/health` not `ok` | Model missing from Ollama, DB disconnected | [§6](#6-failure-scenarios--recovery) |

**Quick health check:**
```bash
# Server is running:
curl -s http://127.0.0.1:8080/health | jq .      # overall status
systemctl is-active tsuyu-rag postgresql ollama   # all three should be "active"

# Server is NOT running (e.g. before deploy / investigating) — use CLI:
cd /opt/tsuyu-rag && ./tsuyu-rag-chatbot check      # DB + Ollama + models + reranker
```

---

## 1. System components

The system depends on **four services**. If one goes down, the effect:

| Service | Role | If down |
|---|---|---|
| `tsuyu-rag` (app) | API + RAG pipeline | No chatbot at all |
| `postgresql` | Store documents, chunks, embeddings, memory | `/chat` & `/ingest` fail (500) |
| `ollama` | Serve Qwen3 (generate) + bge-m3 (embed) | No answers / no embeddings |
| Reranker (TEI, port 8081) | Reorder candidates | System continues without rerank (graceful degradation) if `RERANK_ENABLED` stays `true` but calls fail — monitor logs |

See [MODEL-EN.md](MODEL-EN.md) for model details.

---

## 2. Service control

```bash
# Status & health
systemctl status tsuyu-rag

# Start / stop / restart
sudo systemctl start tsuyu-rag
sudo systemctl stop tsuyu-rag         # graceful: handles SIGTERM, finishes active requests
sudo systemctl restart tsuyu-rag

# Enable at boot
sudo systemctl enable tsuyu-rag

# After changing .env or replacing binary
sudo systemctl restart tsuyu-rag
```

> **Graceful shutdown note:** the application handles SIGTERM (which `systemctl stop` sends),
> so in-progress requests are completed before exiting. No need for `kill -9`
> unless it stalls past systemd's `TimeoutStopSec`.

### Replacing binary (after new build)
```bash
sudo systemctl stop tsuyu-rag
sudo cp target/release/tsuyu-rag-chatbot /opt/tsuyu-rag/tsuyu-rag-chatbot
sudo chown tsuyu:tsuyu /opt/tsuyu-rag/tsuyu-rag-chatbot
sudo systemctl start tsuyu-rag
curl -s http://127.0.0.1:8080/health | jq .
```

---

## 3. Logs

```bash
# Follow logs live
journalctl -u tsuyu-rag -f

# Logs since last boot / last hour / today
journalctl -u tsuyu-rag -b
journalctl -u tsuyu-rag --since "1 hour ago"
journalctl -u tsuyu-rag --since today

# Filter errors only
journalctl -u tsuyu-rag -p err --since today
```

**Log level** controlled by `RUST_LOG` in `.env` (default `info`). For deep diagnosis,
temporarily change to `RUST_LOG=debug`, `systemctl restart tsuyu-rag`, and
**revert to `info`** after investigation (debug mode is noisy & may log more details).

**Log rotation:** journald manages rotation itself. Check & limit size if needed:
```bash
journalctl --disk-usage
sudo journalctl --vacuum-time=30d      # keep only 30 days
```

---

## 4. Database

Schema (see [migrations/0001_initial.sql](migrations/0001_initial.sql)):
- `documents` — one row per file (metadata, size/mtime for incremental ingest)
- `chunks` — text snippets + `embedding vector(1024)` + HNSW & GIN indexes
- `messages` — conversation history (session memory)

### Quick check (CLI)
```bash
# Quick overview without entering psql — counts + DB size
cd /opt/tsuyu-rag && ./tsuyu-rag-chatbot stats
```

### Connect & inspect (psql)
```bash
sudo -u postgres psql tsuyu_rag      # or use your DATABASE_URL

-- Basic counts
SELECT count(*) FROM documents;
SELECT count(*) FROM chunks;
SELECT count(*) FROM messages;

-- Most recently ingested documents
SELECT id, filename, ingested_at FROM documents ORDER BY ingested_at DESC LIMIT 10;

-- Database size
SELECT pg_size_pretty(pg_database_size('tsuyu_rag'));
```

### Periodic maintenance
```sql
-- After large deletions/ingests: update statistics & reclaim space
VACUUM ANALYZE chunks;
VACUUM ANALYZE documents;
```

### Cleaning old conversation memory (optional, if `messages` grows large)
Easy way (recommended) — use CLI, suitable for cron:
```bash
cd /opt/tsuyu-rag && ./tsuyu-rag-chatbot prune-sessions --older-than 90
```
Or direct SQL:
```sql
DELETE FROM messages WHERE created_at < now() - interval '90 days';
```

---

## 4b. Document ingest (manual & scheduled)

Ingest can be triggered **two ways** (both use the same incremental logic):

```bash
# CLI — no server/API key needed (ideal for shell/cron)
cd /opt/tsuyu-rag && ./tsuyu-rag-chatbot ingest            # incremental
cd /opt/tsuyu-rag && ./tsuyu-rag-chatbot ingest --force    # reprocess all

# HTTP — while server is running
curl -X POST http://127.0.0.1:8080/ingest -H "Authorization: Bearer $ADMIN_API_KEY"
```
CLI prints a summary and **exits with non-zero code if any files fail** —
easy to monitor in cron/scripts. See [DOCUMENT-GUIDE-EN.md](DOCUMENT-GUIDE-EN.md) §5.

### Scheduling with systemd timer (recommended)

Example units provided: [deploy/tsuyu-rag-ingest.service](deploy/tsuyu-rag-ingest.service)
(oneshot) + [deploy/tsuyu-rag-ingest.timer](deploy/tsuyu-rag-ingest.timer) (daily at 01:00).

```bash
sudo cp deploy/tsuyu-rag-ingest.service deploy/tsuyu-rag-ingest.timer /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now tsuyu-rag-ingest.timer

# Check schedule & past runs
systemctl list-timers tsuyu-rag-ingest.timer
journalctl -u tsuyu-rag-ingest.service --since today

# Trigger ingest immediately (outside schedule)
sudo systemctl start tsuyu-rag-ingest.service
```

> Alternative: simple cron — `0 1 * * * cd /opt/tsuyu-rag && ./tsuyu-rag-chatbot ingest`.

---

## 5. Monitoring

`/metrics` endpoint (Prometheus format) — see [src/metrics.rs](src/metrics.rs):

```bash
curl -s http://127.0.0.1:8080/metrics
```

| Metric | Meaning | Watch if |
|---|---|---|
| `tsuyu_chat_requests_total` | Total chat requests | — (usage trend) |
| `tsuyu_chat_errors_total` | Failed chat requests | Sudden spike = problem (DB/Ollama) |
| `tsuyu_ingest_runs_total` | Number of ingest triggers | — |
| `tsuyu_retrieval_duration_ms_sum` / `_count` | Average retrieval time | High average = DB/index needs review |
| `tsuyu_generate_duration_ms_sum` / `_count` | Average LLM generation time | High average = GPU/model load |

**Average** = `_sum ÷ _count`. E.g. average generation time:
```bash
curl -s http://127.0.0.1:8080/metrics | awk '
  /tsuyu_generate_duration_ms_sum/{s=$2}
  /tsuyu_generate_duration_ms_count/{c=$2}
  END{ if(c>0) printf "Avg generate: %.0f ms\n", s/c }'
```

High error rate (`tsuyu_chat_errors_total` spiking) is the primary signal to investigate.

---

## 6. Failure scenarios & recovery

### A. `/health` not `ok` or app not responding
```bash
systemctl status tsuyu-rag
journalctl -u tsuyu-rag -p err --since "10 min ago"
```
- **App dead / restarting repeatedly:** check logs for reason (usually bad `.env`,
  DB can't connect, or `BIND_ADDR` port already in use). Fix & `restart`.
- **DB disconnected:** `systemctl status postgresql`; restart if needed.

### B. Ollama down or model missing
`/health` checks that `GEN_MODEL` & `EMBED_MODEL` exist in Ollama.
```bash
systemctl status ollama
ollama list                          # verify qwen3:14b & bge-m3 are there
ollama pull qwen3:14b                # if missing
ollama pull bge-m3
sudo systemctl restart tsuyu-rag
```

### C. Reranker (port 8081) down
- Symptom: app logs show errors calling `/rerank`; answers still come out but
  ordering quality may degrade.
- Restart the TEI reranker service (see README §"Reranker service"). For temporary
  operation without reranker, set `RERANK_ENABLED=false` in `.env` & `restart`.

### D. Chatbot replies "information not found" for valid questions
Not an error — this is the anti-hallucination guardrail rejecting insufficient relevance. Check:
1. Is the relevant document ingested? (`SELECT count(*) FROM chunks;` > 0)
2. Is embedding up? (Ollama + `bge-m3`)
3. Threshold too strict? See `RELEVANCE_MIN_RERANK` / `RELEVANCE_MAX_DISTANCE` —
   defaults are intentionally **loose**; don't tighten without data. Tune using the `distance`
   field in `sources[]` responses from `/chat`. (See README §"Anti-hallucination guardrails".)

### E. After ingest, document doesn't appear in answers
- `POST /ingest` skips unchanged files (checks size + mtime). Force reprocess:
  `POST /ingest?force=true`.
- Verify file is in `DOCS_DIR` (traversed recursively including subfolders).

### F. Changing embedding model / `EMBED_DIM`
If `EMBED_MODEL` or `EMBED_DIM` changes, old embeddings are **incompatible**. The app
reconciles the schema automatically & clears old chunks; after that **re-embedding is required**:
```bash
curl -X POST http://127.0.0.1:8080/ingest?force=true \
     -H "Authorization: Bearer $ADMIN_API_KEY"
```

---

## 7. Backup & recovery (DR)

> **Most important.** Do scheduled backups before any major updates.

### What to back up
1. **Database** (`tsuyu_rag`) — documents, chunks, embeddings, memory. *Required.*
2. **Config file** (`/opt/tsuyu-rag/.env`) — contains API keys & settings. *Required,
   store securely — contains secrets.*
3. **Source document folder** (`DOCS_DIR`) — can be regenerated via ingest if originals
   remain, but back up for safety.

> The binary can be rebuilt from source; does not need backing up.

### Database backup
```bash
# Full backup (compressed)
sudo -u postgres pg_dump -Fc tsuyu_rag > /backup/tsuyu_rag_$(date +%F).dump

# Example daily 02:00 cron job (postgres user's crontab)
0 2 * * * pg_dump -Fc tsuyu_rag > /backup/tsuyu_rag_$(date +\%F).dump && \
          find /backup -name 'tsuyu_rag_*.dump' -mtime +30 -delete
```

### Database restore
```bash
sudo systemctl stop tsuyu-rag

# Restore to a new/empty DB (ensure vector extension exists)
sudo -u postgres createdb tsuyu_rag_restore
sudo -u postgres psql tsuyu_rag_restore -c "CREATE EXTENSION IF NOT EXISTS vector;"
sudo -u postgres pg_restore -d tsuyu_rag_restore /backup/tsuyu_rag_2026-06-02.dump

# After verification, update DATABASE_URL in .env to point to restored DB, or
# rename DB. Then:
sudo systemctl start tsuyu-rag
curl -s http://127.0.0.1:8080/health | jq .
```

### Config & document backup
```bash
sudo cp /opt/tsuyu-rag/.env /backup/env_$(date +%F).bak   # STORE SECURELY (has secrets)
sudo tar czf /backup/docs_$(date +%F).tgz -C /opt/tsuyu-rag docs
```

### Periodically test recovery
Untested backup = no backup. At least quarterly, restore to a test server/DB and verify
chunk count + one `/chat` query works.

---

## 8. Key & certificate rotation

- **API keys** (`API_KEY` / `ADMIN_API_KEY`) in `.env`. To rotate: generate new keys,
  update `.env`, `systemctl restart tsuyu-rag`, and distribute new keys to clients.
- If behind a TLS reverse proxy, monitor certificate expiry separately.

---

## 9. Updates & maintenance window

Recommended sequence for application updates:
```bash
# 1. Back up DB & .env first (§7)
# 2. Build new version (build machine), test
cargo build --release && cargo test
# 3. Stop service, replace binary, run migrations (automatic at start), start
sudo systemctl stop tsuyu-rag
sudo cp target/release/tsuyu-rag-chatbot /opt/tsuyu-rag/
sudo systemctl start tsuyu-rag
# 4. Verify
curl -s http://127.0.0.1:8080/health | jq .
journalctl -u tsuyu-rag --since "2 min ago"
```

> DB migrations run automatically at application start
> ([src/db.rs](src/db.rs) — `run_migrations`). Always back up DB before updates
> that include new migrations.

---

## 10. Contacts & escalation

> Fill in per your organization.

| Role | Name | Contact |
|---|---|---|
| System administrator | _(fill in)_ | _(fill in)_ |
| Application owner | _(fill in)_ | _(fill in)_ |
| Development team | _(fill in)_ | _(fill in)_ |
