//! Organizer-side challenge management (create, edit, publish, files, authors).
use crate::helper::database::{DbResult, get_conn};
use crate::models::chall::DIFFICULTIES;
use chrono::NaiveDateTime;
use mysql_async::prelude::*;
use mysql_async::{Params, Row, Value};
use serde::{Deserialize, Serialize};

pub const CTYPES: &[&str] = &["file", "static", "instance"];
pub const FLAG_TYPES: &[&str] = &["static", "case_insensitive", "regex", "per_instance"];
pub const STATUSES: &[&str] = &["draft", "published", "archived"];

/// Editable fields of a challenge, as sent by the editor or read from challenge.yml.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Fields {
    pub name: String,
    pub description: String,
    pub category: String,
    pub difficulty: String,
    pub points: i64,
    pub ctype: String,
    pub static_url: String,
    pub flag_type: String,
    pub flag: String,
}

impl Fields {
    /// Normalises and validates; returns a user-facing message on error.
    pub fn validate(mut self) -> Result<Fields, String> {
        self.name = self.name.trim().to_string();
        self.category = self.category.trim().to_lowercase();
        self.static_url = self.static_url.trim().to_string();
        self.flag = self.flag.trim().to_string();
        if self.name.is_empty() || self.name.chars().count() > 120 {
            return Err("Name: 1 to 120 characters.".into());
        }
        if self.description.len() > 100_000 {
            return Err("Description is too long (100 000 characters max).".into());
        }
        if self.category.is_empty() {
            self.category = "misc".into();
        }
        if self.category.len() > 32
            || !self
                .category
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err("Category: letters, digits and - only (32 max).".into());
        }
        if self.difficulty.is_empty() {
            self.difficulty = "easy".into();
        }
        if !DIFFICULTIES.contains(&self.difficulty.as_str()) {
            return Err("Unknown difficulty.".into());
        }
        if !(0..=100_000).contains(&self.points) {
            return Err("Points: 0 to 100 000.".into());
        }
        if self.ctype.is_empty() {
            self.ctype = "file".into();
        }
        if !CTYPES.contains(&self.ctype.as_str()) {
            return Err("Unknown challenge type.".into());
        }
        // only http(s) links: a javascript: URL here would run in players' browsers
        if !self.static_url.is_empty()
            && !(self.static_url.starts_with("https://") || self.static_url.starts_with("http://"))
        {
            return Err("URL must start with http:// or https://.".into());
        }
        if self.static_url.len() > 512 {
            return Err("URL is too long.".into());
        }
        if self.flag_type.is_empty() {
            self.flag_type = "static".into();
        }
        if !FLAG_TYPES.contains(&self.flag_type.as_str()) {
            return Err("Unknown flag type.".into());
        }
        if self.flag.len() > 512 {
            return Err("Flag is too long (512 max).".into());
        }
        if self.flag_type == "regex"
            && !self.flag.is_empty()
            && regex::Regex::new(&self.flag).is_err()
        {
            return Err("The flag regex does not compile.".into());
        }
        Ok(self)
    }
}

/// "Cookie Monster!" → "cookie-monster"
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let s: String = out.trim_end_matches('-').chars().take(60).collect();
    if s.is_empty() {
        "challenge".into()
    } else {
        s.trim_end_matches('-').to_string()
    }
}

async fn unique_slug(tenant: &str, base: &str) -> DbResult<String> {
    let taken: Vec<String> = get_conn()
        .await?
        .exec(
            "SELECT slug FROM challs WHERE tenant_uuid = ? AND (slug = ? OR slug LIKE ?)",
            (tenant, base, format!("{base}-%")),
        )
        .await?;
    if !taken.iter().any(|s| s == base) {
        return Ok(base.to_string());
    }
    Ok((2..)
        .map(|i| format!("{base}-{i}"))
        .find(|s| !taken.contains(s))
        .unwrap())
}

pub async fn create(tenant: &str, author: &str, f: &Fields) -> DbResult<(String, String)> {
    let uuid = uuid::Uuid::new_v4().to_string();
    let slug = unique_slug(tenant, &slugify(&f.name)).await?;
    let mut conn = get_conn().await?;
    conn.exec_drop(
        "INSERT INTO challs (uuid, tenant_uuid, slug, name, description, category, difficulty, points, ctype, static_url, flag_type, flag)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        (&uuid, tenant, &slug, &f.name, &f.description, &f.category, &f.difficulty, f.points, &f.ctype, &f.static_url, &f.flag_type, &f.flag),
    )
    .await?;
    conn.exec_drop(
        "INSERT INTO challs_authors (chall_uuid, user_uuid, role) VALUES (?, ?, 'owner')",
        (&uuid, author),
    )
    .await?;
    Ok((uuid, slug))
}

pub async fn update(tenant: &str, uuid: &str, f: &Fields) -> DbResult<()> {
    get_conn()
        .await?
        .exec_drop(
            "UPDATE challs SET name = ?, description = ?, category = ?, difficulty = ?, points = ?, ctype = ?, static_url = ?, flag_type = ?, flag = ?
             WHERE tenant_uuid = ? AND uuid = ?",
            (&f.name, &f.description, &f.category, &f.difficulty, f.points, &f.ctype, &f.static_url, &f.flag_type, &f.flag, tenant, uuid),
        )
        .await
}

pub async fn set_status(tenant: &str, uuid: &str, status: &str) -> DbResult<()> {
    get_conn()
        .await?
        .exec_drop(
            "UPDATE challs SET status = ?, published_at = IF(? = 'published', COALESCE(published_at, NOW()), published_at)
             WHERE tenant_uuid = ? AND uuid = ?",
            (status, status, tenant, uuid),
        )
        .await
}

pub async fn delete(tenant: &str, uuid: &str) -> DbResult<Vec<String>> {
    let mut conn = get_conn().await?;
    let keys: Vec<String> = conn
        .exec(
            "SELECT storage_key FROM challs_files WHERE chall_uuid = ?",
            (uuid,),
        )
        .await?;
    // solves reference users without cascade (see migration 003); remove them first
    conn.exec_drop("DELETE FROM challs_solved WHERE chall_uuid = ?", (uuid,))
        .await?;
    conn.exec_drop(
        "DELETE FROM challs_submissions WHERE chall_uuid = ?",
        (uuid,),
    )
    .await?;
    conn.exec_drop(
        "DELETE FROM challs WHERE tenant_uuid = ? AND uuid = ?",
        (tenant, uuid),
    )
    .await?;
    Ok(keys)
}

#[derive(Debug, Serialize)]
pub struct Row_ {
    pub uuid: String,
    pub slug: String,
    pub name: String,
    pub category: String,
    pub points: i32,
    pub status: String,
    pub ctype: String,
    pub solves: i64,
    pub updated_at: NaiveDateTime,
}

/// Every challenge an organizer may manage: all of them for admins, their own for authors.
pub async fn list(
    tenant: &str,
    viewer: &str,
    admin: bool,
    q: &str,
    status: &str,
) -> DbResult<Vec<Row_>> {
    let mut wh = String::from("c.tenant_uuid = ?");
    let mut args: Vec<Value> = vec![tenant.into()];
    if !admin {
        wh.push_str(" AND EXISTS(SELECT 1 FROM challs_authors a WHERE a.chall_uuid = c.uuid AND a.user_uuid = ?)");
        args.push(viewer.into());
    }
    if !q.is_empty() {
        wh.push_str(" AND (c.name LIKE ? OR c.category LIKE ?)");
        let like = format!(
            "%{}%",
            q.replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        args.push(like.clone().into());
        args.push(like.into());
    }
    if STATUSES.contains(&status) {
        wh.push_str(" AND c.status = ?");
        args.push(status.into());
    }
    let rows: Vec<Row> = get_conn()
        .await?
        .exec(
            format!(
                "SELECT c.uuid, c.slug, c.name, c.category, c.points, c.status, c.ctype,
                        (SELECT COUNT(*) FROM challs_solved s WHERE s.chall_uuid = c.uuid), c.updated_at
                 FROM challs c WHERE {wh} ORDER BY FIELD(c.status, 'draft', 'published', 'archived'), c.updated_at DESC LIMIT 1000"
            ),
            Params::Positional(args),
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|mut r| Row_ {
            uuid: r.take(0).unwrap(),
            slug: r.take(1).unwrap(),
            name: r.take(2).unwrap(),
            category: r.take(3).unwrap(),
            points: r.take(4).unwrap(),
            status: r.take(5).unwrap(),
            ctype: r.take(6).unwrap(),
            solves: r.take(7).unwrap(),
            updated_at: r.take(8).unwrap(),
        })
        .collect())
}

pub async fn add_file(
    chall: &str,
    filename: &str,
    key: &str,
    sha256: &str,
    size: u64,
) -> DbResult<String> {
    let uuid = uuid::Uuid::new_v4().to_string();
    get_conn()
        .await?
        .exec_drop(
            "INSERT INTO challs_files (uuid, chall_uuid, filename, storage_key, sha256, size) VALUES (?, ?, ?, ?, ?, ?)",
            (&uuid, chall, filename, key, sha256, size),
        )
        .await?;
    Ok(uuid)
}

pub async fn delete_file(chall: &str, file: &str) -> DbResult<Option<String>> {
    let mut conn = get_conn().await?;
    let key: Option<String> = conn
        .exec_first(
            "SELECT storage_key FROM challs_files WHERE chall_uuid = ? AND uuid = ?",
            (chall, file),
        )
        .await?;
    conn.exec_drop(
        "DELETE FROM challs_files WHERE chall_uuid = ? AND uuid = ?",
        (chall, file),
    )
    .await?;
    Ok(key)
}

/// Adds a co-author by username (same tenant). Returns false if the user does not exist.
pub async fn add_author(tenant: &str, chall: &str, username: &str) -> DbResult<bool> {
    let mut conn = get_conn().await?;
    let uuid: Option<String> = conn
        .exec_first(
            "SELECT uuid FROM users WHERE tenant_uuid = ? AND username = ?",
            (tenant, username),
        )
        .await?;
    let Some(uuid) = uuid else { return Ok(false) };
    conn.exec_drop(
        "INSERT IGNORE INTO challs_authors (chall_uuid, user_uuid, role) VALUES (?, ?, 'coauthor')",
        (chall, &uuid),
    )
    .await?;
    Ok(true)
}

pub async fn remove_author(chall: &str, username: &str) -> DbResult<()> {
    get_conn()
        .await?
        .exec_drop(
            "DELETE a FROM challs_authors a JOIN users u ON u.uuid = a.user_uuid WHERE a.chall_uuid = ? AND u.username = ? AND a.role <> 'owner'",
            (chall, username),
        )
        .await
}

pub async fn slug_owner(tenant: &str, slug: &str) -> DbResult<Option<String>> {
    get_conn()
        .await?
        .exec_first(
            "SELECT uuid FROM challs WHERE tenant_uuid = ? AND slug = ?",
            (tenant, slug),
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_and_validation() {
        assert_eq!(slugify("Cookie Monster!"), "cookie-monster");
        assert_eq!(slugify("  RSA -- baby 2 "), "rsa-baby-2");
        assert_eq!(slugify("!!!"), "challenge");
        let ok = Fields {
            name: " Web ".into(),
            category: "WEB".into(),
            points: 100,
            ..Default::default()
        }
        .validate()
        .unwrap();
        assert_eq!(
            (
                ok.name.as_str(),
                ok.category.as_str(),
                ok.difficulty.as_str(),
                ok.ctype.as_str()
            ),
            ("Web", "web", "easy", "file")
        );
        assert!(
            Fields {
                name: "x".into(),
                static_url: "javascript:alert(1)".into(),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            Fields {
                name: "x".into(),
                flag_type: "regex".into(),
                flag: "(".into(),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            Fields {
                name: "x".into(),
                points: -1,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
}
