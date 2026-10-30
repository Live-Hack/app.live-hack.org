use once_cell::sync::Lazy;
use std::fs;

pub static HTML_404: Lazy<String> = Lazy::new(|| {
    fs::read_to_string("html/404.html").unwrap_or_else(|_| "<h1>404 Not Found</h1>".to_string())
});

pub static HTML_REDIRECT: Lazy<String> = Lazy::new(|| {
    fs::read_to_string("html/redirect.html")
        .unwrap_or_else(|_| "<meta http-equiv=\"refresh\" content=\"0\">".to_string())
});
