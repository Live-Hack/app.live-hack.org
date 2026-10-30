//! Scoreboards. One query for every scope: practice (event_key '') or an event, ranking
//! users or teams. Results are cached a few seconds per (tenant, event, category) so a
//! scoreboard refreshed by hundreds of players costs one query.
use crate::helper::database::{DbResult, get_conn};
use chrono::NaiveDateTime;
use mysql_async::prelude::*;
use mysql_async::{Params, Row, Value};
use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub rank: usize,
    /// user uuid, or team uuid in team events
    pub id: String,
    pub name: String,
    pub score: i64,
    pub solves: i64,
    pub last_solve: NaiveDateTime,
}

const TTL: Duration = Duration::from_secs(5);
type Key = (String, String, String, Option<String>);
type Cached = (Instant, Arc<Vec<Entry>>);
static CACHE: Lazy<Mutex<HashMap<Key, Cached>>> = Lazy::new(Default::default);

pub struct Query<'a> {
    pub tenant: &'a str,
    /// '' for practice
    pub event_key: &'a str,
    pub teams: bool,
    pub category: &'a str,
    /// Only solves before this instant (scoreboard freeze).
    pub until: Option<NaiveDateTime>,
}

/// Full ranking, ties broken by who reached the score first.
pub async fn ranking(q: &Query<'_>) -> DbResult<Arc<Vec<Entry>>> {
    let key = (
        q.tenant.to_string(),
        q.event_key.to_string(),
        format!("{}:{}", q.teams, q.category),
        q.until.map(|u| u.to_string()),
    );
    if let Some((at, list)) = CACHE.lock().unwrap().get(&key)
        && at.elapsed() < TTL
    {
        return Ok(list.clone());
    }
    let points = if q.event_key.is_empty() {
        "c.points".to_string()
    } else {
        "COALESCE((SELECT ec.points_override FROM events_challs ec WHERE ec.event_uuid = s.event_uuid AND ec.chall_uuid = s.chall_uuid), c.points)".to_string()
    };
    let mut wh = String::from("s.tenant_uuid = ? AND s.event_key = ?");
    let mut args: Vec<Value> = vec![q.tenant.into(), q.event_key.into()];
    if !q.category.is_empty() {
        wh.push_str(" AND c.category = ?");
        args.push(q.category.into());
    }
    if let Some(until) = q.until {
        wh.push_str(" AND s.solved_at < ?");
        args.push(until.into());
    }
    let (name_join, name_col, visible) = if q.teams {
        (
            "JOIN teams t ON t.uuid = s.scope_key",
            "t.name",
            "t.banned_at IS NULL",
        )
    } else {
        // organizers know the flags: they stay off the practice scoreboard
        (
            "JOIN users u ON u.uuid = s.scope_key",
            "u.username",
            "u.banned_at IS NULL AND (u.role = 'player' OR s.event_key <> '') \
             AND NOT EXISTS(SELECT 1 FROM events_players ep WHERE ep.event_uuid = s.event_uuid \
                            AND ep.user_uuid = s.user_uuid AND ep.banned_at IS NOT NULL)",
        )
    };
    let rows: Vec<Row> = get_conn()
        .await?
        .exec(
            format!(
                "SELECT s.scope_key, {name_col}, SUM({points}) AS score, COUNT(*) AS solves, MAX(s.solved_at) AS last
                 FROM challs_solved s JOIN challs c ON c.uuid = s.chall_uuid {name_join}
                 WHERE {wh} AND {visible}
                 GROUP BY s.scope_key, {name_col}
                 ORDER BY score DESC, last ASC, {name_col} LIMIT 100000"
            ),
            Params::Positional(args),
        )
        .await?;
    let list: Vec<Entry> = rows
        .into_iter()
        .enumerate()
        .map(|(i, mut r)| Entry {
            rank: i + 1,
            id: r.take(0).unwrap(),
            name: r.take(1).unwrap(),
            score: r.take::<Option<i64>, _>(2).unwrap().unwrap_or(0),
            solves: r.take(3).unwrap(),
            last_solve: r.take(4).unwrap(),
        })
        .collect();
    let list = Arc::new(list);
    let mut cache = CACHE.lock().unwrap();
    if cache.len() > 1000 {
        cache.retain(|_, (at, _)| at.elapsed() < TTL);
    }
    cache.insert(key, (Instant::now(), list.clone()));
    Ok(list)
}

#[derive(Debug, Serialize)]
pub struct SolveRow {
    pub slug: String,
    pub name: String,
    pub category: String,
    pub points: i32,
    pub solved_at: NaiveDateTime,
}

/// A user's practice solves, newest first.
pub async fn user_solves(tenant: &str, user: &str) -> DbResult<Vec<SolveRow>> {
    get_conn()
        .await?
        .exec_map(
            "SELECT c.slug, c.name, c.category, c.points, s.solved_at FROM challs_solved s JOIN challs c ON c.uuid = s.chall_uuid
             WHERE s.tenant_uuid = ? AND s.user_uuid = ? AND s.event_key = '' ORDER BY s.solved_at DESC LIMIT 1000",
            (tenant, user),
            |(slug, name, category, points, solved_at)| SolveRow { slug, name, category, points, solved_at },
        )
        .await
}

#[derive(Debug, Serialize)]
pub struct FeedItem {
    pub username: String,
    pub slug: String,
    pub name: String,
    pub points: i32,
    pub solved_at: NaiveDateTime,
    pub first_blood: bool,
}

/// Latest practice solves of the platform.
pub async fn feed(tenant: &str, limit: u32) -> DbResult<Vec<FeedItem>> {
    let rows: Vec<Row> = get_conn()
        .await?
        .exec(
            "SELECT u.username, c.slug, c.name, c.points, s.solved_at,
                    NOT EXISTS(SELECT 1 FROM challs_solved o WHERE o.chall_uuid = s.chall_uuid AND o.event_key = '' AND o.solved_at < s.solved_at)
             FROM challs_solved s JOIN challs c ON c.uuid = s.chall_uuid JOIN users u ON u.uuid = s.user_uuid
             WHERE s.tenant_uuid = ? AND s.event_key = '' AND u.banned_at IS NULL AND c.status = 'published'
             ORDER BY s.solved_at DESC LIMIT ?",
            (tenant, limit),
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|mut r| FeedItem {
            username: r.take(0).unwrap(),
            slug: r.take(1).unwrap(),
            name: r.take(2).unwrap(),
            points: r.take(3).unwrap(),
            solved_at: r.take(4).unwrap(),
            first_blood: r.take::<i64, _>(5).unwrap() != 0,
        })
        .collect())
}

/// Points per category for one user (practice).
pub async fn user_categories(tenant: &str, user: &str) -> DbResult<Vec<(String, i64, i64)>> {
    get_conn()
        .await?
        .exec(
            "SELECT c.category, COUNT(*), SUM(c.points) FROM challs_solved s JOIN challs c ON c.uuid = s.chall_uuid
             WHERE s.tenant_uuid = ? AND s.user_uuid = ? AND s.event_key = '' GROUP BY c.category ORDER BY 3 DESC",
            (tenant, user),
        )
        .await
}

/// Published practice challenge count per category, to show progress.
pub async fn category_totals(tenant: &str) -> DbResult<HashMap<String, i64>> {
    let rows: Vec<(String, i64)> = get_conn()
        .await?
        .exec(
            "SELECT category, COUNT(*) FROM challs WHERE tenant_uuid = ? AND status = 'published' AND practice = 1 GROUP BY category",
            (tenant,),
        )
        .await?;
    Ok(rows.into_iter().collect())
}
