use crate::helper::request::Ctx;
use crate::helper::template::{Vars, escape_html as e};
use crate::models::admin;
use crate::models::user::{self, Role};
use crate::web::page::{PageResult, RouteResult, encode, page, require_role};
use std::fmt::Write;

fn guard(ctx: &Ctx) -> Option<RouteResult> {
    require_role(ctx, Role::Admin).err()
}

fn ts(t: chrono::NaiveDateTime) -> String {
    let s = t.format("%Y-%m-%d %H:%M:%S");
    format!(r#"<time datetime="{s}Z">{s}</time>"#)
}

/// 24 bars, last one = current hour.
fn sparkline(values: &[i64]) -> String {
    let max = values.iter().copied().max().unwrap_or(0).max(1) as f64;
    let mut out = String::from(
        r#"<svg class="spark" viewBox="0 0 240 60" preserveAspectRatio="none" role="img" aria-label="Solves per hour, last 24 hours">"#,
    );
    for (i, v) in values.iter().enumerate() {
        let h = (*v as f64 / max * 56.0).max(if *v > 0 { 2.0 } else { 0.5 });
        let _ = write!(
            out,
            r#"<rect x="{}" y="{:.1}" width="8" height="{:.1}" rx="2"><title>{v} solve(s), {}h ago</title></rect>"#,
            i * 10 + 1,
            60.0 - h,
            h,
            23 - i
        );
    }
    out.push_str("</svg>");
    out
}

pub async fn dashboard(ctx: &Ctx) -> PageResult {
    if let Some(r) = guard(ctx) {
        return Ok(r);
    }
    let t = ctx.tenant_uuid();
    let s = admin::stats(t).await?;
    let hourly = admin::solves_per_hour(t).await?;
    let mut shared = String::new();
    for a in admin::audit(t, "flag.shared", 10).await? {
        let _ = write!(
            shared,
            r#"<li><b>{}</b> submitted another lab's flag <span class="muted small mono">{}</span>{}</li>"#,
            e(a.actor.as_deref().unwrap_or("?")),
            e(a.meta.as_deref().unwrap_or("")),
            ts(a.at)
        );
    }
    if shared.is_empty() {
        shared = r#"<li class="muted">No shared flags detected.</li>"#.into();
    }
    let mut feed = String::new();
    for a in admin::attempts(t, None, 12).await? {
        let _ = write!(
            feed,
            r#"<li><a href="/users/{u}">{u}</a> {verb} <a href="/challenges/{slug}">{c}</a>{ev}{at}</li>"#,
            u = e(&a.username),
            verb = if a.correct {
                r#"<span class="badge badge--ok">solved</span>"#
            } else {
                r#"<span class="badge badge--muted">tried</span>"#
            },
            slug = e(&a.slug),
            c = e(&a.chall),
            ev = a
                .event
                .map(|t| format!(r#" <span class="muted small">· {}</span>"#, e(&t)))
                .unwrap_or_default(),
            at = ts(a.at),
        );
    }
    if feed.is_empty() {
        feed = r#"<li class="muted">No activity yet.</li>"#.into();
    }
    let mut v = Vars::new();
    v.set("players", s.players.to_string())
        .set("organizers", s.organizers.to_string())
        .set("banned", s.banned.to_string())
        .set("published", s.published.to_string())
        .set("drafts", s.drafts.to_string())
        .set("live_labs", s.live_labs.to_string())
        .set("solves_24h", s.solves_24h.to_string())
        .set("attempts_24h", s.attempts_24h.to_string())
        .set("shared_24h", s.shared_flags_24h.to_string())
        .set(
            "shared_class",
            if s.shared_flags_24h > 0 {
                "stat--alert"
            } else {
                ""
            },
        )
        .set("live_events", s.live_events.to_string())
        .set("spark", sparkline(&hourly))
        .set("shared", shared)
        .set("feed", feed)
        .set(
            "labs_state",
            if crate::helper::labs::enabled() {
                "connected"
            } else {
                "not configured"
            },
        );
    page(ctx, "admin/dashboard.html", "Admin", v)
}

fn role_select(u: &user::User) -> String {
    let opts: String = ["player", "author", "admin"]
        .iter()
        .map(|r| {
            format!(
                r#"<option value="{r}"{}>{r}</option>"#,
                if *r == u.role.as_str() {
                    " selected"
                } else {
                    ""
                }
            )
        })
        .collect();
    format!(
        r#"<select class="select select--sm" data-role-for="{}" aria-label="Role of {}">{opts}</select>"#,
        u.uuid,
        e(&u.username)
    )
}

pub async fn users(ctx: &Ctx) -> PageResult {
    if let Some(r) = guard(ctx) {
        return Ok(r);
    }
    let me = ctx.user.as_ref().unwrap();
    let q = ctx.param("q");
    let list = admin::users(ctx.tenant_uuid(), q, 300).await?;
    let mut rows = String::new();
    for u in &list {
        let own = u.uuid == me.uuid;
        let actions = if own {
            r#"<span class="muted small">you</span>"#.to_string()
        } else {
            format!(
                r#"<button class="btn btn--sm btn--ghost" type="button" data-post="/api/v1/admin/users/{id}/reset-link" data-then="link">Reset link</button>
<button class="btn btn--sm {cls}" type="button" data-post="/api/v1/admin/users/{id}/ban" data-post-json='{{"banned":{on}}}' {confirm} data-then="reload">{label}</button>"#,
                id = u.uuid,
                cls = if u.banned {
                    "btn--ghost"
                } else {
                    "btn--danger"
                },
                on = !u.banned,
                confirm = if u.banned {
                    ""
                } else {
                    r#"data-confirm="Confirm ban""#
                },
                label = if u.banned { "Unban" } else { "Ban" },
            )
        };
        let _ = write!(
            rows,
            r#"<tr{cls}><td><a href="/admin/users/{id}">{name}</a>{ban}<div class="muted small">{email}</div></td><td>{role}</td><td>{created}</td><td class="actions">{actions}</td></tr>"#,
            cls = if u.banned { r#" class="is-muted""# } else { "" },
            id = u.uuid,
            name = e(&u.username),
            ban = if u.banned {
                r#" <span class="badge badge--danger">banned</span>"#
            } else {
                ""
            },
            email = e(u.email.as_deref().unwrap_or("")),
            role = if own {
                format!(r#"<span class="badge">{}</span>"#, u.role.as_str())
            } else {
                role_select(u)
            },
            created = ts(u.created_at),
        );
    }
    if rows.is_empty() {
        rows = r#"<tr><td colspan="4" class="muted">No user matches.</td></tr>"#.into();
    }
    let mut v = Vars::new();
    v.set("rows", rows)
        .set("q", q)
        .set("count", list.len().to_string())
        .set("registration", ctx.tenant.setting("registration"));
    page(ctx, "admin/users.html", "Users", v)
}

pub async fn user_detail(ctx: &Ctx, uuid: &str) -> PageResult {
    if let Some(r) = guard(ctx) {
        return Ok(r);
    }
    let Some(u) = user::get(ctx.tenant_uuid(), uuid).await? else {
        return Ok(RouteResult::NotFound);
    };
    let mut rows = String::new();
    for a in admin::attempts(ctx.tenant_uuid(), Some(&u.uuid), 300).await? {
        let _ = write!(
            rows,
            r#"<tr><td><a href="/challenges/{slug}">{c}</a></td><td>{ok}</td><td>{ev}</td><td class="mono small">{ip}</td><td>{at}</td></tr>"#,
            slug = e(&a.slug),
            c = e(&a.chall),
            ok = if a.correct {
                r#"<span class="badge badge--ok">correct</span>"#
            } else {
                r#"<span class="badge badge--muted">wrong</span>"#
            },
            ev = e(a.event.as_deref().unwrap_or("practice")),
            ip = e(&a.ip),
            at = ts(a.at),
        );
    }
    if rows.is_empty() {
        rows = r#"<tr><td colspan="5" class="muted">No attempts yet.</td></tr>"#.into();
    }
    let mut v = Vars::new();
    v.set("uuid", u.uuid.as_str())
        .set("username", u.username.as_str())
        .set("email", u.email.as_deref().unwrap_or("—"))
        .set("role", u.role.as_str())
        .set("created", ts(u.created_at))
        .set(
            "status",
            if u.banned {
                r#"<span class="badge badge--danger">banned</span>"#
            } else {
                r#"<span class="badge badge--ok">active</span>"#
            },
        )
        .set("rows", rows)
        .set("profile", format!("/users/{}", encode(&u.username)));
    page(ctx, "admin/user.html", &u.username, v)
}

pub async fn labs(ctx: &Ctx) -> PageResult {
    if let Some(r) = guard(ctx) {
        return Ok(r);
    }
    let mut rows = String::new();
    for l in admin::labs(ctx.tenant_uuid()).await? {
        let _ = write!(
            rows,
            r#"<tr><td>{chall}</td><td>{who}</td><td><span class="badge {cls}">{st}</span></td><td class="num mono">{port}</td><td>{created}</td><td>{exp}</td>
<td class="actions"><button class="btn btn--sm btn--ghost" type="button" data-logs="/api/v1/admin/labs/{id}/logs">Logs</button>
<button class="btn btn--sm btn--danger" type="button" data-post="/api/v1/admin/labs/{id}/stop" data-confirm="Confirm stop" data-then="reload">Stop</button></td></tr>"#,
            chall = e(&l.chall),
            who = match &l.team {
                Some(t) => format!(
                    "{} <span class=\"muted small\">({})</span>",
                    e(t),
                    e(&l.username)
                ),
                None => e(&l.username),
            },
            cls = if l.status == "running" {
                "badge--ok"
            } else {
                "badge--warn"
            },
            st = l.status,
            port = l.host_port,
            created = ts(l.created_at),
            exp = ts(l.expires_at),
            id = l.uuid,
        );
    }
    if rows.is_empty() {
        rows = r#"<tr><td colspan="7" class="muted">No lab is running.</td></tr>"#.into();
    }
    let mut v = Vars::new();
    v.set("rows", rows)
        .set("max", crate::helper::labs::MAX_INSTANCES.to_string())
        .set("host", crate::helper::labs::INSTANCE_HOST.as_str())
        .set(
            "state",
            if crate::helper::labs::enabled() {
                "Docker connected"
            } else {
                "Docker not configured (DOCKER_HOST)"
            },
        );
    page(ctx, "admin/labs.html", "Labs", v)
}

pub async fn audit(ctx: &Ctx) -> PageResult {
    if let Some(r) = guard(ctx) {
        return Ok(r);
    }
    let action = ctx.param("action");
    let mut rows = String::new();
    for a in admin::audit(ctx.tenant_uuid(), action, 300).await? {
        let _ = write!(
            rows,
            r#"<tr><td>{at}</td><td>{actor}</td><td><span class="mono small">{action}</span></td><td class="muted small">{tt} {tu}</td><td class="mono small meta">{meta}</td></tr>"#,
            at = ts(a.at),
            actor = e(a.actor.as_deref().unwrap_or("—")),
            action = e(&a.action),
            tt = e(&a.target_type),
            tu = e(a
                .target_uuid
                .as_deref()
                .map(|u| &u[..8.min(u.len())])
                .unwrap_or("")),
            meta = e(a.meta.as_deref().unwrap_or("")),
        );
    }
    if rows.is_empty() {
        rows = r#"<tr><td colspan="5" class="muted">Nothing recorded.</td></tr>"#.into();
    }
    let tabs: String = [
        ("", "All"),
        ("user.", "Users"),
        ("chall.", "Challenges"),
        ("event.", "Events"),
        ("lab.", "Labs"),
        ("flag.shared", "Shared flags"),
        ("settings", "Settings"),
    ]
    .iter()
    .map(|(a, l)| {
        format!(
            r#"<a class="chip{}" href="/admin/audit{}">{l}</a>"#,
            if *a == action { " is-active" } else { "" },
            if a.is_empty() {
                String::new()
            } else {
                format!("?action={a}")
            }
        )
    })
    .collect();
    let mut v = Vars::new();
    v.set("rows", rows).set("tabs", tabs);
    page(ctx, "admin/audit.html", "Audit log", v)
}

pub async fn settings(ctx: &Ctx) -> PageResult {
    if let Some(r) = guard(ctx) {
        return Ok(r);
    }
    let sel = |key: &str, opts: &[(&str, &str)], default: &str| -> String {
        let cur = match ctx.tenant.setting(key) {
            "" => default,
            c => c,
        };
        opts.iter()
            .map(|(v, l)| {
                format!(
                    r#"<option value="{v}"{}>{l}</option>"#,
                    if *v == cur { " selected" } else { "" }
                )
            })
            .collect()
    };
    let mut v = Vars::new();
    v.set("site_name", ctx.tenant.site_name())
        .set("flag_format", ctx.tenant.setting("flag_format"))
        .set(
            "reg_options",
            sel(
                "registration",
                &[
                    ("open", "Open to anyone"),
                    ("invite", "Invitation link only"),
                    ("closed", "Closed (accounts created by admins)"),
                ],
                "open",
            ),
        )
        .set(
            "sb_options",
            sel(
                "scoreboard",
                &[
                    ("visible", "Visible to players"),
                    ("hidden", "Hidden from players"),
                ],
                "visible",
            ),
        )
        .set(
            "labs_options",
            sel(
                "labs",
                &[
                    ("enabled", "Players can start labs"),
                    ("disabled", "Paused"),
                ],
                "enabled",
            ),
        );
    page(ctx, "admin/settings.html", "Settings", v)
}
