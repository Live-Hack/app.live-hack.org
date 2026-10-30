use crate::api::v1::core::{self, ApiResult, err, not_found, respond};
use crate::api::v1::{account, admin, auth, challenges, manage, score};
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
        ["challenges"] => challenges::list(ctx).await,
        ["scoreboard"] => score::scoreboard(ctx).await,
        ["users", name] => score::profile(ctx, name).await,
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
        ["challenges", uuid, "submit"] => challenges::submit(ctx, uuid, body).await,
        ["manage", "challenges"] => manage::create(ctx, body).await,
        ["manage", "challenges", uuid] => manage::update(ctx, uuid, body).await,
        ["manage", "challenges", uuid, "status"] => manage::set_status(ctx, uuid, body).await,
        ["manage", "challenges", uuid, "delete"] => manage::delete(ctx, uuid).await,
        ["manage", "challenges", uuid, "files", file, "delete"] => {
            manage::delete_file(ctx, uuid, file).await
        }
        ["manage", "challenges", uuid, "authors"] => manage::add_author(ctx, uuid, body).await,
        ["manage", "challenges", uuid, "authors", name, "remove"] => {
            manage::remove_author(ctx, uuid, name).await
        }
        ["manage", "preview"] => manage::preview(ctx, body),
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
    let segs = segments(&path);
    if let ["manage", "challenges", uuid, "export"] = segs.as_slice() {
        return manage::export(&ctx, uuid).await;
    }
    respond(route_get(&ctx, &segs).await)
}

/// Streamed bodies (file uploads, zip imports) skip the JSON body reader.
async fn route_stream(ctx: &Ctx, segs: &[&str], payload: web::Payload) -> Option<ApiResult> {
    Some(match segs {
        ["manage", "challenges", uuid, "files"] => manage::upload(ctx, uuid, payload).await,
        ["manage", "import"] => manage::import(ctx, payload).await,
        _ => return None,
    })
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
    let segs = segments(&path);
    if matches!(
        segs.as_slice(),
        ["manage", "challenges", _, "files"] | ["manage", "import"]
    ) {
        return respond(
            route_stream(&ctx, &segs, payload)
                .await
                .expect("matched above"),
        );
    }
    let body = get_request_body(payload).await;
    respond(route_post(&ctx, &segs, &body).await)
}
