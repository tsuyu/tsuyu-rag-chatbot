//! Lapisan HTTP (Axum): definisi router + handler endpoint.

mod admin;
pub(crate) mod chat;
mod documents;
pub(crate) mod health;
mod ingest;
mod metrics;
mod sessions;

use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, post};
use axum::Router;

use crate::state::AppState;

/// Bina router penuh aplikasi.
pub fn router(state: AppState) -> Router {
    let max_body = state.config.max_body_bytes;

    // Endpoint pengguna: chat & baca dokumen. Terima key pengguna ATAU admin.
    let user_routes = Router::new()
        .route("/chat", post(chat::chat))
        .route("/chat/stream", post(chat::chat_stream))
        .route("/documents", get(documents::list_documents))
        // Fragmen HTML jadual dokumen untuk UI admin (baca sahaja).
        .route("/admin/documents", get(admin::admin_documents))
        // Baca kad watak (persona) semasa.
        .route("/admin/character", get(admin::get_character))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::auth::require_user_key,
        ));

    // Endpoint admin: menulis/memusnah. Terima HANYA key admin (atau API_KEY jika
    // ADMIN_API_KEY tidak ditetapkan).
    let admin_routes = Router::new()
        .route("/ingest", post(ingest::ingest))
        .route("/documents/:id", delete(documents::delete_document))
        .route("/sessions/:id", delete(sessions::clear_session))
        // Tindakan UI admin yang memulangkan HTML (ingest segerak + padam).
        .route("/admin/ingest", post(admin::admin_ingest))
        .route("/admin/documents/:id", delete(admin::admin_delete))
        // Kemas kini kad watak (persona).
        .route("/admin/character", axum::routing::put(admin::put_character))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::auth::require_admin_key,
        ));

    // Endpoint terbuka: frontend dan semakan kesihatan.
    Router::new()
        .route("/", get(frontend))
        .route("/admin", get(admin::admin_page))
        .route("/health", get(health::health))
        .route("/metrics", get(metrics::metrics))
        .merge(user_routes)
        .merge(admin_routes)
        // Had kadar per-IP (aktif jika RATE_LIMIT_RPM > 0) + had saiz badan permintaan.
        // Dipakai pada semua laluan; `/health` ringan jadi jarang tercetus.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::ratelimit::limit,
        ))
        .layer(DefaultBodyLimit::max(max_body))
        .with_state(state)
}

/// Frontend ringkas (htmx) — boleh diupgrade kemudian.
async fn frontend() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("../../static/index.html"))
}
