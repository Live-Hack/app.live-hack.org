use crate::helper::request::Ctx;
use crate::helper::template::{Vars, escape_html as e};
use crate::models::event::{self, Event};
use crate::models::manage;
use crate::models::user::Role;
use crate::web::page::{PageResult, RouteResult, page, require_role};
use crate::web::routes::events::phase_badge;
use std::fmt::Write;

fn iso(t: chrono::NaiveDateTime) -> String {
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub async fn list(ctx: &Ctx) -> PageResult {
    if let Err(r) = require_role(ctx, Role::Author) {
        return Ok(r);
    }
    let events = event::list(ctx.tenant_uuid(), true).await?;
    let mut rows = String::new();
    for ev in &events {
        let _ = write!(
            rows,
            r#"<tr><td><a href="/manage/events/{uuid}">{title}</a><div class="muted small mono">{slug}</div></td><td>{badge}</td><td><time datetime="{s}">{s}</time></td><td><time datetime="{en}">{en}</time></td><td>{mode}</td><td class="num">{n}</td>
<td class="actions"><a class="btn btn--sm btn--ghost" href="/events/{slug}">View</a> <a class="btn btn--sm" href="/manage/events/{uuid}">Edit</a></td></tr>"#,
            uuid = ev.uuid,
            title = e(&ev.title),
            slug = e(&ev.slug),
            badge = phase_badge(ev),
            s = iso(ev.starts_at),
            en = iso(ev.ends_at),
            mode = if ev.team_mode { "teams" } else { "solo" },
            n = event::participants(ev).await?,
        );
    }
    if rows.is_empty() {
        rows = r#"<tr><td colspan="7"><div class="empty"><h3>No events yet</h3><p>Create one to run a time-boxed CTF.</p></div></td></tr>"#.into();
    }
    let mut v = Vars::new();
    v.set("rows", rows);
    page(ctx, "manage/events.html", "Manage events", v)
}

fn opts(list: &[(&str, &str)], cur: &str) -> String {
    list.iter()
        .map(|(v, l)| {
            format!(
                r#"<option value="{v}"{}>{l}</option>"#,
                if *v == cur { " selected" } else { "" }
            )
        })
        .collect()
}

/// `uuid` = None for the creation form.
pub async fn edit(ctx: &Ctx, uuid: Option<&str>) -> PageResult {
    if let Err(r) = require_role(ctx, Role::Author) {
        return Ok(r);
    }
    let ev: Option<Event> = match uuid {
        Some(u) => match event::get(ctx.tenant_uuid(), u).await? {
            Some(ev) => Some(ev),
            None => return Ok(RouteResult::NotFound),
        },
        None => None,
    };
    let mut v = Vars::new();
    let now = chrono::Utc::now().naive_utc();
    let (start, end) = match &ev {
        Some(ev) => (ev.starts_at, ev.ends_at),
        None => (
            now + chrono::Duration::days(7),
            now + chrono::Duration::days(9),
        ),
    };
    v.set(
        "ev_title",
        ev.as_ref().map(|e| e.title.as_str()).unwrap_or(""),
    )
    .set(
        "description",
        ev.as_ref().map(|e| e.description.as_str()).unwrap_or(""),
    )
    .set("starts_at", iso(start))
    .set("ends_at", iso(end))
    .set(
        "freeze_at",
        ev.as_ref()
            .and_then(|e| e.freeze_at)
            .map(iso)
            .unwrap_or_default(),
    )
    .set(
        "mode_options",
        opts(
            &[("true", "Teams"), ("false", "Solo")],
            if ev.as_ref().is_none_or(|e| e.team_mode) {
                "true"
            } else {
                "false"
            },
        ),
    )
    .set(
        "max_team_size",
        ev.as_ref()
            .map(|e| e.max_team_size)
            .unwrap_or(4)
            .to_string(),
    )
    .set(
        "reg_options",
        opts(
            &[
                ("open", "Open"),
                ("code", "With a code"),
                ("closed", "Closed"),
            ],
            ev.as_ref()
                .map(|e| e.registration.as_str())
                .unwrap_or("open"),
        ),
    )
    .set(
        "join_code",
        ev.as_ref().map(|e| e.join_code.as_str()).unwrap_or(""),
    )
    .set(
        "after_options",
        opts(
            &[("practice", "Move them to practice"), ("hide", "Hide them")],
            ev.as_ref()
                .map(|e| e.after_end.as_str())
                .unwrap_or("practice"),
        ),
    )
    .set(
        "status_options",
        opts(
            &[
                ("draft", "Draft (organizers only)"),
                ("published", "Published"),
            ],
            ev.as_ref().map(|e| e.status.as_str()).unwrap_or("draft"),
        ),
    );

    let Some(ev) = ev else {
        v.set("action", "/api/v1/manage/events")
            .set("then", "data-then=\"open\"")
            .set("heading", "New event");
        return page(ctx, "manage/event_edit.html", "New event", v);
    };
    v.set("action", format!("/api/v1/manage/events/{}", ev.uuid))
        .set("heading", ev.title.as_str())
        .set("slug", ev.slug.as_str())
        .set("badge", phase_badge(&ev));

    // challenge picker: event ones first, then the rest
    let in_event = event::challs(&ev, "", true).await?;
    let me = ctx.user.as_ref().unwrap();
    let all = manage::list(ctx.tenant_uuid(), &me.uuid, me.role == Role::Admin, "", "").await?;
    let mut picked = String::new();
    for c in &in_event {
        let _ = write!(
            picked,
            r#"<li><a href="/challenges/{slug}">{name}</a><span class="muted small">{cat} · {pts} pts</span><button class="btn btn--sm btn--ghost" type="button" data-post="/api/v1/manage/events/{ev}/challenges/{uuid}/remove" data-then="reload">Remove</button></li>"#,
            slug = e(&c.slug),
            name = e(&c.name),
            cat = e(&c.category),
            pts = c.points,
            ev = ev.uuid,
            uuid = c.uuid,
        );
    }
    if picked.is_empty() {
        picked = r#"<li class="muted">No challenges yet.</li>"#.into();
    }
    let mut options = String::new();
    for c in all
        .iter()
        .filter(|c| !in_event.iter().any(|x| x.uuid == c.uuid) && c.status != "archived")
    {
        let _ = write!(
            options,
            r#"<option value="{}">{} ({}, {} pts, {})</option>"#,
            c.uuid,
            e(&c.name),
            e(&c.category),
            c.points,
            c.status
        );
    }

    // participants with disqualify toggles (admins)
    let admin = ctx.has_role(Role::Admin);
    let mut people = String::new();
    if ev.team_mode {
        for t in event::teams(&ev.uuid).await? {
            let action = if admin {
                format!(
                    r#"<button class="btn btn--sm {cls}" type="button" data-post="/api/v1/manage/events/{ev}/disqualify/{id}" data-post-json='{{"disqualified":{on}}}' data-then="reload">{label}</button>"#,
                    cls = if t.banned {
                        "btn--ghost"
                    } else {
                        "btn--danger"
                    },
                    ev = ev.uuid,
                    id = t.uuid,
                    on = !t.banned,
                    label = if t.banned { "Requalify" } else { "Disqualify" }
                )
            } else {
                String::new()
            };
            let _ = write!(
                people,
                r#"<li><span>{}{}</span><span class="muted small">{} member(s)</span>{action}</li>"#,
                e(&t.name),
                if t.banned { " · disqualified" } else { "" },
                t.members
            );
        }
    }
    if people.is_empty() {
        people = r#"<li class="muted">Nobody registered yet.</li>"#.into();
    }
    v.set("picked", picked)
        .set("chall_options", options)
        .set("people", people)
        .set("uuid", ev.uuid.as_str())
        .set("then", "data-reload");
    if admin {
        v.set("delete", format!(r#"<button class="btn btn--danger" type="button" data-post="/api/v1/manage/events/{}/delete" data-confirm="Click again to delete" data-redirect="/manage/events">Delete event</button>"#, ev.uuid));
    }
    page(
        ctx,
        "manage/event_edit.html",
        &format!("Edit {}", ev.title),
        v,
    )
}
