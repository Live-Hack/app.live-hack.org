use crate::helper::request::Ctx;
use crate::helper::trace::trace_error;
use crate::models::user::{Role, User};
use actix_web::HttpResponse;
use actix_web::http::StatusCode;
use serde_json::{Value, json};

pub struct Reply {
    pub status: StatusCode,
    pub body: Value,
    pub cookie: Option<String>,
}

pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

pub type ApiResult = Result<Reply, ApiError>;

pub fn ok(body: Value) -> ApiResult {
    Ok(Reply {
        status: StatusCode::OK,
        body,
        cookie: None,
    })
}

pub fn created(body: Value) -> ApiResult {
    Ok(Reply {
        status: StatusCode::CREATED,
        body,
        cookie: None,
    })
}

pub fn with_cookie(body: Value, cookie: String) -> ApiResult {
    Ok(Reply {
        status: StatusCode::OK,
        body,
        cookie: Some(cookie),
    })
}

pub fn err(status: StatusCode, message: impl Into<String>) -> ApiError {
    ApiError {
        status,
        message: message.into(),
    }
}

pub fn bad(message: impl Into<String>) -> ApiError {
    err(StatusCode::BAD_REQUEST, message)
}

pub fn not_found() -> ApiError {
    err(StatusCode::NOT_FOUND, "Not found")
}

pub fn too_many() -> ApiError {
    err(
        StatusCode::TOO_MANY_REQUESTS,
        "Too many attempts, wait a minute and try again.",
    )
}

impl From<mysql_async::Error> for ApiError {
    fn from(e: mysql_async::Error) -> Self {
        trace_error(format!("database: {e}"));
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal error, try again later.",
        )
    }
}

pub fn require_user(ctx: &Ctx) -> Result<&User, ApiError> {
    ctx.user
        .as_ref()
        .ok_or_else(|| err(StatusCode::UNAUTHORIZED, "Sign in first."))
}

pub fn require_role(ctx: &Ctx, role: Role) -> Result<&User, ApiError> {
    let user = require_user(ctx)?;
    if user.role >= role {
        Ok(user)
    } else {
        Err(err(
            StatusCode::FORBIDDEN,
            "You do not have access to this action.",
        ))
    }
}

pub fn respond(result: ApiResult) -> HttpResponse {
    match result {
        Ok(reply) => {
            let mut res = HttpResponse::build(reply.status);
            res.insert_header(("Cache-Control", "no-store"));
            if let Some(c) = reply.cookie {
                res.append_header(("Set-Cookie", c));
            }
            res.json(reply.body)
        }
        Err(e) => HttpResponse::build(e.status)
            .insert_header(("Cache-Control", "no-store"))
            .json(json!({ "error": e.message })),
    }
}
