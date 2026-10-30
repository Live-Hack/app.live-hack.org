use crate::api::v1::core::{ApiResult, bad, err, ok, too_many, with_cookie};
use crate::helper::crypto::{hash_password, verify_password};
use crate::helper::ratelimit;
use crate::helper::request::{Ctx, field, field_raw};
use crate::models::token::{self, Kind};
use crate::models::user::{self, MIN_PASSWORD, Role, valid_email, valid_username};
use crate::models::{audit, is_duplicate, session};
use actix_web::http::StatusCode;
use serde_json::{Value, json};

pub fn me(ctx: &Ctx) -> ApiResult {
    match &ctx.user {
        Some(u) => ok(json!({ "user": u })),
        None => ok(json!({ "user": null })),
    }
}

fn check_password(pw: &str) -> Result<(), crate::api::v1::core::ApiError> {
    if pw.chars().count() < MIN_PASSWORD {
        return Err(bad(format!(
            "Password must be at least {MIN_PASSWORD} characters."
        )));
    }
    if pw.len() > 256 {
        return Err(bad("Password is too long (256 characters max)."));
    }
    Ok(())
}

async fn open_session(ctx: &Ctx, user: &user::User) -> ApiResult {
    let token = session::create(user, &ctx.user_agent, &ctx.ip).await?;
    with_cookie(
        json!({ "user": user }),
        session::cookie_header(&token, session::TTL_DAYS * 86400),
    )
}

/// First admin of an empty tenant.
pub async fn setup(ctx: &Ctx, body: &Value) -> ApiResult {
    let username = field(body, "username");
    let password = field_raw(body, "password");
    if !valid_username(username) {
        return Err(bad(
            "Username: 3 to 32 characters, letters, digits, _ - . only.",
        ));
    }
    check_password(password)?;
    let hash = hash_password(password.to_string()).await;
    let Some(uuid) = user::create_first_admin(ctx.tenant_uuid(), username, &hash).await? else {
        return Err(err(
            StatusCode::CONFLICT,
            "This platform is already set up.",
        ));
    };
    let site_name = field(body, "site_name");
    if !site_name.is_empty() {
        crate::helper::tenant::set_setting(
            ctx.tenant_uuid(),
            "site_name",
            &site_name.chars().take(64).collect::<String>(),
        )
        .await?;
    }
    audit::log(
        ctx.tenant_uuid(),
        Some(&uuid),
        "setup",
        "user",
        Some(&uuid),
        Value::Null,
    )
    .await?;
    let user = user::get(ctx.tenant_uuid(), &uuid)
        .await?
        .expect("just created");
    open_session(ctx, &user).await
}

pub async fn login(ctx: &Ctx, body: &Value) -> ApiResult {
    let login = field(body, "login");
    let password = field_raw(body, "password");
    if login.is_empty() || password.is_empty() {
        return Err(bad("Enter your username and password."));
    }
    let t = ctx.tenant_uuid();
    // Per-IP limit stays high: a whole classroom often shares one NAT address at the start of an event.
    if !ratelimit::allow(&format!("login:ip:{t}:{}", ctx.ip), 300, 60).await?
        || !ratelimit::allow(&format!("login:user:{t}:{}", login.to_lowercase()), 10, 60).await?
    {
        return Err(too_many());
    }
    let found = user::find_for_login(t, login).await?;
    let (user, hash) = match found {
        Some((u, h)) => (Some(u), Some(h)),
        None => (None, None),
    };
    if !verify_password(password.to_string(), hash).await {
        return Err(err(StatusCode::UNAUTHORIZED, "Wrong username or password."));
    }
    let user = user.expect("verified");
    if user.banned {
        return Err(err(StatusCode::FORBIDDEN, "This account is suspended."));
    }
    open_session(ctx, &user).await
}

pub async fn register(ctx: &Ctx, body: &Value) -> ApiResult {
    let t = ctx.tenant_uuid();
    let invite = field(body, "invite");
    match ctx.tenant.setting("registration") {
        "closed" => {
            return Err(err(
                StatusCode::FORBIDDEN,
                "Registration is closed. Ask an organizer for an account.",
            ));
        }
        "invite" if invite.is_empty() => {
            return Err(err(
                StatusCode::FORBIDDEN,
                "Registration needs an invitation link.",
            ));
        }
        _ => {}
    }
    let username = field(body, "username");
    let email = field(body, "email");
    let password = field_raw(body, "password");
    if !valid_username(username) {
        return Err(bad(
            "Username: 3 to 32 characters, letters, digits, _ - . only.",
        ));
    }
    if !email.is_empty() && !valid_email(email) {
        return Err(bad("This email address is not valid."));
    }
    check_password(password)?;
    if !ratelimit::allow(&format!("register:{t}:{}", ctx.ip), 100, 600).await? {
        return Err(too_many());
    }

    let needs_invite = !invite.is_empty() || ctx.tenant.setting("registration") == "invite";
    if needs_invite && token::consume(t, Kind::Invite, invite).await?.is_none() {
        return Err(err(
            StatusCode::FORBIDDEN,
            "This invitation link is invalid or expired.",
        ));
    }

    let hash = hash_password(password.to_string()).await;
    let email = (!email.is_empty()).then_some(email);
    let uuid = match user::create(t, username, email, &hash, Role::Player).await {
        Ok(uuid) => uuid,
        Err(e) if is_duplicate(&e) => {
            if needs_invite {
                token::refund(invite).await?;
            }
            return Err(err(
                StatusCode::CONFLICT,
                "This username or email is already used.",
            ));
        }
        Err(e) => return Err(e.into()),
    };
    audit::log(
        t,
        Some(&uuid),
        "register",
        "user",
        Some(&uuid),
        json!({ "invite": needs_invite }),
    )
    .await?;
    let user = user::get(t, &uuid).await?.expect("just created");
    open_session(ctx, &user).await
}

pub async fn logout(ctx: &Ctx) -> ApiResult {
    if let Some(t) = &ctx.token {
        session::delete(t).await?;
    }
    with_cookie(json!({ "ok": true }), session::cookie_header("", 0))
}

/// Sets a new password from an admin-generated reset link and signs out every session.
pub async fn reset(ctx: &Ctx, body: &Value) -> ApiResult {
    let t = ctx.tenant_uuid();
    let tok = field(body, "token");
    let password = field_raw(body, "password");
    check_password(password)?;
    if !ratelimit::allow(&format!("reset:{t}:{}", ctx.ip), 20, 60).await? {
        return Err(too_many());
    }
    let Some(Some(uuid)) = token::consume(t, Kind::Reset, tok).await? else {
        return Err(err(
            StatusCode::FORBIDDEN,
            "This reset link is invalid or expired.",
        ));
    };
    let hash = hash_password(password.to_string()).await;
    user::set_password(t, &uuid, &hash).await?;
    session::delete_all(&uuid, None).await?;
    audit::log(
        t,
        Some(&uuid),
        "password.reset",
        "user",
        Some(&uuid),
        Value::Null,
    )
    .await?;
    ok(json!({ "ok": true }))
}
