use crate::helper::config::CONFIG;
use mysql_async::prelude::*;
use mysql_async::{Conn, Opts, OptsBuilder, Pool, PoolConstraints, PoolOpts};
use once_cell::sync::OnceCell;

pub type DbResult<T> = Result<T, mysql_async::Error>;

static DB_POOL: OnceCell<Pool> = OnceCell::new();

fn get_pool() -> &'static Pool {
    DB_POOL.get_or_init(|| {
        let constraints = PoolConstraints::new(2, CONFIG.db_pool_max.max(2)).unwrap();
        let opts = OptsBuilder::default()
            .ip_or_hostname(CONFIG.db_host.clone())
            .tcp_port(CONFIG.db_port)
            .db_name(Some(CONFIG.db_name.clone()))
            .user(Some(CONFIG.db_user.clone()))
            .pass(Some(CONFIG.db_password.clone()))
            .pool_opts(PoolOpts::default().with_constraints(constraints))
            .stmt_cache_size(256);

        Pool::new(Opts::from(opts))
    })
}

pub async fn get_conn() -> DbResult<Conn> {
    get_pool().get_conn().await
}

pub async fn check_db_is_up() -> bool {
    match get_conn().await {
        Ok(mut conn) => conn.query_drop("SELECT 1").await.is_ok(),
        Err(_) => false,
    }
}
