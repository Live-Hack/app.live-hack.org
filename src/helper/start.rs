use crate::helper::trace::{trace_error, trace_logs};
use crate::helper::{database, migrate::run_migrations, ratelimit, template, tenant};
use crate::models::session;
use std::time::Duration;

pub async fn start() {
    wait_for_the_db_to_up().await;

    if let Err(e) = run_migrations().await {
        trace_error(format!("Migration failed: {}", e));
        std::process::exit(1);
    }
    if let Err(e) = tenant::reload().await {
        trace_error(format!("Loading tenants failed: {}", e));
        std::process::exit(1);
    }
    trace_logs(format!("{} templates loaded", template::preload()));
    trace_logs("Database setup completed !".to_owned());
}

/// Background housekeeping. Safe to run on every replica.
pub fn spawn_background_tasks() {
    actix_web::rt::spawn(async {
        // first tick fires immediately: orphan labs from a crash are cleaned at boot
        let mut tick = actix_web::rt::time::interval(Duration::from_secs(
            std::env::var("SWEEP_SECONDS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
        ));
        let mut n: u64 = 0;
        loop {
            tick.tick().await;
            n += 1;
            if let Err(e) = tenant::reload().await {
                trace_error(format!("tenant reload: {e}"));
            }
            match crate::models::event::close_ended(None).await {
                Ok(closed) if !closed.is_empty() => {
                    trace_logs(format!("closed {} ended event(s)", closed.len()))
                }
                Ok(_) => {}
                Err(e) => trace_error(format!("closing events: {e}")),
            }
            crate::helper::labs::sweep().await;
            if n.is_multiple_of(20) {
                let _ = ratelimit::cleanup().await;
                let _ = session::cleanup().await;
            }
        }
    });
}

async fn wait_for_the_db_to_up() {
    while !database::check_db_is_up().await {
        trace_logs("Waiting for the database to be up".to_owned());
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
