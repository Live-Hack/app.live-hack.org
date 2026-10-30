use crate::helper::tenant::{self, Tenant};
use crate::helper::trace::trace_error;
use crate::models::session;
use crate::models::user::{Role, User};
use actix_web::{HttpRequest, web};
use futures::StreamExt;
use qstring::QString;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

const MAX_SIZE: usize = 2_097_152;

/// Everything a page or API handler needs about the current request.
pub struct Ctx {
    pub path: String,
    pub method: &'static str,
    pub ip: String,
    pub user_agent: String,
    pub host: String,
    pub params: HashMap<String, String>,
    pub tenant: Arc<Tenant>,
    pub user: Option<User>,
    /// Raw session token from the cookie, when the session is valid.
    pub token: Option<String>,
    /// Same-origin proof for state-changing API calls (see `is_trusted_write`).
    trusted_write: bool,
}

pub type RequestData = Ctx;

impl Ctx {
    pub fn param(&self, key: &str) -> &str {
        self.params.get(key).map(String::as_str).unwrap_or("")
    }
    pub fn has_role(&self, role: Role) -> bool {
        self.user.as_ref().is_some_and(|u| u.role >= role)
    }
    pub fn tenant_uuid(&self) -> &str {
        &self.tenant.uuid
    }
    pub fn is_trusted_write(&self) -> bool {
        self.trusted_write
    }
    /// Absolute URL on the current tenant's host.
    pub fn url(&self, path: &str) -> String {
        let scheme = if crate::helper::config::CONFIG.secure_cookies {
            "https"
        } else {
            "http"
        };
        format!("{scheme}://{}{path}", self.host)
    }
}

/// Resolves tenant and session. None when no tenant serves this host.
pub async fn build(req: &HttpRequest, path: &str, method: &'static str) -> Option<Ctx> {
    let host = req.connection_info().host().to_string();
    let tenant = tenant::resolve(&host)?;

    let mut user = None;
    let mut token = None;
    if let Some(c) = req.cookie(session::COOKIE) {
        match session::lookup(&tenant.uuid, c.value()).await {
            Ok(Some(u)) => {
                user = Some(u);
                token = Some(c.value().to_string());
            }
            Ok(None) => {}
            Err(e) => trace_error(format!("session lookup: {e}")),
        }
    }

    let header = |name: &str| {
        req.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    };
    // Cross-site pages cannot set custom headers without a CORS preflight, which we never allow.
    // An Origin header, when present, must match the Host.
    let origin = header("origin");
    let origin_ok = origin.is_empty()
        || origin
            .split("://")
            .nth(1)
            .is_some_and(|o| o.eq_ignore_ascii_case(&host));
    let trusted_write = header("x-requested-with") == "fetch" && origin_ok;

    Some(Ctx {
        path: format!("/{}", path),
        method,
        ip: req
            .peer_addr()
            .map(|a| a.ip().to_string())
            .unwrap_or_default(),
        user_agent: header("user-agent").chars().take(255).collect(),
        host,
        params: QString::from(req.query_string()).into_iter().collect(),
        tenant,
        user,
        token,
        trusted_write,
    })
}

/// Reads a JSON body up to 2 MiB. Anything invalid becomes `Value::Null`.
pub async fn get_request_body(mut payload: web::Payload) -> Value {
    let mut body = web::BytesMut::new();
    while let Some(chunk) = payload.next().await {
        let Ok(chunk) = chunk else { return Value::Null };
        if body.len() + chunk.len() > MAX_SIZE {
            return Value::Null;
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).unwrap_or(Value::Null)
}

/// String field from a JSON body, trimmed; "" if missing.
pub fn field<'a>(body: &'a Value, key: &str) -> &'a str {
    body.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
}

/// Same as `field` but not trimmed (passwords, flags with spaces).
pub fn field_raw<'a>(body: &'a Value, key: &str) -> &'a str {
    body.get(key).and_then(Value::as_str).unwrap_or("")
}

pub fn field_u64(body: &Value, key: &str) -> Option<u64> {
    body.get(key)
        .and_then(|v| v.as_u64().or_else(|| v.as_str()?.trim().parse().ok()))
}
