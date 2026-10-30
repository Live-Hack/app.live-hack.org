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
}

fn var(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

pub static CONFIG: Lazy<Config> = Lazy::new(|| Config {
    port: var("PORT", "8080").parse().expect("PORT must be a number"),
    base_url: var("BASE_URL", "http://localhost:8080"),
    db_host: var("DB_HOST", "127.0.0.1"),
    db_port: var("DB_PORT", "3306")
        .parse()
        .expect("DB_PORT must be a number"),
    db_name: var("DB_NAME", "livehack"),
    db_user: var("DB_USER", "livehack"),
    db_password: var("DB_PASSWORD", "livehack"),
});
