//! Player API for lab instances.
use crate::api::v1::challenges::resolve_scope;
use crate::api::v1::core::{
    ApiError, ApiResult, bad, created, err, not_found, ok, require_user, too_many,
};
use crate::helper::labs;
use crate::helper::ratelimit;
use crate::helper::request::Ctx;
use crate::models::audit;
use crate::models::chall::{self, Chall};
use crate::models::instance::{self, Instance, Runtime};
use crate::models::solve::Scope;
use crate::models::user::Role;
use actix_web::http::StatusCode;
use serde_json::{Value, json};

/// The challenge, its runtime and the scope the caller plays it in.
async fn lab_context(ctx: &Ctx, uuid: &str) -> Result<(Chall, Runtime, Scope, String), ApiError> {
    let u = require_user(ctx)?;
    let Some(c) = chall::get(ctx.tenant_uuid(), uuid).await? else {
        return Err(not_found());
    };
    let organizer = ctx.has_role(Role::Admin) || chall::is_author(&c.uuid, &u.uuid).await?;
    let scope = if c.status == "published" {
        resolve_scope(ctx, &c, &u.uuid).await?
    } else {
        None
    };
    let scope = match (scope, organizer) {
        (Some(s), _) => s,
        (None, true) => Scope::default(),
        (None, false) => return Err(not_found()),
    };
    if c.ctype != "instance" {
        return Err(bad("This challenge has no lab."));
    }
    let Some(rt) = instance::runtime(&c.uuid).await? else {
        return Err(err(
            StatusCode::SERVICE_UNAVAILABLE,
            "The organizers have not configured this lab yet.",
        ));
    };
    Ok((c, rt, scope, u.uuid.clone()))
}

pub fn describe(i: Option<&Instance>, chall: &str, rt: &Runtime) -> Value {
    let Some(i) = i else {
        return json!({ "state": "none" });
    };
    if i.chall_uuid != chall {
        return json!({ "state": "elsewhere", "message": "You already have a lab running for another challenge. Stop it first.", "chall": i.chall_uuid });
    }
    let host = labs::INSTANCE_HOST.as_str();
    let url = if rt.protocol == "http" {
        format!("http://{host}:{}", i.host_port)
    } else {
        format!("nc {host} {}", i.host_port)
    };
    let ttl = rt.ttl_minutes as i64;
    let can_extend = (i.expires_at - i.created_at).num_minutes() < ttl * 4;
    match i.status.as_str() {
        "running" => {
            json!({ "state": "running", "url": url, "protocol": rt.protocol, "host": host, "port": i.host_port,
                             "expires_at": i.expires_at.format("%Y-%m-%dT%H:%M:%SZ").to_string(), "can_extend": can_extend })
        }
        "starting" => json!({ "state": "starting" }),
        "failed" => {
            json!({ "state": "failed", "message": "The lab could not start. Try again, or tell the organizers if it keeps failing." })
        }
        _ => json!({ "state": "none" }),
    }
}

pub async fn status(ctx: &Ctx, uuid: &str) -> ApiResult {
    let (c, rt, scope, user) = lab_context(ctx, uuid).await?;
    let live = instance::active_for_scope(&scope, &user).await?;
    if live.is_none()
        && let Some(last) = instance::latest(&c.uuid, &scope, &user).await?
        && last.status == "failed"
        && (chrono::Utc::now().naive_utc() - last.created_at).num_minutes() < 2
    {
        return ok(describe(Some(&last), &c.uuid, &rt));
    }
    ok(describe(live.as_ref(), &c.uuid, &rt))
}

pub async fn start(ctx: &Ctx, uuid: &str) -> ApiResult {
    let (c, rt, scope, user) = lab_context(ctx, uuid).await?;
    if !labs::enabled() {
        return Err(err(
            StatusCode::SERVICE_UNAVAILABLE,
            "Labs are not enabled on this platform.",
        ));
    }
    if let Some(i) = instance::active_for_scope(&scope, &user).await? {
        return ok(describe(Some(&i), &c.uuid, &rt));
    }
    if !ratelimit::allow(&format!("lab:{}", scope.scope_key(&user)), 5, 60).await? {
        return Err(too_many());
    }
    if instance::count_live().await? >= *labs::MAX_INSTANCES {
        return Err(err(
            StatusCode::SERVICE_UNAVAILABLE,
            "All lab slots are busy. Try again in a few minutes.",
        ));
    }
    // per-instance flags are new for each lab; other types expose the challenge flag as $FLAG
    let flag = if c.flag_type == "per_instance" {
        labs::new_flag(ctx.tenant.setting("flag_format"))
    } else {
        c.flag.clone()
    };
    let Some(inst) = instance::reserve(
        ctx.tenant_uuid(),
        &c.uuid,
        &user,
        &scope,
        &flag,
        rt.ttl_minutes,
    )
    .await?
    else {
        let i = instance::active_for_scope(&scope, &user).await?;
        return ok(describe(i.as_ref(), &c.uuid, &rt));
    };
    labs::spawn_launch(inst.clone(), ctx.tenant_uuid().to_string(), rt, flag);
    audit::log(
        ctx.tenant_uuid(),
        Some(&user),
        "lab.start",
        "instance",
        Some(&inst),
        json!({ "chall": c.slug }),
    )
    .await?;
    created(json!({ "state": "starting" }))
}

pub async fn extend(ctx: &Ctx, uuid: &str) -> ApiResult {
    let (c, rt, scope, user) = lab_context(ctx, uuid).await?;
    let Some(i) = instance::active_for_scope(&scope, &user)
        .await?
        .filter(|i| i.chall_uuid == c.uuid)
    else {
        return Err(bad("No running lab."));
    };
    if !instance::extend(&i.uuid, rt.ttl_minutes).await? {
        return Err(bad("This lab cannot be extended anymore."));
    }
    let i = instance::get(ctx.tenant_uuid(), &i.uuid).await?;
    let mut out = describe(i.as_ref(), &c.uuid, &rt);
    out["message"] = json!("Lab extended.");
    ok(out)
}

pub async fn stop(ctx: &Ctx, uuid: &str) -> ApiResult {
    let (_, _, scope, user) = lab_context(ctx, uuid).await?;
    let Some(i) = instance::active_for_scope(&scope, &user).await? else {
        return ok(json!({ "state": "none" }));
    };
    labs::destroy(&i.container_id, &i.network_id).await;
    instance::mark_stopped(&i.uuid, None).await?;
    audit::log(
        ctx.tenant_uuid(),
        Some(&user),
        "lab.stop",
        "instance",
        Some(&i.uuid),
        Value::Null,
    )
    .await?;
    ok(json!({ "state": "none", "message": "Lab stopped." }))
}
