extern crate app_live_hack;
use actix_web::{App, HttpServer};
use app_live_hack::{api, assets, web};
use std::fs;

use app_live_hack::helper::config::CONFIG;
use app_live_hack::helper::start::start;
use app_live_hack::helper::trace::{init_trace, trace_logs};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!(
        "{}",
        fs::read_to_string("utils/ascii.art").unwrap_or_default()
    );
    init_trace();
    trace_logs("Server is starting...".to_string());
    start().await;

    // No CORS layer: the front is served from the same origin as the API.
    HttpServer::new(|| {
        App::new()
            .service(api::init::init_v1api())
            .service(assets::init::init_assets())
            .service(web::init::init_web())
    })
    .bind(("0.0.0.0", CONFIG.port))?
    .run()
    .await
}
