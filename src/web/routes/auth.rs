use crate::helper::request::Ctx;
use crate::helper::template::Vars;
use crate::models::token::{self, Kind};
use crate::web::page::{PageResult, RouteResult, page, safe_next};

pub fn setup_page(ctx: &Ctx) -> PageResult {
    page(ctx, "auth/setup.html", "Set up", Vars::new())
}

pub fn login_page(ctx: &Ctx) -> PageResult {
    let next = safe_next(ctx.param("next"));
    if ctx.user.is_some() {
        return Ok(RouteResult::Redirect(next.to_string()));
    }
    let mut v = Vars::new();
    v.set("next", next);
    if ctx.tenant.setting("registration") == "open" {
        v.set("register_link", r#"<span class="sign__text">No account yet? <a href="/auth/register">Sign up</a></span>"#);
    }
    page(ctx, "auth/login.html", "Sign in", v)
}

pub async fn register_page(ctx: &Ctx) -> PageResult {
    if ctx.user.is_some() {
        return Ok(RouteResult::Redirect("/".into()));
    }
    let invite = ctx.param("invite");
    let mode = ctx.tenant.setting("registration");
    let mut v = Vars::new();
    v.set("invite", invite);
    let blocked = if !invite.is_empty() {
        (!token::peek(ctx.tenant_uuid(), Kind::Invite, invite).await?).then_some(
            "This invitation link is invalid or expired. Ask an organizer for a new one.",
        )
    } else {
        match mode {
            "closed" => Some(
                "Registration is closed on this platform. Ask an organizer to create your account.",
            ),
            "invite" => Some("Registration needs an invitation link. Ask an organizer for one."),
            _ => None,
        }
    };
    match blocked {
        Some(msg) => {
            v.set("message", msg);
            page(ctx, "auth/register_closed.html", "Sign up", v)
        }
        None => page(ctx, "auth/register.html", "Sign up", v),
    }
}

pub async fn reset_page(ctx: &Ctx) -> PageResult {
    let tok = ctx.param("token");
    let mut v = Vars::new();
    v.set("token", tok);
    if tok.is_empty() || !token::peek(ctx.tenant_uuid(), Kind::Reset, tok).await? {
        v.set(
            "message",
            "This reset link is invalid or expired. Ask an organizer for a new one.",
        );
        return page(ctx, "auth/register_closed.html", "Reset password", v);
    }
    page(ctx, "auth/reset.html", "Reset password", v)
}
