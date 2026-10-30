use crate::helper::trace::trace_logs;
use actix_web::{HttpRequest, web};
use futures::StreamExt;
use qstring::QString;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

const MAX_SIZE: usize = 2_097_152;

#[derive(Debug, Clone, Serialize)]
pub struct RequestData {
    pub path: String,
    pub user_ip: String,
    pub method: String,
    pub parsed_params: HashMap<String, String>,
}

pub type RequestDataAPIV1 = RequestData;

pub fn log_request(path: &str, req: &HttpRequest, method: &str) -> RequestData {
    let r = RequestData {
        path: format!("/{}", path),
        method: method.to_string(),
        user_ip: get_ip_address(req),
        parsed_params: get_parsed_params(req),
    };
    trace_logs(format!("Request: {} {}", r.method, r.path));
    r
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

fn get_parsed_params(req: &HttpRequest) -> HashMap<String, String> {
    QString::from(req.query_string()).into_iter().collect()
}

fn get_ip_address(req: &HttpRequest) -> String {
    // peer_addr, not realip_remote_addr: X-Forwarded-For is client-controlled
    // until a trusted reverse proxy is configured.
    req.peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
