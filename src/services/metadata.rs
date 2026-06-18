//! Membaca metadata dokumen dari fail sidecar `<dokumen>.meta.json`.
//!
//! Contoh: untuk `polisi-cuti.pdf`, sidecar ialah `polisi-cuti.pdf.meta.json`:
//! ```json
//! { "category": "hr", "department": "Sumber Manusia", "year": 2024, "security": "dalaman" }
//! ```
//! Jika sidecar tiada atau tidak sah, metadata kosong dipulangkan (dokumen tetap di-ingest).

use std::path::Path;

use crate::models::DocumentMeta;

/// Bina laluan sidecar untuk satu dokumen: `<path>.meta.json`.
pub fn sidecar_path(doc_path: &Path) -> std::path::PathBuf {
    let mut s = doc_path.as_os_str().to_os_string();
    s.push(".meta.json");
    std::path::PathBuf::from(s)
}

/// Muat metadata untuk dokumen pada `doc_path`. Pulang `DocumentMeta::default()`
/// jika sidecar tiada; ralat hurai dicatat sebagai amaran (bukan kegagalan ingest).
pub async fn load_for(doc_path: &Path) -> DocumentMeta {
    let meta_path = sidecar_path(doc_path);

    let bytes = match tokio::fs::read(&meta_path).await {
        Ok(b) => b,
        Err(_) => return DocumentMeta::default(), // tiada sidecar = tiada metadata
    };

    match serde_json::from_slice::<DocumentMeta>(&bytes) {
        Ok(meta) => meta,
        Err(e) => {
            tracing::warn!("sidecar metadata tidak sah ({}): {e}", meta_path.display());
            DocumentMeta::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_path_betul() {
        let p = Path::new("/docs/polisi-cuti.pdf");
        assert_eq!(
            sidecar_path(p),
            Path::new("/docs/polisi-cuti.pdf.meta.json")
        );
    }

    #[test]
    fn hurai_meta_lengkap() {
        let json = r#"{"category":"hr","department":"SM","year":2024,"security":"dalaman"}"#;
        let m: DocumentMeta = serde_json::from_str(json).unwrap();
        assert_eq!(m.category.as_deref(), Some("hr"));
        assert_eq!(m.department.as_deref(), Some("SM"));
        assert_eq!(m.year, Some(2024));
        assert_eq!(m.security.as_deref(), Some("dalaman"));
    }

    #[test]
    fn hurai_meta_separa() {
        // Medan yang tiada jadi None, bukan ralat.
        let json = r#"{"category":"polisi"}"#;
        let m: DocumentMeta = serde_json::from_str(json).unwrap();
        assert_eq!(m.category.as_deref(), Some("polisi"));
        assert!(m.year.is_none());
        assert!(m.department.is_none());
    }
}
