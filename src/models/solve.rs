use crate::helper::crypto::{ct_eq, sha256};
use crate::helper::database::{DbResult, get_conn};
use crate::models::is_duplicate;
use mysql_async::prelude::*;
use regex::RegexBuilder;

/// Who a solve counts for: practice (no event) or an event, as a user or a team.
#[derive(Debug, Clone, Default)]
pub struct Scope {
    pub event_uuid: Option<String>,
    pub team_uuid: Option<String>,
}

impl Scope {
    pub fn event_key(&self) -> &str {
        self.event_uuid.as_deref().unwrap_or("")
    }
    pub fn scope_key<'a>(&'a self, user: &'a str) -> &'a str {
        self.team_uuid.as_deref().unwrap_or(user)
    }
}

/// Compares a submitted flag with the expected one. Whitespace around the value is ignored.
pub fn check_flag(flag_type: &str, expected: &str, submitted: &str) -> bool {
    let got = submitted.trim();
    let want = expected.trim();
    if got.is_empty() || want.is_empty() {
        return false;
    }
    match flag_type {
        "case_insensitive" => ct_eq(
            got.to_lowercase().as_bytes(),
            want.to_lowercase().as_bytes(),
        ),
        "regex" => RegexBuilder::new(&format!("^(?:{want})$"))
            .size_limit(1 << 20)
            .build()
            .map(|re| re.is_match(got))
            .unwrap_or(false),
        // static and per_instance (the per-instance value is passed as `expected`)
        _ => ct_eq(got.as_bytes(), want.as_bytes()),
    }
}

pub async fn record_submission(
    tenant: &str,
    chall: &str,
    user: &str,
    scope: &Scope,
    value: &str,
    correct: bool,
    ip: &str,
) -> DbResult<()> {
    get_conn()
        .await?
        .exec_drop(
            "INSERT INTO challs_submissions (tenant_uuid, chall_uuid, user_uuid, team_uuid, event_uuid, value_hash, correct, ip)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            (tenant, chall, user, &scope.team_uuid, &scope.event_uuid, sha256(value.trim()), correct, ip),
        )
        .await
}

/// Records a solve. Returns false if this scope had already solved it (unique key).
pub async fn record_solve(tenant: &str, chall: &str, user: &str, scope: &Scope) -> DbResult<bool> {
    let res = get_conn()
        .await?
        .exec_drop(
            "INSERT INTO challs_solved (tenant_uuid, chall_uuid, user_uuid, team_uuid, event_uuid) VALUES (?, ?, ?, ?, ?)",
            (tenant, chall, user, &scope.team_uuid, &scope.event_uuid),
        )
        .await;
    match res {
        Ok(()) => Ok(true),
        Err(e) if is_duplicate(&e) => Ok(false),
        Err(e) => Err(e),
    }
}

pub async fn solve_count(chall: &str, event_key: &str) -> DbResult<i64> {
    Ok(get_conn()
        .await?
        .exec_first(
            "SELECT COUNT(*) FROM challs_solved WHERE chall_uuid = ? AND event_key = ?",
            (chall, event_key),
        )
        .await?
        .unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::check_flag;

    #[test]
    fn flags() {
        assert!(check_flag("static", "LH{abc}", " LH{abc}\n"));
        assert!(!check_flag("static", "LH{abc}", "lh{abc}"));
        assert!(check_flag("case_insensitive", "LH{AbC}", "lh{abc}"));
        assert!(check_flag("regex", r"LH\{[0-9]{3}\}", "LH{123}"));
        assert!(!check_flag("regex", r"LH\{[0-9]{3}\}", "LH{123}x"));
        assert!(!check_flag("regex", r"LH\{[0-9]{3}\}", "xLH{123}"));
        assert!(!check_flag("regex", "(", "("));
        assert!(!check_flag("static", "", ""));
    }
}
