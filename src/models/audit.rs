use crate::helper::database::{DbResult, get_conn};
use mysql_async::prelude::*;
use serde_json::Value;

pub async fn log(
    tenant: &str,
    actor: Option<&str>,
    action: &str,
    target_type: &str,
    target: Option<&str>,
    meta: Value,
) -> DbResult<()> {
    let meta = if meta.is_null() {
        None
    } else {
        Some(meta.to_string())
    };
    get_conn()
        .await?
        .exec_drop(
            "INSERT INTO audit_log (tenant_uuid, actor_uuid, action, target_type, target_uuid, meta) VALUES (?, ?, ?, ?, ?, ?)",
            (tenant, actor, action, target_type, target, meta),
        )
        .await
}
