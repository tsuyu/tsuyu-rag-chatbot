//! Pengesahan API key dua peringkat (Bearer token) sebagai middleware Axum.
//!
//! - **Pengguna**: endpoint biasa (chat, senarai dokumen) — terima key pengguna ATAU admin.
//! - **Admin**: operasi menulis/memusnah (ingest, padam) — terima HANYA key admin.
//!
//! Reka bentuk keserasian:
//! - Jika `API_KEY` tidak ditetapkan → pengesahan pengguna dimatikan (pembangunan).
//! - Jika `ADMIN_API_KEY` tidak ditetapkan → endpoint admin jatuh balik ke `API_KEY`
//!   (mod satu-key). Jika kedua-dua tiada → endpoint admin juga terbuka (pembangunan).

use axum::extract::{Request, State};
use axum::http::header::AUTHORIZATION;
use axum::middleware::Next;
use axum::response::Response;
use secrecy::ExposeSecret;

use crate::error::AppError;
use crate::state::AppState;

/// Middleware untuk endpoint pengguna: benarkan jika key sepadan `API_KEY` ATAU `ADMIN_API_KEY`.
pub async fn require_user_key(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    // Kumpul key yang diterima (abaikan yang None); dedah rahsia hanya di sini.
    let accepted: Vec<&str> = [
        state.config.api_key.as_ref(),
        state.config.admin_api_key.as_ref(),
    ]
    .into_iter()
    .flatten()
    .map(|k| k.expose_secret().as_str())
    .collect();

    // Tiada key dikonfigurasi langsung → pengesahan dimatikan.
    if accepted.is_empty() {
        return Ok(next.run(req).await);
    }

    authorize(&accepted, &req)?;
    Ok(next.run(req).await)
}

/// Middleware untuk endpoint admin: benarkan hanya jika key sepadan key admin berkesan.
///
/// Key admin berkesan = `ADMIN_API_KEY` jika ditetapkan; jika tidak, jatuh balik ke
/// `API_KEY` (mod satu-key). Jika kedua-dua tiada → terbuka (pembangunan).
pub async fn require_admin_key(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let admin_key = state
        .config
        .admin_api_key
        .as_ref()
        .or(state.config.api_key.as_ref())
        .map(|k| k.expose_secret().as_str());

    let Some(expected) = admin_key else {
        return Ok(next.run(req).await);
    };

    authorize(&[expected], &req)?;
    Ok(next.run(req).await)
}

/// Sahkan `Authorization: Bearer <key>` sepadan salah satu `accepted` (constant-time).
fn authorize(accepted: &[&str], req: &Request) -> Result<(), AppError> {
    let provided = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim);

    match provided {
        Some(key) if accepted.iter().any(|e| constant_time_eq(key.as_bytes(), e.as_bytes())) => {
            Ok(())
        }
        _ => Err(AppError::Unauthorized),
    }
}

/// Perbandingan masa-tetap (constant-time) untuk mengelak kebocoran maklumat
/// melalui perbezaan masa pelaksanaan.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
