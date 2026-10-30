//! Organizer side of events (authors and admins).
use crate::api::v1::core::{ApiError, ApiResult, bad, created, not_found, ok, require_role};
use crate::helper::request::{Ctx, field};
use crate::models::event::{self, Event, EventFields};
use crate::models::manage::slugify;
use crate::models::user::Role;
use crate::models::{audit, chall};
use chrono::NaiveDateTime;
use serde_json::{Value, json};

/// "2026-11-02T18:00", "2026-11-02T18:00:00Z", "2026-11-02 18:00:00" → UTC naive time.
pub fn parse_time(s: &str) -> Option<NaiveDateTime> {
    let s = s.trim().trim_end_matches('Z').replace(' ', "T");
    let s = s.split('.').next().unwrap_or("");
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M"))
        .ok()
}

fn fields(body: &Value) -> Result<EventFields, ApiError> {
    let title = field(body, "title");
    if title.is_empty() || title.chars().count() > 120 {
        return Err(bad("Title: 1 to 120 characters."));
    }
    let starts_at = parse_time(field(body, "starts_at")).ok_or_else(|| bad("Set a start date."))?;
    let ends_at = parse_time(field(body, "ends_at")).ok_or_else(|| bad("Set an end date."))?;
    if ends_at <= starts_at {
        return Err(bad("The event must end after it starts."));
    }
    let freeze_at = match field(body, "freeze_at") {
        "" => None,
        f => Some(parse_time(f).ok_or_else(|| bad("Invalid freeze date."))?),
    };
    if freeze_at.is_some_and(|f| f < starts_at || f > ends_at) {
        return Err(bad("The freeze must fall between start and end."));
    }
    let team_mode = body
        .get("team_mode")
        .and_then(|v| {
            v.as_bool()
                .or_else(|| v.as_str().map(|s| s == "true" || s == "1"))
        })
        .unwrap_or(true);
    let max_team_size = body
        .get("max_team_size")
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
        .unwrap_or(4);
    if !(1..=50).contains(&max_team_size) {
        return Err(bad("Team size: 1 to 50."));
    }
    let registration = match field(body, "registration") {
        "" => "open",
        r => r,
    };
    if !["open", "code", "closed"].contains(&registration) {
        return Err(bad("Unknown registration mode."));
    }
    let join_code = field(body, "join_code");
    if registration == "code" && join_code.is_empty() {
        return Err(bad("Set the registration code."));
    }
    let after_end = match field(body, "after_end") {
        "" => "practice",
        a => a,
    };
    let status = match field(body, "status") {
        "" => "draft",
        s => s,
    };
    if !["hide", "practice"].contains(&after_end) || !["draft", "published"].contains(&status) {
        return Err(bad("Invalid option."));
    }
    let description = body
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("");
    if description.len() > 100_000 {
        return Err(bad("Description is too long."));
    }
    Ok(EventFields {
        title: title.into(),
        description: description.into(),
        starts_at,
        ends_at,
        freeze_at,
        team_mode,
        max_team_size: max_team_size as i32,
        registration: registration.into(),
        join_code: join_code.chars().take(64).collect(),
        after_end: after_end.into(),
        status: status.into(),
    })
}

async fn managed(ctx: &Ctx, uuid: &str) -> Result<Event, ApiError> {
    require_role(ctx, Role::Author)?;
    event::get(ctx.tenant_uuid(), uuid)
        .await?
        .ok_or_else(not_found)
}

pub async fn create(ctx: &Ctx, body: &Value) -> ApiResult {
    let u = require_role(ctx, Role::Author)?;
    let f = fields(body)?;
    let base = slugify(&f.title);
    let mut slug = base.clone();
    let mut i = 2;
    while event::slug_taken(ctx.tenant_uuid(), &slug).await? {
        slug = format!("{base}-{i}");
        i += 1;
    }
    let uuid = event::create(ctx.tenant_uuid(), &slug, &f).await?;
    audit::log(
        ctx.tenant_uuid(),
        Some(&u.uuid),
        "event.create",
        "event",
        Some(&uuid),
        json!({ "slug": slug }),
    )
    .await?;
    created(
        json!({ "uuid": uuid, "slug": slug, "edit": format!("/manage/events/{uuid}"), "message": "Event created." }),
    )
}

pub async fn update(ctx: &Ctx, uuid: &str, body: &Value) -> ApiResult {
    let ev = managed(ctx, uuid).await?;
    let f = fields(body)?;
    event::update(ctx.tenant_uuid(), &ev.uuid, &f).await?;
    audit::log(
        ctx.tenant_uuid(),
        ctx.user.as_ref().map(|u| u.uuid.as_str()),
        "event.update",
        "event",
        Some(&ev.uuid),
        Value::Null,
    )
    .await?;
    ok(json!({ "ok": true, "message": "Saved." }))
}

pub async fn delete(ctx: &Ctx, uuid: &str) -> ApiResult {
    let u = require_role(ctx, Role::Admin)?;
    let ev = managed(ctx, uuid).await?;
    event::delete(ctx.tenant_uuid(), &ev.uuid).await?;
    audit::log(
        ctx.tenant_uuid(),
        Some(&u.uuid),
        "event.delete",
        "event",
        Some(&ev.uuid),
        json!({ "slug": ev.slug }),
    )
    .await?;
    ok(json!({ "ok": true, "message": "Event deleted." }))
}

pub async fn add_chall(ctx: &Ctx, uuid: &str, body: &Value) -> ApiResult {
    let ev = managed(ctx, uuid).await?;
    let Some(c) = chall::get(ctx.tenant_uuid(), field(body, "chall")).await? else {
        return Err(not_found());
    };
    let points = body
        .get("points")
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.trim().parse().ok()))
        .map(|p| p.clamp(0, 100_000) as i32);
    event::add_chall(&ev, &c.uuid, points).await?;
    ok(json!({ "ok": true, "message": format!("{} added to the event.", c.name) }))
}

pub async fn remove_chall(ctx: &Ctx, uuid: &str, chall_uuid: &str) -> ApiResult {
    let ev = managed(ctx, uuid).await?;
    event::remove_chall(&ev, chall_uuid).await?;
    ok(json!({ "ok": true, "message": "Removed from the event." }))
}

/// Disqualify / requalify a team (team events) or a player (solo events).
pub async fn disqualify(ctx: &Ctx, uuid: &str, target: &str, body: &Value) -> ApiResult {
    let u = require_role(ctx, Role::Admin)?;
    let ev = managed(ctx, uuid).await?;
    let on = body
        .get("disqualified")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let done = if ev.team_mode {
        event::set_team_banned(&ev.uuid, target, on).await?
    } else {
        event::set_player_banned(&ev.uuid, target, on).await?
    };
    if !done {
        return Err(not_found());
    }
    audit::log(
        ctx.tenant_uuid(),
        Some(&u.uuid),
        if on {
            "event.disqualify"
        } else {
            "event.requalify"
        },
        "event",
        Some(&ev.uuid),
        json!({ "target": target }),
    )
    .await?;
    ok(json!({ "ok": true, "message": if on { "Disqualified." } else { "Back in the ranking." } }))
}

#[cfg(test)]
mod tests {
    use super::parse_time;

    #[test]
    fn times() {
        let a = parse_time("2026-11-02T18:00").unwrap();
        assert_eq!(a.to_string(), "2026-11-02 18:00:00");
        assert_eq!(
            parse_time("2026-11-02T18:00:30.000Z").unwrap().to_string(),
            "2026-11-02 18:00:30"
        );
        assert_eq!(parse_time("2026-11-02 18:00:00").unwrap(), a);
        assert!(parse_time("tomorrow").is_none());
    }
}
