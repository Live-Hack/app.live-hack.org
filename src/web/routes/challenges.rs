use crate::helper::markdown;
use crate::helper::request::Ctx;
use crate::helper::storage;
use crate::helper::template::{Vars, render};
use crate::models::chall::{self, Card, DIFFICULTIES, Filter};
use crate::models::user::Role;
use crate::web::page::{PageResult, RouteResult, encode, page, require_login};
use std::fmt::Write;

const PER_PAGE: u32 = 24;

pub fn card_html(c: &Card, href: &str) -> String {
    let mut v = Vars::new();
    v.set("href", href)
        .set("name", c.name.as_str())
        .set("category", c.category.as_str())
        .set("cat_class", cat_class(&c.category))
        .set("difficulty", c.difficulty.as_str())
        .set("points", c.points.to_string())
        .set("solves", c.solves.to_string())
        .set(
            "solved_class",
            if c.solved {
                " chall-grid-card--solved"
            } else {
                ""
            },
        )
        .set(
            "solved_badge",
            if c.solved {
                r#"<div class="chall-grid-card__solved-badge" title="Solved">✓</div>"#
            } else {
                ""
            },
        )
        .set(
            "kind",
            match c.ctype.as_str() {
                "instance" => "Lab",
                "static" => "Online",
                _ => "Files",
            },
        );
    render("partials/chall_card.html", &v)
}

/// CSS class for a category colour; unknown categories use the accent colour.
pub fn cat_class(cat: &str) -> String {
    let known = [
        "crypto",
        "forensics",
        "misc",
        "osint",
        "pwn",
        "reverse",
        "web",
        "hardware",
        "stego",
        "network",
    ];
    if known.contains(&cat) {
        format!("cat-{cat}")
    } else {
        "cat-other".into()
    }
}

fn option(out: &mut String, value: &str, label: &str, current: &str) {
    let sel = if value == current { " selected" } else { "" };
    let _ = write!(
        out,
        r#"<option value="{}"{sel}>{}</option>"#,
        crate::helper::template::escape_html(value),
        crate::helper::template::escape_html(label)
    );
}

/// Page links keeping the current filters.
pub fn pager(base: &str, query: &str, page: u32, pages: u32) -> String {
    if pages <= 1 {
        return String::new();
    }
    let link = |p: u32| {
        let sep = if query.is_empty() { "" } else { "&" };
        format!("{base}?{query}{sep}page={p}")
    };
    let mut out = String::from(r#"<nav class="pager" aria-label="Pages">"#);
    if page > 1 {
        let _ = write!(out, r#"<a href="{}" rel="prev">‹</a>"#, link(page - 1));
    }
    for p in 1..=pages {
        if p == 1 || p == pages || p.abs_diff(page) <= 2 {
            let cls = if p == page {
                r#" class="is-active" aria-current="page""#
            } else {
                ""
            };
            let _ = write!(out, r#"<a href="{}"{cls}>{p}</a>"#, link(p));
        } else if p.abs_diff(page) == 3 {
            out.push_str("<span>…</span>");
        }
    }
    if page < pages {
        let _ = write!(out, r#"<a href="{}" rel="next">›</a>"#, link(page + 1));
    }
    out.push_str("</nav>");
    out
}

pub async fn list(ctx: &Ctx) -> PageResult {
    let user = match require_login(ctx) {
        Ok(u) => u,
        Err(r) => return Ok(r),
    };
    let f = Filter {
        q: ctx.param("q").chars().take(64).collect(),
        category: ctx.param("category").to_string(),
        difficulty: ctx.param("difficulty").to_string(),
        state: ctx.param("state").to_string(),
        sort: ctx.param("sort").to_string(),
        page: ctx.param("page").parse().unwrap_or(1).max(1),
        per_page: PER_PAGE,
    };
    let t = ctx.tenant_uuid();
    let (cards, total) = chall::list_practice(t, &user.uuid, &f).await?;
    let cats = chall::practice_categories(t).await?;

    let mut grid = String::new();
    for c in &cards {
        grid.push_str(&card_html(c, &format!("/challenges/{}", c.slug)));
    }
    if cards.is_empty() {
        grid = r#"<div class="empty"><h3>No challenges found</h3><p>Try other filters, or come back when the organizers publish new challenges.</p></div>"#.into();
    }

    let mut cat_opts = String::new();
    option(&mut cat_opts, "", "All categories", &f.category);
    for c in &cats {
        option(&mut cat_opts, c, c, &f.category);
    }
    let mut diff_opts = String::new();
    option(&mut diff_opts, "", "Any difficulty", &f.difficulty);
    for d in DIFFICULTIES {
        option(&mut diff_opts, d, d, &f.difficulty);
    }
    let mut state_opts = String::new();
    for (v, l) in [
        ("", "All"),
        ("unsolved", "Not solved"),
        ("solved", "Solved"),
    ] {
        option(&mut state_opts, v, l, &f.state);
    }
    let mut sort_opts = String::new();
    for (v, l) in [
        ("", "Newest"),
        ("points", "Points ↑"),
        ("points_desc", "Points ↓"),
        ("solves", "Most solved"),
        ("name", "Name"),
    ] {
        option(&mut sort_opts, v, l, &f.sort);
    }

    let query: String = ["q", "category", "difficulty", "state", "sort"]
        .iter()
        .filter(|k| !ctx.param(k).is_empty())
        .map(|k| format!("{k}={}", encode(ctx.param(k))))
        .collect::<Vec<_>>()
        .join("&");
    let pages = total.div_ceil(PER_PAGE as u64) as u32;

    let mut v = Vars::new();
    v.set("q", f.q.as_str())
        .set(
            "count",
            format!("{total} challenge{}", if total == 1 { "" } else { "s" }),
        )
        .set("grid", grid)
        .set("cat_options", cat_opts)
        .set("diff_options", diff_opts)
        .set("state_options", state_opts)
        .set("sort_options", sort_opts)
        .set("pager", pager("/challenges", &query, f.page, pages));
    page(ctx, "challenges/list.html", "Challenges", v)
}

fn human_size(n: i64) -> String {
    let n = n as f64;
    match n {
        n if n >= 1_048_576.0 => format!("{:.1} MB", n / 1_048_576.0),
        n if n >= 1024.0 => format!("{:.0} KB", n / 1024.0),
        n => format!("{n} B"),
    }
}

/// Organizers see drafts; players only published challenges.
async fn visible(
    ctx: &Ctx,
    slug: &str,
) -> Result<Option<(chall::Chall, bool)>, mysql_async::Error> {
    let Some(user) = &ctx.user else {
        return Ok(None);
    };
    let Some(c) = chall::get_by_slug(ctx.tenant_uuid(), slug).await? else {
        return Ok(None);
    };
    let organizer = ctx.has_role(Role::Admin) || chall::is_author(&c.uuid, &user.uuid).await?;
    if c.status == "published" || organizer {
        Ok(Some((c, organizer)))
    } else {
        Ok(None)
    }
}

pub async fn detail(ctx: &Ctx, slug: &str) -> PageResult {
    let user = match require_login(ctx) {
        Ok(u) => u,
        Err(r) => return Ok(r),
    };
    let Some((c, _organizer)) = visible(ctx, slug).await? else {
        return Ok(RouteResult::NotFound);
    };
    let (solvers, solves) = chall::solvers(&c.uuid, "", 10).await?;
    let solved_at = chall::solved_at(&c.uuid, "", &user.uuid).await?;
    let files = chall::files(&c.uuid).await?;
    let authors = chall::author_names(&c.uuid).await?;

    let mut files_html = String::new();
    for f in &files {
        let mut v = Vars::new();
        v.set(
            "href",
            format!(
                "/challenges/{}/files/{}/{}",
                c.slug,
                f.uuid,
                encode(&f.filename)
            ),
        )
        .set("name", f.filename.as_str())
        .set("size", human_size(f.size))
        .set("sha", f.sha256.as_str());
        files_html.push_str(&render("partials/chall_file.html", &v));
    }
    if c.ctype == "static" && !c.static_url.is_empty() {
        let mut v = Vars::new();
        v.set("url", c.static_url.as_str());
        files_html.push_str(&render("partials/chall_link.html", &v));
    }
    if files_html.is_empty() {
        files_html = r#"<p class="muted">No files for this challenge.</p>"#.into();
    }

    let mut solvers_html = String::new();
    for (i, s) in solvers.iter().enumerate() {
        let _ = write!(
            solvers_html,
            r#"<li><a href="/users/{0}">{0}</a>{1}<time datetime="{2}Z">{2}</time></li>"#,
            crate::helper::template::escape_html(&s.username),
            if i == 0 {
                r#" <span class="badge badge--danger">first blood</span>"#
            } else {
                ""
            },
            s.solved_at.format("%Y-%m-%d %H:%M:%S"),
        );
    }
    if solvers_html.is_empty() {
        solvers_html = r#"<li class="muted">Nobody yet. Be the first.</li>"#.into();
    }

    let mut v = Vars::new();
    v.set("uuid", c.uuid.as_str())
        .set("name", c.name.as_str())
        .set("category", c.category.as_str())
        .set("cat_class", cat_class(&c.category))
        .set("difficulty", c.difficulty.as_str())
        .set("points", c.points.to_string())
        .set("solves", solves.to_string())
        .set("description", markdown::render(&c.description))
        .set("files", files_html)
        .set(
            "authors",
            if authors.is_empty() {
                "—".to_string()
            } else {
                authors.join(", ")
            },
        )
        .set("solvers", solvers_html)
        .set("flag_format", ctx.tenant.setting("flag_format"));
    match solved_at {
        Some(at) => {
            v.set("solved", format!(r#"<div class="chall-status chall-status--solved"><div class="chall-status__text"><strong>Solved</strong><span>on <time datetime="{0}Z">{0}</time></span></div></div>"#, at.format("%Y-%m-%d %H:%M:%S")));
        }
        None => {
            v.set(
                "flag_form",
                render(
                    "partials/flag_form.html",
                    &v_flag(&c.uuid, ctx.tenant.setting("flag_format")),
                ),
            );
        }
    }
    if c.status != "published" {
        v.set("notice", format!(r#"<div class="notice">This challenge is a <b>{}</b>. Only organizers can see it; flags you submit here are checked but not recorded.</div>"#, c.status));
        if solved_at.is_none() {
            v.set(
                "flag_form",
                render(
                    "partials/flag_form.html",
                    &v_flag(&c.uuid, ctx.tenant.setting("flag_format")),
                ),
            );
        }
    }
    page(ctx, "challenges/detail.html", &c.name, v)
}

fn v_flag(uuid: &str, format: &str) -> Vars {
    let mut v = Vars::new();
    v.set("uuid", uuid).set("flag_format", format);
    v
}

/// Streams a challenge file to a signed-in player.
pub async fn file(ctx: &Ctx, slug: &str, file_uuid: &str) -> PageResult {
    if let Err(r) = require_login(ctx) {
        return Ok(r);
    }
    let Some((c, _)) = visible(ctx, slug).await? else {
        return Ok(RouteResult::NotFound);
    };
    let Some(f) = chall::file(&c.uuid, file_uuid).await? else {
        return Ok(RouteResult::NotFound);
    };
    match storage::path(ctx.tenant_uuid(), &f.storage_key) {
        Some(path) if path.is_file() => Ok(RouteResult::File(path, f.filename)),
        _ => Ok(RouteResult::NotFound),
    }
}
