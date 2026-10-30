use crate::helper::request::Ctx;
use crate::helper::template::{Vars, escape_html as e};
use crate::models::chall::{self, DEFAULT_CATEGORIES, DIFFICULTIES};
use crate::models::manage::{self, CTYPES, FLAG_TYPES};
use crate::models::user::Role;
use crate::web::page::{PageResult, RouteResult, page, require_role};
use std::fmt::Write;

fn status_badge(status: &str) -> &'static str {
    match status {
        "published" => r#"<span class="badge badge--ok">published</span>"#,
        "archived" => r#"<span class="badge badge--muted">archived</span>"#,
        _ => r#"<span class="badge badge--warn">draft</span>"#,
    }
}

pub async fn list(ctx: &Ctx) -> PageResult {
    let user = match require_role(ctx, Role::Author) {
        Ok(u) => u,
        Err(r) => return Ok(r),
    };
    let q = ctx.param("q");
    let status = ctx.param("status");
    let rows = manage::list(
        ctx.tenant_uuid(),
        &user.uuid,
        user.role == Role::Admin,
        q,
        status,
    )
    .await?;
    let mut body = String::new();
    for r in &rows {
        let _ = write!(
            body,
            r#"<tr><td><a href="/manage/challenges/{uuid}">{name}</a><div class="muted mono small">{slug}</div></td>
<td><span class="chall-grid-card__cat {cat_class}">{cat}</span></td><td class="num">{pts}</td><td>{ctype}</td><td>{badge}</td>
<td class="num">{solves}</td><td><time datetime="{upd}Z">{upd}</time></td>
<td class="actions"><a class="btn btn--sm btn--ghost" href="/challenges/{slug}">View</a> <a class="btn btn--sm" href="/manage/challenges/{uuid}">Edit</a></td></tr>"#,
            uuid = r.uuid,
            slug = e(&r.slug),
            name = e(&r.name),
            cat = e(&r.category),
            cat_class = crate::web::routes::challenges::cat_class(&r.category),
            pts = r.points,
            ctype = e(&r.ctype),
            badge = status_badge(&r.status),
            solves = r.solves,
            upd = r.updated_at.format("%Y-%m-%d %H:%M:%S"),
        );
    }
    if rows.is_empty() {
        body = r#"<tr><td colspan="8"><div class="empty"><h3>No challenges yet</h3><p>Create one, or import a zip of ctfcli <code>challenge.yml</code> folders.</p></div></td></tr>"#.into();
    }
    let tabs: String = [
        ("", "All"),
        ("draft", "Drafts"),
        ("published", "Published"),
        ("archived", "Archived"),
    ]
    .iter()
    .map(|(v, l)| {
        let active = if *v == status { " is-active" } else { "" };
        let href = if v.is_empty() {
            "/manage/challenges".to_string()
        } else {
            format!("/manage/challenges?status={v}")
        };
        format!(r#"<a class="chip{active}" href="{href}">{l}</a>"#)
    })
    .collect();
    let mut v = Vars::new();
    v.set("rows", body)
        .set("tabs", tabs)
        .set("q", q)
        .set("status", status)
        .set("count", rows.len().to_string());
    page(ctx, "manage/list.html", "Manage challenges", v)
}

fn options(list: &[&str], current: &str) -> String {
    list.iter()
        .map(|o| {
            format!(
                r#"<option value="{o}"{}>{}</option>"#,
                if *o == current { " selected" } else { "" },
                o.replace('_', " ")
            )
        })
        .collect()
}

pub async fn edit(ctx: &Ctx, uuid: &str) -> PageResult {
    let user = match require_role(ctx, Role::Author) {
        Ok(u) => u,
        Err(r) => return Ok(r),
    };
    let Some(c) = chall::get(ctx.tenant_uuid(), uuid).await? else {
        return Ok(RouteResult::NotFound);
    };
    if user.role != Role::Admin && !chall::is_author(&c.uuid, &user.uuid).await? {
        return Ok(RouteResult::NotFound);
    }
    let files = chall::files(&c.uuid).await?;
    let authors = chall::author_names(&c.uuid).await?;

    let mut files_html = String::new();
    for f in &files {
        let _ = write!(
            files_html,
            r#"<li><span class="mono">{name}</span><span class="muted small">{size} B · {sha}</span><button class="btn btn--sm btn--ghost" type="button" data-post="/api/v1/manage/challenges/{uuid}/files/{fid}/delete" data-confirm="Remove?" data-then="reload">Remove</button></li>"#,
            name = e(&f.filename),
            size = f.size,
            sha = &f.sha256[..f.sha256.len().min(12)],
            uuid = c.uuid,
            fid = f.uuid,
        );
    }
    let mut authors_html = String::new();
    for (i, a) in authors.iter().enumerate() {
        let remove = if i == 0 {
            r#"<span class="badge badge--muted">owner</span>"#.to_string()
        } else {
            format!(
                r#"<button class="btn btn--sm btn--ghost" type="button" data-post="/api/v1/manage/challenges/{}/authors/{}/remove" data-then="reload">Remove</button>"#,
                c.uuid,
                e(a)
            )
        };
        let _ = write!(authors_html, r#"<li><span>{}</span>{remove}</li>"#, e(a));
    }
    let datalist: String = DEFAULT_CATEGORIES
        .iter()
        .map(|c| format!(r#"<option value="{c}">"#))
        .collect();
    let status_actions = match c.status.as_str() {
        "published" => format!(
            r#"<button class="btn btn--ghost" type="button" data-post-json='{{"status":"draft"}}' data-post="/api/v1/manage/challenges/{}/status" data-then="reload">Unpublish</button>"#,
            c.uuid
        ),
        "archived" => format!(
            r#"<button class="btn btn--ghost" type="button" data-post-json='{{"status":"draft"}}' data-post="/api/v1/manage/challenges/{}/status" data-then="reload">Restore as draft</button>"#,
            c.uuid
        ),
        _ => format!(
            r#"<button class="btn btn--ok" type="button" data-post-json='{{"status":"published"}}' data-post="/api/v1/manage/challenges/{}/status" data-then="reload">Publish</button>"#,
            c.uuid
        ),
    };
    let archive = if c.status != "archived" {
        format!(
            r#"<button class="btn btn--ghost" type="button" data-post-json='{{"status":"archived"}}' data-post="/api/v1/manage/challenges/{}/status" data-then="reload">Archive</button>"#,
            c.uuid
        )
    } else {
        String::new()
    };

    let rt = crate::models::instance::runtime(&c.uuid).await?.unwrap_or(
        crate::models::instance::Runtime {
            protocol: "http".into(),
            cpu_millis: 500,
            memory_mb: 256,
            ttl_minutes: 60,
            ..Default::default()
        },
    );
    let mut v = Vars::new();
    v.set("rt_image", rt.image.as_str())
        .set(
            "rt_port",
            if rt.port > 0 {
                rt.port.to_string()
            } else {
                String::new()
            },
        )
        .set("rt_proto_options", options(&["http", "tcp"], &rt.protocol))
        .set("rt_cpu", rt.cpu_millis.to_string())
        .set("rt_mem", rt.memory_mb.to_string())
        .set("rt_ttl", rt.ttl_minutes.to_string())
        .set("rt_env", rt.env.as_str())
        .set("rt_ro", if rt.readonly_rootfs { "checked" } else { "" });
    v.set("uuid", c.uuid.as_str())
        .set("slug", c.slug.as_str())
        .set("name", c.name.as_str())
        .set("description", c.description.as_str())
        .set("category", c.category.as_str())
        .set("cat_datalist", datalist)
        .set("diff_options", options(DIFFICULTIES, &c.difficulty))
        .set("ctype_options", options(CTYPES, &c.ctype))
        .set("flag_type_options", options(FLAG_TYPES, &c.flag_type))
        .set("points", c.points.to_string())
        .set("static_url", c.static_url.as_str())
        .set("flag", c.flag.as_str())
        .set("status_badge", status_badge(&c.status))
        .set("status_actions", status_actions)
        .set("archive_action", archive)
        .set("files", files_html)
        .set("authors", authors_html)
        .set("flag_format", ctx.tenant.setting("flag_format"));
    page(ctx, "manage/edit.html", &format!("Edit {}", c.name), v)
}
