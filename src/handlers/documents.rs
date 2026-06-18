//! Endpoint pengurusan dokumen: senarai & padam.

use axum::extract::{Path, State};
use axum::Json;

use crate::error::AppError;
use crate::models::{DeleteResponse, DocumentListResponse};
use crate::services::documents as doc_svc;
use crate::state::AppState;

/// GET /documents — senarai semua dokumen + bilangan chunk.
pub async fn list_documents(
    State(state): State<AppState>,
) -> Result<Json<DocumentListResponse>, AppError> {
    let documents = doc_svc::list_documents(&state).await?;
    Ok(Json(DocumentListResponse {
        count: documents.len(),
        documents,
    }))
}

/// DELETE /documents/:id — padam satu dokumen dan chunknya.
pub async fn delete_document(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<DeleteResponse>, AppError> {
    let affected = doc_svc::delete_document(&state, id).await?;
    if affected == 0 {
        return Err(AppError::NotFound(format!("dokumen id {id} tidak dijumpai")));
    }
    Ok(Json(DeleteResponse { deleted: true, id }))
}
