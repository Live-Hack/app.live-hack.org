//! Challenge files on local disk, under `DATA_DIR/files/<tenant>/<key>`.
//! ponytail: local disk only; an S3 backend would implement the same three functions.
use once_cell::sync::Lazy;
use std::path::PathBuf;

pub static DATA_DIR: Lazy<PathBuf> =
    Lazy::new(|| PathBuf::from(std::env::var("DATA_DIR").unwrap_or_else(|_| "data".into())));

pub fn path(tenant: &str, key: &str) -> Option<PathBuf> {
    // keys are generated uuids; reject anything else so a key can never escape the folder
    let ok = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    (ok(tenant) && ok(key)).then(|| DATA_DIR.join("files").join(tenant).join(key))
}
