//! Read models for the admin screens.
use crate::helper::database::{DbResult, get_conn};
use crate::models::user::{COLS, User};
use chrono::NaiveDateTime;
use mysql_async::prelude::*;
use mysql_async::{Params, Row, Value};
use serde::Serialize;

#[derive(Debug, Serialize, Default)]
pub struct Stats {
    pub players: i64,
    pub organizers: i64,
    pub banned: i64,
    pub published: i64,
    pub drafts: i64,
    pub live_labs: i64,
    pub solves_24h: i64,
    pub attempts_24h: i64,
    pub shared_flags_24h: i64,
    pub live_events: i64,
}

pub async fn stats(tenant: &str) -> DbResult<Stats> {
    let mut conn = get_conn().await?;
    let row: Option<Row> = conn
        .exec_first(
            "SELECT
               (SELECT COUNT(*) FROM users WHERE tenant_uuid = ? AND role = 'player' AND banned_at IS NULL),
               (SELECT COUNT(*) FROM users WHERE tenant_uuid = ? AND role <> 'player'),
               (SELECT COUNT(*) FROM users WHERE tenant_uuid = ? AND banned_at IS NOT NULL),
               (SELECT COUNT(*) FROM challs WHERE tenant_uuid = ? AND status = 'published'),
               (SELECT COUNT(*) FROM challs WHERE tenant_uuid = ? AND status = 'draft'),
               (SELECT COUNT(*) FROM instances WHERE tenant_uuid = ? AND status IN ('starting','running')),
               (SELECT COUNT(*) FROM challs_solved WHERE tenant_uuid = ? AND solved_at > UTC_TIMESTAMP() - INTERVAL 1 DAY),
               (SELECT COUNT(*) FROM challs_submissions WHERE tenant_uuid = ? AND created_at > UTC_TIMESTAMP() - INTERVAL 1 DAY),
               (SELECT COUNT(*) FROM audit_log WHERE tenant_uuid = ? AND action = 'flag.shared' AND created_at > UTC_TIMESTAMP() - INTERVAL 1 DAY),
               (SELECT COUNT(*) FROM events WHERE tenant_uuid = ? AND status = 'published' AND starts_at <= UTC_TIMESTAMP() AND ends_at > UTC_TIMESTAMP())",
            Params::Positional(vec![Value::from(tenant); 10]),
        )
        .await?;
    let Some(mut r) = row else {
        return Ok(Stats::default());
    };
    Ok(Stats {
        players: r.take(0).unwrap(),
        organizers: r.take(1).unwrap(),
        banned: r.take(2).unwrap(),
        published: r.take(3).unwrap(),
        drafts: r.take(4).unwrap(),
        live_labs: r.take(5).unwrap(),
        solves_24h: r.take(6).unwrap(),
        attempts_24h: r.take(7).unwrap(),
        shared_flags_24h: r.take(8).unwrap(),
        live_events: r.take(9).unwrap(),
    })
}

/// Solves per hour over the last 24 hours (oldest first, 24 buckets).
pub async fn solves_per_hour(tenant: &str) -> DbResult<Vec<i64>> {
    let rows: Vec<(i64, i64)> = get_conn()
        .await?
        .exec(
            "SELECT TIMESTAMPDIFF(HOUR, solved_at, UTC_TIMESTAMP()) AS h, COUNT(*) FROM challs_solved
             WHERE tenant_uuid = ? AND solved_at > UTC_TIMESTAMP() - INTERVAL 24 HOUR GROUP BY h",
            (tenant,),
        )
        .await?;
    let mut out = vec![0i64; 24];
    for (h, n) in rows {
        if (0..24).contains(&h) {
            out[23 - h as usize] = n;
        }
    }
    Ok(out)
}

pub async fn users(tenant: &str, q: &str, limit: u32) -> DbResult<Vec<User>> {
    let like = format!(
        "%{}%",
        q.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let rows: Vec<Row> = get_conn()
        .await?
        .exec(
            format!("SELECT {COLS} FROM users u WHERE u.tenant_uuid = ? AND (u.username LIKE ? OR COALESCE(u.email, '') LIKE ?) ORDER BY u.created_at DESC LIMIT ?"),
            (tenant, &like, &like, limit),
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|mut r| User::from_row(&mut r))
        .collect())
}

#[derive(Debug, Serialize)]
pub struct Attempt {
    pub username: String,
    pub chall: String,
    pub slug: String,
    pub correct: bool,
    pub event: Option<String>,
    pub ip: String,
    pub at: NaiveDateTime,
}

/// Latest flag attempts, optionally for one user.
pub async fn attempts(tenant: &str, user: Option<&str>, limit: u32) -> DbResult<Vec<Attempt>> {
    let rows: Vec<Row> = get_conn()
        .await?
        .exec(
            "SELECT u.username, c.name, c.slug, s.correct, e.title, s.ip, s.created_at FROM challs_submissions s
             JOIN users u ON u.uuid = s.user_uuid JOIN challs c ON c.uuid = s.chall_uuid LEFT JOIN events e ON e.uuid = s.event_uuid
             WHERE s.tenant_uuid = ? AND (? IS NULL OR s.user_uuid = ?) ORDER BY s.id DESC LIMIT ?",
            (tenant, user, user, limit),
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|mut r| Attempt {
            username: r.take(0).unwrap(),
            chall: r.take(1).unwrap(),
            slug: r.take(2).unwrap(),
            correct: r.take::<i64, _>(3).unwrap() != 0,
            event: r.take(4).unwrap(),
            ip: r.take(5).unwrap(),
            at: r.take(6).unwrap(),
        })
        .collect())
}

#[derive(Debug, Serialize)]
pub struct AuditRow {
    pub actor: Option<String>,
    pub action: String,
    pub target_type: String,
    pub target_uuid: Option<String>,
    pub meta: Option<String>,
    pub at: NaiveDateTime,
}

pub async fn audit(tenant: &str, action_prefix: &str, limit: u32) -> DbResult<Vec<AuditRow>> {
    get_conn()
        .await?
        .exec_map(
            "SELECT u.username, a.action, a.target_type, a.target_uuid, CAST(a.meta AS CHAR), a.created_at FROM audit_log a
             LEFT JOIN users u ON u.uuid = a.actor_uuid WHERE a.tenant_uuid = ? AND a.action LIKE ? ORDER BY a.id DESC LIMIT ?",
            (tenant, format!("{}%", action_prefix.replace('%', "")), limit),
            |(actor, action, target_type, target_uuid, meta, at)| AuditRow { actor, action, target_type, target_uuid, meta, at },
        )
        .await
}

#[derive(Debug, Serialize)]
pub struct LabRow {
    pub uuid: String,
    pub username: String,
    pub team: Option<String>,
    pub chall: String,
    pub status: String,
    pub host_port: i32,
    pub expires_at: NaiveDateTime,
    pub created_at: NaiveDateTime,
}

pub async fn labs(tenant: &str) -> DbResult<Vec<LabRow>> {
    get_conn()
        .await?
        .exec_map(
            "SELECT i.uuid, u.username, t.name, c.name, i.status, i.host_port, i.expires_at, i.created_at FROM instances i
             JOIN users u ON u.uuid = i.user_uuid JOIN challs c ON c.uuid = i.chall_uuid LEFT JOIN teams t ON t.uuid = i.team_uuid
             WHERE i.tenant_uuid = ? AND i.status IN ('starting','running') ORDER BY i.created_at DESC",
            (tenant,),
            |(uuid, username, team, chall, status, host_port, expires_at, created_at)| LabRow { uuid, username, team, chall, status, host_port, expires_at, created_at },
        )
        .await
}

/// Live labs of a user (theirs, not their team's): stopped when the user is banned.
pub async fn user_labs(user: &str) -> DbResult<Vec<(String, String, String)>> {
    get_conn()
        .await?
        .exec("SELECT uuid, container_id, network_id FROM instances WHERE user_uuid = ? AND status IN ('starting','running')", (user,))
        .await
}
