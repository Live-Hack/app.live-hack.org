use crate::api::v1::core::{ApiResult, bad, err, ok, require_user};
use crate::helper::crypto::{hash_password, verify_password};
use crate::helper::request::{Ctx, field, field_raw};
use crate::models::user::{self, MIN_PASSWORD, valid_username};
use crate::models::{audit, is_duplicate, session};
use actix_web::http::StatusCode;
use serde_json::{Value, json};

pub async fn sessions(ctx: &Ctx) -> ApiResult {
    let u = require_user(ctx)?;
    let list = session::list(&u.uuid, ctx.token.as_deref().unwrap_or("")).await?;
    ok(json!({ "sessions": list }))
}

pub async fn username(ctx: &Ctx, body: &Value) -> ApiResult {
    let u = require_user(ctx)?;
    let name = field(body, "username");
    if !valid_username(name) {
        return Err(bad(
            "Username: 3 to 32 characters, letters, digits, _ - . only.",
        ));
    }
    match user::set_username(ctx.tenant_uuid(), &u.uuid, name).await {
        Ok(()) => {}
        Err(e) if is_duplicate(&e) => {
            return Err(err(StatusCode::CONFLICT, "This username is already used."));
        }
        Err(e) => return Err(e.into()),
    }
    audit::log(
        ctx.tenant_uuid(),
        Some(&u.uuid),
        "user.rename",
        "user",
        Some(&u.uuid),
        json!({ "from": u.username, "to": name }),
    )
    .await?;
    ok(json!({ "ok": true, "message": "Username updated." }))
}

/// Changes the password and signs out every other session.
pub async fn password(ctx: &Ctx, body: &Value) -> ApiResult {
    let u = require_user(ctx)?;
    let current = field_raw(body, "current");
    let new = field_raw(body, "password");
    if new.chars().count() < MIN_PASSWORD || new.len() > 256 {
        return Err(bad(format!(
            "Password must be {MIN_PASSWORD} to 256 characters."
        )));
    }
    let hash = user::password_hash(&u.uuid).await?;
    if !verify_password(current.to_string(), hash).await {
        return Err(err(StatusCode::FORBIDDEN, "Current password is wrong."));
    }
    let new_hash = hash_password(new.to_string()).await;
    user::set_password(ctx.tenant_uuid(), &u.uuid, &new_hash).await?;
    let revoked = session::delete_all(&u.uuid, ctx.token.as_deref()).await?;
    audit::log(
        ctx.tenant_uuid(),
        Some(&u.uuid),
        "password.change",
        "user",
        Some(&u.uuid),
        Value::Null,
    )
    .await?;
    ok(
        json!({ "ok": true, "revoked": revoked, "message": "Password updated. Other sessions were signed out." }),
    )
}

pub async fn revoke_others(ctx: &Ctx) -> ApiResult {
    let u = require_user(ctx)?;
    let revoked = session::delete_all(&u.uuid, ctx.token.as_deref()).await?;
    ok(
        json!({ "ok": true, "revoked": revoked, "message": format!("{revoked} other session(s) signed out.") }),
    )
}
