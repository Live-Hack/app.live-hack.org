pub mod audit;
pub mod session;
pub mod token;
pub mod user;

/// MySQL "Duplicate entry" error, raised by UNIQUE keys.
pub fn is_duplicate(e: &mysql_async::Error) -> bool {
    matches!(e, mysql_async::Error::Server(s) if s.code == 1062)
}
