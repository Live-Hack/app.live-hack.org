# API

Every page works on top of a JSON API under `/api/v1`. It uses the same session cookie as
the site: sign in through `/api/v1/auth/login` and keep the cookie.

## Conventions

- Responses are JSON. Errors are `{"error": "message"}` with a 4xx/5xx status.
- **Writes are `POST`** and must carry `X-Requested-With: fetch`; a request with an `Origin`
  header from another host is refused. This is the CSRF protection: browsers cannot send
  that header cross-site without a CORS preflight, and Live Hack allows no CORS.
- File uploads and zip imports send the **raw file as the body**
  (`Content-Type: application/octet-stream`), not multipart.

```bash
B=https://ctf.example.org/api/v1
curl -c jar -H 'X-Requested-With: fetch' -H 'Content-Type: application/json' \
     -d '{"login":"alice","password":"…"}' $B/auth/login
curl -b jar "$B/challenges?category=web&state=unsolved"
curl -b jar -H 'X-Requested-With: fetch' -H 'Content-Type: application/json' \
     -d '{"flag":"LH{...}"}' $B/challenges/<uuid>/submit
```

## Endpoints

### Accounts

| Method | Path | Body / query |
| --- | --- | --- |
| GET | `/me` | |
| POST | `/setup` | `username`, `password`, `site_name` (empty platform only) |
| POST | `/auth/login` | `login` (username or email), `password` |
| POST | `/auth/register` | `username`, `password`, `email?`, `invite?` |
| POST | `/auth/logout` | |
| POST | `/auth/reset` | `token`, `password` |
| GET | `/account/sessions` | |
| POST | `/account/username` | `username` |
| POST | `/account/password` | `current`, `password` |
| POST | `/account/sessions/revoke` | |

### Playing

| Method | Path | Body / query |
| --- | --- | --- |
| GET | `/challenges` | `q`, `category`, `difficulty`, `state` (solved/unsolved), `type`, `sort`, `page`, `per_page` |
| POST | `/challenges/{uuid}/submit` | `flag` → `status`: correct / wrong / already_solved |
| GET | `/challenges/{uuid}/instance` | lab state |
| POST | `/challenges/{uuid}/instance` | start a lab |
| POST | `/challenges/{uuid}/instance/extend` | |
| POST | `/challenges/{uuid}/instance/stop` | |
| GET | `/scoreboard` | `category` |
| GET | `/users/{username}` | |
| POST | `/events/{slug}/register` | `code?` (solo events) |
| POST | `/events/{slug}/teams` | `name`, `code?` |
| POST | `/events/{slug}/teams/join` | `invite` |
| POST | `/events/{slug}/teams/leave` | |
| POST | `/events/{slug}/teams/kick` | `username` (captain) |
| GET | `/events/{slug}/scoreboard` | |
| GET | `/events/{slug}/ctftime` | public after the end |

### Organizers (authors and admins)

| Method | Path | Body |
| --- | --- | --- |
| POST | `/manage/challenges` | `name` → new draft |
| POST | `/manage/challenges/{uuid}` | `name`, `description`, `category`, `difficulty`, `points`, `ctype`, `static_url`, `flag_type`, `flag` |
| POST | `/manage/challenges/{uuid}/status` | `status`: draft / published / archived |
| POST | `/manage/challenges/{uuid}/runtime` | `image`, `port`, `protocol`, `cpu_millis`, `memory_mb`, `ttl_minutes`, `env`, `readonly_rootfs` |
| POST | `/manage/challenges/{uuid}/files?name=x` | raw file |
| POST | `/manage/challenges/{uuid}/files/{file}/delete` | |
| POST | `/manage/challenges/{uuid}/authors` | `username` |
| POST | `/manage/challenges/{uuid}/authors/{username}/remove` | |
| POST | `/manage/challenges/{uuid}/delete` | owner or admin |
| GET | `/manage/challenges/{uuid}/export` | zip |
| POST | `/manage/import` | raw zip |
| POST | `/manage/preview` | `markdown` → `html` |
| POST | `/manage/events` | `title`, `description`, `starts_at`, `ends_at`, `freeze_at`, `team_mode`, `max_team_size`, `registration`, `join_code`, `after_end`, `status` |
| POST | `/manage/events/{uuid}` | same |
| POST | `/manage/events/{uuid}/challenges` | `chall`, `points?` |
| POST | `/manage/events/{uuid}/challenges/{chall}/remove` | |
| POST | `/manage/events/{uuid}/disqualify/{team-or-user}` | `disqualified` (admin) |
| POST | `/manage/events/{uuid}/delete` | admin |

Times are ISO 8601 in UTC (`2026-11-02T18:00:00Z`).

### Admins

| Method | Path | Body |
| --- | --- | --- |
| GET | `/admin/stats` | |
| GET / POST | `/admin/settings` | `site_name`, `flag_format`, `registration`, `scoreboard`, `labs` |
| POST | `/admin/users` | `username`, `email?`, `role` → set-password link |
| POST | `/admin/invites` | `uses`, `hours` → link |
| POST | `/admin/users/{uuid}/role` | `role` |
| POST | `/admin/users/{uuid}/ban` | `banned` |
| POST | `/admin/users/{uuid}/reset-link` | |
| POST | `/admin/users/{uuid}/sessions/revoke` | |
| GET | `/admin/labs/{uuid}/logs` | |
| POST | `/admin/labs/{uuid}/stop` | |
