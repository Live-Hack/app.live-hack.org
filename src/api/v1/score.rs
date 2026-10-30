use crate::api::v1::core::{ApiResult, err, not_found, ok, require_user};
use crate::helper::request::Ctx;
use crate::models::score::{self, Query};
use crate::models::user;
use crate::web::routes::score::scoreboard_hidden;
use actix_web::http::StatusCode;
use serde_json::json;

pub async fn scoreboard(ctx: &Ctx) -> ApiResult {
    let me = require_user(ctx)?;
    if scoreboard_hidden(ctx) {
        return Err(err(StatusCode::FORBIDDEN, "The scoreboard is hidden."));
    }
    let list = score::ranking(&Query {
        tenant: ctx.tenant_uuid(),
        event_key: "",
        teams: false,
        category: ctx.param("category"),
        until: None,
    })
    .await?;
    let mine = list.iter().find(|r| r.id == me.uuid);
    ok(json!({ "standings": &list[..list.len().min(1000)], "total": list.len(), "me": mine }))
}

pub async fn profile(ctx: &Ctx, username: &str) -> ApiResult {
    require_user(ctx)?;
    let Some(u) = user::get_by_username(ctx.tenant_uuid(), username).await? else {
        return Err(not_found());
    };
    if u.banned {
        return Err(not_found());
    }
    let solves = score::user_solves(ctx.tenant_uuid(), &u.uuid).await?;
    let list = score::ranking(&Query {
        tenant: ctx.tenant_uuid(),
        event_key: "",
        teams: false,
        category: "",
        until: None,
    })
    .await?;
    let entry = if scoreboard_hidden(ctx) {
        None
    } else {
        list.iter().find(|r| r.id == u.uuid)
    };
    ok(
        json!({ "username": u.username, "role": u.role, "rank": entry.map(|e| e.rank), "score": entry.map(|e| e.score), "solves": solves }),
    )
}
