//! Player side of events: registration, teams, scoreboard, CTFtime feed.
use crate::api::v1::core::{ApiError, ApiResult, bad, err, not_found, ok, require_user, too_many};
use crate::helper::ratelimit;
use crate::helper::request::{Ctx, field};
use crate::models::audit;
use crate::models::event::{self, Event, Phase, TeamError};
use crate::models::score::{self, Query};
use crate::models::user::Role;
use actix_web::http::StatusCode;
use serde_json::{Value, json};

/// Published event (drafts only for admins/authors).
pub async fn visible_event(ctx: &Ctx, slug: &str) -> Result<Event, ApiError> {
    let Some(ev) = event::get_by_slug(ctx.tenant_uuid(), slug).await? else {
        return Err(not_found());
    };
    if ev.status != "published" && !ctx.has_role(Role::Author) {
        return Err(not_found());
    }
    Ok(ev)
}

fn check_code(ev: &Event, body: &Value) -> Result<(), ApiError> {
    if ev.registration == "code" && field(body, "code") != ev.join_code {
        return Err(err(
            StatusCode::FORBIDDEN,
            "This event needs a registration code. Ask the organizers.",
        ));
    }
    Ok(())
}

fn ensure_open(ev: &Event) -> Result<(), ApiError> {
    if !ev.can_register() {
        return Err(err(
            StatusCode::FORBIDDEN,
            "Registration for this event is closed.",
        ));
    }
    Ok(())
}

pub async fn register(ctx: &Ctx, slug: &str, body: &Value) -> ApiResult {
    let u = require_user(ctx)?;
    let ev = visible_event(ctx, slug).await?;
    if ev.team_mode {
        return Err(bad("This is a team event: create or join a team."));
    }
    ensure_open(&ev)?;
    check_code(&ev, body)?;
    event::register_solo(&ev.uuid, &u.uuid).await?;
    ok(json!({ "ok": true, "message": "You are registered." }))
}

fn valid_team_name(n: &str) -> bool {
    let len = n.chars().count();
    (2..=32).contains(&len) && !n.chars().any(char::is_control)
}

fn team_error(e: TeamError) -> ApiError {
    match e {
        TeamError::NameTaken => err(StatusCode::CONFLICT, "A team already uses this name."),
        TeamError::AlreadyInTeam => err(
            StatusCode::CONFLICT,
            "You are already in a team for this event.",
        ),
        TeamError::Full => err(StatusCode::CONFLICT, "This team is full."),
        TeamError::NotFound => err(StatusCode::NOT_FOUND, "No team with this invite code."),
        TeamError::Locked => err(
            StatusCode::CONFLICT,
            "The team has already scored: members can no longer change.",
        ),
    }
}

pub async fn create_team(ctx: &Ctx, slug: &str, body: &Value) -> ApiResult {
    let u = require_user(ctx)?;
    let ev = visible_event(ctx, slug).await?;
    if !ev.team_mode {
        return Err(bad("This is a solo event."));
    }
    ensure_open(&ev)?;
    check_code(&ev, body)?;
    let name = field(body, "name");
    if !valid_team_name(name) {
        return Err(bad("Team name: 2 to 32 characters."));
    }
    let team = event::create_team(&ev, &u.uuid, name)
        .await?
        .map_err(team_error)?;
    audit::log(
        ctx.tenant_uuid(),
        Some(&u.uuid),
        "team.create",
        "team",
        Some(&team.uuid),
        json!({ "event": ev.slug, "name": name }),
    )
    .await?;
    ok(
        json!({ "ok": true, "team": team, "message": "Team created. Share the invite code with your teammates." }),
    )
}

pub async fn join_team(ctx: &Ctx, slug: &str, body: &Value) -> ApiResult {
    let u = require_user(ctx)?;
    let ev = visible_event(ctx, slug).await?;
    if !ev.team_mode {
        return Err(bad("This is a solo event."));
    }
    ensure_open(&ev)?;
    // invite codes are guessable only by brute force: slow that down
    if !ratelimit::allow(&format!("join:{}", u.uuid), 10, 60).await? {
        return Err(too_many());
    }
    let team = event::join_team(&ev, &u.uuid, field(body, "invite"))
        .await?
        .map_err(team_error)?;
    audit::log(
        ctx.tenant_uuid(),
        Some(&u.uuid),
        "team.join",
        "team",
        Some(&team.uuid),
        Value::Null,
    )
    .await?;
    ok(json!({ "ok": true, "message": format!("You joined {}.", team.name) }))
}

pub async fn leave_team(ctx: &Ctx, slug: &str) -> ApiResult {
    let u = require_user(ctx)?;
    let ev = visible_event(ctx, slug).await?;
    let Some(team) = event::team_of(&ev.uuid, &u.uuid).await? else {
        return Err(bad("You are not in a team."));
    };
    event::leave_team(&ev, &team, &u.uuid)
        .await?
        .map_err(team_error)?;
    audit::log(
        ctx.tenant_uuid(),
        Some(&u.uuid),
        "team.leave",
        "team",
        Some(&team.uuid),
        Value::Null,
    )
    .await?;
    ok(json!({ "ok": true, "message": "You left the team." }))
}

pub async fn kick(ctx: &Ctx, slug: &str, body: &Value) -> ApiResult {
    let u = require_user(ctx)?;
    let ev = visible_event(ctx, slug).await?;
    let Some(team) = event::team_of(&ev.uuid, &u.uuid).await? else {
        return Err(bad("You are not in a team."));
    };
    if team.captain_uuid != u.uuid {
        return Err(err(
            StatusCode::FORBIDDEN,
            "Only the captain can remove members.",
        ));
    }
    let name = field(body, "username");
    let members = event::members(&team.uuid).await?;
    let Some((target, _)) = members.iter().find(|(_, n)| n == name) else {
        return Err(not_found());
    };
    if *target == u.uuid {
        return Err(bad("Use Leave team to leave it yourself."));
    }
    event::leave_team(&ev, &team, target)
        .await?
        .map_err(team_error)?;
    ok(json!({ "ok": true, "message": format!("{name} removed from the team.") }))
}

/// Standings as players see them: frozen at freeze_at for everyone but organizers.
pub async fn standings(
    ctx: &Ctx,
    ev: &Event,
) -> Result<std::sync::Arc<Vec<score::Entry>>, ApiError> {
    let until = if ev.frozen() && !ctx.has_role(Role::Author) {
        ev.freeze_at
    } else {
        None
    };
    Ok(score::ranking(&Query {
        tenant: ctx.tenant_uuid(),
        event_key: &ev.uuid,
        teams: ev.team_mode,
        category: "",
        until,
    })
    .await?)
}

pub async fn scoreboard(ctx: &Ctx, slug: &str) -> ApiResult {
    require_user(ctx)?;
    let ev = visible_event(ctx, slug).await?;
    let list = standings(ctx, &ev).await?;
    ok(
        json!({ "event": ev.slug, "phase": ev.phase(), "frozen": ev.frozen(), "teams": ev.team_mode, "standings": *list }),
    )
}

/// CTFtime scoreboard feed. Public once the event is over (CTFtime fetches it without a session).
pub async fn ctftime(ctx: &Ctx, slug: &str) -> ApiResult {
    let Some(ev) = event::get_by_slug(ctx.tenant_uuid(), slug).await? else {
        return Err(not_found());
    };
    let organizer = ctx.has_role(Role::Author);
    if !organizer && (ev.status != "published" || ev.phase() != Phase::Ended) {
        return Err(err(
            StatusCode::FORBIDDEN,
            "The CTFtime feed opens when the event ends.",
        ));
    }
    let list = score::ranking(&Query {
        tenant: ctx.tenant_uuid(),
        event_key: &ev.uuid,
        teams: ev.team_mode,
        category: "",
        until: None,
    })
    .await?;
    let standings: Vec<Value> = list
        .iter()
        .map(|e| json!({ "pos": e.rank, "team": e.name, "score": e.score }))
        .collect();
    ok(json!({ "standings": standings }))
}
