//! Memori perbualan: simpan & muat sejarah mesej sesi dalam PostgreSQL.
//!
//! Membolehkan soalan susulan ("dan untuk staf kontrak pula?") difahami dengan
//! menyuntik beberapa giliran terakhir ke dalam prompt.
//!
//! Query menggunakan makro `sqlx::query!` — disahkan pada masa kompil terhadap skema.

use crate::error::AppError;
use crate::models::Message;
use crate::state::AppState;

/// Muat `turns` mesej terkini bagi satu sesi, dalam susunan kronologi (lama → baru).
pub async fn load_recent(
    state: &AppState,
    session_id: &str,
    turns: i64,
) -> Result<Vec<Message>, AppError> {
    // Ambil N terkini (DESC) kemudian balikkan supaya kronologi betul untuk prompt.
    // Susun ikut `id` (BIGSERIAL monotonik), BUKAN created_at: kedua-dua mesej satu
    // giliran disimpan dalam satu transaksi dan berkongsi `now()` yang sama, jadi
    // created_at tidak boleh bezakan susunan user/assistant dalam giliran itu.
    let rows = sqlx::query!(
        r#"
        SELECT role, content
        FROM (
            SELECT role, content, id
            FROM messages
            WHERE session_id = $1
            ORDER BY id DESC
            LIMIT $2
        ) recent
        ORDER BY id ASC
        "#,
        session_id,
        turns,
    )
    .fetch_all(&state.db)
    .await?;

    let msgs = rows
        .into_iter()
        .map(|r| Message {
            role: r.role,
            content: r.content,
        })
        .collect();
    Ok(msgs)
}

/// Simpan satu pasang giliran (soalan pengguna + jawapan pembantu) dalam satu transaksi.
pub async fn save_turn(
    state: &AppState,
    session_id: &str,
    question: &str,
    answer: &str,
) -> Result<(), AppError> {
    let mut tx = state.db.begin().await?;

    sqlx::query!(
        "INSERT INTO messages (session_id, role, content) VALUES ($1, 'user', $2)",
        session_id,
        question,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        "INSERT INTO messages (session_id, role, content) VALUES ($1, 'assistant', $2)",
        session_id,
        answer,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// Padam semua mesej bagi satu sesi. Pulang bilangan mesej dipadam.
pub async fn clear_session(state: &AppState, session_id: &str) -> Result<u64, AppError> {
    let res = sqlx::query!("DELETE FROM messages WHERE session_id = $1", session_id)
        .execute(&state.db)
        .await?;
    Ok(res.rows_affected())
}

/// Padam mesej yang lebih lama daripada `days` hari (pengekalan data / dasar PDPA).
/// Pulang bilangan mesej dipadam. Digunakan oleh perintah CLI `prune-sessions`.
pub async fn prune_older_than(state: &AppState, days: i64) -> Result<u64, AppError> {
    // `make_interval(days => $1)` mengelak interpolasi rentetan; $1 ialah int4.
    let res = sqlx::query!(
        "DELETE FROM messages WHERE created_at < now() - make_interval(days => $1)",
        days as i32,
    )
    .execute(&state.db)
    .await?;
    Ok(res.rows_affected())
}
