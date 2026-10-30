# Events & teams

An event is a time-boxed CTF with its own challenge set and scoreboard, next to the
permanent practice area.

## Settings

| Field | Meaning |
| --- | --- |
| Start / end | Shown in each viewer's time zone, stored in UTC. |
| Freeze | Optional. From this time until the end, players see the scoreboard as it was; organizers see it live. |
| Mode | Solo, or teams with a maximum size. |
| Registration | open, with a code, or closed. |
| After the end | Move the challenges to practice, or hide them. |
| Visibility | Draft (organizers only) or published. |

Challenges are added from the event editor, optionally with event-specific points.

## Phases

| Phase | Players see |
| --- | --- |
| Upcoming | Description, countdown, registration. |
| Live | Challenges (registered players only), countdown to the end, top 10. A solve counts for the event. |
| Ended | Final scoreboard; challenges are back in practice (or hidden), labs of the event are stopped. |

The end-of-event job runs in the background on every replica and only once per event.

## Teams

- A player creates a team (name unique in the event) and becomes captain, or joins one with
  its 16-character invite code (case-insensitive, no look-alike characters).
- One team per player per event (a database key guarantees it). Joins are serialised per
  team, so a team cannot exceed its size.
- The captain can remove members. When the captain leaves, the oldest member takes over;
  the last member leaving deletes the team.
- **Once a team has scored, its members can no longer leave or be removed**, so points
  cannot follow a player to another team.
- A solve is shared by the team: the first member to submit the flag scores for everyone.

## Fair play

- Admins disqualify a team (or a solo player) from the event editor: it disappears from the
  scoreboard and can no longer submit. Requalifying restores it.
- Per-lab flags make flag sharing visible: a flag from another team's lab is refused and
  logged as a *shared flag*.

## CTFtime

`/api/v1/events/<slug>/ctftime` returns the standings in CTFtime's format:

```json
{"standings": [{"pos": 1, "team": "Red Team", "score": 1337}]}
```

It is public once the event has ended (CTFtime fetches it without a session); organizers
can read it at any time.
