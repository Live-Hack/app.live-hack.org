use crate::helper::crypto::{random_token, sha256};
use crate::helper::database::{DbResult, get_conn};
use crate::models::user::{COLS, User};
use chrono::{NaiveDateTime, Utc};
use mysql_async::Row;
use mysql_async::prelude::*;
use serde::Serialize;

pub const COOKIE: &str = "lh_session";
pub const TTL_DAYS: i64 = 30;

/// Creates a session and returns the raw token (only its hash is stored).
pub async fn create(user: &User, user_agent: &str, ip: &str) -> DbResult<String> {
    let token = random_token();
    let ua: String = user_agent.chars().take(255).collect();
    get_conn()
        .await?
        .exec_drop(
            "INSERT INTO sessions (token_hash, tenant_uuid, user_uuid, expires_at, user_agent, ip)
             VALUES (?, ?, ?, NOW() + INTERVAL ? DAY, ?, ?)",
            (
                sha256(&token),
                &user.tenant_uuid,
                &user.uuid,
                TTL_DAYS,
                ua,
                ip,
            ),
        )
        .await?;
    Ok(token)
}

/// Token → active, non-banned user of this tenant. Extends the expiry at most once a day.
pub async fn lookup(tenant: &str, token: &str) -> DbResult<Option<User>> {
    if token.len() != 64 {
        return Ok(None);
    }
    let hash = sha256(token);
    let mut conn = get_conn().await?;
    let row: Option<Row> = conn
        .exec_first(
            format!(
                "SELECT {COLS}, s.expires_at < NOW() + INTERVAL {} DAY FROM sessions s
                 JOIN users u ON u.uuid = s.user_uuid
                 WHERE s.token_hash = ? AND s.tenant_uuid = ? AND s.expires_at > NOW() AND u.banned_at IS NULL",
                TTL_DAYS - 1
            ),
            (&hash, tenant),
        )
        .await?;
    let Some(mut row) = row else { return Ok(None) };
    let refresh: i64 = row.take(7).unwrap();
    if refresh != 0 {
        conn.exec_drop(
            "UPDATE sessions SET expires_at = NOW() + INTERVAL ? DAY WHERE token_hash = ?",
            (TTL_DAYS, &hash),
        )
        .await?;
    }
    Ok(Some(User::from_row(&mut row)))
}

pub async fn delete(token: &str) -> DbResult<()> {
    get_conn()
        .await?
        .exec_drop(
            "DELETE FROM sessions WHERE token_hash = ?",
            (sha256(token),),
        )
        .await
}

/// Revokes every session of the user except `keep` (the current one), or all if None.
pub async fn delete_all(user_uuid: &str, keep: Option<&str>) -> DbResult<u64> {
    let mut conn = get_conn().await?;
    match keep {
        Some(t) => {
            conn.exec_drop(
                "DELETE FROM sessions WHERE user_uuid = ? AND token_hash <> ?",
                (user_uuid, sha256(t)),
            )
            .await?
        }
        None => {
            conn.exec_drop("DELETE FROM sessions WHERE user_uuid = ?", (user_uuid,))
                .await?
        }
    }
    Ok(conn.affected_rows())
}

#[derive(Serialize)]
pub struct SessionInfo {
    pub current: bool,
    pub user_agent: String,
    pub ip: String,
    pub created_at: NaiveDateTime,
}

pub async fn list(user_uuid: &str, current: &str) -> DbResult<Vec<SessionInfo>> {
    let current = sha256(current);
    get_conn()
        .await?
        .exec_map(
            "SELECT token_hash, user_agent, ip, created_at FROM sessions WHERE user_uuid = ? AND expires_at > NOW() ORDER BY created_at DESC LIMIT 50",
            (user_uuid,),
            |(h, user_agent, ip, created_at): (Vec<u8>, String, String, NaiveDateTime)| SessionInfo {
                current: h == current,
                user_agent,
                ip,
                created_at,
            },
        )
        .await
}

pub async fn cleanup() -> DbResult<()> {
    let _ = Utc::now();
    get_conn()
        .await?
        .query_drop("DELETE FROM sessions WHERE expires_at < NOW()")
        .await?;
    get_conn()
        .await?
        .query_drop("DELETE FROM user_tokens WHERE expires_at < NOW() OR uses_left <= 0")
        .await
}

pub fn cookie_header(token: &str, max_age: i64) -> String {
    let secure = if crate::helper::config::CONFIG.secure_cookies {
        "; Secure"
    } else {
        ""
    };
    format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}")
}
