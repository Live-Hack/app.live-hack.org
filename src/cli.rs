//! Command line: `app_live_hack import <dir> [--tenant <slug>] [--author <username>] [--publish]`
use crate::helper::bundle::{self, Source};
use crate::helper::database::get_conn;
use mysql_async::prelude::*;

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

/// Returns Some(exit code) when a command ran, None to start the web server.
pub async fn run(args: &[String]) -> Option<i32> {
    match args.get(1).map(String::as_str) {
        Some("import") => Some(import(args).await),
        Some("help" | "--help" | "-h") => {
            println!(
                "usage:\n  app_live_hack                 start the web server\n  app_live_hack import <dir> [--tenant <slug>] [--author <username>] [--publish]"
            );
            Some(0)
        }
        _ => None,
    }
}

async fn import(args: &[String]) -> i32 {
    let Some(dir) = args.get(2).filter(|a| !a.starts_with("--")) else {
        eprintln!(
            "usage: app_live_hack import <dir> [--tenant <slug>] [--author <username>] [--publish]"
        );
        return 2;
    };
    let tenant_slug = flag(args, "--tenant").unwrap_or_else(|| "default".into());
    let publish = args.iter().any(|a| a == "--publish");
    let mut conn = match get_conn().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("database: {e}");
            return 1;
        }
    };
    let tenant: Option<String> = conn
        .exec_first("SELECT uuid FROM tenants WHERE slug = ?", (&tenant_slug,))
        .await
        .unwrap_or(None);
    let Some(tenant) = tenant else {
        eprintln!("unknown tenant '{tenant_slug}'");
        return 1;
    };
    let author: Option<String> = match flag(args, "--author") {
        Some(name) => conn.exec_first("SELECT uuid FROM users WHERE tenant_uuid = ? AND username = ?", (&tenant, name)).await.unwrap_or(None),
        None => conn
            .exec_first("SELECT uuid FROM users WHERE tenant_uuid = ? AND role = 'admin' ORDER BY created_at LIMIT 1", (&tenant,))
            .await
            .unwrap_or(None),
    };
    let Some(author) = author else {
        eprintln!(
            "no author: create the admin account first (open the site), or pass --author <username>"
        );
        return 1;
    };
    let mut src = Source::Dir(dir.into());
    match bundle::import(&tenant, &author, &mut src, publish, async |_: &str| true).await {
        Ok(list) => {
            for i in &list {
                println!(
                    "{} {} ({} file(s))",
                    if i.created { "created" } else { "updated" },
                    i.slug,
                    i.files
                );
            }
            println!("{} challenge(s) imported", list.len());
            0
        }
        Err(e) => {
            eprintln!("import failed: {e}");
            1
        }
    }
}
