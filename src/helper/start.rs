use crate::helper::database;
use crate::helper::migrate::run_migrations;
use crate::helper::trace::{trace_error, trace_logs};

pub async fn start() {
    wait_for_the_db_to_up().await;

    if let Err(e) = run_migrations().await {
        trace_error(format!("Migration failed: {}", e));
        std::process::exit(1);
    }

    trace_logs("Database setup completed !".to_owned());
}

async fn wait_for_the_db_to_up() {
    while !database::check_db_is_up().await {
        trace_logs("Waiting for the database to be up".to_owned());
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
