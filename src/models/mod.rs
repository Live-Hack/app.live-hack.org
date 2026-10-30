pub mod audit;
pub mod chall;
pub mod manage;
pub mod score;
pub mod session;
pub mod solve;
pub mod token;
pub mod user;

/// MySQL "Duplicate entry" error, raised by UNIQUE keys.
pub fn is_duplicate(e: &mysql_async::Error) -> bool {
    matches!(e, mysql_async::Error::Server(s) if s.code == 1062)
}
