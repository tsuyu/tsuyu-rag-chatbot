# Model yang Digunakan — TSUYU RAG Chatbot

Dokumen ini menerangkan setiap model AI dalam stack chatbot ini: kenapa ia dipilih,
peranannya dalam saluran RAG (Retrieval-Augmented Generation), dan konfigurasinya.

Semua model dijalankan **on-premise** (tiada API luar). Aliran data tidak meninggalkan
pelayan TSUYU — penting untuk dokumen kerajaan yang sensitif.

---

## Ringkasan stack

| Peranan | Model | Saiz | Disajikan oleh | Endpoint |
|---|---|---|---|---|
| Penjanaan jawapan | **Qwen3 14B** | ~14B param | Ollama | `POST /api/generate` |
| Embedding (vektor) | **bge-m3** | ~567M param, 1024-dim | Ollama | `POST /api/embed` |
| Reranker (cross-encoder) | **bge-reranker-v2-m3** | ~568M param | TEI (servis berasingan) | `POST /rerank` |
| Tokenizer (chunking) | **cl100k_base** (tiktoken) | — (BPE, bukan model neural) | Terbenam dalam binari | — |

> Konfigurasi melalui `.env` — lihat [.env.example](.env.example). Nama model boleh
> ditukar (`GEN_MODEL`, `EMBED_MODEL`, `RERANKER_MODEL`) tanpa ubah kod.

---

## Aliran data: di mana setiap model masuk

```
                    INGEST (sekali, semasa muat naik dokumen)
  Dokumen ──► [cl100k_base] potong jadi chunk ──► [bge-m3] embed ──► simpan di pgvector
                  tokenizer                          embedding         (PostgreSQL)

                    QUERY (setiap soalan pengguna)
  Soalan ──► [bge-m3] embed ──► carian hybrid (vektor + kata kunci)
                                      │
                                      ▼  ambil RETRIEVE_N calon
                              [bge-reranker-v2-m3] susun semula
                                      │
                                      ▼  ambil TOP_K terbaik
                              guardrail relevansi (tolak jika tak cukup relevan)
                                      │
                                      ▼
                              [Qwen3 14B] jana jawapan dari konteks
```

---

## 1. Qwen3 14B — penjanaan jawapan (LLM)

**Peranan:** Membaca chunk dokumen yang diambil + soalan pengguna, lalu menulis jawapan
dalam Bahasa Malaysia. Inilah "otak" yang menyusun ayat akhir.

**Kenapa model ini:**
- **Sokongan berbilang bahasa kuat**, termasuk Bahasa Malaysia — penting kerana semua
  dokumen dan interaksi dalam BM.
- **14B param** ialah imbangan baik: cukup pintar untuk menaakul atas konteks teknikal
  pertanian/pentadbiran TSUYU, tetapi masih boleh dijalankan pada satu GPU pelayan
  (lwn. model 70B yang memerlukan perkakasan jauh lebih mahal).
- Dijalankan secara tempatan melalui Ollama — tiada data dihantar ke luar.

**Konfigurasi penting:**
- `GEN_MODEL=qwen3:14b`
- `GEN_THINK=false` — Qwen3 ada mod *"thinking"* (reasoning) yang menjana blok
  `<think>...</think>` sebelum jawapan. Untuk RAG, kami **matikan** kerana ia melambatkan
  respons tanpa banyak faedah untuk tugas "jawab dari konteks". Aplikasi juga ada penapis
  (`strip_thinking` / `ThinkFilter` dalam [src/services/generate.rs](src/services/generate.rs))
  yang membuang blok `<think>` daripada output streaming sekiranya model masih
  menghasilkannya.

**Mitigasi keselamatan:** Konteks dokumen dianggap **DATA, bukan arahan**. Prompt sistem
mengarahkan model abaikan sebarang "arahan" yang tertanam dalam dokumen, dan input tidak
dipercayai dineutralkan (`sanitize_untrusted`) untuk halang *prompt injection*.

---

## 2. bge-m3 — model embedding (carian semantik)

**Peranan:** Menukar teks (chunk dokumen semasa ingest, dan soalan semasa query) kepada
**vektor 1024-dimensi**. Vektor yang maknanya serupa akan dekat antara satu sama lain,
membolehkan carian "berdasarkan makna" bukan sekadar padanan perkataan.

**Kenapa model ini:**
- **Multilingual** (BGE-M3 = *Multi-Linguality, Multi-Functionality, Multi-Granularity*) —
  mengendalikan teks BM bercampur istilah Inggeris/nombor dengan baik tanpa pra-normalisasi.
- Output **1024-dim** — perincian baik tanpa terlalu berat untuk pgvector.
- Boleh kendali teks panjang (sehingga ~8192 token), sesuai untuk chunk dokumen.

**Konfigurasi penting:**
- `EMBED_MODEL=bge-m3`
- `EMBED_DIM=1024` — **MESTI sepadan** dengan output model. Jika tukar model embedding,
  kemas kini nilai ini; skema DB akan diselaras automatik dan chunk lama dikosongkan
  (perlu `POST /ingest?force=true` semula).
- `EMBED_BATCH_SIZE=16` — semasa ingest, chunk di-embed secara berkelompok melalui
  `/api/embed` Ollama (jauh lebih pantas daripada satu-satu).

**Disimpan di:** PostgreSQL + pgvector, sebagai jenis `vector(1024)`. Carian persamaan
guna jarak cosine (operator `<=>`).

---

## 3. bge-reranker-v2-m3 — reranker (cross-encoder)

**Peranan:** Selepas carian awal mengembalikan ~`RETRIEVE_N` calon, reranker menilai
semula setiap pasangan **(soalan, chunk)** secara langsung dan memberi skor relevansi
yang lebih tepat. Hanya `TOP_K` chunk terbaik dihantar ke Qwen3.

**Kenapa perlu reranker:**
- Embedding (bge-m3) pantas tetapi *kasar* — ia bandingkan dua vektor yang dikira
  berasingan. Reranker (cross-encoder) baca soalan **dan** chunk bersama-sama, jadi jauh
  lebih tepat menilai relevansi sebenar, walau lebih perlahan.
- Corak **retrieve-banyak → rerank → ambil-sedikit** ini memberi ketepatan tinggi tanpa
  perlu hantar terlalu banyak konteks ke LLM (jimat token + kurang gangguan).

**Konfigurasi penting:**
- `RERANK_ENABLED=true` — boleh dimatikan jika tiada servis reranker; sistem akan terus
  guna susunan carian hybrid sahaja.
- `RERANKER_URL=http://localhost:8081` — disajikan oleh servis **berasingan**
  (HuggingFace TEI — *text-embeddings-inference*), bukan Ollama. Endpoint `/rerank`.
- `RERANKER_MODEL=bge-reranker-v2-m3`
- `RETRIEVE_N=30`, `TOP_K=5` — ambil 30 calon, kekalkan 5 terbaik selepas rerank.

**Kaitan dengan guardrail:** Skor reranker juga digunakan oleh guardrail anti-halusinasi
(`RELEVANCE_MIN_RERANK`) untuk menolak soalan yang tiada konteks cukup relevan —
lihat [README.md](README.md) bahagian guardrail.

---

## 4. cl100k_base (tiktoken) — tokenizer untuk chunking

**Peranan:** Bukan model neural — ini ialah *tokenizer* BPE (Byte-Pair Encoding) yang
mengira saiz teks dalam **token** semasa memotong dokumen kepada chunk. Ini memastikan
setiap chunk muat dalam had konteks model dengan tepat (bukan agakan ikut bilangan
aksara/perkataan).

**Kenapa model ini:**
- **Terbenam dalam binari** (`tiktoken-rs`) — tiada fail luar perlu dimuat turun semasa
  runtime, sesuai untuk deploy on-premise yang ketat.
- `cl100k_base` ialah tokenizer BPE matang yang menganggar saiz token dengan munasabah
  rapat untuk kebanyakan model moden (termasuk Qwen3/bge-m3 untuk tujuan menyaiz chunk).

**Konfigurasi penting:**
- `CHUNK_TOKENS=700` — sasaran saiz setiap chunk (token).
- `CHUNK_OVERLAP=100` — token bertindih antara chunk berjiran, supaya konteks merentas
  sempadan chunk tidak hilang.

Lihat [src/services/chunk.rs](src/services/chunk.rs).

---

## Anggaran keperluan memori (panduan kasar)

| Model | VRAM/RAM anggaran (kuantisasi biasa) |
|---|---|
| Qwen3 14B (Q4_K_M) | ~9–10 GB |
| bge-m3 | ~2–3 GB |
| bge-reranker-v2-m3 | ~2–3 GB |
| **Jumlah** | **~14–16 GB** (GPU disyorkan) |

> Angka sebenar bergantung pada tahap kuantisasi, panjang konteks, dan saiz batch.
> Lihat bahagian *hardware recommendation* dalam [README.md](README.md).

---

## Menukar model

Semua nama model dikawal melalui `.env`, jadi anda boleh tukar tanpa ubah kod:

- **Tukar LLM:** ubah `GEN_MODEL` (cth. `qwen3:32b` jika perkakasan mengizinkan, atau
  model lebih kecil untuk pelayan terhad). Pastikan model wujud dalam Ollama
  (`ollama pull <nama>`). `/health` akan sahkan kewujudannya.
- **Tukar embedding:** ubah `EMBED_MODEL` **dan** `EMBED_DIM` agar sepadan, kemudian
  `POST /ingest?force=true` untuk embed semula semua dokumen.
- **Tukar/matikan reranker:** ubah `RERANKER_MODEL`/`RERANKER_URL`, atau set
  `RERANK_ENABLED=false`.
