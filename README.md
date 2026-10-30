# Live Hack

Open source platform to host CTF challenges and hands-on labs: publish challenges, run
time-boxed events solo or in teams, and give every player their own lab to attack.

- Challenges with files, URLs or per-player labs; static, case-insensitive, regex or per-lab flags
- `challenge.yml` import/export compatible with ctfcli (CTFd challenge repos import as they are)
- Events with teams, invite codes, scoreboard freeze, disqualification and a CTFtime feed
- Labs: one container per player or team, isolated network, limits, unique flags, auto cleanup
- Admin dashboard, audit log, multi-tenant from the ground up
- Rust (actix-web) + MySQL, server-rendered pages, ~7 KB of compressed JavaScript

## Quick start

Requirements: Docker with the Compose plugin.

```bash
git clone <this repository> live-hack && cd live-hack
docker compose up -d --build
```

Open http://localhost:8080 and create the admin account. Try the example lab:

```bash
docker build -t livehack/example-web:latest examples/labs/web-hello
docker compose exec app ./app_live_hack import examples/labs --publish
```

## Documentation

Everything is in [`doc/`](doc/Home.md), mirrored to the GitHub wiki:
[installation](doc/Installation.md), [configuration](doc/Configuration.md),
[challenges](doc/Challenges.md), [challenge format](doc/Challenge-Format.md),
[events](doc/Events-and-Teams.md), [labs](doc/Labs.md), [API](doc/API.md),
[architecture](doc/Architecture.md), [security](doc/Security.md),
[development](doc/Development.md).

## Tests

```bash
cargo test          # unit tests
tests/run.sh        # 300+ end-to-end checks against a fresh Docker instance
```

## License

GPL-3.0, see [LICENSE](LICENSE).
