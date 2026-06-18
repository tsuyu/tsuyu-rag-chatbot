//! Logik pemecahan teks (chunking) berasaskan token sebenar.
//!
//! Kita guna tokenizer BPE (`cl100k_base` via tiktoken-rs) untuk mengira saiz chunk
//! dalam **token**, bukan perkataan. Ini lebih konsisten dengan had konteks model dan
//! lebih tepat untuk teks campuran (BM, tanda baca, kod). Tokenizer dimuat sekali
//! semasa start dan dikongsi melalui `AppState`.

use tiktoken_rs::CoreBPE;

/// Pecah `text` jadi beberapa chunk berdasarkan kiraan token.
///
/// - `tokenizer`      : tokenizer BPE untuk encode/decode.
/// - `chunk_tokens`   : sasaran saiz setiap chunk (dalam token).
/// - `overlap_tokens` : bilangan token bertindih antara chunk berturutan.
///
/// Pertindihan membantu konteks tidak terputus di sempadan chunk.
pub fn chunk_text(
    tokenizer: &CoreBPE,
    text: &str,
    chunk_tokens: usize,
    overlap_tokens: usize,
) -> Vec<String> {
    // Elak chunk kosong/ruang sahaja.
    if text.trim().is_empty() {
        return Vec::new();
    }

    let tokens = tokenizer.encode_with_special_tokens(text);
    if tokens.is_empty() {
        return Vec::new();
    }

    // Saiz mesti sekurang-kurangnya 1, dan overlap mesti kurang dari saiz
    // supaya tetingkap sentiasa bergerak ke hadapan (elak gelung tak terhingga).
    let size = chunk_tokens.max(1);
    let overlap = overlap_tokens.min(size - 1);
    let step = size - overlap;

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < tokens.len() {
        let end = (start + size).min(tokens.len());
        // Decode kembali token jadi teks. Jika gagal (jarang berlaku untuk subset
        // yang sah), langkau chunk itu daripada panik.
        if let Ok(piece) = tokenizer.decode(tokens[start..end].to_vec()) {
            let trimmed = piece.trim();
            if !trimmed.is_empty() {
                chunks.push(trimmed.to_string());
            }
        }
        if end == tokens.len() {
            break;
        }
        start += step;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiktoken_rs::cl100k_base;

    fn tok() -> CoreBPE {
        cl100k_base().expect("muat cl100k_base untuk ujian")
    }

    #[test]
    fn teks_kosong_pulang_vektor_kosong() {
        let t = tok();
        assert!(chunk_text(&t, "", 10, 2).is_empty());
        assert!(chunk_text(&t, "   \n  ", 10, 2).is_empty());
    }

    #[test]
    fn teks_pendek_jadi_satu_chunk() {
        let t = tok();
        let chunks = chunk_text(&t, "Cuti tahunan dua puluh hari.", 100, 10);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Cuti tahunan dua puluh hari.");
    }

    #[test]
    fn teks_panjang_pecah_kepada_banyak_chunk() {
        let t = tok();
        // Bina teks yang pasti melebihi saiz chunk kecil.
        let text = "perkataan ".repeat(200);
        let chunks = chunk_text(&t, &text, 50, 10);
        assert!(chunks.len() > 1, "sepatutnya pecah kepada >1 chunk");
    }

    #[test]
    fn pertindihan_kekal_konteks() {
        let t = tok();
        // Dengan overlap, hujung satu chunk muncul semula di permulaan chunk seterusnya.
        let text = (0..100).map(|i| format!("kata{i}")).collect::<Vec<_>>().join(" ");
        let chunks = chunk_text(&t, &text, 40, 10);
        assert!(chunks.len() >= 2);
        // Token bertindih bermakna gabungan chunk meliputi keseluruhan teks tanpa jurang.
        // Sekadar pastikan kandungan akhir teks hadir dalam chunk terakhir.
        assert!(chunks.last().unwrap().contains("kata99"));
    }

    #[test]
    fn overlap_terlalu_besar_dihadkan() {
        let t = tok();
        // overlap >= size: dihadkan kepada size-1 supaya tetap bergerak (tiada gelung tak henti).
        let text = (0..30).map(|i| format!("x{i}")).collect::<Vec<_>>().join(" ");
        let chunks = chunk_text(&t, &text, 5, 999);
        assert!(!chunks.is_empty());
    }
}
