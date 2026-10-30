use crate::helper::database::{DbResult, get_conn};
use mysql_async::prelude::*;

/// Counts one hit in `bucket` for the current fixed window and returns true while under `max`.
/// Stored in MySQL so every app replica shares the same counters. One round trip.
pub async fn allow(bucket: &str, max: u32, window_secs: i64) -> DbResult<bool> {
    let now = chrono::Utc::now().timestamp();
    // window = start of the current window, in unix seconds
    let window = now - now % window_secs;
    let mut conn = get_conn().await?;
    conn.exec_drop(
        "INSERT INTO rate_limits (bucket, win, hits) VALUES (?, ?, 1) AS n
         ON DUPLICATE KEY UPDATE
            hits = LAST_INSERT_ID(IF(rate_limits.win = n.win, rate_limits.hits + 1, 1)),
            win = n.win",
        (bucket, window),
    )
    .await?;
    // 0 means the row was just inserted (first hit of the window)
    let hits = conn.last_insert_id().unwrap_or(0).max(1);
    Ok(hits <= max as u64)
}

pub async fn cleanup() -> DbResult<()> {
    let oldest = chrono::Utc::now().timestamp() - 2 * 3600;
    get_conn()
        .await?
        .exec_drop("DELETE FROM rate_limits WHERE win < ?", (oldest,))
        .await
}
