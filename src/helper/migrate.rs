use crate::helper::database::get_conn;
use crate::helper::trace::trace_logs;
use mysql_async::prelude::*;
use std::fs;

const MIGRATIONS_DIR: &str = "migrations";

/// Applies every `migrations/NNN_name.sql` not yet listed in `schema_migrations`, in name order.
/// MySQL DDL is not transactional: a failing file stops startup so the admin can fix it by hand.
pub async fn run_migrations() -> Result<(), String> {
    let mut conn = get_conn().await.map_err(|e| e.to_string())?;

    conn.query_drop(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            name VARCHAR(255) PRIMARY KEY,
            applied_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .await
    .map_err(|e| e.to_string())?;

    let applied: Vec<String> = conn
        .query("SELECT name FROM schema_migrations")
        .await
        .map_err(|e| e.to_string())?;

    for name in pending(list_files()?, &applied) {
        let sql = fs::read_to_string(format!("{}/{}", MIGRATIONS_DIR, name))
            .map_err(|e| e.to_string())?;
        conn.query_drop(sql)
            .await
            .map_err(|e| format!("{}: {}", name, e))?;
        conn.exec_drop(
            "INSERT INTO schema_migrations (name) VALUES (?)",
            (name.clone(),),
        )
        .await
        .map_err(|e| e.to_string())?;
        trace_logs(format!("Migration applied: {}", name));
    }
    Ok(())
}

fn list_files() -> Result<Vec<String>, String> {
    let entries = fs::read_dir(MIGRATIONS_DIR).map_err(|e| format!("{}: {}", MIGRATIONS_DIR, e))?;
    Ok(entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".sql"))
        .collect())
}

fn pending(mut files: Vec<String>, applied: &[String]) -> Vec<String> {
    files.sort();
    files.retain(|f| !applied.contains(f));
    files
}

#[cfg(test)]
mod tests {
    use super::pending;

    #[test]
    fn pending_is_sorted_and_skips_applied() {
        let files = vec!["002_b.sql".into(), "001_a.sql".into(), "003_c.sql".into()];
        let applied = vec!["002_b.sql".to_string()];
        assert_eq!(pending(files, &applied), vec!["001_a.sql", "003_c.sql"]);
    }
}
