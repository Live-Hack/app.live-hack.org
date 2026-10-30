use super::dispatch;
use crate::helper::database::check_db_is_up;
use actix_web::{HttpResponse, Responder, Scope, get, web};

#[get("/healthz")]
async fn healthz() -> impl Responder {
    if check_db_is_up().await {
        HttpResponse::Ok().content_type("text/plain").body("ok")
    } else {
        HttpResponse::ServiceUnavailable()
            .content_type("text/plain")
            .body("database unreachable")
    }
}

pub fn init_web() -> Scope {
    web::scope("").service(healthz).service(dispatch::dispatch)
}
