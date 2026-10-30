use crate::helper::config::CONFIG;
use crate::web::core_utils::custom_type::RouteResult;
use crate::web::core_utils::utils::read_html_raw;

pub fn robot() -> RouteResult {
    RouteResult::Text(read_html_raw("html/robots.txt"))
}

/// Served only when the host sets SECURITY_CONTACT, since each install has its own contact.
pub fn security() -> RouteResult {
    match std::env::var("SECURITY_CONTACT") {
        Ok(contact) if !contact.is_empty() => RouteResult::Text(format!(
            "Contact: {}\nCanonical: {}/.well-known/security.txt\nPreferred-Languages: en, fr\n",
            contact, CONFIG.base_url
        )),
        _ => RouteResult::NotFound,
    }
}
