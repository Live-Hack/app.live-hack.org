# Contributing

1. Fork, create a branch, open a pull request against `main`.
2. Before pushing, run:

   ```bash
   cargo fmt
   cargo clippy --all-targets -- -D warnings
   cargo test
   ```

   Then the end-to-end suite, which starts a fresh instance in Docker and checks every
   area for regressions:

   ```bash
   tests/run.sh
   ```

3. Update `doc/` when behaviour changes: it is the GitHub wiki (synced on push to `main`).

## Rules for the code

- SQL: always use `mysql_async` parameters (`params!` or `?`), never `format!` a value into a query.
- Templates: `{{x}}` (HTML-escaped) for any value that may come from a user; `{{{x}}}` only for HTML built in Rust from escaped parts.
- Schema changes: add a new `migrations/NNN_name.sql`; never edit a migration that has been released.
- Logs: never log session tokens, passwords or flags.
- Multi-tenancy: every query filters on `tenant_uuid`.
- Front: no new JavaScript dependency; behaviour goes in `assets/js/app.js` through `data-` attributes.

Security issues: see [SECURITY.md](SECURITY.md), do not open a public issue.
