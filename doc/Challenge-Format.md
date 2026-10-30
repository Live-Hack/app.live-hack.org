# Challenge format

Challenges travel as folders containing a `challenge.yml` and their files. The format is
the one of [ctfcli](https://github.com/CTFd/ctfcli), so an existing CTFd challenge
repository imports without changes. Live Hack reads the fields below and ignores the rest.

```yaml
name: Cookie Monster            # required; the slug is derived from it
author: alice                   # informational
category: web
description: |
  Eat **all** the cookies. Markdown.
value: 250                      # points (alias: points); default 100
difficulty: medium              # Live Hack extension: easy | medium | hard | insane
type: static                    # file | static | instance (ctfcli's standard/dynamic = file)
connection_info: https://cookies.example.org   # static URL (also implies type static)
flags:
  - LH{om_nom}                                  # plain string = static
  - {type: static, content: "LH{om_nom}", data: case_insensitive}
  - {type: regex, content: "LH\\{[0-9]{4}\\}"}
  - {type: per_instance, content: ""}           # unique flag per lab
files:
  - dist/cookies.txt            # relative to this challenge.yml
state: visible                  # visible = publish, hidden = draft; absent = keep / draft
instance:                       # Live Hack extension, for type: instance
  image: registry.example.org/cookies:1.0
  port: 8080
  protocol: http                # http | tcp
  cpu: 500                      # millicores
  memory: 256                   # MB
  ttl: 60                       # minutes
  readonly: false
  env: {MODE: hard}
```

Only the **first** flag is used.

## Importing

- **Web**: Manage challenges → **Import zip**. The zip may contain any number of folders,
  each with a `challenge.yml`.
- **Command line** (from the server, e.g. `docker compose exec app`):

  ```bash
  ./app_live_hack import /path/to/challenges --publish
  ./app_live_hack import /path/to/challenges --tenant default --author alice
  ```

Import is an **upsert by name**: a challenge whose slug already exists is updated in
place, keeping its solves, and its files are replaced. Authors can only overwrite their
own challenges. Paths are confined to the bundle (`..` and absolute paths are refused),
and a bundle with an error changes nothing.

## Exporting

**Export zip** on a challenge gives `<slug>/challenge.yml` and `<slug>/dist/<files>`,
ready to commit to a repository or import elsewhere.
