use crate::helper::request::Ctx;
use crate::helper::template::Vars;
use crate::web::page::{PageResult, page};
use crate::web::routes::score;

pub async fn home(ctx: &Ctx) -> PageResult {
    if ctx.user.is_some() {
        return score::dashboard(ctx).await;
    }
    page(ctx, "home.html", "", Vars::new())
}
