use crate::helper::markdown;
use crate::helper::request::Ctx;
use crate::helper::template::{Vars, escape_html as e, render};
use crate::models::event::{self, Event, Phase};

use crate::models::user::Role;
use crate::web::page::{PageResult, RouteResult, page, require_login};
use crate::web::routes::challenges::card_html;
use crate::web::routes::score::rows_html;
use std::fmt::Write;

fn iso(t: chrono::NaiveDateTime) -> String {
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub fn phase_badge(ev: &Event) -> &'static str {
    if ev.status != "published" {
        return r#"<span class="badge badge--muted">draft</span>"#;
    }
    match ev.phase() {
        Phase::Upcoming => r#"<span class="badge">upcoming</span>"#,
        Phase::Live => r#"<span class="badge badge--ok badge--live">live</span>"#,
        Phase::Ended => r#"<span class="badge badge--muted">ended</span>"#,
    }
}

async fn visible(ctx: &Ctx, slug: &str) -> Result<Option<Event>, mysql_async::Error> {
    let Some(ev) = event::get_by_slug(ctx.tenant_uuid(), slug).await? else {
        return Ok(None);
    };
    Ok((ev.status == "published" || ctx.has_role(Role::Author)).then_some(ev))
}

pub async fn list(ctx: &Ctx) -> PageResult {
    if let Err(r) = require_login(ctx) {
        return Ok(r);
    }
    let events = event::list(ctx.tenant_uuid(), ctx.has_role(Role::Author)).await?;
    let mut live = String::new();
    let mut rest = String::new();
    for ev in &events {
        let n = event::participants(ev).await?;
        let _ = write!(
            if ev.phase() == Phase::Live {
                &mut live
            } else {
                &mut rest
            },
            r#"<a class="tile event-tile" href="/events/{slug}"><div class="tile__top">{badge}<span class="muted small">{mode}</span></div><h3 class="tile__title">{title}</h3>
<p class="muted small"><time datetime="{s}">{s}</time> → <time datetime="{en}">{en}</time></p><div class="tile__meta"><span><b>{n}</b> {who}</span></div></a>"#,
            slug = e(&ev.slug),
            badge = phase_badge(ev),
            mode = if ev.team_mode {
                format!("teams of {}", ev.max_team_size)
            } else {
                "solo".into()
            },
            title = e(&ev.title),
            s = iso(ev.starts_at),
            en = iso(ev.ends_at),
            who = match (ev.team_mode, n) {
                (true, 1) => "team",
                (true, _) => "teams",
                (false, 1) => "player",
                _ => "players",
            },
        );
    }
    let body = if live.is_empty() && rest.is_empty() {
        r#"<div class="empty"><h3>No events yet</h3><p>When the organizers schedule a CTF it shows up here, with a countdown.</p></div>"#.to_string()
    } else {
        format!(
            "{}{}",
            if live.is_empty() {
                String::new()
            } else {
                format!(
                    r#"<div class="section-head"><h2>Live now</h2></div><div class="grid grid--cards">{live}</div>"#
                )
            },
            if rest.is_empty() {
                String::new()
            } else {
                format!(
                    r#"<div class="section-head"><h2>All events</h2></div><div class="grid grid--cards">{rest}</div>"#
                )
            }
        )
    };
    let mut v = Vars::new();
    v.set("body", body);
    if ctx.has_role(Role::Author) {
        v.set(
            "manage",
            r#"<a class="btn btn--ghost" href="/manage/events">Manage events</a>"#,
        );
    }
    page(ctx, "events/list.html", "Events", v)
}

pub async fn detail(ctx: &Ctx, slug: &str) -> PageResult {
    let me = match require_login(ctx) {
        Ok(u) => u,
        Err(r) => return Ok(r),
    };
    let Some(ev) = visible(ctx, slug).await? else {
        return Ok(RouteResult::NotFound);
    };
    let organizer = ctx.has_role(Role::Author);
    let phase = ev.phase();

    // participation
    let join;
    let mut scope_key = me.uuid.clone();
    let mut registered = false;
    let mut v = Vars::new();
    v.set("slug", ev.slug.as_str());
    if ev.team_mode {
        match event::team_of(&ev.uuid, &me.uuid).await? {
            Some(team) => {
                registered = !team.banned;
                scope_key = team.uuid.clone();
                let members = event::members(&team.uuid).await?;
                let captain = team.captain_uuid == me.uuid;
                let mut list = String::new();
                for (uuid, name) in &members {
                    let kick = if captain && *uuid != me.uuid {
                        format!(
                            r#"<button class="btn btn--sm btn--ghost" type="button" data-post="/api/v1/events/{}/teams/kick" data-post-json='{{"username":"{}"}}' data-confirm="Remove?" data-then="reload">Remove</button>"#,
                            e(&ev.slug),
                            e(name)
                        )
                    } else if *uuid == team.captain_uuid {
                        r#"<span class="badge badge--muted">captain</span>"#.into()
                    } else {
                        String::new()
                    };
                    let _ = write!(
                        list,
                        r#"<li><a href="/users/{0}">{0}</a>{kick}</li>"#,
                        e(name)
                    );
                }
                let mut tv = Vars::new();
                tv.set("slug", ev.slug.as_str())
                    .set("name", team.name.as_str())
                    .set("code", team.invite_code.as_str())
                    .set("members", list)
                    .set("count", format!("{}/{}", members.len(), ev.max_team_size))
                    .set("banned", if team.banned { r#"<div class="notice">This team was disqualified by the organizers.</div>"# } else { "" });
                join = render("partials/event_team.html", &tv);
            }
            None if ev.can_register() => {
                let mut tv = Vars::new();
                tv.set("slug", ev.slug.as_str());
                if ev.registration == "code" {
                    tv.set("code_field", r#"<label class="field"><span>Event code</span><input class="input" name="code" required autocomplete="off"></label>"#);
                }
                join = render("partials/event_join_team.html", &tv);
            }
            None => join = r#"<p class="muted">Registration is closed.</p>"#.into(),
        }
    } else {
        match event::solo_state(&ev.uuid, &me.uuid).await? {
            Some(dq) => {
                registered = !dq;
                join = if dq {
                    r#"<div class="notice">You were disqualified from this event.</div>"#.into()
                } else {
                    r#"<p class="registered">You are registered.</p>"#.into()
                };
            }
            None if ev.can_register() => {
                let code = if ev.registration == "code" {
                    r#"<input class="input" name="code" placeholder="Event code" required autocomplete="off">"#
                } else {
                    ""
                };
                join = format!(
                    r#"<form class="row-flex" data-api="/api/v1/events/{}/register" data-reload>{code}<button class="btn" type="submit">Register</button><p class="form__msg" data-msg role="alert"></p></form>"#,
                    e(&ev.slug)
                );
            }
            None => join = r#"<p class="muted">Registration is closed.</p>"#.into(),
        }
    }

    // challenges: during the event for participants, after it for everyone if moved to practice
    let show_challs = organizer
        || (phase == Phase::Live && registered)
        || (phase == Phase::Ended && ev.after_end == "practice");
    let challs = if show_challs {
        let list = event::challs(&ev, &scope_key, organizer).await?;
        if list.is_empty() {
            r#"<div class="empty"><h3>No challenges yet</h3><p>The organizers have not added challenges to this event.</p></div>"#.to_string()
        } else {
            list.iter()
                .map(|c| card_html(c, &format!("/challenges/{}", c.slug)))
                .collect()
        }
    } else {
        match phase {
            Phase::Upcoming => r#"<div class="empty"><h3>Challenges open when the event starts</h3><p>Register now so you are ready at the start.</p></div>"#.into(),
            Phase::Live => r#"<div class="empty"><h3>Register to play</h3><p>Challenges are shown to registered participants.</p></div>"#.into(),
            Phase::Ended => r#"<div class="empty"><h3>This event is over</h3><p>Its challenges are not available anymore.</p></div>"#.into(),
        }
    };

    let (count_label, count_to) = match phase {
        Phase::Upcoming => ("Starts in", iso(ev.starts_at)),
        Phase::Live => ("Ends in", iso(ev.ends_at)),
        Phase::Ended => ("Ended", String::new()),
    };
    let list = crate::api::v1::events::standings(ctx, &ev)
        .await
        .map_err(|_| mysql_async::Error::Other("standings".into()))?;
    let mut top = rows_html(&list[..list.len().min(10)], &scope_key, |r| {
        (!ev.team_mode).then(|| format!("/users/{}", crate::web::page::encode(&r.name)))
    });
    if top.is_empty() {
        top = r#"<tr><td colspan="5" class="muted">No solves yet.</td></tr>"#.into();
    }

    v.set("ev_title", ev.title.as_str())
        .set("badge", phase_badge(&ev))
        .set("description", markdown::render(&ev.description))
        .set("starts", iso(ev.starts_at))
        .set("ends", iso(ev.ends_at))
        .set(
            "mode",
            if ev.team_mode {
                format!("Teams of up to {}", ev.max_team_size)
            } else {
                "Solo".into()
            },
        )
        .set("participants", event::participants(&ev).await?.to_string())
        .set("count_label", count_label)
        .set("count_to", count_to)
        .set("join", join)
        .set("challs", challs)
        .set("top", top)
        .set(
            "frozen",
            if ev.frozen() && phase == Phase::Live {
                r#"<span class="badge badge--warn">frozen</span>"#
            } else {
                ""
            },
        );
    if organizer {
        v.set(
            "manage",
            format!(
                r#"<a class="btn btn--ghost" href="/manage/events/{}">Edit event</a>"#,
                ev.uuid
            ),
        );
    }
    page(ctx, "events/detail.html", &ev.title, v)
}

pub async fn scoreboard(ctx: &Ctx, slug: &str) -> PageResult {
    let me = match require_login(ctx) {
        Ok(u) => u,
        Err(r) => return Ok(r),
    };
    let Some(ev) = visible(ctx, slug).await? else {
        return Ok(RouteResult::NotFound);
    };
    let list = crate::api::v1::events::standings(ctx, &ev)
        .await
        .map_err(|_| mysql_async::Error::Other("standings".into()))?;
    let mine = if ev.team_mode {
        event::team_of(&ev.uuid, &me.uuid)
            .await?
            .map(|t| t.uuid)
            .unwrap_or_default()
    } else {
        me.uuid.clone()
    };
    let mut rows = rows_html(&list, &mine, |r| {
        (!ev.team_mode).then(|| format!("/users/{}", crate::web::page::encode(&r.name)))
    });
    if rows.is_empty() {
        rows = r#"<tr><td colspan="5"><div class="empty"><h3>No solves yet</h3></div></td></tr>"#
            .into();
    }
    let mut v = Vars::new();
    v.set("slug", ev.slug.as_str())
        .set("ev_title", ev.title.as_str())
        .set("rows", rows)
        .set("who", if ev.team_mode { "Team" } else { "Player" })
        .set("frozen", if ev.frozen() && !ctx.has_role(Role::Author) && ev.phase() == Phase::Live {
            r#"<div class="notice">The scoreboard is frozen: solves after the freeze appear when the event ends.</div>"#
        } else {
            ""
        })
        .set("ctftime", if ev.phase() == Phase::Ended || ctx.has_role(Role::Author) {
            format!(r#"<a class="btn btn--ghost btn--sm" href="/api/v1/events/{}/ctftime">CTFtime JSON</a>"#, e(&ev.slug))
        } else {
            String::new()
        });
    page(
        ctx,
        "events/scoreboard.html",
        &format!("{} scoreboard", ev.title),
        v,
    )
}
