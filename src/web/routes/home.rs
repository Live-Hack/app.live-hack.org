use crate::helper::request::Ctx;
use crate::helper::template::Vars;
use crate::web::page::{PageResult, page};

pub fn home(ctx: &Ctx) -> PageResult {
    page(ctx, "home.html", "", Vars::new())
}
