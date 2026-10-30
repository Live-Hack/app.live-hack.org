use once_cell::sync::Lazy;
use std::env;

/// Runtime configuration, read once from environment variables.
/// Defaults match docker-compose.yml so a fresh clone runs without a .env file.
pub struct Config {
    pub port: u16,
    pub base_url: String,
    pub db_host: String,
    pub db_port: u16,
    pub db_name: String,
    pub db_user: String,
    pub db_password: String,
    pub db_pool_max: usize,
    /// Session cookie gets `Secure` when the platform is served over HTTPS.
    pub secure_cookies: bool,
}

fn var(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

pub static CONFIG: Lazy<Config> = Lazy::new(|| {
    let base_url = var("BASE_URL", "http://localhost:8080");
    Config {
        port: var("PORT", "8080").parse().expect("PORT must be a number"),
        secure_cookies: base_url.starts_with("https://"),
        base_url,
        db_host: var("DB_HOST", "127.0.0.1"),
        db_port: var("DB_PORT", "3306")
            .parse()
            .expect("DB_PORT must be a number"),
        db_name: var("DB_NAME", "livehack"),
        db_user: var("DB_USER", "livehack"),
        db_password: var("DB_PASSWORD", "livehack"),
        db_pool_max: var("DB_POOL_MAX", "32")
            .parse()
            .expect("DB_POOL_MAX must be a number"),
    }
});
