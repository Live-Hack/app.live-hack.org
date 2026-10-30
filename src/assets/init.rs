use actix_web::{HttpResponse, Responder, Scope, get, web};
use once_cell::sync::Lazy;
use serde_json::Value;
use std::fs;

//save the json value to a global variable
pub static TYPES: Lazy<Value> = Lazy::new(|| {
    let content = fs::read_to_string("utils/types.json").unwrap_or_else(|_| "[]".to_string());
    serde_json::from_str(&content).unwrap_or(Value::Array(vec![]))
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
pub async fn get_assets(path: web::Path<String>) -> impl Responder {
    if !is_safe_path(&path) {
        return not_found();
    }
    let Some(mime) = mime_for(&path) else {
        return not_found();
    };
    match fs::read(format!("./assets/{}", path)) {
        Ok(file) => HttpResponse::Ok().content_type(mime).body(file),
        Err(_) => not_found(),
    }
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
