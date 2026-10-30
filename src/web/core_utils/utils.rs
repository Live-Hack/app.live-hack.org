use crate::web::core_utils::custom_type::RouteResult;
use std::fs;

/// Helper function to read an HTML file and wrap in RouteResult::Html.
pub fn read_html(path: &str) -> RouteResult {
    match fs::read_to_string(path) {
        Ok(contents) => RouteResult::Html(contents),
        Err(_) => RouteResult::NotFound,
    }
}

pub fn read_html_raw(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_default()
}
