use actix_web::Scope;
use actix_web::web;

pub fn init_v1api() -> Scope {
    web::scope("/api/v1")
        .service(crate::api::v1::main::v1_handler)
        .service(crate::api::v1::main::v1_handlerpost)
}
