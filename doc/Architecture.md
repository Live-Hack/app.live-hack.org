# Architecture

```
            ┌────────────── app (actix-web, one binary) ──────────────┐
browser ──▶ │ /assets   static files, immutable when versioned        │
            │ /api/v1   JSON API  (api/v1/*)                          │──▶ MySQL
            │ /…        server-rendered pages (web/routes/*)          │
            │ background: sweeper, event closing, settings reload     │──▶ docker-proxy ──▶ Docker
            └─────────────────────────────────────────────────────────┘
```

## Code layout

| Path | Content |
| --- | --- |
| `src/web/` | Page dispatcher (`dispatch.rs`), layout helpers (`page.rs`), one module per area in `routes/`. |
| `src/api/v1/` | JSON API: router (`main.rs`), responses (`core.rs`), one module per area. |
| `src/models/` | SQL access per domain (users, sessions, challenges, solves, events, instances, scores…). |
| `src/helper/` | Config, DB pool, migrations, templates, markdown, crypto, rate limits, storage, labs, tenancy. |
| `html/` | Templates. `inject/` = shared blocks, `partials/` = fragments rendered from Rust. |
| `assets/` | `main.css` (the original Live Hack theme), `app.css` (components), `app.js` (all behaviour). |
| `migrations/` | Numbered SQL files applied once each at start-up. |
| `tests/` | `run.sh` + `e2e.py`, the end-to-end suite. |

## Request flow

1. The dispatcher (or the API router) builds a `Ctx`: tenant from the `Host` header, user
   from the session cookie (one indexed query), query parameters, client IP.
2. Routing is a `match` on path segments.
3. Handlers query MySQL through the models and render a template or return JSON.
4. Responses are compressed (brotli/gzip) and carry security headers.

## Templates

Files in `html/` are parsed **once at start-up** into segments. `{{name}}` is HTML-escaped,
`{{{name}}}` is inserted as is (only for HTML built by the server), `{{inject_x}}` includes
`html/inject/x.html`. Rendering is a single pass with no allocation per placeholder.
Escaping by default means a forgotten escape cannot become an XSS.

## Multi-tenancy

Every row belongs to a tenant (`tenant_uuid`): users, sessions, challenges, events,
settings, labs, audit. A tenant is one platform, selected by the request's `Host` header;
the tenant whose `host` is NULL is the default. Tenants and their settings are cached in
memory and reloaded every `SWEEP_SECONDS`. Sessions are bound to their tenant, so a cookie
from one platform is useless on another. Creating tenants is done in SQL for now.

## Scaling

The app keeps no state that matters in memory: sessions, rate limits, lab slots and
event closing live in MySQL, so several app replicas can run behind a load balancer.
In-memory caches (templates, tenants, 5-second rankings) are safe to have per replica.

- One Docker host runs all labs; a larger deployment would add a scheduler over several
  hosts or a Kubernetes backend (`helper/labs.rs` is the only place that talks to Docker).
- Challenge files are on a local volume; shared storage (NFS, or an S3 backend in
  `helper/storage.rs`) is needed with several replicas.

## Background jobs

Every `SWEEP_SECONDS`, on every replica (all jobs are idempotent):

- reload tenants and settings;
- close ended events (challenges back to practice or hidden), once per event;
- stop expired labs and labs of ended events, remove orphan containers and networks;
- every 20 runs: purge old rate-limit counters, expired sessions and used tokens.
