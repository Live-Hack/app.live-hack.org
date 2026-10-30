# Administration

All admin screens are under `/admin` (admins only; other roles get a 404).

| Screen | What you do there |
| --- | --- |
| Overview | Players, organizers, published challenges and drafts, running labs, solves and attempts over 24 hours, **shared flags** (red when any), solves per hour, live activity. |
| Users | Search; create accounts (with a set-password link); invitation links; change roles inline; ban / unban; reset links. |
| User page | Every flag attempt of the user (challenge, result, practice or event, IP, time); sign out everywhere. |
| Challenges | Same as **Manage challenges**, all authors' challenges. |
| Events | Create and edit events, pick challenges, disqualify teams. |
| Labs | Running labs, their logs, stop. |
| Audit log | Who did what: accounts, challenges, events, labs, shared flags, settings; filterable. |
| Settings | Platform name, flag format, registration, scoreboard visibility, labs pause. |

Admins cannot demote or ban themselves, so a platform always keeps an admin. Banning a user
signs them out and stops their labs.
