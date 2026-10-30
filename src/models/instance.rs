use crate::helper::database::{DbResult, get_conn};
use crate::models::is_duplicate;
use crate::models::solve::Scope;
use chrono::NaiveDateTime;
use mysql_async::Row;
use mysql_async::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Runtime {
    pub image: String,
    pub port: i32,
    pub protocol: String,
    pub cpu_millis: i32,
    pub memory_mb: i32,
    pub ttl_minutes: i32,
    /// KEY=VALUE per line
    pub env: String,
    pub readonly_rootfs: bool,
}

impl Runtime {
    pub fn validate(mut self) -> Result<Runtime, String> {
        self.image = self.image.trim().to_string();
        if self.image.is_empty()
            || self.image.len() > 255
            || self.image.chars().any(|c| c.is_whitespace())
        {
            return Err(
                "Image: a Docker image reference, e.g. registry.example.com/web-101:1.0".into(),
            );
        }
        if !(1..=65535).contains(&self.port) {
            return Err("Port: 1 to 65535.".into());
        }
        if self.protocol.is_empty() {
            self.protocol = "http".into();
        }
        if !["http", "tcp"].contains(&self.protocol.as_str()) {
            return Err("Protocol: http or tcp.".into());
        }
        if !(50..=8000).contains(&self.cpu_millis) {
            return Err("CPU: 50 to 8000 millicores.".into());
        }
        if !(16..=16384).contains(&self.memory_mb) {
            return Err("Memory: 16 to 16384 MB.".into());
        }
        if !(5..=1440).contains(&self.ttl_minutes) {
            return Err("Lifetime: 5 to 1440 minutes.".into());
        }
        for line in self.env.lines().filter(|l| !l.trim().is_empty()) {
            let Some((k, _)) = line.split_once('=') else {
                return Err(format!("Environment line '{line}' needs KEY=VALUE."));
            };
            if k.is_empty()
                || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                || k == "FLAG"
            {
                return Err(format!(
                    "Invalid variable name '{k}' (FLAG is set by the platform)."
                ));
            }
        }
        Ok(self)
    }

    pub fn env_pairs(&self) -> Vec<String> {
        self.env
            .lines()
            .map(str::trim)
            .filter(|l| l.contains('='))
            .map(String::from)
            .collect()
    }
}

pub async fn runtime(chall: &str) -> DbResult<Option<Runtime>> {
    let r: Option<Row> = get_conn()
        .await?
        .exec_first("SELECT image, port, protocol, cpu_millis, memory_mb, ttl_minutes, env, readonly_rootfs FROM challs_runtime WHERE chall_uuid = ?", (chall,))
        .await?;
    Ok(r.map(|mut r| Runtime {
        image: r.take(0).unwrap(),
        port: r.take(1).unwrap(),
        protocol: r.take(2).unwrap(),
        cpu_millis: r.take(3).unwrap(),
        memory_mb: r.take(4).unwrap(),
        ttl_minutes: r.take(5).unwrap(),
        env: r.take(6).unwrap(),
        readonly_rootfs: r.take::<i64, _>(7).unwrap() != 0,
    }))
}

pub async fn set_runtime(chall: &str, rt: &Runtime) -> DbResult<()> {
    get_conn()
        .await?
        .exec_drop(
            "INSERT INTO challs_runtime (chall_uuid, image, port, protocol, cpu_millis, memory_mb, ttl_minutes, env, readonly_rootfs)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) AS n
             ON DUPLICATE KEY UPDATE image = n.image, port = n.port, protocol = n.protocol, cpu_millis = n.cpu_millis,
               memory_mb = n.memory_mb, ttl_minutes = n.ttl_minutes, env = n.env, readonly_rootfs = n.readonly_rootfs",
            (chall, &rt.image, rt.port, &rt.protocol, rt.cpu_millis, rt.memory_mb, rt.ttl_minutes, &rt.env, rt.readonly_rootfs),
        )
        .await
}

#[derive(Debug, Clone, Serialize)]
pub struct Instance {
    pub uuid: String,
    #[serde(skip)]
    pub tenant_uuid: String,
    pub chall_uuid: String,
    pub user_uuid: String,
    pub team_uuid: Option<String>,
    pub event_uuid: Option<String>,
    #[serde(skip)]
    pub container_id: String,
    #[serde(skip)]
    pub network_id: String,
    pub host_port: i32,
    #[serde(skip)]
    pub flag: String,
    pub status: String,
    pub error: String,
    pub expires_at: NaiveDateTime,
    pub created_at: NaiveDateTime,
}

const COLS: &str = "uuid, tenant_uuid, chall_uuid, user_uuid, team_uuid, event_uuid, container_id, network_id, host_port, flag, status, error, expires_at, created_at";

fn from_row(mut r: Row) -> Instance {
    Instance {
        uuid: r.take(0).unwrap(),
        tenant_uuid: r.take(1).unwrap(),
        chall_uuid: r.take(2).unwrap(),
        user_uuid: r.take(3).unwrap(),
        team_uuid: r.take(4).unwrap(),
        event_uuid: r.take(5).unwrap(),
        container_id: r.take(6).unwrap(),
        network_id: r.take(7).unwrap(),
        host_port: r.take(8).unwrap(),
        flag: r.take(9).unwrap(),
        status: r.take(10).unwrap(),
        error: r.take(11).unwrap(),
        expires_at: r.take(12).unwrap(),
        created_at: r.take(13).unwrap(),
    }
}

/// The live (starting/running) instance of a scope, whatever the challenge.
pub async fn active_for_scope(scope: &Scope, user: &str) -> DbResult<Option<Instance>> {
    let r: Option<Row> = get_conn()
        .await?
        .exec_first(
            format!("SELECT {COLS} FROM instances WHERE active_key = ?"),
            (format!("{}:{}", scope.event_key(), scope.scope_key(user)),),
        )
        .await?;
    Ok(r.map(from_row))
}

/// Latest instance of this scope on this challenge (live or not), for its flag.
pub async fn latest(chall: &str, scope: &Scope, user: &str) -> DbResult<Option<Instance>> {
    let r: Option<Row> = get_conn()
        .await?
        .exec_first(
            format!("SELECT {COLS} FROM instances WHERE chall_uuid = ? AND event_key = ? AND scope_key = ? ORDER BY created_at DESC LIMIT 1"),
            (chall, scope.event_key(), scope.scope_key(user)),
        )
        .await?;
    Ok(r.map(from_row))
}

/// Reserves the scope's single live slot. None if the scope already has a live instance.
pub async fn reserve(
    tenant: &str,
    chall: &str,
    user: &str,
    scope: &Scope,
    flag: &str,
    ttl_minutes: i32,
) -> DbResult<Option<String>> {
    let uuid = uuid::Uuid::new_v4().to_string();
    let res = get_conn()
        .await?
        .exec_drop(
            "INSERT INTO instances (uuid, tenant_uuid, chall_uuid, user_uuid, team_uuid, event_uuid, flag, status, expires_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 'starting', UTC_TIMESTAMP() + INTERVAL ? MINUTE)",
            (&uuid, tenant, chall, user, &scope.team_uuid, &scope.event_uuid, flag, ttl_minutes),
        )
        .await;
    match res {
        Ok(()) => Ok(Some(uuid)),
        Err(e) if is_duplicate(&e) => Ok(None),
        Err(e) => Err(e),
    }
}

pub async fn mark_running(uuid: &str, container: &str, network: &str, port: i32) -> DbResult<()> {
    get_conn()
        .await?
        .exec_drop("UPDATE instances SET status = 'running', container_id = ?, network_id = ?, host_port = ? WHERE uuid = ?", (container, network, port, uuid))
        .await
}

pub async fn mark_stopped(uuid: &str, failed: Option<&str>) -> DbResult<()> {
    get_conn()
        .await?
        .exec_drop(
            "UPDATE instances SET status = IF(? IS NULL, 'stopped', 'failed'), error = COALESCE(?, error), stopped_at = UTC_TIMESTAMP() WHERE uuid = ?",
            (failed, failed.map(|e| e.chars().take(250).collect::<String>()), uuid),
        )
        .await
}

/// Pushes the expiry by one lifetime, capped at 4 lifetimes after creation.
pub async fn extend(uuid: &str, ttl_minutes: i32) -> DbResult<bool> {
    let mut conn = get_conn().await?;
    conn.exec_drop(
        "UPDATE instances SET expires_at = LEAST(expires_at + INTERVAL ? MINUTE, created_at + INTERVAL ? MINUTE)
         WHERE uuid = ? AND status = 'running' AND expires_at < created_at + INTERVAL ? MINUTE",
        (ttl_minutes, ttl_minutes * 4, uuid, ttl_minutes * 4),
    )
    .await?;
    Ok(conn.affected_rows() == 1)
}

pub async fn count_live() -> DbResult<i64> {
    Ok(get_conn()
        .await?
        .query_first("SELECT COUNT(*) FROM instances WHERE status IN ('starting','running')")
        .await?
        .unwrap_or(0))
}

/// Live instances past their expiry, or of events that ended.
pub async fn expired() -> DbResult<Vec<Instance>> {
    let rows: Vec<Row> = get_conn()
        .await?
        .query(format!(
            "SELECT {COLS} FROM instances i WHERE i.status IN ('starting','running') AND (i.expires_at <= UTC_TIMESTAMP()
               OR (i.event_uuid IS NOT NULL AND EXISTS(SELECT 1 FROM events e WHERE e.uuid = i.event_uuid AND e.ends_at <= UTC_TIMESTAMP()))
               OR (i.status = 'starting' AND i.created_at < UTC_TIMESTAMP() - INTERVAL 10 MINUTE))"
        ))
        .await?;
    Ok(rows.into_iter().map(from_row).collect())
}

pub async fn live() -> DbResult<Vec<Instance>> {
    let rows: Vec<Row> = get_conn()
        .await?
        .query(format!(
            "SELECT {COLS} FROM instances WHERE status IN ('starting','running')"
        ))
        .await?;
    Ok(rows.into_iter().map(from_row).collect())
}

pub async fn get(tenant: &str, uuid: &str) -> DbResult<Option<Instance>> {
    let r: Option<Row> = get_conn()
        .await?
        .exec_first(
            format!("SELECT {COLS} FROM instances WHERE tenant_uuid = ? AND uuid = ?"),
            (tenant, uuid),
        )
        .await?;
    Ok(r.map(from_row))
}

/// The scope that owns this flag value on this challenge, if it is someone's instance flag.
pub async fn flag_owner(tenant: &str, chall: &str, flag: &str) -> DbResult<Option<Instance>> {
    let r: Option<Row> = get_conn()
        .await?
        .exec_first(format!("SELECT {COLS} FROM instances WHERE tenant_uuid = ? AND chall_uuid = ? AND flag = ? LIMIT 1"), (tenant, chall, flag))
        .await?;
    Ok(r.map(from_row))
}

/// Live labs of a user: their own, and their teams' in events. With the challenge name/slug.
pub async fn live_for_user(user: &str) -> DbResult<Vec<(Instance, String, String)>> {
    let rows: Vec<Row> = get_conn()
        .await?
        .exec(
            format!(
                "SELECT {}, c.name, c.slug FROM instances i JOIN challs c ON c.uuid = i.chall_uuid
                 WHERE i.status IN ('starting','running')
                   AND (i.user_uuid = ? OR i.team_uuid IN (SELECT team_uuid FROM teams_members WHERE user_uuid = ?))",
                COLS.split(", ").map(|c| format!("i.{c}")).collect::<Vec<_>>().join(", ")
            ),
            (user, user),
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|mut r| {
            let slug: String = r.take(15).unwrap();
            let name: String = r.take(14).unwrap();
            (from_row(r), name, slug)
        })
        .collect())
}
