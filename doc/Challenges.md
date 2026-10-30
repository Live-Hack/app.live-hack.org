# Challenges

## Types

| Type | Players get |
| --- | --- |
| `file` | Files to download (stored on the platform, SHA-256 shown). |
| `static` | A shared URL (`http://` or `https://` only), e.g. a service the organizers run themselves. |
| `instance` | A private [lab](Labs) started on demand. |

A challenge also has a name, a markdown description, a category (free text, lower case;
`crypto`, `forensics`, `misc`, `osint`, `pwn`, `reverse`, `web` are suggested and coloured),
a difficulty (`easy`, `medium`, `hard`, `insane`) and points.

## Flags

| Check | Accepts |
| --- | --- |
| static | Exactly the flag (surrounding spaces ignored). |
| case_insensitive | The flag in any case. |
| regex | A full match of the regex (anchored, size-limited). |
| per_instance | The flag of the player's own lab, unique per lab. A flag copied from another player's lab is refused and reported as a *shared flag* to admins. |

Comparisons are constant-time. Every attempt is logged with the SHA-256 of the value
(never the value itself), the IP and the time; admins see them per player.

## Life cycle

`draft` → `published` → `archived`. Only published challenges are visible to players.
A challenge cannot be published without a flag (except per-lab flags), an online
challenge without its URL, or a lab without its image and port. Every change is recorded
in the audit log.

**Testing**: authors and admins open the player page of a draft (**Preview & test flag**)
and submit flags; the answer says whether it is right without recording anything.

## Practice and events

A published challenge appears in the permanent **practice** catalogue unless it belongs
to an upcoming or live [event](Events-and-Teams): adding it to such an event takes it out
of practice until the event ends. A solve counts for the live event the player takes part
in, otherwise for practice.

## Editing

**Manage challenges** (authors: their own, admins: all):

- details with a live markdown preview; an *unsaved changes* marker warns before leaving;
- files: drag and drop, uploaded as a raw stream (no size spike in memory), name reduced
  to its last path component;
- lab settings when the type is `instance`;
- co-authors (by username); only the owner or an admin can delete;
- **Export zip** / **Import zip** in [challenge.yml format](Challenge-Format).

Markdown is rendered safely: raw HTML is dropped and `javascript:` links are neutralised.
