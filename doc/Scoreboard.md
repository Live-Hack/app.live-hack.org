# Scoreboard

## Rules

- Score = sum of the points of the challenges solved (event-specific points during an event).
- Ties go to whoever reached the score first (earliest last solve).
- The first solver of a challenge gets a **first blood** badge.
- Practice ranks players with the `player` role; authors and admins know the flags and stay
  off it. Banned players and disqualified teams are removed.

## Boards

| Board | Where |
| --- | --- |
| Practice, all categories or one | `/scoreboard` |
| Event (players or teams), frozen for players after the freeze time | `/events/<slug>/scoreboard` |
| Top 10 | Home dashboard and event pages |

Player profiles (`/users/<name>`) show rank, points, solves, per-category progress and the
solved challenges.

## Performance

A ranking is one SQL query over the solves, cached in memory for 5 seconds per scope
(practice or event, category, freeze). A scoreboard refreshed by hundreds of players
during an event costs one query every 5 seconds per replica, and new solves show up
within those 5 seconds.

Admins can hide the scoreboard from players in **Admin → Settings**.
