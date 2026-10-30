use crate::helper::database::{DbResult, get_conn};
use chrono::NaiveDateTime;
use mysql_async::prelude::*;
use mysql_async::{Row, TxOpts};
use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Player,
    Author,
    Admin,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Player => "player",
            Role::Author => "author",
            Role::Admin => "admin",
        }
    }
    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "player" => Some(Role::Player),
            "author" => Some(Role::Author),
            "admin" => Some(Role::Admin),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub uuid: String,
    #[serde(skip)]
    pub tenant_uuid: String,
    pub username: String,
    pub email: Option<String>,
    pub role: Role,
    pub banned: bool,
    pub created_at: NaiveDateTime,
}

pub const COLS: &str =
    "u.uuid, u.tenant_uuid, u.username, u.email, u.role, u.banned_at IS NOT NULL, u.created_at";

impl User {
    /// Builds from the first 7 columns of `COLS`.
    pub fn from_row(row: &mut Row) -> User {
        let role: String = row.take(4).unwrap();
        User {
            uuid: row.take(0).unwrap(),
            tenant_uuid: row.take(1).unwrap(),
            username: row.take(2).unwrap(),
            email: row.take(3).unwrap(),
            role: Role::parse(&role).unwrap_or(Role::Player),
            banned: row.take::<i64, _>(5).unwrap() != 0,
            created_at: row.take(6).unwrap(),
        }
    }
}

pub fn valid_username(name: &str) -> bool {
    (3..=32).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

pub fn valid_email(email: &str) -> bool {
    email.len() <= 255
        && email
            .split_once('@')
            .is_some_and(|(a, d)| !a.is_empty() && d.contains('.') && !d.starts_with('.'))
        && !email.chars().any(char::is_whitespace)
}

pub const MIN_PASSWORD: usize = 10;

pub async fn get(tenant: &str, uuid: &str) -> DbResult<Option<User>> {
    let mut conn = get_conn().await?;
    let row: Option<Row> = conn
        .exec_first(
            format!("SELECT {COLS} FROM users u WHERE u.tenant_uuid = ? AND u.uuid = ?"),
            (tenant, uuid),
        )
        .await?;
    Ok(row.map(|mut r| User::from_row(&mut r)))
}

pub async fn get_by_username(tenant: &str, username: &str) -> DbResult<Option<User>> {
    let mut conn = get_conn().await?;
    let row: Option<Row> = conn
        .exec_first(
            format!("SELECT {COLS} FROM users u WHERE u.tenant_uuid = ? AND u.username = ?"),
            (tenant, username),
        )
        .await?;
    Ok(row.map(|mut r| User::from_row(&mut r)))
}

/// Login by username or email; returns the password hash with the user.
pub async fn find_for_login(tenant: &str, login: &str) -> DbResult<Option<(User, String)>> {
    let mut conn = get_conn().await?;
    let row: Option<Row> = conn
        .exec_first(
            format!("SELECT {COLS}, u.password_hash FROM users u WHERE u.tenant_uuid = ? AND (u.username = ? OR u.email = ?) LIMIT 1"),
            (tenant, login, login),
        )
        .await?;
    Ok(row.map(|mut r| {
        let hash: String = r.take(7).unwrap();
        (User::from_row(&mut r), hash)
    }))
}

pub async fn password_hash(uuid: &str) -> DbResult<Option<String>> {
    get_conn()
        .await?
        .exec_first("SELECT password_hash FROM users WHERE uuid = ?", (uuid,))
        .await
}

pub async fn create(
    tenant: &str,
    username: &str,
    email: Option<&str>,
    hash: &str,
    role: Role,
) -> DbResult<String> {
    let uuid = uuid::Uuid::new_v4().to_string();
    get_conn()
        .await?
        .exec_drop(
            "INSERT INTO users (uuid, tenant_uuid, username, email, password_hash, role) VALUES (?, ?, ?, ?, ?, ?)",
            (&uuid, tenant, username, email, hash, role.as_str()),
        )
        .await?;
    Ok(uuid)
}

/// Creates the first admin of a tenant. Returns None if the tenant already has users
/// (row lock on the tenant makes two concurrent setups safe).
pub async fn create_first_admin(
    tenant: &str,
    username: &str,
    hash: &str,
) -> DbResult<Option<String>> {
    let mut conn = get_conn().await?;
    let mut tx = conn.start_transaction(TxOpts::default()).await?;
    let _: Option<String> = tx
        .exec_first(
            "SELECT uuid FROM tenants WHERE uuid = ? FOR UPDATE",
            (tenant,),
        )
        .await?;
    let count: i64 = tx
        .exec_first(
            "SELECT COUNT(*) FROM users WHERE tenant_uuid = ?",
            (tenant,),
        )
        .await?
        .unwrap_or(0);
    if count > 0 {
        tx.rollback().await?;
        return Ok(None);
    }
    let uuid = uuid::Uuid::new_v4().to_string();
    tx.exec_drop(
        "INSERT INTO users (uuid, tenant_uuid, username, password_hash, role) VALUES (?, ?, ?, ?, 'admin')",
        (&uuid, tenant, username, hash),
    )
    .await?;
    tx.commit().await?;
    mark_setup_done(tenant);
    Ok(Some(uuid))
}

static SETUP_DONE: Lazy<RwLock<HashSet<String>>> = Lazy::new(Default::default);

fn mark_setup_done(tenant: &str) {
    SETUP_DONE.write().unwrap().insert(tenant.to_string());
}

/// True once the tenant has at least one user. Cached: a tenant never goes back to empty.
pub async fn setup_done(tenant: &str) -> DbResult<bool> {
    if SETUP_DONE.read().unwrap().contains(tenant) {
        return Ok(true);
    }
    let any: Option<i64> = get_conn()
        .await?
        .exec_first(
            "SELECT 1 FROM users WHERE tenant_uuid = ? LIMIT 1",
            (tenant,),
        )
        .await?;
    if any.is_some() {
        mark_setup_done(tenant);
    }
    Ok(any.is_some())
}

pub async fn set_username(tenant: &str, uuid: &str, username: &str) -> DbResult<()> {
    get_conn()
        .await?
        .exec_drop(
            "UPDATE users SET username = ? WHERE tenant_uuid = ? AND uuid = ?",
            (username, tenant, uuid),
        )
        .await
}

pub async fn set_password(tenant: &str, uuid: &str, hash: &str) -> DbResult<()> {
    get_conn()
        .await?
        .exec_drop(
            "UPDATE users SET password_hash = ? WHERE tenant_uuid = ? AND uuid = ?",
            (hash, tenant, uuid),
        )
        .await
}

pub async fn set_role(tenant: &str, uuid: &str, role: Role) -> DbResult<()> {
    get_conn()
        .await?
        .exec_drop(
            "UPDATE users SET role = ? WHERE tenant_uuid = ? AND uuid = ?",
            (role.as_str(), tenant, uuid),
        )
        .await
}

pub async fn set_banned(tenant: &str, uuid: &str, banned: bool) -> DbResult<()> {
    get_conn()
        .await?
        .exec_drop(
            "UPDATE users SET banned_at = IF(?, COALESCE(banned_at, NOW()), NULL) WHERE tenant_uuid = ? AND uuid = ?",
            (banned, tenant, uuid),
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation() {
        assert!(valid_username("alice_01"));
        assert!(!valid_username("al"));
        assert!(!valid_username("al ice"));
        assert!(!valid_username("<script>"));
        assert!(valid_email("a@b.io"));
        assert!(!valid_email("a@b"));
        assert!(!valid_email("a b@c.io"));
    }
}
