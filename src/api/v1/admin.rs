use crate::api::v1::core::{ApiResult, bad, created, not_found, ok, require_role};
use crate::helper::request::{Ctx, field_u64};
use crate::helper::tenant::set_setting;
use crate::models::token::{self, Kind};
use crate::models::user::{self, Role};
use crate::models::{audit, session};
use serde_json::{Value, json};

/// Settings an admin may change, with their allowed values (None = free text).
const SETTINGS: &[(&str, Option<&[&str]>)] = &[
    ("site_name", None),
    ("flag_format", None),
    ("registration", Some(&["open", "invite", "closed"])),
    ("scoreboard", Some(&["visible", "hidden"])),
    ("labs", Some(&["enabled", "disabled"])),
];

pub fn get_settings(ctx: &Ctx) -> ApiResult {
    require_role(ctx, Role::Admin)?;
    let map: serde_json::Map<String, Value> = SETTINGS
        .iter()
        .map(|(k, _)| (k.to_string(), json!(ctx.tenant.setting(k))))
        .collect();
    ok(json!({ "settings": map }))
}

pub async fn set_settings(ctx: &Ctx, body: &Value) -> ApiResult {
    let admin = require_role(ctx, Role::Admin)?;
    let mut changed = Vec::new();
    for (key, allowed) in SETTINGS {
        let Some(v) = body.get(*key).and_then(Value::as_str) else {
            continue;
        };
        let v = v.trim();
        if v.is_empty() || v.len() > 128 || allowed.is_some_and(|a| !a.contains(&v)) {
            return Err(bad(format!("Invalid value for {key}.")));
        }
        set_setting(ctx.tenant_uuid(), key, v).await?;
        changed.push(*key);
    }
    audit::log(
        ctx.tenant_uuid(),
        Some(&admin.uuid),
        "settings.update",
        "tenant",
        Some(ctx.tenant_uuid()),
        json!({ "keys": changed }),
    )
    .await?;
    ok(json!({ "ok": true, "message": "Settings saved." }))
}

pub async fn create_invite(ctx: &Ctx, body: &Value) -> ApiResult {
    let admin = require_role(ctx, Role::Admin)?;
    let uses = field_u64(body, "uses").unwrap_or(1).clamp(1, 10_000) as u32;
    let hours = field_u64(body, "hours").unwrap_or(72).clamp(1, 24 * 90) as u32;
    let tok = token::create(
        ctx.tenant_uuid(),
        Kind::Invite,
        None,
        uses,
        hours * 60,
        &admin.uuid,
    )
    .await?;
    audit::log(
        ctx.tenant_uuid(),
        Some(&admin.uuid),
        "invite.create",
        "tenant",
        None,
        json!({ "uses": uses, "hours": hours }),
    )
    .await?;
    let path = format!("/auth/register?invite={tok}");
    created(json!({ "url": ctx.url(&path), "path": path, "uses": uses, "hours": hours }))
}

/// One-time link valid 30 minutes, handed to the user by the admin.
pub async fn reset_link(ctx: &Ctx, uuid: &str) -> ApiResult {
    let admin = require_role(ctx, Role::Admin)?;
    let Some(target) = user::get(ctx.tenant_uuid(), uuid).await? else {
        return Err(not_found());
    };
    let tok = token::create(
        ctx.tenant_uuid(),
        Kind::Reset,
        Some(&target.uuid),
        1,
        30,
        &admin.uuid,
    )
    .await?;
    audit::log(
        ctx.tenant_uuid(),
        Some(&admin.uuid),
        "reset.create",
        "user",
        Some(&target.uuid),
        Value::Null,
    )
    .await?;
    let path = format!("/auth/reset?token={tok}");
    created(json!({ "url": ctx.url(&path), "path": path, "username": target.username }))
}

fn target_guard(
    admin: &crate::models::user::User,
    target: &str,
) -> Result<(), crate::api::v1::core::ApiError> {
    if admin.uuid == target {
        return Err(bad("You cannot change your own account from here."));
    }
    Ok(())
}

pub async fn stats(ctx: &Ctx) -> ApiResult {
    require_role(ctx, Role::Admin)?;
    let s = crate::models::admin::stats(ctx.tenant_uuid()).await?;
    let hourly = crate::models::admin::solves_per_hour(ctx.tenant_uuid()).await?;
    ok(json!({ "stats": s, "solves_per_hour": hourly }))
}

/// Admin-created account: random password, a 72 h link lets the person choose theirs.
pub async fn create_user(ctx: &Ctx, body: &Value) -> ApiResult {
    let admin = require_role(ctx, Role::Admin)?;
    let username = crate::helper::request::field(body, "username");
    let email = crate::helper::request::field(body, "email");
    let role = Role::parse(crate::helper::request::field(body, "role")).unwrap_or(Role::Player);
    if !user::valid_username(username) {
        return Err(bad(
            "Username: 3 to 32 characters, letters, digits, _ - . only.",
        ));
    }
    if !email.is_empty() && !user::valid_email(email) {
        return Err(bad("This email address is not valid."));
    }
    let hash = crate::helper::crypto::hash_password(crate::helper::crypto::random_token()).await;
    let uuid = match user::create(
        ctx.tenant_uuid(),
        username,
        (!email.is_empty()).then_some(email),
        &hash,
        role,
    )
    .await
    {
        Ok(u) => u,
        Err(e) if crate::models::is_duplicate(&e) => {
            return Err(crate::api::v1::core::err(
                actix_web::http::StatusCode::CONFLICT,
                "This username or email is already used.",
            ));
        }
        Err(e) => return Err(e.into()),
    };
    let tok = token::create(
        ctx.tenant_uuid(),
        Kind::Reset,
        Some(&uuid),
        1,
        72 * 60,
        &admin.uuid,
    )
    .await?;
    audit::log(
        ctx.tenant_uuid(),
        Some(&admin.uuid),
        "user.create",
        "user",
        Some(&uuid),
        json!({ "username": username, "role": role.as_str() }),
    )
    .await?;
    let path = format!("/auth/reset?token={tok}");
    created(
        json!({ "uuid": uuid, "url": ctx.url(&path), "path": path, "message": format!("Account created. Send {username} the link to choose a password (valid 72 h).") }),
    )
}

pub async fn set_role(ctx: &Ctx, uuid: &str, body: &Value) -> ApiResult {
    let admin = require_role(ctx, Role::Admin)?;
    target_guard(admin, uuid)?;
    let Some(role) = Role::parse(crate::helper::request::field(body, "role")) else {
        return Err(bad("Unknown role."));
    };
    let Some(target) = user::get(ctx.tenant_uuid(), uuid).await? else {
        return Err(not_found());
    };
    user::set_role(ctx.tenant_uuid(), &target.uuid, role).await?;
    audit::log(
        ctx.tenant_uuid(),
        Some(&admin.uuid),
        "user.role",
        "user",
        Some(&target.uuid),
        json!({ "from": target.role.as_str(), "to": role.as_str() }),
    )
    .await?;
    ok(json!({ "ok": true, "message": format!("{} is now {}.", target.username, role.as_str()) }))
}

/// Ban or unban. A ban also signs the user out and stops their labs.
pub async fn ban(ctx: &Ctx, uuid: &str, body: &Value) -> ApiResult {
    let admin = require_role(ctx, Role::Admin)?;
    target_guard(admin, uuid)?;
    let on = body.get("banned").and_then(Value::as_bool).unwrap_or(true);
    let Some(target) = user::get(ctx.tenant_uuid(), uuid).await? else {
        return Err(not_found());
    };
    user::set_banned(ctx.tenant_uuid(), &target.uuid, on).await?;
    if on {
        session::delete_all(&target.uuid, None).await?;
        for (inst, container, network) in crate::models::admin::user_labs(&target.uuid).await? {
            crate::helper::labs::destroy(&container, &network).await;
            crate::models::instance::mark_stopped(&inst, None).await?;
        }
    }
    audit::log(
        ctx.tenant_uuid(),
        Some(&admin.uuid),
        if on { "user.ban" } else { "user.unban" },
        "user",
        Some(&target.uuid),
        Value::Null,
    )
    .await?;
    ok(
        json!({ "ok": true, "message": if on { format!("{} is banned.", target.username) } else { format!("{} can play again.", target.username) } }),
    )
}

pub async fn revoke_sessions(ctx: &Ctx, uuid: &str) -> ApiResult {
    let admin = require_role(ctx, Role::Admin)?;
    let Some(target) = user::get(ctx.tenant_uuid(), uuid).await? else {
        return Err(not_found());
    };
    let n = session::delete_all(
        &target.uuid,
        if target.uuid == admin.uuid {
            ctx.token.as_deref()
        } else {
            None
        },
    )
    .await?;
    audit::log(
        ctx.tenant_uuid(),
        Some(&admin.uuid),
        "user.sessions.revoke",
        "user",
        Some(&target.uuid),
        json!({ "count": n }),
    )
    .await?;
    ok(json!({ "ok": true, "message": format!("{n} session(s) signed out.") }))
}

pub async fn stop_lab(ctx: &Ctx, uuid: &str) -> ApiResult {
    let admin = require_role(ctx, Role::Admin)?;
    let Some(i) = crate::models::instance::get(ctx.tenant_uuid(), uuid).await? else {
        return Err(not_found());
    };
    crate::helper::labs::destroy(&i.container_id, &i.network_id).await;
    crate::models::instance::mark_stopped(&i.uuid, None).await?;
    audit::log(
        ctx.tenant_uuid(),
        Some(&admin.uuid),
        "lab.kill",
        "instance",
        Some(&i.uuid),
        Value::Null,
    )
    .await?;
    ok(json!({ "ok": true, "message": "Lab stopped." }))
}

pub async fn lab_logs(ctx: &Ctx, uuid: &str) -> ApiResult {
    require_role(ctx, Role::Admin)?;
    let Some(i) = crate::models::instance::get(ctx.tenant_uuid(), uuid).await? else {
        return Err(not_found());
    };
    let logs = crate::helper::labs::logs(&i.container_id, 200)
        .await
        .unwrap_or_else(|e| format!("(logs unavailable: {e})"));
    ok(json!({ "logs": logs }))
}
