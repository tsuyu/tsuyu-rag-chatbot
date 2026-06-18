//! Endpoint pengurusan sesi memori perbualan.

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};

use crate::error::AppError;
use crate::services::memory;
use crate::state::AppState;

/// DELETE /sessions/:id — kosongkan memori perbualan bagi satu sesi.
pub async fn clear_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let deleted = memory::clear_session(&state, &session_id).await?;
    Ok(Json(json!({ "cleared": true, "messages_deleted": deleted })))
}
