use crate::helper::request::Ctx;
use crate::helper::template::{Vars, escape_html as e};
use crate::models::chall::{self, Filter};
use crate::models::score::{self, Entry, Query};
use crate::models::user::{self, Role};
use crate::web::page::{PageResult, RouteResult, page, require_login};
use crate::web::routes::challenges::{card_html, cat_class, pager};
use std::fmt::Write;

const PER_PAGE: usize = 50;

pub fn scoreboard_hidden(ctx: &Ctx) -> bool {
    ctx.tenant.setting("scoreboard") == "hidden" && !ctx.has_role(Role::Admin)
}

fn medal(rank: usize) -> String {
    match rank {
        1 => r#"<span class="dash-rank dash-rank--gold">1</span>"#.into(),
        2 => r#"<span class="dash-rank dash-rank--silver">2</span>"#.into(),
        3 => r#"<span class="dash-rank dash-rank--bronze">3</span>"#.into(),
        n => format!(r#"<span class="dash-rank">{n}</span>"#),
    }
}

/// Table rows for a ranking slice; `me` is highlighted. `href` builds the profile link.
pub fn rows_html(list: &[Entry], me: &str, href: impl Fn(&Entry) -> Option<String>) -> String {
    let mut out = String::new();
    for r in list {
        let name = match href(r) {
            Some(h) => format!(r#"<a href="{h}">{}</a>"#, e(&r.name)),
            None => e(&r.name),
        };
        let _ = write!(
            out,
            r#"<tr{cls}><td>{rank}</td><td>{name}</td><td class="num"><b>{score}</b></td><td class="num">{solves}</td><td><time datetime="{last}Z">{last}</time></td></tr>"#,
            cls = if r.id == me { r#" class="is-me""# } else { "" },
            rank = medal(r.rank),
            score = r.score,
            solves = r.solves,
            last = r.last_solve.format("%Y-%m-%d %H:%M:%S"),
        );
    }
    out
}

pub async fn scoreboard(ctx: &Ctx) -> PageResult {
    let me = match require_login(ctx) {
        Ok(u) => u,
        Err(r) => return Ok(r),
    };
    if scoreboard_hidden(ctx) {
        let mut v = Vars::new();
        v.set("code", "Hidden").set(
            "message",
            "The organizers have hidden the scoreboard for now.",
        );
        return page(ctx, "error.html", "Scoreboard", v);
    }
    let category = ctx.param("category");
    let list = score::ranking(&Query {
        tenant: ctx.tenant_uuid(),
        event_key: "",
        teams: false,
        category,
        until: None,
    })
    .await?;
    let pages = list.len().div_ceil(PER_PAGE).max(1) as u32;
    let page_n = ctx
        .param("page")
        .parse::<u32>()
        .unwrap_or(1)
        .clamp(1, pages);
    let start = (page_n as usize - 1) * PER_PAGE;
    let slice = &list[start.min(list.len())..(start + PER_PAGE).min(list.len())];

    let mut rows = rows_html(slice, &me.uuid, |r| {
        Some(format!("/users/{}", crate::web::page::encode(&r.name)))
    });
    if rows.is_empty() {
        rows = r#"<tr><td colspan="5"><div class="empty"><h3>No solves yet</h3><p>The first flag puts you on top.</p></div></td></tr>"#.into();
    }
    let mine = list.iter().find(|r| r.id == me.uuid);
    let me_box = match mine {
        Some(r) => format!(
            r#"<div class="stat"><b>#{}</b><span>your rank</span></div><div class="stat"><b>{}</b><span>your points</span></div>"#,
            r.rank, r.score
        ),
        None => r#"<div class="stat"><b>—</b><span>your rank</span></div>"#.into(),
    };
    let mut tabs = format!(
        r#"<a class="chip{}" href="/scoreboard">All</a>"#,
        if category.is_empty() {
            " is-active"
        } else {
            ""
        }
    );
    for c in chall::practice_categories(ctx.tenant_uuid()).await? {
        let _ = write!(
            tabs,
            r#"<a class="chip{}" href="/scoreboard?category={1}">{1}</a>"#,
            if c == category { " is-active" } else { "" },
            e(&c)
        );
    }
    let query = if category.is_empty() {
        String::new()
    } else {
        format!("category={}", crate::web::page::encode(category))
    };
    let mut v = Vars::new();
    v.set("rows", rows)
        .set("tabs", tabs)
        .set("me_box", me_box)
        .set("players", list.len().to_string())
        .set("pager", pager("/scoreboard", &query, page_n, pages));
    page(ctx, "score/scoreboard.html", "Scoreboard", v)
}

pub async fn profile(ctx: &Ctx, username: &str) -> PageResult {
    let me = match require_login(ctx) {
        Ok(u) => u,
        Err(r) => return Ok(r),
    };
    let Some(u) = user::get_by_username(ctx.tenant_uuid(), username).await? else {
        return Ok(RouteResult::NotFound);
    };
    if u.banned && !ctx.has_role(Role::Admin) {
        return Ok(RouteResult::NotFound);
    }
    let own = u.uuid == me.uuid;
    let hidden = scoreboard_hidden(ctx) && !own;
    let list = score::ranking(&Query {
        tenant: ctx.tenant_uuid(),
        event_key: "",
        teams: false,
        category: "",
        until: None,
    })
    .await?;
    let entry = list.iter().find(|r| r.id == u.uuid);
    let solves = score::user_solves(ctx.tenant_uuid(), &u.uuid).await?;
    let cats = score::user_categories(ctx.tenant_uuid(), &u.uuid).await?;
    let totals = score::category_totals(ctx.tenant_uuid()).await?;

    let mut bars = String::new();
    for (cat, n, pts) in &cats {
        let total = totals.get(cat).copied().unwrap_or(*n).max(1);
        let pct = (*n as f64 / total as f64 * 100.0).round() as i64;
        let _ = write!(
            bars,
            r#"<li class="{cls}"><span class="bars__label">{c}</span><span class="bars__track"><i style="width:{pct}%"></i></span><span class="bars__val">{n}/{total} · {pts} pts</span></li>"#,
            cls = cat_class(cat),
            c = e(cat),
        );
    }
    if bars.is_empty() {
        bars = r#"<li class="muted">No solves yet.</li>"#.into();
    }
    let mut table = String::new();
    for s in &solves {
        let _ = write!(
            table,
            r#"<tr><td><a href="/challenges/{slug}">{name}</a></td><td><span class="chall-grid-card__cat {cc}">{cat}</span></td><td class="num">{pts}</td><td><time datetime="{at}Z">{at}</time></td></tr>"#,
            slug = e(&s.slug),
            name = e(&s.name),
            cc = cat_class(&s.category),
            cat = e(&s.category),
            pts = s.points,
            at = s.solved_at.format("%Y-%m-%d %H:%M:%S"),
        );
    }
    if table.is_empty() {
        table = r#"<tr><td colspan="4" class="muted">Nothing solved yet.</td></tr>"#.into();
    }
    let mut v = Vars::new();
    v.set("username", u.username.as_str())
        .set(
            "initial",
            u.username
                .chars()
                .next()
                .unwrap_or('?')
                .to_ascii_uppercase()
                .to_string(),
        )
        .set("role", u.role.as_str())
        .set("member_since", u.created_at.format("%Y-%m-%d").to_string())
        .set(
            "rank",
            if hidden {
                "—".into()
            } else {
                entry
                    .map(|r| format!("#{}", r.rank))
                    .unwrap_or_else(|| "—".into())
            },
        )
        .set(
            "score",
            if hidden {
                "—".into()
            } else {
                entry.map(|r| r.score).unwrap_or(0).to_string()
            },
        )
        .set("solves", solves.len().to_string())
        .set("bars", bars)
        .set("table", table);
    if own {
        v.set(
            "edit",
            r#"<a class="btn btn--ghost btn--sm" href="/account/settings">Edit account</a>"#,
        );
    }
    page(ctx, "score/profile.html", &u.username, v)
}

/// Signed-in home: score, next challenges, activity, top 10.
pub async fn dashboard(ctx: &Ctx) -> PageResult {
    let me = ctx.user.as_ref().expect("dashboard needs a user");
    let t = ctx.tenant_uuid();
    let list = score::ranking(&Query {
        tenant: t,
        event_key: "",
        teams: false,
        category: "",
        until: None,
    })
    .await?;
    let mine = list.iter().find(|r| r.id == me.uuid);
    let hidden = scoreboard_hidden(ctx);
    let (next, _) = chall::list_practice(
        t,
        &me.uuid,
        &Filter {
            state: "unsolved".into(),
            sort: "points".into(),
            page: 1,
            per_page: 3,
            ..Default::default()
        },
    )
    .await?;
    let (_, total) = chall::list_practice(
        t,
        &me.uuid,
        &Filter {
            page: 1,
            per_page: 1,
            ..Default::default()
        },
    )
    .await?;
    let solved = score::user_solves(t, &me.uuid).await?.len();

    let mut next_html = String::new();
    for c in &next {
        next_html.push_str(&card_html(c, &format!("/challenges/{}", c.slug)));
    }
    if next_html.is_empty() {
        next_html = r#"<div class="empty"><h3>All caught up</h3><p>You solved everything published. New challenges will show up here.</p></div>"#.into();
    }
    let mut feed = String::new();
    for f in score::feed(t, 10).await? {
        let _ = write!(
            feed,
            r#"<li><a href="/users/{u}">{u}</a> solved <a href="/challenges/{slug}">{name}</a>{fb}<span class="feed__pts">+{pts}</span><time datetime="{at}Z">{at}</time></li>"#,
            u = e(&f.username),
            slug = e(&f.slug),
            name = e(&f.name),
            fb = if f.first_blood {
                r#" <span class="badge badge--danger">first blood</span>"#
            } else {
                ""
            },
            pts = f.points,
            at = f.solved_at.format("%Y-%m-%d %H:%M:%S"),
        );
    }
    if feed.is_empty() {
        feed = r#"<li class="muted">No activity yet.</li>"#.into();
    }
    let top = if hidden {
        r#"<tr><td colspan="5" class="muted">The scoreboard is hidden.</td></tr>"#.to_string()
    } else {
        let t10 = &list[..list.len().min(10)];
        let r = rows_html(t10, &me.uuid, |r| {
            Some(format!("/users/{}", crate::web::page::encode(&r.name)))
        });
        if r.is_empty() {
            r#"<tr><td colspan="5" class="muted">Nobody on the board yet.</td></tr>"#.into()
        } else {
            r
        }
    };
    let mut v = Vars::new();
    v.set("username", me.username.as_str())
        .set("score", mine.map(|r| r.score).unwrap_or(0).to_string())
        .set(
            "rank",
            if hidden {
                "—".into()
            } else {
                mine.map(|r| format!("#{}", r.rank))
                    .unwrap_or_else(|| "—".into())
            },
        )
        .set("progress", format!("{solved}/{total}"))
        .set("players", list.len().to_string())
        .set("next", next_html)
        .set("feed", feed)
        .set("top", top);
    page(ctx, "score/dashboard.html", "", v)
}
