use crate::helper::database::{DbResult, get_conn};
use crate::models::chall::Card;
use crate::models::is_duplicate;
use crate::models::solve::Scope;
use chrono::{NaiveDateTime, Utc};
use mysql_async::prelude::*;
use mysql_async::{Params, Row, TxOpts, Value};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub uuid: String,
    #[serde(skip)]
    pub tenant_uuid: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub starts_at: NaiveDateTime,
    pub ends_at: NaiveDateTime,
    pub freeze_at: Option<NaiveDateTime>,
    pub team_mode: bool,
    pub max_team_size: i32,
    pub registration: String,
    #[serde(skip)]
    pub join_code: String,
    pub after_end: String,
    pub status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Upcoming,
    Live,
    Ended,
}

impl Event {
    pub fn phase(&self) -> Phase {
        let now = Utc::now().naive_utc();
        if now < self.starts_at {
            Phase::Upcoming
        } else if now < self.ends_at {
            Phase::Live
        } else {
            Phase::Ended
        }
    }
    /// True while players should see the frozen board.
    pub fn frozen(&self) -> bool {
        self.freeze_at.is_some_and(|f| Utc::now().naive_utc() >= f)
    }
    pub fn can_register(&self) -> bool {
        self.registration != "closed" && self.phase() != Phase::Ended && self.status == "published"
    }
}

const COLS: &str = "e.uuid, e.tenant_uuid, e.slug, e.title, e.description, e.starts_at, e.ends_at, e.freeze_at, e.team_mode, \
                    e.max_team_size, e.registration, e.join_code, e.after_end, e.status";

fn from_row(mut r: Row) -> Event {
    Event {
        uuid: r.take(0).unwrap(),
        tenant_uuid: r.take(1).unwrap(),
        slug: r.take(2).unwrap(),
        title: r.take(3).unwrap(),
        description: r.take(4).unwrap(),
        starts_at: r.take(5).unwrap(),
        ends_at: r.take(6).unwrap(),
        freeze_at: r.take(7).unwrap(),
        team_mode: r.take::<i64, _>(8).unwrap() != 0,
        max_team_size: r.take(9).unwrap(),
        registration: r.take(10).unwrap(),
        join_code: r.take(11).unwrap(),
        after_end: r.take(12).unwrap(),
        status: r.take(13).unwrap(),
    }
}

pub async fn get_by_slug(tenant: &str, slug: &str) -> DbResult<Option<Event>> {
    let r: Option<Row> = get_conn()
        .await?
        .exec_first(
            format!("SELECT {COLS} FROM events e WHERE e.tenant_uuid = ? AND e.slug = ?"),
            (tenant, slug),
        )
        .await?;
    Ok(r.map(from_row))
}

pub async fn get(tenant: &str, uuid: &str) -> DbResult<Option<Event>> {
    let r: Option<Row> = get_conn()
        .await?
        .exec_first(
            format!("SELECT {COLS} FROM events e WHERE e.tenant_uuid = ? AND e.uuid = ?"),
            (tenant, uuid),
        )
        .await?;
    Ok(r.map(from_row))
}

/// Published events (or all for organizers), newest first.
pub async fn list(tenant: &str, all: bool) -> DbResult<Vec<Event>> {
    let rows: Vec<Row> = get_conn()
        .await?
        .exec(
            format!("SELECT {COLS} FROM events e WHERE e.tenant_uuid = ? AND (e.status = 'published' OR ?) ORDER BY e.starts_at DESC LIMIT 500"),
            (tenant, all),
        )
        .await?;
    Ok(rows.into_iter().map(from_row).collect())
}

pub struct EventFields {
    pub title: String,
    pub description: String,
    pub starts_at: NaiveDateTime,
    pub ends_at: NaiveDateTime,
    pub freeze_at: Option<NaiveDateTime>,
    pub team_mode: bool,
    pub max_team_size: i32,
    pub registration: String,
    pub join_code: String,
    pub after_end: String,
    pub status: String,
}

pub async fn create(tenant: &str, slug: &str, f: &EventFields) -> DbResult<String> {
    let uuid = uuid::Uuid::new_v4().to_string();
    get_conn()
        .await?
        .exec_drop(
            "INSERT INTO events (uuid, tenant_uuid, slug, title, description, starts_at, ends_at, freeze_at, team_mode, max_team_size, registration, join_code, after_end, status)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            Params::Positional(vec![uuid.as_str().into(), tenant.into(), slug.into(), f.title.as_str().into(), f.description.as_str().into(), f.starts_at.into(), f.ends_at.into(), f.freeze_at.into(), f.team_mode.into(), f.max_team_size.into(), f.registration.as_str().into(), f.join_code.as_str().into(), f.after_end.as_str().into(), f.status.as_str().into()]),
        )
        .await?;
    Ok(uuid)
}

pub async fn update(tenant: &str, uuid: &str, f: &EventFields) -> DbResult<()> {
    get_conn()
        .await?
        .exec_drop(
            "UPDATE events SET title = ?, description = ?, starts_at = ?, ends_at = ?, freeze_at = ?, team_mode = ?, max_team_size = ?,
                    registration = ?, join_code = ?, after_end = ?, status = ?,
                    closed_at = IF(? > UTC_TIMESTAMP(), NULL, closed_at)
             WHERE tenant_uuid = ? AND uuid = ?",
            Params::Positional(vec![f.title.as_str().into(), f.description.as_str().into(), f.starts_at.into(), f.ends_at.into(), f.freeze_at.into(), f.team_mode.into(), f.max_team_size.into(), f.registration.as_str().into(), f.join_code.as_str().into(), f.after_end.as_str().into(), f.status.as_str().into(), f.ends_at.into(), Value::from(tenant), Value::from(uuid)]),
        )
        .await
}

pub async fn delete(tenant: &str, uuid: &str) -> DbResult<()> {
    let mut conn = get_conn().await?;
    conn.exec_drop("DELETE FROM challs_solved WHERE event_uuid = ?", (uuid,))
        .await?;
    conn.exec_drop(
        "DELETE FROM events WHERE tenant_uuid = ? AND uuid = ?",
        (tenant, uuid),
    )
    .await
}

pub async fn slug_taken(tenant: &str, slug: &str) -> DbResult<bool> {
    let r: Option<i64> = get_conn()
        .await?
        .exec_first(
            "SELECT 1 FROM events WHERE tenant_uuid = ? AND slug = ?",
            (tenant, slug),
        )
        .await?;
    Ok(r.is_some())
}

// ---------------------------------------------------------------- challenges

/// Adds a challenge. While the event has not ended it is taken out of practice,
/// so players cannot solve it early there.
pub async fn add_chall(ev: &Event, chall: &str, points: Option<i32>) -> DbResult<()> {
    let mut conn = get_conn().await?;
    conn.exec_drop(
        "INSERT INTO events_challs (event_uuid, chall_uuid, points_override) VALUES (?, ?, ?) AS n
         ON DUPLICATE KEY UPDATE points_override = n.points_override",
        (&ev.uuid, chall, points),
    )
    .await?;
    if ev.phase() != Phase::Ended {
        conn.exec_drop(
            "UPDATE challs SET practice = 0 WHERE uuid = ? AND tenant_uuid = ?",
            (chall, &ev.tenant_uuid),
        )
        .await?;
    }
    Ok(())
}

pub async fn remove_chall(ev: &Event, chall: &str) -> DbResult<()> {
    get_conn()
        .await?
        .exec_drop(
            "DELETE FROM events_challs WHERE event_uuid = ? AND chall_uuid = ?",
            (&ev.uuid, chall),
        )
        .await
}

/// Event challenges (published only unless `all`) with the scope's solved flag and event solve counts.
pub async fn challs(ev: &Event, scope_key: &str, all: bool) -> DbResult<Vec<Card>> {
    let rows: Vec<Row> = get_conn()
        .await?
        .exec(
            "SELECT c.uuid, c.slug, c.name, c.category, c.difficulty, COALESCE(ec.points_override, c.points), c.ctype,
                    (SELECT COUNT(*) FROM challs_solved s WHERE s.chall_uuid = c.uuid AND s.event_key = ec.event_uuid),
                    EXISTS(SELECT 1 FROM challs_solved s WHERE s.chall_uuid = c.uuid AND s.event_key = ec.event_uuid AND s.scope_key = ?)
             FROM events_challs ec JOIN challs c ON c.uuid = ec.chall_uuid
             WHERE ec.event_uuid = ? AND (c.status = 'published' OR ?)
             ORDER BY c.category, COALESCE(ec.points_override, c.points), c.name",
            (scope_key, &ev.uuid, all),
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(crate::models::chall::card_from_row)
        .collect())
}

pub async fn chall_points(ev: &str, chall: &str) -> DbResult<Option<i32>> {
    let r: Option<Option<i32>> = get_conn()
        .await?
        .exec_first(
            "SELECT COALESCE(ec.points_override, c.points) FROM events_challs ec JOIN challs c ON c.uuid = ec.chall_uuid WHERE ec.event_uuid = ? AND ec.chall_uuid = ?",
            (ev, chall),
        )
        .await?;
    Ok(r.flatten())
}

// ---------------------------------------------------------------- participation

#[derive(Debug, Clone, Serialize)]
pub struct Team {
    pub uuid: String,
    pub name: String,
    pub invite_code: String,
    pub captain_uuid: String,
    pub banned: bool,
}

/// The player's team in this event, if any.
pub async fn team_of(ev: &str, user: &str) -> DbResult<Option<Team>> {
    let r: Option<(String, String, String, String, i64)> = get_conn()
        .await?
        .exec_first(
            "SELECT t.uuid, t.name, t.invite_code, t.captain_uuid, t.banned_at IS NOT NULL FROM teams_members m JOIN teams t ON t.uuid = m.team_uuid
             WHERE m.event_uuid = ? AND m.user_uuid = ?",
            (ev, user),
        )
        .await?;
    Ok(r.map(|(uuid, name, invite_code, captain_uuid, b)| Team {
        uuid,
        name,
        invite_code,
        captain_uuid,
        banned: b != 0,
    }))
}

pub async fn members(team: &str) -> DbResult<Vec<(String, String)>> {
    get_conn()
        .await?
        .exec("SELECT u.uuid, u.username FROM teams_members m JOIN users u ON u.uuid = m.user_uuid WHERE m.team_uuid = ? ORDER BY m.joined_at", (team,))
        .await
}

/// Solo registration state: None = not registered, Some(true) = disqualified.
pub async fn solo_state(ev: &str, user: &str) -> DbResult<Option<bool>> {
    let r: Option<i64> = get_conn()
        .await?
        .exec_first("SELECT banned_at IS NOT NULL FROM events_players WHERE event_uuid = ? AND user_uuid = ?", (ev, user))
        .await?;
    Ok(r.map(|b| b != 0))
}

pub async fn register_solo(ev: &str, user: &str) -> DbResult<()> {
    get_conn()
        .await?
        .exec_drop(
            "INSERT IGNORE INTO events_players (event_uuid, user_uuid) VALUES (?, ?)",
            (ev, user),
        )
        .await
}

fn invite_code() -> String {
    // 16 chars from a 32-symbol alphabet without look-alikes: 80 bits
    use rand::Rng;
    const A: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::rngs::OsRng;
    (0..16)
        .map(|_| A[rng.gen_range(0..A.len())] as char)
        .collect()
}

pub enum TeamError {
    NameTaken,
    AlreadyInTeam,
    Full,
    NotFound,
    Locked,
}

pub async fn create_team(ev: &Event, user: &str, name: &str) -> DbResult<Result<Team, TeamError>> {
    let uuid = uuid::Uuid::new_v4().to_string();
    let code = invite_code();
    let mut conn = get_conn().await?;
    let mut tx = conn.start_transaction(TxOpts::default()).await?;
    let res = tx
        .exec_drop(
            "INSERT INTO teams (uuid, tenant_uuid, event_uuid, name, invite_code, captain_uuid) VALUES (?, ?, ?, ?, ?, ?)",
            (&uuid, &ev.tenant_uuid, &ev.uuid, name, &code, user),
        )
        .await;
    match res {
        Ok(()) => {}
        Err(e) if is_duplicate(&e) => return Ok(Err(TeamError::NameTaken)),
        Err(e) => return Err(e),
    }
    match tx
        .exec_drop(
            "INSERT INTO teams_members (team_uuid, user_uuid, event_uuid) VALUES (?, ?, ?)",
            (&uuid, user, &ev.uuid),
        )
        .await
    {
        Ok(()) => {}
        Err(e) if is_duplicate(&e) => return Ok(Err(TeamError::AlreadyInTeam)),
        Err(e) => return Err(e),
    }
    tx.commit().await?;
    Ok(Ok(Team {
        uuid,
        name: name.to_string(),
        invite_code: code,
        captain_uuid: user.to_string(),
        banned: false,
    }))
}

/// Joins by invite code. The team row is locked so two joins cannot overfill it.
pub async fn join_team(ev: &Event, user: &str, code: &str) -> DbResult<Result<Team, TeamError>> {
    let mut conn = get_conn().await?;
    let mut tx = conn.start_transaction(TxOpts::default()).await?;
    let team: Option<(String, String, String, i64)> = tx
        .exec_first(
            "SELECT uuid, name, captain_uuid, banned_at IS NOT NULL FROM teams WHERE event_uuid = ? AND invite_code = ? FOR UPDATE",
            (&ev.uuid, code.trim().to_uppercase()),
        )
        .await?;
    let Some((uuid, name, captain, banned)) = team else {
        return Ok(Err(TeamError::NotFound));
    };
    if banned != 0 {
        return Ok(Err(TeamError::NotFound));
    }
    let size: i64 = tx
        .exec_first(
            "SELECT COUNT(*) FROM teams_members WHERE team_uuid = ?",
            (&uuid,),
        )
        .await?
        .unwrap_or(0);
    if size >= ev.max_team_size as i64 {
        return Ok(Err(TeamError::Full));
    }
    match tx
        .exec_drop(
            "INSERT INTO teams_members (team_uuid, user_uuid, event_uuid) VALUES (?, ?, ?)",
            (&uuid, user, &ev.uuid),
        )
        .await
    {
        Ok(()) => {}
        Err(e) if is_duplicate(&e) => return Ok(Err(TeamError::AlreadyInTeam)),
        Err(e) => return Err(e),
    }
    tx.commit().await?;
    Ok(Ok(Team {
        uuid,
        name,
        invite_code: code.to_string(),
        captain_uuid: captain,
        banned: false,
    }))
}

async fn team_has_solves(conn: &mut mysql_async::Conn, ev: &str, team: &str) -> DbResult<bool> {
    let r: Option<i64> = conn
        .exec_first(
            "SELECT 1 FROM challs_solved WHERE event_key = ? AND team_uuid = ? LIMIT 1",
            (ev, team),
        )
        .await?;
    Ok(r.is_some())
}

/// Leaves (or kicks `user` from) the team. Refused once the team scored: points cannot follow a player.
pub async fn leave_team(ev: &Event, team: &Team, user: &str) -> DbResult<Result<(), TeamError>> {
    let mut conn = get_conn().await?;
    if team_has_solves(&mut conn, &ev.uuid, &team.uuid).await? {
        return Ok(Err(TeamError::Locked));
    }
    conn.exec_drop(
        "DELETE FROM teams_members WHERE team_uuid = ? AND user_uuid = ?",
        (&team.uuid, user),
    )
    .await?;
    let next: Option<String> = conn
        .exec_first(
            "SELECT user_uuid FROM teams_members WHERE team_uuid = ? ORDER BY joined_at LIMIT 1",
            (&team.uuid,),
        )
        .await?;
    match next {
        None => {
            conn.exec_drop("DELETE FROM teams WHERE uuid = ?", (&team.uuid,))
                .await?
        }
        Some(n) if team.captain_uuid == user => {
            conn.exec_drop(
                "UPDATE teams SET captain_uuid = ? WHERE uuid = ?",
                (n, &team.uuid),
            )
            .await?
        }
        Some(_) => {}
    }
    Ok(Ok(()))
}

pub async fn set_team_banned(ev: &str, team: &str, banned: bool) -> DbResult<bool> {
    let mut conn = get_conn().await?;
    conn.exec_drop(
        "UPDATE teams SET banned_at = IF(?, COALESCE(banned_at, UTC_TIMESTAMP()), NULL) WHERE event_uuid = ? AND uuid = ?",
        (banned, ev, team),
    )
    .await?;
    Ok(conn.affected_rows() == 1)
}

pub async fn set_player_banned(ev: &str, user: &str, banned: bool) -> DbResult<bool> {
    let mut conn = get_conn().await?;
    conn.exec_drop(
        "UPDATE events_players SET banned_at = IF(?, COALESCE(banned_at, UTC_TIMESTAMP()), NULL) WHERE event_uuid = ? AND user_uuid = ?",
        (banned, ev, user),
    )
    .await?;
    Ok(conn.affected_rows() == 1)
}

#[derive(Debug, Serialize)]
pub struct TeamRow {
    pub uuid: String,
    pub name: String,
    pub members: i64,
    pub banned: bool,
}

pub async fn teams(ev: &str) -> DbResult<Vec<TeamRow>> {
    get_conn()
        .await?
        .exec_map(
            "SELECT t.uuid, t.name, (SELECT COUNT(*) FROM teams_members m WHERE m.team_uuid = t.uuid), t.banned_at IS NOT NULL FROM teams t WHERE t.event_uuid = ? ORDER BY t.name",
            (ev,),
            |(uuid, name, members, b): (String, String, i64, i64)| TeamRow { uuid, name, members, banned: b != 0 },
        )
        .await
}

pub async fn participants(ev: &Event) -> DbResult<i64> {
    let sql = if ev.team_mode {
        "SELECT COUNT(*) FROM teams WHERE event_uuid = ?"
    } else {
        "SELECT COUNT(*) FROM events_players WHERE event_uuid = ?"
    };
    Ok(get_conn()
        .await?
        .exec_first(sql, (&ev.uuid,))
        .await?
        .unwrap_or(0))
}

// ---------------------------------------------------------------- scope

/// The live event in which this user plays this challenge, as (event, scope).
/// None when the challenge is not part of a live event the user takes part in
/// (or the user / team is disqualified).
pub async fn live_scope(tenant: &str, chall: &str, user: &str) -> DbResult<Option<(Event, Scope)>> {
    let rows: Vec<Row> = get_conn()
        .await?
        .exec(
            format!(
                "SELECT {COLS} FROM events e JOIN events_challs ec ON ec.event_uuid = e.uuid
                 WHERE e.tenant_uuid = ? AND ec.chall_uuid = ? AND e.status = 'published'
                   AND e.starts_at <= UTC_TIMESTAMP() AND e.ends_at > UTC_TIMESTAMP()"
            ),
            (tenant, chall),
        )
        .await?;
    for ev in rows.into_iter().map(from_row) {
        if ev.team_mode {
            if let Some(t) = team_of(&ev.uuid, user).await?
                && !t.banned
            {
                let scope = Scope {
                    event_uuid: Some(ev.uuid.clone()),
                    team_uuid: Some(t.uuid),
                };
                return Ok(Some((ev, scope)));
            }
        } else if solo_state(&ev.uuid, user).await? == Some(false) {
            let scope = Scope {
                event_uuid: Some(ev.uuid.clone()),
                team_uuid: None,
            };
            return Ok(Some((ev, scope)));
        }
    }
    Ok(None)
}

/// End-of-event job: moves challenges back to practice if asked. Runs once per event.
pub async fn close_ended(tenant_filter: Option<&str>) -> DbResult<Vec<String>> {
    let mut conn = get_conn().await?;
    let ended: Vec<(String, String)> = conn
        .exec(
            "SELECT uuid, after_end FROM events WHERE closed_at IS NULL AND ends_at <= UTC_TIMESTAMP() AND (? IS NULL OR tenant_uuid = ?)",
            (tenant_filter, tenant_filter),
        )
        .await?;
    let mut closed = Vec::new();
    for (uuid, after_end) in ended {
        if after_end == "practice" {
            conn.exec_drop("UPDATE challs c JOIN events_challs ec ON ec.chall_uuid = c.uuid SET c.practice = 1 WHERE ec.event_uuid = ?", (&uuid,)).await?;
        }
        conn.exec_drop(
            "UPDATE events SET closed_at = UTC_TIMESTAMP() WHERE uuid = ?",
            (&uuid,),
        )
        .await?;
        closed.push(uuid);
    }
    Ok(closed)
}
