use crate::helper::database::{DbResult, get_conn};
use chrono::NaiveDateTime;
use mysql_async::prelude::*;
use mysql_async::{Params, Row, Value};
use serde::Serialize;

pub const DIFFICULTIES: &[&str] = &["easy", "medium", "hard", "insane"];
pub const DEFAULT_CATEGORIES: &[&str] = &[
    "crypto",
    "forensics",
    "misc",
    "osint",
    "pwn",
    "reverse",
    "web",
];

#[derive(Debug, Clone, Serialize)]
pub struct Chall {
    pub uuid: String,
    #[serde(skip)]
    pub tenant_uuid: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub difficulty: String,
    pub points: i32,
    pub ctype: String,
    pub static_url: String,
    pub flag_type: String,
    #[serde(skip)]
    pub flag: String,
    pub status: String,
    pub practice: bool,
    pub published_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
}

const COLS: &str = "c.uuid, c.tenant_uuid, c.slug, c.name, c.description, c.category, c.difficulty, c.points, c.ctype, \
                    c.static_url, c.flag_type, c.flag, c.status, c.practice, c.published_at, c.created_at";

impl Chall {
    fn from_row(mut r: Row) -> Chall {
        Chall {
            uuid: r.take(0).unwrap(),
            tenant_uuid: r.take(1).unwrap(),
            slug: r.take(2).unwrap(),
            name: r.take(3).unwrap(),
            description: r.take(4).unwrap(),
            category: r.take(5).unwrap(),
            difficulty: r.take(6).unwrap(),
            points: r.take(7).unwrap(),
            ctype: r.take(8).unwrap(),
            static_url: r.take(9).unwrap(),
            flag_type: r.take(10).unwrap(),
            flag: r.take(11).unwrap(),
            status: r.take(12).unwrap(),
            practice: r.take::<i64, _>(13).unwrap() != 0,
            published_at: r.take(14).unwrap(),
            created_at: r.take(15).unwrap(),
        }
    }
}

/// A challenge as shown in lists, with its solve count and the viewer's state.
#[derive(Debug, Clone, Serialize)]
pub struct Card {
    pub uuid: String,
    pub slug: String,
    pub name: String,
    pub category: String,
    pub difficulty: String,
    pub points: i32,
    pub ctype: String,
    pub solves: i64,
    pub solved: bool,
}

#[derive(Default)]
pub struct Filter {
    pub q: String,
    pub category: String,
    pub difficulty: String,
    /// "", "solved" or "unsolved"
    pub state: String,
    pub sort: String,
    pub page: u32,
    pub per_page: u32,
}

/// Published practice challenges with filters, plus the total count for pagination.
pub async fn list_practice(tenant: &str, viewer: &str, f: &Filter) -> DbResult<(Vec<Card>, u64)> {
    let mut wh = String::from("c.tenant_uuid = ? AND c.status = 'published' AND c.practice = 1");
    let mut args: Vec<Value> = vec![tenant.into()];
    if !f.q.is_empty() {
        wh.push_str(" AND c.name LIKE ?");
        args.push(
            format!(
                "%{}%",
                f.q.replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )
            .into(),
        );
    }
    if !f.category.is_empty() {
        wh.push_str(" AND c.category = ?");
        args.push(f.category.as_str().into());
    }
    if !f.difficulty.is_empty() {
        wh.push_str(" AND c.difficulty = ?");
        args.push(f.difficulty.as_str().into());
    }
    let solved_sql = "EXISTS(SELECT 1 FROM challs_solved s WHERE s.chall_uuid = c.uuid AND s.event_key = '' AND s.user_uuid = ?)";
    match f.state.as_str() {
        "solved" => {
            wh.push_str(&format!(" AND {solved_sql}"));
            args.push(viewer.into());
        }
        "unsolved" => {
            wh.push_str(&format!(" AND NOT {solved_sql}"));
            args.push(viewer.into());
        }
        _ => {}
    }
    let order = match f.sort.as_str() {
        "points" => "c.points ASC, c.name",
        "points_desc" => "c.points DESC, c.name",
        "solves" => "solves DESC, c.name",
        "name" => "c.name",
        _ => "c.published_at DESC, c.name",
    };
    let mut conn = get_conn().await?;
    let total: u64 = conn
        .exec_first(
            format!("SELECT COUNT(*) FROM challs c WHERE {wh}"),
            Params::Positional(args.clone()),
        )
        .await?
        .unwrap_or(0);

    let per = f.per_page.clamp(1, 200);
    let mut list_args: Vec<Value> = vec![viewer.into()];
    list_args.extend(args);
    list_args.push(per.into());
    list_args.push((f.page.saturating_sub(1) * per).into());
    let rows: Vec<Row> = conn
        .exec(
            format!(
                "SELECT c.uuid, c.slug, c.name, c.category, c.difficulty, c.points, c.ctype,
                        (SELECT COUNT(*) FROM challs_solved s WHERE s.chall_uuid = c.uuid AND s.event_key = '') AS solves,
                        {solved_sql} AS solved
                 FROM challs c WHERE {wh} ORDER BY {order} LIMIT ? OFFSET ?"
            ),
            Params::Positional(list_args),
        )
        .await?;
    Ok((rows.into_iter().map(card_from_row).collect(), total))
}

pub fn card_from_row(mut r: Row) -> Card {
    Card {
        uuid: r.take(0).unwrap(),
        slug: r.take(1).unwrap(),
        name: r.take(2).unwrap(),
        category: r.take(3).unwrap(),
        difficulty: r.take(4).unwrap(),
        points: r.take(5).unwrap(),
        ctype: r.take(6).unwrap(),
        solves: r.take(7).unwrap(),
        solved: r.take::<i64, _>(8).unwrap() != 0,
    }
}

/// Categories that have at least one published practice challenge.
pub async fn practice_categories(tenant: &str) -> DbResult<Vec<String>> {
    get_conn()
        .await?
        .exec(
            "SELECT DISTINCT category FROM challs WHERE tenant_uuid = ? AND status = 'published' AND practice = 1 ORDER BY category",
            (tenant,),
        )
        .await
}

pub async fn get_by_slug(tenant: &str, slug: &str) -> DbResult<Option<Chall>> {
    let row: Option<Row> = get_conn()
        .await?
        .exec_first(
            format!("SELECT {COLS} FROM challs c WHERE c.tenant_uuid = ? AND c.slug = ?"),
            (tenant, slug),
        )
        .await?;
    Ok(row.map(Chall::from_row))
}

pub async fn get(tenant: &str, uuid: &str) -> DbResult<Option<Chall>> {
    let row: Option<Row> = get_conn()
        .await?
        .exec_first(
            format!("SELECT {COLS} FROM challs c WHERE c.tenant_uuid = ? AND c.uuid = ?"),
            (tenant, uuid),
        )
        .await?;
    Ok(row.map(Chall::from_row))
}

pub async fn is_author(chall: &str, user: &str) -> DbResult<bool> {
    let r: Option<i64> = get_conn()
        .await?
        .exec_first(
            "SELECT 1 FROM challs_authors WHERE chall_uuid = ? AND user_uuid = ?",
            (chall, user),
        )
        .await?;
    Ok(r.is_some())
}

pub async fn author_names(chall: &str) -> DbResult<Vec<String>> {
    get_conn()
        .await?
        .exec(
            "SELECT u.username FROM challs_authors a JOIN users u ON u.uuid = a.user_uuid WHERE a.chall_uuid = ? ORDER BY a.role, u.username",
            (chall,),
        )
        .await
}

#[derive(Debug, Clone, Serialize)]
pub struct File {
    pub uuid: String,
    pub filename: String,
    #[serde(skip)]
    pub storage_key: String,
    pub sha256: String,
    pub size: i64,
}

pub async fn files(chall: &str) -> DbResult<Vec<File>> {
    get_conn()
        .await?
        .exec_map(
            "SELECT uuid, filename, storage_key, sha256, size FROM challs_files WHERE chall_uuid = ? ORDER BY filename",
            (chall,),
            |(uuid, filename, storage_key, sha256, size)| File { uuid, filename, storage_key, sha256, size },
        )
        .await
}

pub async fn file(chall: &str, uuid: &str) -> DbResult<Option<File>> {
    Ok(files(chall).await?.into_iter().find(|f| f.uuid == uuid))
}

#[derive(Debug, Clone, Serialize)]
pub struct Solver {
    pub username: String,
    pub solved_at: NaiveDateTime,
}

/// Solvers in a scope ('' = practice), oldest first, so the first one is the first blood.
pub async fn solvers(chall: &str, event_key: &str, limit: u32) -> DbResult<(Vec<Solver>, i64)> {
    let mut conn = get_conn().await?;
    let count: i64 = conn
        .exec_first(
            "SELECT COUNT(*) FROM challs_solved WHERE chall_uuid = ? AND event_key = ?",
            (chall, event_key),
        )
        .await?
        .unwrap_or(0);
    let list = conn
        .exec_map(
            "SELECT u.username, s.solved_at FROM challs_solved s JOIN users u ON u.uuid = s.user_uuid
             WHERE s.chall_uuid = ? AND s.event_key = ? ORDER BY s.solved_at LIMIT ?",
            (chall, event_key, limit),
            |(username, solved_at)| Solver { username, solved_at },
        )
        .await?;
    Ok((list, count))
}

/// When the scope (user or team) solved the challenge in this event scope, if it did.
pub async fn solved_at(
    chall: &str,
    event_key: &str,
    scope_key: &str,
) -> DbResult<Option<NaiveDateTime>> {
    get_conn()
        .await?
        .exec_first(
            "SELECT solved_at FROM challs_solved WHERE chall_uuid = ? AND event_key = ? AND scope_key = ?",
            (chall, event_key, scope_key),
        )
        .await
}
