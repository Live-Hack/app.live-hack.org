# Development

## Setup

```bash
docker compose up -d db            # MySQL only
cargo run                          # the app on http://localhost:8080
```

The app reloads nothing on the fly: restart it after changing templates or assets.

## Checks

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                         # unit tests
tests/run.sh                       # end-to-end suite on a fresh instance
```

`tests/run.sh` builds the images, starts a **fresh and empty** stack under its own compose
project (`lhtest`, port 18080, its own volumes), runs `tests/e2e.py` against it, and tears
it down (`KEEP=1` keeps it running for a look). The suite drives the platform like a
browser and an API client: setup, accounts, challenges, flags, imports, scoreboards,
events, real lab containers, admin screens and multi-tenant isolation — over 300 checks.
Every change runs the whole suite, so regressions in earlier areas show up.

`tests/e2e.py` needs only Python's standard library and the Docker CLI.

## Conventions

- **SQL**: parameters only (`params!`, `?`), never a value formatted into a query. Every
  query filters on `tenant_uuid`.
- **Templates**: `{{x}}` for anything that may come from a user; `{{{x}}}` only for HTML
  built in Rust from escaped parts.
- **Schema changes**: a new `migrations/NNN_name.sql`; never edit a released migration.
- **API**: writes are `POST` with a JSON body; return `ok(json!)` / `Err(bad(...))`.
- **Front**: behaviour goes in `assets/js/app.js` through `data-` attributes; styles in
  `assets/css/app.css`, built on the tokens of `main.css`.
- **Logs**: never log tokens, passwords or flags.

## Adding a page

1. Template in `html/<area>/<page>.html` with `{{inject_head}}`, `{{inject_header}}`,
   `{{inject_footer}}`.
2. Handler in `src/web/routes/<area>.rs` returning `page(ctx, "<area>/<page>.html", title, vars)`.
3. A line in the `match` of `src/web/dispatch.rs`.
4. Checks in `tests/e2e.py`.

## Documentation

This `doc/` folder is the single source of the GitHub wiki. The workflow
`.github/workflows/wiki-sync.yml` mirrors it to `<repo>.wiki.git` on every push to `main`
that touches `doc/` (the wiki must have been initialised once by creating any page in the
GitHub UI). Do not edit the wiki in the browser: the next sync overwrites it.

Pages are `Title-With-Dashes.md`, linked without extension: `[Labs](Labs)`.
`_Sidebar.md` is the navigation.
