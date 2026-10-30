//! Organizer API: create, edit, publish challenges, files, authors, import/export.
use crate::api::v1::core::{ApiError, ApiResult, bad, created, err, not_found, ok, require_role};
use crate::helper::bundle::{self, Source};
use crate::helper::markdown;
use crate::helper::request::{Ctx, field};
use crate::helper::storage::{self, StoreError};
use crate::models::audit;
use crate::models::chall::{self, Chall};
use crate::models::manage::{self, Fields, STATUSES};
use crate::models::user::{Role, User};
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, web};
use serde_json::{Value, json};

/// The challenge, if this organizer may manage it (admins: all, authors: their own).
pub async fn managed(ctx: &Ctx, uuid: &str) -> Result<(Chall, User), ApiError> {
    let user = require_role(ctx, Role::Author)?.clone();
    let Some(c) = chall::get(ctx.tenant_uuid(), uuid).await? else {
        return Err(not_found());
    };
    if user.role == Role::Admin || chall::is_author(&c.uuid, &user.uuid).await? {
        Ok((c, user))
    } else {
        Err(not_found())
    }
}

fn fields_from(body: &Value) -> Fields {
    Fields {
        name: field(body, "name").into(),
        description: body
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .into(),
        category: field(body, "category").into(),
        difficulty: field(body, "difficulty").into(),
        points: body
            .get("points")
            .and_then(|v| v.as_i64().or_else(|| v.as_str()?.trim().parse().ok()))
            .unwrap_or(-1),
        ctype: field(body, "ctype").into(),
        static_url: field(body, "static_url").into(),
        flag_type: field(body, "flag_type").into(),
        flag: field(body, "flag").into(),
    }
}

pub async fn create(ctx: &Ctx, body: &Value) -> ApiResult {
    let user = require_role(ctx, Role::Author)?;
    let mut f = Fields {
        name: field(body, "name").into(),
        points: 100,
        ..Default::default()
    };
    if f.name.is_empty() {
        f.name = "New challenge".into();
    }
    let f = f.validate().map_err(bad)?;
    let (uuid, slug) = manage::create(ctx.tenant_uuid(), &user.uuid, &f).await?;
    audit::log(
        ctx.tenant_uuid(),
        Some(&user.uuid),
        "chall.create",
        "chall",
        Some(&uuid),
        json!({ "slug": slug }),
    )
    .await?;
    created(json!({ "uuid": uuid, "slug": slug, "edit": format!("/manage/challenges/{uuid}") }))
}

pub async fn update(ctx: &Ctx, uuid: &str, body: &Value) -> ApiResult {
    let (c, user) = managed(ctx, uuid).await?;
    let f = fields_from(body).validate().map_err(bad)?;
    if c.status == "published" && f.flag.is_empty() && f.flag_type != "per_instance" {
        return Err(bad("A published challenge needs a flag."));
    }
    manage::update(ctx.tenant_uuid(), &c.uuid, &f).await?;
    audit::log(
        ctx.tenant_uuid(),
        Some(&user.uuid),
        "chall.update",
        "chall",
        Some(&c.uuid),
        Value::Null,
    )
    .await?;
    ok(json!({ "ok": true, "message": "Saved." }))
}

pub async fn set_status(ctx: &Ctx, uuid: &str, body: &Value) -> ApiResult {
    let (c, user) = managed(ctx, uuid).await?;
    let status = field(body, "status");
    if !STATUSES.contains(&status) {
        return Err(bad("Unknown status."));
    }
    if status == "published" {
        if c.flag.trim().is_empty() && c.flag_type != "per_instance" {
            return Err(bad("Set a flag before publishing."));
        }
        if c.ctype == "static" && c.static_url.is_empty() {
            return Err(bad("An online challenge needs its URL before publishing."));
        }
    }
    manage::set_status(ctx.tenant_uuid(), &c.uuid, status).await?;
    audit::log(
        ctx.tenant_uuid(),
        Some(&user.uuid),
        &format!("chall.{status}"),
        "chall",
        Some(&c.uuid),
        json!({ "from": c.status }),
    )
    .await?;
    let msg = match status {
        "published" => "Published. Players can see it now.",
        "draft" => "Moved back to draft. Players no longer see it.",
        _ => "Archived.",
    };
    ok(json!({ "ok": true, "status": status, "message": msg }))
}

/// Owner or admin; removes solves and files too.
pub async fn delete(ctx: &Ctx, uuid: &str) -> ApiResult {
    let (c, user) = managed(ctx, uuid).await?;
    if user.role != Role::Admin {
        let names = chall::author_names(&c.uuid).await?;
        if names.first() != Some(&user.username) {
            return Err(err(
                StatusCode::FORBIDDEN,
                "Only the owner or an admin can delete this challenge.",
            ));
        }
    }
    for key in manage::delete(ctx.tenant_uuid(), &c.uuid).await? {
        storage::remove(ctx.tenant_uuid(), &key).await;
    }
    audit::log(
        ctx.tenant_uuid(),
        Some(&user.uuid),
        "chall.delete",
        "chall",
        Some(&c.uuid),
        json!({ "slug": c.slug, "name": c.name }),
    )
    .await?;
    ok(json!({ "ok": true, "message": "Challenge deleted." }))
}

pub async fn delete_file(ctx: &Ctx, uuid: &str, file: &str) -> ApiResult {
    let (c, user) = managed(ctx, uuid).await?;
    let Some(key) = manage::delete_file(&c.uuid, file).await? else {
        return Err(not_found());
    };
    storage::remove(ctx.tenant_uuid(), &key).await;
    audit::log(
        ctx.tenant_uuid(),
        Some(&user.uuid),
        "chall.file.delete",
        "chall",
        Some(&c.uuid),
        json!({ "file": file }),
    )
    .await?;
    ok(json!({ "ok": true, "message": "File removed." }))
}

pub async fn add_author(ctx: &Ctx, uuid: &str, body: &Value) -> ApiResult {
    let (c, _) = managed(ctx, uuid).await?;
    if !manage::add_author(ctx.tenant_uuid(), &c.uuid, field(body, "username")).await? {
        return Err(bad("No user with this username."));
    }
    ok(json!({ "ok": true, "message": "Co-author added." }))
}

pub async fn remove_author(ctx: &Ctx, uuid: &str, username: &str) -> ApiResult {
    let (c, _) = managed(ctx, uuid).await?;
    manage::remove_author(&c.uuid, username).await?;
    ok(json!({ "ok": true, "message": "Co-author removed." }))
}

pub fn preview(ctx: &Ctx, body: &Value) -> ApiResult {
    require_role(ctx, Role::Author)?;
    let src = body.get("markdown").and_then(Value::as_str).unwrap_or("");
    ok(json!({ "html": markdown::render(src) }))
}

fn store_error(e: StoreError) -> ApiError {
    match e {
        StoreError::TooLarge => err(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "File too large ({} MB max).",
                *storage::MAX_UPLOAD / 1024 / 1024
            ),
        ),
        StoreError::Io(e) => {
            crate::helper::trace::trace_error(format!("upload: {e}"));
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not store the file.",
            )
        }
    }
}

/// Raw body upload: POST .../files?name=dump.pcap (no multipart, streamed to disk).
pub async fn upload(ctx: &Ctx, uuid: &str, payload: web::Payload) -> ApiResult {
    let (c, user) = managed(ctx, uuid).await?;
    let name = storage::clean_filename(ctx.param("name"));
    let s = storage::store(ctx.tenant_uuid(), payload)
        .await
        .map_err(store_error)?;
    let file = manage::add_file(&c.uuid, &name, &s.key, &s.sha256, s.size).await?;
    audit::log(
        ctx.tenant_uuid(),
        Some(&user.uuid),
        "chall.file.add",
        "chall",
        Some(&c.uuid),
        json!({ "name": name, "size": s.size }),
    )
    .await?;
    created(json!({ "uuid": file, "name": name, "sha256": s.sha256, "size": s.size }))
}

/// Raw zip body with one or more challenge.yml (ctfcli format). Upserts by slug.
pub async fn import(ctx: &Ctx, payload: web::Payload) -> ApiResult {
    let user = require_role(ctx, Role::Author)?.clone();
    let t = ctx.tenant_uuid().to_string();
    let tmp = storage::store(&t, payload).await.map_err(store_error)?;
    let path = storage::path(&t, &tmp.key).expect("generated key");
    let archive = tokio::task::spawn_blocking(move || {
        std::fs::File::open(&path)
            .map_err(|e| e.to_string())
            .and_then(|f| zip::ZipArchive::new(f).map_err(|e| e.to_string()))
    })
    .await
    .map_err(|e| bad(e.to_string()))?;
    let result = match archive {
        Ok(z) => {
            let mut src = Source::Zip(z);
            let admin = user.role == Role::Admin;
            let uid = user.uuid.clone();
            bundle::import(&t, &user.uuid, &mut src, false, async |chall_uuid: &str| {
                admin || chall::is_author(chall_uuid, &uid).await.unwrap_or(false)
            })
            .await
        }
        Err(_) => Err("This is not a valid zip file.".into()),
    };
    storage::remove(&t, &tmp.key).await;
    let imported = result.map_err(bad)?;
    audit::log(
        &t,
        Some(&user.uuid),
        "chall.import",
        "tenant",
        None,
        json!({ "count": imported.len() }),
    )
    .await?;
    let msg = format!(
        "{} challenge(s) imported as drafts or updated.",
        imported.len()
    );
    ok(json!({ "ok": true, "imported": imported, "message": msg }))
}

pub async fn export(ctx: &Ctx, uuid: &str) -> HttpResponse {
    let c = match managed(ctx, uuid).await {
        Ok((c, _)) => c,
        Err(e) => return crate::api::v1::core::respond(Err(e)),
    };
    match bundle::export(ctx.tenant_uuid(), &c).await {
        Ok(bytes) => HttpResponse::Ok()
            .content_type("application/zip")
            .insert_header((
                "Content-Disposition",
                format!("attachment; filename=\"{}.zip\"", c.slug),
            ))
            .insert_header(("Cache-Control", "no-store"))
            .body(bytes),
        Err(e) => crate::api::v1::core::respond(Err(err(StatusCode::INTERNAL_SERVER_ERROR, e))),
    }
}
