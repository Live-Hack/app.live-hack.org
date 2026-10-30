//! Tenants (one platform each) cached in memory, resolved from the Host header.
use crate::helper::database::{DbResult, get_conn};
use mysql_async::prelude::*;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug)]
pub struct Tenant {
    pub uuid: String,
    pub slug: String,
    pub host: Option<String>,
    pub settings: HashMap<String, String>,
}

impl Tenant {
    pub fn setting(&self, key: &str) -> &str {
        self.settings.get(key).map(String::as_str).unwrap_or("")
    }
    pub fn site_name(&self) -> &str {
        match self.setting("site_name") {
            "" => "Live Hack",
            s => s,
        }
    }
}

#[derive(Default)]
struct Cache {
    by_host: HashMap<String, Arc<Tenant>>,
    default: Option<Arc<Tenant>>,
}

static CACHE: Lazy<RwLock<Cache>> = Lazy::new(|| RwLock::new(Cache::default()));

/// Loads every tenant and its settings. Called at startup, after a settings change,
/// and periodically so all replicas converge.
pub async fn reload() -> DbResult<()> {
    let mut conn = get_conn().await?;
    let tenants: Vec<(String, String, Option<String>)> =
        conn.query("SELECT uuid, slug, host FROM tenants").await?;
    let settings: Vec<(String, String, String)> = conn
        .query("SELECT tenant_uuid, `key`, value FROM settings")
        .await?;

    let mut cache = Cache::default();
    for (uuid, slug, host) in tenants {
        let settings = settings
            .iter()
            .filter(|(t, _, _)| *t == uuid)
            .map(|(_, k, v)| (k.clone(), v.clone()))
            .collect();
        let t = Arc::new(Tenant {
            uuid,
            slug,
            host: host.clone(),
            settings,
        });
        match host {
            Some(h) => {
                cache.by_host.insert(h.to_ascii_lowercase(), t);
            }
            None => cache.default = Some(t),
        }
    }
    *CACHE.write().unwrap() = cache;
    Ok(())
}

/// Host header (port stripped) → tenant, falling back to the default tenant.
pub fn resolve(host: &str) -> Option<Arc<Tenant>> {
    let host = host
        .rsplit_once(':')
        .map_or(host, |(h, _)| h)
        .to_ascii_lowercase();
    let cache = CACHE.read().unwrap();
    cache
        .by_host
        .get(&host)
        .cloned()
        .or_else(|| cache.default.clone())
}

pub async fn set_setting(tenant_uuid: &str, key: &str, value: &str) -> DbResult<()> {
    get_conn()
        .await?
        .exec_drop(
            "INSERT INTO settings (tenant_uuid, `key`, value) VALUES (?, ?, ?) AS n
             ON DUPLICATE KEY UPDATE value = n.value",
            (tenant_uuid, key, value),
        )
        .await?;
    reload().await
}
