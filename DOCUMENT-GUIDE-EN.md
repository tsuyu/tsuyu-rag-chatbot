# DOCUMENT PREPARATION GUIDE — TSUYU RAG Chatbot

A guide for staff who **prepare & upload documents** into the system. The quality of
chatbot answers depends directly on the quality of ingested documents. For the full
ingest API instructions see [README-EN.md](README-EN.md) §"`POST /ingest`".

---

## 1. Supported file types

The system extracts text from four formats (see [src/services/ingest.rs](src/services/ingest.rs)):

| Format | Extension | Notes |
|---|---|---|
| PDF | `.pdf` | Text extracted **per-page** → citations can show page numbers |
| Word | `.docx` | Main text from `word/document.xml` (not old `.doc`) |
| Plain text | `.txt` | Used directly |
| Markdown | `.md` | Used directly |

> Other format files (images, `.xlsx`, `.pptx`, old `.doc`) are **ignored** during ingest.

---

## 2. Document quality (most important)

The chatbot is only as good as the text it can read. Note:

- **PDFs must be text PDFs, not scans (images).** A scanned/photographed PDF is an *image* —
  there is no text to extract, so nothing can be searched. If you only have a scanned copy,
  run **OCR** first (e.g. save it as a "searchable PDF") before uploading.
- **Avoid documents with many complex tables/columns.** Complex layouts sometimes extract
  garbled. If possible, provide a simpler text version.
- **Ensure text is selectable.** Quick test: open the PDF, try to *select* & *copy* text.
  If you can't, it's an image.

---

## 3. Naming conventions & folder organization

- Place documents in the `DOCS_DIR` folder (set in `.env`).
- Ingest is **recursive** — subfolders are traversed. You can organize by category:
  ```
  docs/
    policy/
      leave-2024.pdf
      leave-2024.pdf.meta.json
    sop/
      payment.docx
  ```
- **Meaningful file names** help users identify sources (file name appears in answer
  source lists). Avoid names like `document1.pdf`.

---

## 4. Metadata (sidecar `.meta.json` file)

You can add metadata to each document by creating a file named
**`<full-filename>.meta.json`** next to it. Example: for `leave-2024.pdf`, create
`leave-2024.pdf.meta.json`.

### Format
```json
{
  "category": "hr",
  "department": "Human Resources",
  "year": 2024,
  "security": "internal"
}
```

| Field | Type | Meaning | Example |
|---|---|---|---|
| `category` | text | Document type/category | `"policy"`, `"sop"`, `"hr"` |
| `department` | text | Owning department | `"Human Resources"` |
| `year` | number | Document year | `2024` |
| `security` | text | Security level | `"public"`, `"internal"`, `"confidential"` |

- **All fields optional** — include only what you have. `{"category":"policy"}` is valid.
- Metadata is used for **search filtering** (`filter` in `/chat`) and **displayed**
  in `sources[].meta`. See [README-EN.md](README-EN.md) §"Metadata".
- See also [SECURITY-EN.md](SECURITY-EN.md) §1 for the meaning of `security` levels.

---

## 5. Uploading / ingesting

After placing files in `DOCS_DIR`, there are **three ways** to trigger ingest:

### Method A — Web UI (easiest)

Open `http://<server>:8080/admin` in a browser. Enter the `ADMIN_API_KEY`, click
**"Ingest (incremental)"** or **"Force ingest (all)"**. The page shows the document list,
metadata, chunk counts, and delete buttons — suitable for non-technical users.
This UI is server-rendered (Askama templates) and does not require `curl`.

### Method B — CLI (recommended for automation/cron)

Run the same binary with the `ingest` command — **no server or API key needed**.
It reads the same `.env`, prints a summary, then exits.

```bash
# Incremental ingest — skip unchanged files
tsuyu-rag-chatbot ingest

# Force reprocess all files
tsuyu-rag-chatbot ingest --force
```

Example output:
```
Ingest complete: 3 documents processed, 128 chunks saved, 12 unchanged (skipped), 0 failed.
```
> Non-zero **exit code** if any files failed to process — useful for scripts/cron.

### Method C — HTTP (while server is running)

```bash
# Normal ingest — skip unchanged files
curl -X POST http://127.0.0.1:8080/ingest \
     -H "Authorization: Bearer $ADMIN_API_KEY"

# Force reprocess
curl -X POST "http://127.0.0.1:8080/ingest?force=true" \
     -H "Authorization: Bearer $ADMIN_API_KEY"
```
> The HTTP endpoint runs ingest in the **background** (monitor logs for progress) and
> can only be triggered by the **`ADMIN_API_KEY`** holder. Useful for ad-hoc uploads
> without shell access to the server.

### Incremental ingest

All three methods use the same logic: the system compares file size + modification time
(mtime) (and sidecar files). Unchanged files are **skipped** — so repeated ingest is fast
and safe. Use `--force` / `?force=true` to force reprocessing (e.g. after changing
embedding model or if extraction looks wrong).

### Automatic scheduling (cron)

Example daily ingest at 01:00 using CLI:
```bash
# crontab -e (as user 'tsuyu')
0 1 * * * cd /opt/tsuyu-rag && ./tsuyu-rag-chatbot ingest >> /var/log/tsuyu-ingest.log 2>&1
```
> Ensure `.env` is readable from `WorkingDirectory` (or set `APP_ENV_FILE`).
> See [RUNBOOK-EN.md](RUNBOOK-EN.md) for systemd timer option.

---

## 6. Updating & deleting documents

- **Update:** replace the file in `DOCS_DIR` (content changed → mtime changes → ingest
  will reprocess it automatically on next run).
- **Delete:** remove from the system via API (cascades to delete related chunks):
  ```bash
  # List documents to get the id
  curl -s http://127.0.0.1:8080/documents -H "Authorization: Bearer $API_KEY"
  # Delete by id
  curl -X DELETE http://127.0.0.1:8080/documents/<id> \
       -H "Authorization: Bearer $ADMIN_API_KEY"
  ```
  > Deleting the file from `DOCS_DIR` alone **does not** remove it from the DB — use `DELETE`.

---

## 7. After uploading — verify

1. Check the count: `GET /documents` should list the new document.
2. Ask a question you know the answer to is in that document.
3. Verify the **sources** list points to the correct document (and page for PDFs).
4. If the chatbot says "not found" for a question it should answer, see
   [RUNBOOK-EN.md](RUNBOOK-EN.md) §6D & §6E.

---

## Quick checklist

- [ ] File in supported format (text `.pdf` / `.docx` / `.txt` / `.md`)
- [ ] PDF text is selectable (not a scan; if it is, OCR first)
- [ ] Meaningful file name
- [ ] `.meta.json` sidecar file created (if metadata/filtering desired)
- [ ] Placed in `DOCS_DIR`
- [ ] `POST /ingest` run with `ADMIN_API_KEY`
- [ ] Verified with a test question
