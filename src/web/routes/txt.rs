use crate::helper::request::Ctx;
use crate::web::page::RouteResult;

pub fn robot() -> RouteResult {
    RouteResult::Text("User-agent: *\nDisallow: /\n".to_string())
}

/// Served only when the host sets SECURITY_CONTACT, since each install has its own contact.
pub fn security(ctx: &Ctx) -> RouteResult {
    match std::env::var("SECURITY_CONTACT") {
        Ok(contact) if !contact.is_empty() => RouteResult::Text(format!(
            "Contact: {}\nCanonical: {}\nPreferred-Languages: en, fr\n",
            contact,
            ctx.url("/.well-known/security.txt")
        )),
        _ => RouteResult::NotFound,
    }
}
