# Live Hack

**An open source platform to host CTF challenges and hands-on labs.**

Live Hack is what a team, a school or a club installs to run its own Capture The Flag:
publish challenges, run time-boxed events solo or in teams, and give every player their
own lab instance to attack. It is a single Rust binary (actix-web) backed by MySQL, with
server-rendered pages and a few kilobytes of JavaScript.

This folder (`doc/`) is the source of the GitHub wiki: edit pages here, the wiki is
regenerated on every push (see [Development](Development#documentation)).

---

## What it does

| Area | In short |
| --- | --- |
| [Challenges](Challenges) | Catalogue with filters, markdown descriptions, files, static URLs or per-player labs; static, case-insensitive, regex or per-lab flags. |
| [Challenge format](Challenge-Format) | `challenge.yml` compatible with ctfcli: import a CTFd challenge repo as it is, export any challenge as a zip. |
| [Events & teams](Events-and-Teams) | Time-boxed CTFs with their own challenge set, teams with invite codes, scoreboard freeze, disqualification, CTFtime feed. |
| [Labs](Labs) | One container per player (or per team during an event), isolated network, resource limits, unique flags, automatic cleanup. |
| [Scoreboard](Scoreboard) | Practice and event rankings, category boards, first bloods, player profiles. |
| [Accounts & roles](Accounts-and-Roles) | First-run setup, open / invite-only / closed registration, admin-made accounts, reset links, sessions. |
| [Administration](Administration) | Dashboard, users, running labs with logs, audit log, settings. |
| [API](API) | Everything the pages do is a JSON call under `/api/v1`. |

## Running it

- **[Installation](Installation)** - `docker compose up`, first setup, production checklist, upgrades and backups.
- **[Configuration](Configuration)** - environment variables and platform settings.
- **[Security](Security)** - the security model and how to report a vulnerability.

## Under the hood

- **[Architecture](Architecture)** - request flow, templates, multi-tenancy, caching, background jobs, scaling.
- **[Development](Development)** - local setup, the end-to-end test suite, conventions, migrations.
