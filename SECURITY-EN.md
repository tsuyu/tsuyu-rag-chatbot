# SECURITY & DATA GOVERNANCE — TSUYU RAG Chatbot

This document describes the threat model, security controls, data classification, and
governance policies for this system. Directed at **ICT security officers, system
administrators, and auditors**. For daily operations see [RUNBOOK-EN.md](RUNBOOK-EN.md).

> **Core principle:** this system is designed to be **fully on-premise** — no data is
> sent to external services. All models (LLM, embedding, reranker) run locally.
> This is the most important security control for sensitive government documents.

---

## 1. Data classification

This system processes TSUYU documents that may contain **Restricted** or **Confidential**
information. Each document can be tagged with its security level via the `security` field
in the sidecar metadata (`<document>.meta.json`), stored in the `documents.security` column.

| Level (proposed) | Meaning | Handling |
|---|---|---|
| `public` | Can be disclosed publicly | No additional restrictions |
| `internal` | TSUYU internal use | Access via API key only |
| `confidential` | Sensitive, restricted access | Consider isolation / mandatory metadata filter |

> **Important:** Currently, **access control is at the API key level (user vs admin)**,
> not per-document. If `confidential` and `public` documents are in the same DB,
> any `API_KEY` holder can receive excerpts from both. If strict per-document isolation
> is required, consider:
> - Running **separate instances** for confidential documents, or
> - Enforcing metadata filters (`filter`) based on caller role (requires additional
>   development — does not exist yet).

---

## 2. Threat model (summary)

| Threat | Vector | Existing controls |
|---|---|---|
| Unauthorized access | API calls without key | Bearer authentication, constant-time comparison |
| Privilege escalation | User triggers ingest/delete | Two tiers: `API_KEY` vs `ADMIN_API_KEY` |
| Prompt injection | Malicious instructions embedded in documents | Input sanitization + "DATA not instructions" directive |
| Abuse/DoS | Request flooding, large bodies | Per-IP rate limit + body size limit |
| Hallucination (false "facts" leaked) | LLM answers without source | Pre-LLM relevance guardrail |
| Data exposure in transit | Network traffic | Local bind + TLS reverse proxy (deploy) |
| Secret leakage | API keys in `.env`/logs | File permissions, don't log keys |

---

## 3. Existing security controls

### 3.1 Authentication & authorization
- **Two-tier keys** (`Authorization: Bearer <key>` header):
  - `API_KEY` — user: `/chat`, `/chat/stream`, `GET /documents`.
  - `ADMIN_API_KEY` — admin: `POST /ingest`, `DELETE /documents/:id`, `DELETE /sessions/:id`.
  - Admin key also passes user endpoints.
- Key comparison uses **constant time** (`constant_time_eq`) — prevents timing attacks.
- If both keys are empty, authentication is **disabled** — **do not** deploy to production
  this way. Always set at least `API_KEY`.

### 3.2 Prompt injection mitigation
- Document context & conversation history are treated as **DATA, not instructions**. The
  system prompt directs the model to ignore any "instructions" embedded in documents.
- Untrusted input is sanitized (`sanitize_untrusted`) — fake delimiters like
  `=== ... ===` are stripped so documents can't spoof prompt structure.

### 3.3 Rate limiting & request size
- `RATE_LIMIT_RPM` — per-IP request limit per minute (fixed-window).
- `MAX_BODY_BYTES` — request body size limit (default 2 MiB) — blocks large bodies.

### 3.4 Anti-hallucination guardrail
- Before calling the LLM, the system evaluates context relevance (reranker score / cosine distance).
  If not sufficiently relevant, it **rejects** without generating an answer — reducing the risk
  of fabricated answers. See [README-EN.md](README-EN.md) §"Anti-hallucination guardrails".

### 3.5 Network isolation
- `BIND_ADDR` defaults to `127.0.0.1:8080` — listens on localhost only. Network
  exposure should go through a **reverse proxy** (nginx/Caddy) with TLS.

---

## 4. Deployment hardening checklist

Before going to production, verify:

- [ ] `API_KEY` **and** `ADMIN_API_KEY` set to strong random values (≥32 characters).
- [ ] `.env` owned by user `tsuyu`, permission `600` (`chmod 600 /opt/tsuyu-rag/.env`).
- [ ] `BIND_ADDR=127.0.0.1:8080` (not `0.0.0.0`) unless behind a controlled proxy.
- [ ] Reverse proxy with **TLS** enabled for non-localhost access.
- [ ] Firewall allows only necessary ports; Ollama (11434) & reranker (8081) ports
      are **not** exposed outside the server.
- [ ] PostgreSQL listens locally only; strong DB password.
- [ ] `RATE_LIMIT_RPM` & `MAX_BODY_BYTES` set reasonably for expected load.
- [ ] Application runs as a **non-root user** (`tsuyu`) — already in the systemd unit.
- [ ] DB & `.env` backups scheduled + stored securely (see [RUNBOOK-EN.md](RUNBOOK-EN.md) §7).
- [ ] `RUST_LOG=info` in production (not `debug` — reduces detail leakage).
- [ ] API key rotation scheduled periodically.

---

## 5. Secret handling

- Secrets (API keys, DB password) reside only in `/opt/tsuyu-rag/.env`. **Do not**
  commit to git (already in `.gitignore`).
- In code, config secrets are wrapped in `secrecy::SecretString` — they are **not printed**
  by `Debug`/log and their memory is zeroed when dropped; actual values are exposed only
  at necessary boundaries (DB connection, constant-time auth key comparison).
- **Do not log keys.** During diagnosis, avoid long-running `RUST_LOG=debug`.
- When a team member leaves / key is compromised: **rotate key** immediately
  (see [RUNBOOK-EN.md](RUNBOOK-EN.md) §8) and redistribute.

---

## 6. Data retention & deletion (PDPA-aligned)

> Aligned with TSUYU's record retention policies & personal data protection principles.
> Fill in actual durations per your organization's policy.

| Data | Location | Retention period (suggested) | Deletion |
|---|---|---|---|
| Source documents & chunks | `documents`, `chunks` | As long as document is valid | `DELETE /documents/:id` (cascades to chunks) |
| Conversation history | `messages` | _(fill in — e.g. 90 days)_ | `DELETE /sessions/:id` or scheduled SQL purge |
| Application logs | journald | _(fill in — e.g. 30 days)_ | `journalctl --vacuum-time=30d` |
| Backups | `/backup` | _(fill in — e.g. 30 days)_ | Cron rotation (see RUNBOOK §7) |

**Privacy note:** user questions are stored in `messages` for conversation memory.
If questions may contain personal data, limit `MEMORY_TURNS`, reduce retention period,
or disable memory (`MEMORY_ENABLED=false`) for sensitive cases.

**Retention enforcement:** delete old memory on a schedule with the CLI command
(suitable for cron — see [RUNBOOK-EN.md](RUNBOOK-EN.md) §4):
```bash
tsuyu-rag-chatbot prune-sessions --older-than 90
```

**Right to deletion:** to delete an individual session's trace:
```bash
curl -X DELETE http://127.0.0.1:8080/sessions/<session_id> \
     -H "Authorization: Bearer $ADMIN_API_KEY"
```

---

## 7. Auditing

- **Access logs:** journald records application activity. For full audit trail (who called
  what), consider access logging at the reverse proxy level (IP, time, endpoint,
  status) — the proxy is the best place because it sees requests before auth.
- **Document changes:** `documents.ingested_at` records when a document was loaded. Deletions
  leave no trace in the DB — if deletion auditing is required, log at the proxy or
  add an audit table (additional development).
- **Metrics:** `/metrics` gives aggregate counts (not per-user) — useful for anomaly
  detection (error/request spikes).

---

## 8. Incident response (summary)

| Incident | Immediate action |
|---|---|
| API key compromised | Rotate key (RUNBOOK §8), check proxy access logs for abuse |
| Suspected unauthorized access | Stop service (`systemctl stop tsuyu-rag`), audit logs, rotate keys |
| Confidential document accidentally ingested | `DELETE /documents/:id`, `VACUUM`, verify not in chunks |
| Server compromised | Isolate from network, restore from clean backup, rotate all secrets |

> Adjust & align with TSUYU's official ICT incident response procedures.

---

## 9. Known limitations & residual risks

Transparent about what this system does **not** do:
- **No per-document access control** — access at API key level only (see §1).
- **No encryption at rest** built in — relies on OS/DB-level disk encryption if required.
- **No per-user audit trail** built in — relies on reverse proxy logs.
- **Guardrails reduce, not eliminate, hallucinations** — users must always check the
  cited source excerpts.
- **Shared API keys** — not per-user identity; no SSO/OIDC built in.

These items can be addressed through additional development if security policy demands.
