# PANDUAN PENYEDIAAN DOKUMEN — TSUYU RAG Chatbot

Panduan untuk staf yang **menyediakan & memuat naik dokumen** ke dalam sistem. Kualiti
jawapan chatbot bergantung terus pada kualiti dokumen yang di-ingest. Untuk arahan API
ingest penuh lihat [README.md](README.md) §"`POST /ingest`".

---

## 1. Jenis fail yang disokong

Sistem mengekstrak teks daripada empat format (lihat [src/services/ingest.rs](src/services/ingest.rs)):

| Format | Sambungan | Catatan |
|---|---|---|
| PDF | `.pdf` | Teks diekstrak **per-muka-surat** → petikan boleh tunjuk nombor muka surat |
| Word | `.docx` | Teks utama dari `word/document.xml` (bukan `.doc` lama) |
| Teks biasa | `.txt` | Terus diguna |
| Markdown | `.md` | Terus diguna |

> Fail format lain (imej, `.xlsx`, `.pptx`, `.doc` lama) **diabaikan** semasa ingest.

---

## 2. Kualiti dokumen (paling penting)

Chatbot hanya sebaik teks yang ia boleh baca. Perhatikan:

- **PDF mesti PDF teks, bukan imbasan (scan).** PDF hasil imbasan/foto ialah *imej* —
  tiada teks untuk diekstrak, jadi tiada apa boleh dicari. Jika hanya ada salinan
  imbasan, jalankan **OCR** dahulu (cth. simpan semula sebagai "PDF boleh dicari"/
  *searchable PDF*) sebelum muat naik.
- **Elakkan dokumen banyak jadual/lajur kompleks.** Susun atur rumit kadang diekstrak
  bercelaru. Jika boleh, sediakan versi teks yang lebih mudah.
- **Pastikan teks boleh dipilih.** Ujian cepat: buka PDF, cuba *select* & *copy* teks.
  Jika tak boleh, ia imej.

---

## 3. Konvensyen penamaan & susunan folder

- Letak dokumen dalam folder `DOCS_DIR` (ditetapkan dalam `.env`).
- Ingest **rekursif** — subfolder turut diselak. Anda boleh susun ikut kategori:
  ```
  docs/
    polisi/
      cuti-2024.pdf
      cuti-2024.pdf.meta.json
    sop/
      pembayaran.docx
  ```
- **Nama fail bermakna** memudahkan pengguna mengenali sumber (nama fail muncul dalam
  senarai sumber jawapan). Elakkan nama seperti `dokumen1.pdf`.

---

## 4. Metadata (fail sidecar `.meta.json`)

Anda boleh tambah metadata pada setiap dokumen dengan mencipta fail bernama
**`<namafail-penuh>.meta.json`** di sebelahnya. Contoh: untuk `cuti-2024.pdf`, cipta
`cuti-2024.pdf.meta.json`.

### Format
```json
{
  "category": "hr",
  "department": "Sumber Manusia",
  "year": 2024,
  "security": "dalaman"
}
```

| Medan | Jenis | Maksud | Contoh |
|---|---|---|---|
| `category` | teks | Jenis/kategori dokumen | `"polisi"`, `"sop"`, `"hr"` |
| `department` | teks | Jabatan/bahagian pemilik | `"Sumber Manusia"` |
| `year` | nombor | Tahun dokumen | `2024` |
| `security` | teks | Tahap keselamatan | `"terbuka"`, `"dalaman"`, `"sulit"` |

- **Semua medan pilihan** — sertakan yang anda ada sahaja. `{"category":"polisi"}` sah.
- Metadata digunakan untuk **penapisan carian** (`filter` dalam `/chat`) dan **dipaparkan**
  dalam `sources[].meta`. Lihat [README.md](README.md) §"Metadata dokumen".
- Lihat juga [KESELAMATAN.md](KESELAMATAN.md) §1 untuk maksud tahap `security`.

---

## 5. Memuat naik / ingest

Selepas meletak fail dalam `DOCS_DIR`, ada **tiga cara** mencetus ingest:

### Cara A — UI web (paling mudah)

Buka `http://<pelayan>:8080/admin` dalam pelayar. Masukkan `ADMIN_API_KEY`, klik
**“Ingest (tokokan)”** atau **“Ingest paksa (semua)”**. Halaman memaparkan senarai
dokumen, metadata, bilangan chunk, dan butang padam — sesuai untuk pengguna bukan teknikal.
UI ini dirender pelayan (templat Askama) dan tidak memerlukan `curl`.

### Cara B — CLI (disyorkan untuk automasi/cron)

Jalankan binari yang sama dengan perintah `ingest` — **tiada pelayan atau API key
diperlukan**. Ia membaca `.env` yang sama, mencetak ringkasan, lalu keluar.

```bash
# Ingest tokokan — langkau fail yang tak berubah
tsuyu-rag-chatbot ingest

# Paksa proses semula semua fail
tsuyu-rag-chatbot ingest --force
```

Output contoh:
```
Ingest selesai: 3 dokumen diproses, 128 chunk disimpan, 12 tidak berubah (dilangkau), 0 gagal.
```
> Kod keluar **bukan-sifar** jika ada fail gagal diproses — berguna untuk skrip/cron.

### Cara C — HTTP (semasa pelayan berjalan)

```bash
# Ingest biasa — langkau fail yang tak berubah
curl -X POST http://127.0.0.1:8080/ingest \
     -H "Authorization: Bearer $ADMIN_API_KEY"

# Paksa proses semula
curl -X POST "http://127.0.0.1:8080/ingest?force=true" \
     -H "Authorization: Bearer $ADMIN_API_KEY"
```
> Endpoint HTTP menjalankan ingest di **latar belakang** (pantau log untuk kemajuan) dan
> hanya boleh dicetus oleh pemegang **`ADMIN_API_KEY`**. Berguna untuk muat naik ad-hoc
> tanpa akses shell ke pelayan.

### Ingest tokokan (incremental)

Ketiga-tiga cara menggunakan logik yang sama: sistem membandingkan saiz + masa ubah suai
(mtime) fail (dan fail sidecar). Fail yang tidak berubah **dilangkau** — jadi ingest
berulang adalah pantas dan selamat. Guna `--force` / `?force=true` untuk memaksa proses
semula (cth. selepas tukar model embedding atau jika ekstraksi nampak salah).

### Penjadualan automatik (cron)

Contoh ingest harian 01:00 menggunakan CLI:
```bash
# crontab -e (sebagai pengguna 'tsuyu')
0 1 * * * cd /opt/tsuyu-rag && ./tsuyu-rag-chatbot ingest >> /var/log/tsuyu-ingest.log 2>&1
```
> Pastikan `.env` boleh dibaca dari `WorkingDirectory` (atau set `APP_ENV_FILE`).
> Lihat [RUNBOOK.md](RUNBOOK.md) untuk pilihan systemd timer.

---

## 6. Mengemas kini & memadam dokumen

- **Kemas kini:** ganti fail di `DOCS_DIR` (kandungan berubah → mtime berubah → ingest
  akan memprosesnya semula secara automatik pada larian seterusnya).
- **Padam:** keluarkan dari sistem melalui API (cascade buang chunk berkaitan):
  ```bash
  # Senarai dokumen untuk dapatkan id
  curl -s http://127.0.0.1:8080/documents -H "Authorization: Bearer $API_KEY"
  # Padam ikut id
  curl -X DELETE http://127.0.0.1:8080/documents/<id> \
       -H "Authorization: Bearer $ADMIN_API_KEY"
  ```
  > Memadam fail dari `DOCS_DIR` sahaja **tidak** membuangnya dari DB — guna `DELETE`.

---

## 7. Selepas memuat naik — sahkan

1. Semak kiraan: `GET /documents` patut menyenaraikan dokumen baru.
2. Tanya satu soalan yang anda tahu jawapannya ada dalam dokumen itu.
3. Sahkan senarai **sumber** menunjuk dokumen yang betul (dan muka surat untuk PDF).
4. Jika chatbot kata "tidak menjumpai" untuk soalan yang sepatutnya dijawab, lihat
   [RUNBOOK.md](RUNBOOK.md) §6D & §6E.

---

## Senarai semak ringkas

- [ ] Fail dalam format disokong (`.pdf` teks / `.docx` / `.txt` / `.md`)
- [ ] PDF boleh dipilih teksnya (bukan imbasan; jika ya, OCR dahulu)
- [ ] Nama fail bermakna
- [ ] Fail `.meta.json` sidecar dicipta (jika mahu metadata/penapisan)
- [ ] Diletak dalam `DOCS_DIR`
- [ ] `POST /ingest` dijalankan dengan `ADMIN_API_KEY`
- [ ] Disahkan melalui satu soalan ujian
