// Shared tracing module. Logs go to stdout as JSON; never log session tokens or passwords.
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

pub fn init_trace() {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
}

pub fn trace_logs(message: String) {
    info!("{}", message);
}

pub fn trace_warn(message: String) {
    warn!("{}", message);
}

pub fn trace_error(message: String) {
    error!("{}", message);
}
