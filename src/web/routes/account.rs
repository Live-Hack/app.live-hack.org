use crate::helper::request::Ctx;
use crate::helper::template::Vars;
use crate::web::page::{PageResult, page, require_login};

pub fn settings_page(ctx: &Ctx) -> PageResult {
    let user = match require_login(ctx) {
        Ok(u) => u,
        Err(r) => return Ok(r),
    };
    let mut v = Vars::new();
    v.set("username", user.username.as_str())
        .set("role", user.role.as_str())
        .set(
            "member_since",
            user.created_at.format("%Y-%m-%d").to_string(),
        );
    page(ctx, "account/settings.html", "Settings", v)
}
