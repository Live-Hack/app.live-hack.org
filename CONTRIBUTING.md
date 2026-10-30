# Contributing

1. Fork, create a branch, open a pull request against `main`.
2. Before pushing, run:

   ```bash
   cargo fmt
   cargo clippy --all-targets -- -D warnings
   cargo test
   ```

   Tests need the database: `docker compose up -d db`.

## Rules for the code

- SQL: always use `mysql_async` parameters (`params!` or `?`), never `format!` a value into a query.
- Templates: any value that comes from a user goes through `replace_text` (HTML-escaped). `replace_html` is only for trusted fragments.
- Schema changes: add a new `migrations/NNN_name.sql`; never edit a migration that has been released.
- Logs: never log session tokens, passwords or flags.

Security issues: see [SECURITY.md](SECURITY.md), do not open a public issue.
