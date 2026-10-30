use crate::api::v1::core::{ApiResult, bad, created, not_found, ok, require_role};
use crate::helper::request::{Ctx, field_u64};
use crate::helper::tenant::set_setting;
use crate::models::audit;
use crate::models::token::{self, Kind};
use crate::models::user::{self, Role};
use serde_json::{Value, json};

/// Settings an admin may change, with their allowed values (None = free text).
const SETTINGS: &[(&str, Option<&[&str]>)] = &[
    ("site_name", None),
    ("flag_format", None),
    ("registration", Some(&["open", "invite", "closed"])),
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
