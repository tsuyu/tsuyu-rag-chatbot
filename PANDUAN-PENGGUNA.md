# PANDUAN PENGGUNA — TSUYU RAG Chatbot

Panduan ringkas untuk **pengguna TSUYU** yang menggunakan chatbot ini untuk mencari
maklumat dalam dokumen rasmi. Tiada pengetahuan teknikal diperlukan.

---

## Apa itu chatbot ini?

Ia pembantu soal-jawab yang menjawab **berdasarkan dokumen rasmi TSUYU sahaja** (polisi,
SOP, surat pekeliling, dsb.) yang telah dimuat naik ke dalam sistem. Anda bertanya dalam
**Bahasa Malaysia**, dan ia akan:

1. Mencari petikan paling relevan daripada dokumen TSUYU,
2. Menyusun jawapan berdasarkan petikan tersebut,
3. Menyenaraikan **dokumen sumber** supaya anda boleh semak sendiri.

Ia **bukan** seperti chatbot internet umum — ia tidak menjawab dari "pengetahuan umum"
atau internet. Jika jawapan tiada dalam dokumen TSUYU, ia akan berkata begitu.

---

## Cara bertanya dengan berkesan

| Amalan baik | Contoh |
|---|---|
| **Spesifik** | "Berapa hari cuti tahunan untuk pegawai gred 41?" — lebih baik daripada "cuti?" |
| **Satu topik setiap soalan** | Tanya satu perkara, kemudian tanya susulan |
| **Guna istilah dalam dokumen** | Jika dokumen guna "elaun lebih masa", guna istilah itu |
| **Soalan susulan dibenarkan** | "Bagaimana pula untuk gred 44?" — ia ingat konteks perbualan |

**Elakkan:**
- Soalan terlalu umum ("ceritakan tentang TSUYU")
- Beberapa soalan berbeza dalam satu ayat
- Menganggap ia tahu maklumat di luar dokumen (cuaca, berita, hal peribadi)

---

## Membaca jawapan

Setiap jawapan disertakan **senarai sumber** — dokumen dan (untuk PDF) nombor muka surat
tempat maklumat diambil.

> ✅ **Sentiasa semak sumber.** Untuk keputusan penting, buka dokumen asal yang
> disenaraikan dan sahkan maklumat itu sendiri. Chatbot membantu anda *mencari* maklumat
> dengan cepat — ia tidak menggantikan dokumen rasmi.

### Menyimpan jawapan

- **Salin** — klik butang 📋 di bawah mana-mana jawapan untuk menyalin teksnya.
- **Cetak** — klik 🖨️ untuk mencetak seluruh perbualan (atau simpan sebagai PDF melalui
  dialog cetak pelayar).
- **Eksport** — klik ⬇️ untuk memuat turun perbualan sebagai fail `.md` atau `.txt`
  (mengandungi soalan, jawapan, dan senarai rujukan).

Semua ini berlaku dalam pelayar anda sahaja — tiada data dihantar ke mana-mana.

---

## Bila chatbot kata "tidak menjumpai maklumat"

Anda mungkin melihat mesej seperti:

> *"Maaf, saya tidak menjumpai maklumat berkaitan soalan ini dalam dokumen TSUYU…"*

Ini **bukan kerosakan** — ia ciri keselamatan (*guardrail*) yang menghalang chatbot
daripada "mereka-reka" jawapan apabila ia tidak menemui maklumat yang cukup berkaitan.
Lebih baik ia berkata "tidak tahu" daripada memberi jawapan salah.

**Apa boleh anda buat:**
- **Cuba semula dengan perkataan lain** — guna istilah yang mungkin digunakan dalam dokumen.
- **Lebih spesifik** atau lebih umum.
- Jika anda yakin dokumen berkaitan sepatutnya ada, **maklumkan pentadbir** — mungkin
  dokumen itu belum dimuat naik.

---

## Had yang perlu anda tahu

- **Hanya tahu apa yang ada dalam dokumen yang dimuat naik.** Dokumen baru perlu
  dimuat naik oleh pentadbir dahulu.
- **Boleh tersilap.** Walaupun ada guardrail, AI kadang-kadang silap mentafsir.
  Sumber disediakan supaya anda boleh semak — gunakannya.
- **Bukan nasihat rasmi.** Jawapan ialah bantuan mencari maklumat, bukan keputusan
  rasmi atau nasihat undang-undang.
- **Perbualan mungkin disimpan** untuk membolehkan soalan susulan. Elakkan memasukkan
  maklumat peribadi sensitif yang tidak perlu.

---

## Soalan lazim (FAQ)

**Boleh saya tanya dalam bahasa Inggeris?**
Sistem dioptimumkan untuk Bahasa Malaysia. Soalan BM memberi hasil terbaik.

**Kenapa jawapan untuk soalan sama kadang berbeza sedikit?**
AI menjana ayat semula setiap kali. Maknanya patut konsisten; jika berbeza ketara,
semak sumber.

**Adakah data saya dihantar ke luar?**
Tidak. Seluruh sistem berjalan dalam pelayan TSUYU — tiada data ke internet.

**Saya jumpa jawapan yang salah. Apa patut saya buat?**
Semak dokumen sumber yang disenaraikan, dan maklumkan pentadbir sistem supaya dokumen
boleh dikemas kini jika perlu.

---

> Untuk pentadbir & staf IT, lihat [README.md](README.md), [RUNBOOK.md](RUNBOOK.md),
> dan [PANDUAN-DOKUMEN.md](PANDUAN-DOKUMEN.md).
