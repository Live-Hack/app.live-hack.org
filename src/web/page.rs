use crate::assets::init::ASSET_VERSION;
use crate::helper::request::Ctx;
use crate::helper::template::{Vars, render};
use crate::models::user::{Role, User};

pub enum RouteResult {
    Html(String),
    Text(String),
    Redirect(String),
    /// File download: path on disk, filename shown to the user.
    File(std::path::PathBuf, String),
    NotFound,
}

pub type PageResult = Result<RouteResult, mysql_async::Error>;

/// Renders a full page: layout variables first, then the page's own (which win on conflict).
pub fn page(ctx: &Ctx, template: &str, title: &str, vars: Vars) -> PageResult {
    Ok(RouteResult::Html(page_html(ctx, template, title, vars)))
}

pub fn page_html(ctx: &Ctx, template: &str, title: &str, vars: Vars) -> String {
    let mut all = base_vars(ctx, title);
    all.extend(vars);
    render(template, &all)
}

fn base_vars(ctx: &Ctx, title: &str) -> Vars {
    let mut v = Vars::new();
    let site = ctx.tenant.site_name();
    v.set("v", ASSET_VERSION.as_str())
        .set("site_name", site)
        .set(
            "title",
            if title.is_empty() {
                site.to_string()
            } else {
                format!("{title} · {site}")
            },
        )
        .set("path", ctx.path.as_str());
    let menu = match &ctx.user {
        Some(u) => {
            let mut m = Vars::new();
            m.set("username", u.username.as_str())
                .set(
                    "initial",
                    u.username
                        .chars()
                        .next()
                        .unwrap_or('?')
                        .to_ascii_uppercase()
                        .to_string(),
                )
                .set("role", u.role.as_str());
            let mut links = String::new();
            if u.role >= Role::Author {
                links.push_str(
                    r#"<li><a href="/manage/challenges"><span>Manage challenges</span></a></li>"#,
                );
                links.push_str(
                    r#"<li><a href="/manage/events"><span>Manage events</span></a></li>"#,
                );
            }
            if u.role >= Role::Admin {
                links.push_str(r#"<li><a href="/admin"><span>Admin</span></a></li>"#);
            }
            m.set("admin_link", links);
            render("partials/user_menu.html", &m)
        }
        None => render("partials/signin.html", &Vars::new()),
    };
    v.set("user_menu", menu);
    v
}

/// Logged-in user, or a redirect to the login page that comes back here.
pub fn require_login(ctx: &Ctx) -> Result<&User, RouteResult> {
    ctx.user
        .as_ref()
        .ok_or_else(|| RouteResult::Redirect(format!("/auth/login?next={}", encode(&ctx.path))))
}

pub fn require_role(ctx: &Ctx, role: Role) -> Result<&User, RouteResult> {
    let u = require_login(ctx)?;
    if u.role >= role {
        Ok(u)
    } else {
        Err(RouteResult::NotFound)
    }
}

/// Minimal percent-encoding for a path placed in a query string.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Only same-site relative paths are accepted as redirect targets.
pub fn safe_next(next: &str) -> &str {
    if next.starts_with('/') && !next.starts_with("//") && !next.contains('\\') {
        next
    } else {
        "/"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_and_encode() {
        assert_eq!(safe_next("/challenges"), "/challenges");
        assert_eq!(safe_next("//evil.com"), "/");
        assert_eq!(safe_next("https://evil.com"), "/");
        assert_eq!(safe_next("/\\evil.com"), "/");
        assert_eq!(encode("/a b?c"), "/a%20b%3Fc");
    }
}
