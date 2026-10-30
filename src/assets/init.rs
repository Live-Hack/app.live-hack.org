use actix_web::{HttpRequest, HttpResponse, Scope, get, web};
use once_cell::sync::Lazy;
use serde_json::Value;
use std::fs;
use std::hash::{Hash, Hasher};

pub static TYPES: Lazy<Value> = Lazy::new(|| {
    let content = fs::read_to_string("utils/types.json").unwrap_or_else(|_| "[]".to_string());
    serde_json::from_str(&content).unwrap_or(Value::Array(vec![]))
});

/// Hash of the CSS/JS files, appended as `?v=` so browsers can cache assets forever
/// and still pick up a new release.
pub static ASSET_VERSION: Lazy<String> = Lazy::new(|| {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for dir in ["assets/css", "assets/js"] {
        let mut files: Vec<_> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .collect();
        files.sort();
        for f in files {
            fs::read(&f).unwrap_or_default().hash(&mut h);
        }
    }
    format!("{:x}", h.finish() & 0xffff_ffff)
});

fn not_found() -> HttpResponse {
    HttpResponse::NotFound()
        .content_type("text/plain")
        .body("404 Not Found")
}

/// Rejects `..` segments so `/assets/../config` cannot leave the assets folder.
fn is_safe_path(path: &str) -> bool {
    !path.split(['/', '\\']).any(|s| s == "..")
}

fn mime_for(path: &str) -> Option<&'static str> {
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_lowercase())?;
    TYPES.as_array()?.iter().find_map(|t| {
        let exts = t["ext"].as_array()?;
        exts.iter()
            .any(|e| e.as_str() == Some(ext.as_str()))
            .then(|| t["type"].as_str())
            .flatten()
    })
}

#[get("/{path:.*}")]
pub async fn get_assets(path: web::Path<String>, req: HttpRequest) -> HttpResponse {
    if !is_safe_path(&path) {
        return not_found();
    }
    let Some(mime) = mime_for(&path) else {
        return not_found();
    };
    let Ok(file) = web::block({
        let p = format!("./assets/{}", path);
        move || fs::read(p)
    })
    .await
    .unwrap_or_else(|_| Err(std::io::ErrorKind::Other.into())) else {
        return not_found();
    };
    // versioned URLs never change content; others are revalidated hourly
    let cache = if req.query_string().starts_with("v=") {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=3600"
    };
    HttpResponse::Ok()
        .content_type(mime)
        .insert_header(("Cache-Control", cache))
        .insert_header(("X-Content-Type-Options", "nosniff"))
        .body(file)
}

pub fn init_assets() -> Scope {
    web::scope("/assets").service(get_assets)
}

#[cfg(test)]
mod tests {
    use super::is_safe_path;

    #[test]
    fn traversal_is_rejected() {
        assert!(is_safe_path("css/main.css"));
        assert!(!is_safe_path("../Cargo.toml"));
        assert!(!is_safe_path("css/../../etc/passwd"));
        assert!(!is_safe_path("css\\..\\x"));
    }
}
