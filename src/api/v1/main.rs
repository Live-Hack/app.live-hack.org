use crate::api::v1::core::{self, ApiResult, err, not_found, respond};
use crate::api::v1::{account, admin, auth};
use crate::helper::request::{Ctx, build, get_request_body};
use actix_web::http::StatusCode;
use actix_web::{HttpRequest, HttpResponse, get, post, web};
use serde_json::{Value, json};

//   _____     _    _____         _
//  |  _  |___|_|  | __  |___ _ _| |_ ___ ___
//  |     | . | |  |    -| . | | |  _| -_|_ -|
//  |__|__|  _|_|  |__|__|___|___|_| |___|___|
//        |_|

async fn route_get(ctx: &Ctx, segs: &[&str]) -> ApiResult {
    match segs {
        [] => core::ok(json!({ "name": "live-hack", "version": env!("CARGO_PKG_VERSION") })),
        ["me"] => auth::me(ctx),
        ["account", "sessions"] => account::sessions(ctx).await,
        ["admin", "settings"] => admin::get_settings(ctx),
        _ => Err(not_found()),
    }
}

async fn route_post(ctx: &Ctx, segs: &[&str], body: &Value) -> ApiResult {
    match segs {
        ["setup"] => auth::setup(ctx, body).await,
        ["auth", "login"] => auth::login(ctx, body).await,
        ["auth", "register"] => auth::register(ctx, body).await,
        ["auth", "logout"] => auth::logout(ctx).await,
        ["auth", "reset"] => auth::reset(ctx, body).await,
        ["account", "username"] => account::username(ctx, body).await,
        ["account", "password"] => account::password(ctx, body).await,
        ["account", "sessions", "revoke"] => account::revoke_others(ctx).await,
        ["admin", "settings"] => admin::set_settings(ctx, body).await,
        ["admin", "invites"] => admin::create_invite(ctx, body).await,
        ["admin", "users", uuid, "reset-link"] => admin::reset_link(ctx, uuid).await,
        _ => Err(not_found()),
    }
}

fn segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

#[get("/{path:.*}")]
pub async fn v1_get(path: web::Path<String>, req: HttpRequest) -> HttpResponse {
    let Some(ctx) = build(&req, &path, "GET").await else {
        return respond(Err(not_found()));
    };
    respond(route_get(&ctx, &segments(&path)).await)
}

#[post("/{path:.*}")]
pub async fn v1_post(
    path: web::Path<String>,
    payload: web::Payload,
    req: HttpRequest,
) -> HttpResponse {
    let Some(ctx) = build(&req, &path, "POST").await else {
        return respond(Err(not_found()));
    };
    if !ctx.is_trusted_write() {
        return respond(Err(err(
            StatusCode::FORBIDDEN,
            "Missing X-Requested-With: fetch header or cross-origin request.",
        )));
    }
    let body = get_request_body(payload).await;
    respond(route_post(&ctx, &segments(&path), &body).await)
}
