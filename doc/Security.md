# Security

## Reporting a vulnerability

Use GitHub's **Report a vulnerability** button on the repository (Security tab). Do not
open a public issue. Each running platform is operated by its host: for a problem with a
deployment, use its `/.well-known/security.txt`.

## Model

| Area | Measure |
| --- | --- |
| Passwords | Argon2id, computed off the async workers; unknown usernames take the same time as wrong passwords. |
| Sessions | 256-bit random tokens, only their SHA-256 stored; `HttpOnly`, `SameSite=Lax`, `Secure` over HTTPS; revocable. |
| CSRF | Writes require `X-Requested-With: fetch` and a same-host `Origin`; no CORS. |
| XSS | Templates escape by default; markdown drops raw HTML and unsafe links; URLs in challenges must be http(s). |
| SQL | Parameterised queries only (`mysql_async`). |
| Brute force | MySQL-backed rate limits on sign-in, sign-up, reset, flags, team joins, lab starts. |
| Files | Login required; storage keys are generated; names cleaned; size capped; zip imports confined. |
| Redirects | Only same-site relative paths are accepted after sign-in. |
| Headers | `X-Frame-Options: DENY`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: same-origin`. |
| Flags | Stored for checking but never sent to players; attempts logged as SHA-256; per-lab flags detect sharing. |
| Labs | Docker reached through a filtered proxy; per-lab network, resource limits, dropped capabilities, `no-new-privileges`. |
| Audit | Account, challenge, event, lab and settings changes are recorded with their author. |

## What the host must do

- Serve the platform over HTTPS and set `BASE_URL` accordingly.
- Change `DB_PASSWORD`.
- Keep the Docker host dedicated to labs where possible: lab images are written by
  organizers and run with internet access by default.
