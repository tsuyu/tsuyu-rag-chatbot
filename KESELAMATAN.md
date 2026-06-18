# KESELAMATAN & TADBIR URUS DATA — TSUYU RAG Chatbot

Dokumen ini menerangkan model ancaman, kawalan keselamatan, pengelasan data, dan dasar
tadbir urus untuk sistem ini. Ditujukan untuk **pegawai keselamatan ICT, pentadbir
sistem, dan juruaudit**. Untuk operasi harian lihat [RUNBOOK.md](RUNBOOK.md).

> **Prinsip teras:** sistem ini direka **on-premise sepenuhnya** — tiada data dihantar
> ke perkhidmatan luar. Semua model (LLM, embedding, reranker) dijalankan tempatan.
> Ini ialah kawalan keselamatan paling penting untuk dokumen kerajaan sensitif.

---

## 1. Pengelasan data

Sistem ini memproses dokumen TSUYU yang mungkin mengandungi maklumat **Terhad** atau
**Sulit**. Setiap dokumen boleh ditanda tahap keselamatannya melalui medan `security`
dalam metadata sidecar (`<dokumen>.meta.json`), yang disimpan dalam lajur
`documents.security`.

| Tahap (cadangan) | Maksud | Pengendalian |
|---|---|---|
| `terbuka` | Boleh didedah umum | Tiada sekatan tambahan |
| `terhad` | Kegunaan dalaman TSUYU | Akses melalui API key sahaja |
| `sulit` | Sensitif, akses terhad | Pertimbang pengasingan / penapis metadata wajib |

> **Penting:** Buat masa ini, **kawalan akses adalah pada peringkat API key (pengguna vs
> admin)**, bukan per-dokumen. Jika dokumen `sulit` dan `terbuka` berada dalam DB yang
> sama, mana-mana pemegang `API_KEY` boleh menerima petikan daripada kedua-duanya. Jika
> pengasingan ketat per-dokumen diperlukan, pertimbangkan:
> - Jalankan **instance berasingan** untuk dokumen sulit, atau
> - Wajibkan penapis metadata (`filter`) mengikut peranan pemanggil (perlu pembangunan
>   tambahan — belum wujud).

---

## 2. Model ancaman (ringkas)

| Ancaman | Vektor | Kawalan sedia ada |
|---|---|---|
| Akses tanpa kebenaran | Panggilan API tanpa key | Pengesahan Bearer, perbandingan masa-tetap |
| Peningkatan keistimewaan | Pengguna cetus ingest/padam | Dua peringkat: `API_KEY` vs `ADMIN_API_KEY` |
| Prompt injection | Arahan jahat tertanam dalam dokumen | Sanitasi input + arahan "DATA bukan arahan" |
| Penyalahgunaan/DoS | Banjiran permintaan, badan besar | Had kadar per-IP + had saiz badan |
| Halusinasi (kebocoran "fakta" palsu) | LLM menjawab tanpa sumber | Guardrail relevansi pra-LLM |
| Pendedahan data semasa transit | Trafik rangkaian | Bind tempatan + reverse proxy TLS (deploy) |
| Kebocoran rahsia | API key dalam `.env`/log | Kebenaran fail, jangan log key |

---

## 3. Kawalan keselamatan sedia ada

### 3.1 Pengesahan & kebenaran
- **Dua peringkat key** (header `Authorization: Bearer <key>`):
  - `API_KEY` — pengguna: `/chat`, `/chat/stream`, `GET /documents`.
  - `ADMIN_API_KEY` — admin: `POST /ingest`, `DELETE /documents/:id`, `DELETE /sessions/:id`.
  - Key admin juga melepasi endpoint pengguna.
- Perbandingan key guna **masa-tetap** (`constant_time_eq`) — menghalang serangan masa.
- Jika kedua-dua key kosong, pengesahan **dimatikan** — **jangan** deploy pengeluaran
  begini. Sentiasa tetapkan sekurang-kurangnya `API_KEY`.

### 3.2 Mitigasi prompt injection
- Konteks dokumen & sejarah perbualan dianggap **DATA, bukan arahan**. Prompt sistem
  mengarahkan model mengabaikan sebarang "arahan" tertanam dalam dokumen.
- Input tidak dipercayai dineutralkan (`sanitize_untrusted`) — penanda palsu seperti
  `=== ... ===` dilucutkan supaya dokumen tak boleh memalsukan struktur prompt.

### 3.3 Had kadar & saiz permintaan
- `RATE_LIMIT_RPM` — had permintaan per-IP setiap minit (fixed-window).
- `MAX_BODY_BYTES` — had saiz badan permintaan (lalai 2 MiB) — halang badan besar.

### 3.4 Guardrail anti-halusinasi
- Sebelum memanggil LLM, sistem menilai relevansi konteks (skor reranker / jarak cosine).
  Jika tak cukup relevan, ia **menolak** tanpa menjana jawapan — mengurangkan risiko
  jawapan direka-reka. Lihat [README.md](README.md) §"Guardrail anti-halusinasi".

### 3.5 Pengasingan rangkaian
- `BIND_ADDR` lalai `127.0.0.1:8080` — hanya dengar pada localhost. Pendedahan ke
  rangkaian sepatutnya melalui **reverse proxy** (nginx/Caddy) dengan TLS.

---

## 4. Senarai semak pengukuhan (hardening) deploy

Sebelum pengeluaran, sahkan:

- [ ] `API_KEY` **dan** `ADMIN_API_KEY` ditetapkan kepada nilai rawak kuat (≥32 aksara).
- [ ] `.env` dimiliki pengguna `tsuyu`, kebenaran `600` (`chmod 600 /opt/tsuyu-rag/.env`).
- [ ] `BIND_ADDR=127.0.0.1:8080` (bukan `0.0.0.0`) melainkan di belakang proxy terkawal.
- [ ] Reverse proxy dengan **TLS** diaktifkan untuk akses bukan-localhost.
- [ ] Firewall hanya benarkan port yang perlu; port Ollama (11434) & reranker (8081)
      **tidak** terdedah ke luar pelayan.
- [ ] PostgreSQL hanya dengar tempatan; kata laluan DB kuat.
- [ ] `RATE_LIMIT_RPM` & `MAX_BODY_BYTES` ditetapkan munasabah untuk beban dijangka.
- [ ] Aplikasi berjalan sebagai pengguna **bukan-root** (`tsuyu`) — sudah dalam unit systemd.
- [ ] Sandaran DB & `.env` berjadual + disimpan tersulit (lihat [RUNBOOK.md](RUNBOOK.md) §7).
- [ ] `RUST_LOG=info` di pengeluaran (bukan `debug` — kurangkan kebocoran butiran).
- [ ] Putaran API key dijadualkan secara berkala.

---

## 5. Pengendalian rahsia

- Rahsia (API key, kata laluan DB) berada dalam `/opt/tsuyu-rag/.env` sahaja. **Jangan**
  commit ke git (sudah dalam `.gitignore`).
- Dalam kod, rahsia config dibungkus `secrecy::SecretString` — ia **tidak dicetak** oleh
  `Debug`/log dan memorinya di-zero-kan apabila digugurkan; nilai sebenar didedah hanya
  di sempadan yang perlu (sambungan DB, perbandingan key auth masa-tetap).
- **Jangan log key.** Semasa diagnosis, elak `RUST_LOG=debug` jangka panjang.
- Apabila ahli pasukan berhenti / key tercompromi: **putar key** serta-merta
  (lihat [RUNBOOK.md](RUNBOOK.md) §8) dan agih semula.

---

## 6. Pengekalan & pemadaman data (gaya PDPA)

> Selaras dengan dasar pengekalan rekod TSUYU & prinsip perlindungan data peribadi.
> Isi tempoh sebenar mengikut dasar organisasi.

| Data | Lokasi | Tempoh pengekalan (cadangan) | Pemadaman |
|---|---|---|---|
| Dokumen sumber & chunk | `documents`, `chunks` | Selagi dokumen sah | `DELETE /documents/:id` (cascade buang chunk) |
| Sejarah perbualan | `messages` | _(isi — cth. 90 hari)_ | `DELETE /sessions/:id` atau SQL purge berjadual |
| Log aplikasi | journald | _(isi — cth. 30 hari)_ | `journalctl --vacuum-time=30d` |
| Sandaran | `/backup` | _(isi — cth. 30 hari)_ | Putaran cron (lihat RUNBOOK §7) |

**Perhatian privasi:** soalan pengguna disimpan dalam `messages` untuk memori perbualan.
Jika soalan mungkin mengandungi data peribadi, hadkan `MEMORY_TURNS`, kurangkan tempoh
pengekalan, atau matikan memori (`MEMORY_ENABLED=false`) untuk kes sensitif.

**Penguatkuasaan pengekalan:** padam memori lama secara berjadual dengan perintah CLI
(sesuai untuk cron — lihat [RUNBOOK.md](RUNBOOK.md) §4):
```bash
tsuyu-rag-chatbot prune-sessions --older-than 90
```

**Hak pemadaman:** untuk memadam jejak sesi individu:
```bash
curl -X DELETE http://127.0.0.1:8080/sessions/<session_id> \
     -H "Authorization: Bearer $ADMIN_API_KEY"
```

---

## 7. Pengauditan

- **Log akses:** journald merekod aktiviti aplikasi. Untuk jejak audit penuh (siapa
  panggil apa), pertimbang log akses di peringkat reverse proxy (IP, masa, endpoint,
  status) — proxy ialah tempat terbaik kerana ia nampak permintaan sebelum auth.
- **Perubahan dokumen:** `documents.ingested_at` merekod bila dokumen dimuat. Pemadaman
  tidak meninggalkan jejak dalam DB — jika audit pemadaman diperlukan, log di proxy atau
  tambah jadual audit (pembangunan tambahan).
- **Metrik:** `/metrics` memberi kiraan agregat (bukan per-pengguna) — berguna untuk
  pengesanan anomali (lonjakan ralat/permintaan).

---

## 8. Tindak balas insiden (ringkas)

| Insiden | Tindakan segera |
|---|---|
| API key tercompromi | Putar key (RUNBOOK §8), semak log akses proxy untuk penyalahgunaan |
| Capaian tanpa kebenaran disyaki | Henti servis (`systemctl stop tsuyu-rag`), audit log, putar key |
| Dokumen sulit tersilap ingest | `DELETE /documents/:id`, `VACUUM`, sahkan tiada dalam chunk |
| Pelayan dikompromi | Asingkan dari rangkaian, pulih dari sandaran bersih, putar semua rahsia |

> Laraskan & padankan dengan prosedur tindak balas insiden ICT rasmi TSUYU.

---

## 9. Had & risiko baki yang diketahui

Telus tentang apa yang sistem ini **tidak** lakukan:
- **Tiada kawalan akses per-dokumen** — akses pada peringkat API key sahaja (lihat §1).
- **Tiada penyulitan pada rehat** terbina — bergantung pada penyulitan cakera/DB peringkat
  OS jika diperlukan.
- **Tiada jejak audit per-pengguna** terbina — bergantung pada log reverse proxy.
- **Guardrail mengurangkan, bukan menghapus, halusinasi** — pengguna mesti sentiasa
  menyemak petikan sumber yang disertakan.
- **API key dikongsi** — bukan identiti per-pengguna; tiada SSO/OIDC terbina.

Item ini boleh ditangani melalui pembangunan tambahan jika dasar keselamatan menuntut.
