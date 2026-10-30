# Live Hack

Open source platform to host CTF challenges and hands-on labs: publish challenges, run events, give each player their own lab instance.

> Status: early development (phase 00, foundations). See the roadmap for what is planned.

## Run it locally

Requirements: Docker with the Compose plugin.

```bash
git clone <this repo> && cd app.live-hack.org
docker compose up --build
```

Then open http://localhost:8080. The health check is at `/healthz`.

To change ports or passwords, copy `.env.example` to `.env` and edit it.

## Develop without Docker for the app

```bash
docker compose up -d db
cargo run
```

The app reads its settings from environment variables (see `.env.example`); the defaults match the compose database.
Database migrations live in `migrations/` and are applied at startup.

## Project layout

| Path | Content |
|------|---------|
| `src/web` | HTML pages: dispatcher and routes |
| `src/api/v1` | JSON API under `/api/v1` |
| `src/assets` | Static file server for `/assets` |
| `src/helper` | Config, database, migrations, templating helpers |
| `html/` | Page templates; `{{inject_x}}` includes `html/inject/x.html` |
| `assets/` | CSS, JS, images |
| `migrations/` | Numbered SQL files, applied once each |

## License

GPL-3.0, see [LICENSE](LICENSE).
