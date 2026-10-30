# Configuration

## Environment variables

Every variable has a default that works with `docker compose up`.

| Variable | Default | Meaning |
| --- | --- | --- |
| `PORT` | `8080` | HTTP port (inside the container). |
| `BASE_URL` | `http://localhost:8080` | Public URL. `https://` turns on `Secure` cookies and absolute links. |
| `DB_HOST` / `DB_PORT` | `127.0.0.1` / `3306` | MySQL server. |
| `DB_NAME` / `DB_USER` / `DB_PASSWORD` | `livehack` | MySQL database and credentials. |
| `DB_POOL_MAX` | `32` | Maximum MySQL connections per app process. |
| `DATA_DIR` | `data` (`/app/data` in Docker) | Where challenge files are stored. |
| `MAX_UPLOAD_MB` | `200` | Largest file or zip an organizer can upload. |
| `DOCKER_HOST` | unset (`tcp://docker-proxy:2375` in compose) | Docker engine for [labs](Labs). Unset = labs disabled. |
| `INSTANCE_HOST` | host of `BASE_URL` | Host name shown to players for lab URLs. |
| `INSTANCE_BIND` | `0.0.0.0` | Host IP lab ports are published on. |
| `MAX_INSTANCES` | `50` | Labs running at the same time, all players together. |
| `LABS_NAMESPACE` | `livehack` (compose: project name) | Label separating the labs of several platforms on one Docker host. |
| `SWEEP_SECONDS` | `30` | Period of the background jobs (lab cleanup, end of events, settings reload). |
| `SECURITY_CONTACT` | unset | `Contact:` line of `/.well-known/security.txt`; unset = no file. |
| `RUST_LOG` | `info` | Log level filter. Logs are JSON lines on stdout. |

## Platform settings

Changed by admins in **Admin → Settings** (stored per platform, applied within seconds
on every replica):

| Setting | Values | Effect |
| --- | --- | --- |
| Platform name | text | Header, page titles, sign-in pages. |
| Flag format | e.g. `LH{...}` | Shown to players as the expected format. For per-lab flags, `...` is replaced by the random secret. |
| Registration | open / invite / closed | Who can create an account. See [Accounts & roles](Accounts-and-Roles#registration). |
| Scoreboard | visible / hidden | Hidden: players see neither the board nor other players' ranks; admins still do. |
| Labs | enabled / disabled | Pause lab starts without touching Docker. |
