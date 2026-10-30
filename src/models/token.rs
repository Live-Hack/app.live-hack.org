use crate::helper::crypto::{random_token, sha256};
use crate::helper::database::{DbResult, get_conn};
use mysql_async::prelude::*;

#[derive(Clone, Copy)]
pub enum Kind {
    Reset,
    Invite,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Reset => "reset",
            Kind::Invite => "invite",
        }
    }
}

pub async fn create(
    tenant: &str,
    kind: Kind,
    user_uuid: Option<&str>,
    uses: u32,
    ttl_minutes: u32,
    created_by: &str,
) -> DbResult<String> {
    let token = random_token();
    get_conn()
        .await?
        .exec_drop(
            "INSERT INTO user_tokens (token_hash, tenant_uuid, kind, user_uuid, uses_left, expires_at, created_by)
             VALUES (?, ?, ?, ?, ?, NOW() + INTERVAL ? MINUTE, ?)",
            (sha256(&token), tenant, kind.as_str(), user_uuid, uses, ttl_minutes, created_by),
        )
        .await?;
    Ok(token)
}

/// True if the token is valid right now (does not consume it).
pub async fn peek(tenant: &str, kind: Kind, token: &str) -> DbResult<bool> {
    let row: Option<i64> = get_conn()
        .await?
        .exec_first(
            "SELECT 1 FROM user_tokens WHERE token_hash = ? AND tenant_uuid = ? AND kind = ? AND uses_left > 0 AND expires_at > NOW()",
            (sha256(token), tenant, kind.as_str()),
        )
        .await?;
    Ok(row.is_some())
}

/// Atomically uses one use of the token. Returns Some(user_uuid) on success
/// (user_uuid is None for invites).
pub async fn consume(tenant: &str, kind: Kind, token: &str) -> DbResult<Option<Option<String>>> {
    let hash = sha256(token);
    let mut conn = get_conn().await?;
    conn.exec_drop(
        "UPDATE user_tokens SET uses_left = uses_left - 1
         WHERE token_hash = ? AND tenant_uuid = ? AND kind = ? AND uses_left > 0 AND expires_at > NOW()",
        (&hash, tenant, kind.as_str()),
    )
    .await?;
    if conn.affected_rows() != 1 {
        return Ok(None);
    }
    let user: Option<Option<String>> = conn
        .exec_first(
            "SELECT user_uuid FROM user_tokens WHERE token_hash = ?",
            (&hash,),
        )
        .await?;
    Ok(user)
}

/// Gives back a use consumed by a request that then failed (e.g. username taken).
pub async fn refund(token: &str) -> DbResult<()> {
    get_conn()
        .await?
        .exec_drop(
            "UPDATE user_tokens SET uses_left = uses_left + 1 WHERE token_hash = ?",
            (sha256(token),),
        )
        .await
}
