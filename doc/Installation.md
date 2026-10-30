# Installation

Live Hack ships as a Docker image and a `docker-compose.yml` with three services:

| Service | Role |
| --- | --- |
| `app` | The platform (Rust binary, port 8080 in the container). |
| `db` | MySQL 8.4. Data in the `db-data` volume. |
| `docker-proxy` | [tecnativa/docker-socket-proxy](https://github.com/Tecnativa/docker-socket-proxy): gives the app the container, network and image APIs of the host's Docker, nothing else. Only needed for [labs](Labs). |

Challenge files live in the `app-data` volume (`/app/data`).

## Quick start

```bash
git clone <this repository> live-hack && cd live-hack
docker compose up -d --build
```

Open http://localhost:8080. An empty platform shows the **setup page**: choose the
platform name and create the administrator account. Then:

1. Pick a registration mode in **Admin → Settings** (open, invitation link only, or closed).
2. Create challenges in **Manage challenges**, or import a zip of `challenge.yml` folders
   (see [Challenge format](Challenge-Format)).
3. Optionally schedule an [event](Events-and-Teams).

Health check: `GET /healthz` answers `ok` when the database is reachable.

## Production checklist

- **Passwords**: copy `.env.example` to `.env` and set `DB_PASSWORD`.
- **Public URL**: set `BASE_URL=https://ctf.example.org`. When it starts with `https://`,
  session cookies get the `Secure` flag.
- **TLS**: put a reverse proxy (Caddy, Traefik, nginx) in front of port 8080. Keep the
  `Host` header: it selects the platform ([multi-tenancy](Architecture#multi-tenancy)).
- **Labs**: set `INSTANCE_HOST` to the public name players use to reach lab ports, and
  open the ephemeral port range (32768-60999 on Linux by default) on the firewall of the
  Docker host. See [Labs](Labs#operations).
- **Security contact**: set `SECURITY_CONTACT=mailto:security@example.org` to publish
  `/.well-known/security.txt`.
- **Uploads**: `MAX_UPLOAD_MB` (default 200) and the body size limit of your reverse proxy
  must agree.

## Upgrades

```bash
git pull
docker compose up -d --build
```

Migrations in `migrations/` run automatically at start-up, once each, in order. A failing
migration stops the app with the error in the logs (MySQL DDL is not transactional: fix
the cause, then restart).

## Backups

Two things hold state: the database and the files volume.

```bash
docker compose exec db sh -c 'mysqldump -u"$MYSQL_USER" -p"$MYSQL_PASSWORD" "$MYSQL_DATABASE"' > livehack.sql
docker run --rm -v live-hack_app-data:/data -v "$PWD":/out busybox tar czf /out/files.tgz -C /data .
```

(The volume name is prefixed with your compose project name.)

## Without Docker for the app

```bash
docker compose up -d db
cargo run --release
```

The binary reads its configuration from environment variables; the defaults match the
compose database on `127.0.0.1:3306`. Run it from the repository root: it loads `html/`,
`assets/`, `utils/` and `migrations/` from the working directory.
