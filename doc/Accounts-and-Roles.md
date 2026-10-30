# Accounts & roles

## Roles

| Role | Can |
| --- | --- |
| `player` | Play challenges and events, start labs. |
| `author` | Everything a player can, plus write, test and publish **their own** challenges and manage events. Authors are kept off the practice scoreboard. |
| `admin` | Everything: all challenges and events, users, labs, audit log, settings. |

## First run

A platform with no account shows only `/setup`. The first account created there is an
admin. The creation locks the platform row, so two simultaneous setups cannot both win.

## Registration

| Mode | Behaviour |
| --- | --- |
| open | Anyone signs up at `/auth/register`. |
| invite | Sign-up needs an invitation link (`/auth/register?invite=…`). An admin creates links in **Admin → Users** with a number of uses and a lifetime. |
| closed | No sign-up. Admins create accounts themselves. |

Email is optional. Usernames: 3 to 32 characters, letters, digits, `_ - .`, unique without
regard to case.

## Accounts created by an admin

**Admin → Users → Create an account** creates the account with a random password and
returns a one-time link (valid 72 hours) the person uses to choose a password. Live Hack
sends no email: the admin hands the link over.

## Lost password

There is no email reset. An admin opens **Admin → Users**, clicks **Reset link** and gives
the link to the player (single use, valid 30 minutes). Using it signs out every session of
that account.

## Sessions

- A session is a random 256-bit token in an `HttpOnly`, `SameSite=Lax` cookie (`Secure`
  over HTTPS). Only its SHA-256 is stored.
- Sessions last 30 days and are extended as the player comes back (at most one write per day).
- **Settings** lists a player's sessions and can sign out the others; changing the password
  does it automatically. Admins can sign a user out everywhere.
- A banned user's sessions stop working at once and their labs are stopped.

## Rate limits

Counters are kept in MySQL so every replica shares them.

| Action | Limit |
| --- | --- |
| Sign-in | 10 per minute per username, 300 per minute per IP (a classroom shares one address) |
| Sign-up | 100 per 10 minutes per IP |
| Reset link use | 20 per minute per IP |
| Flag submission | 10 per minute per challenge and 60 per minute per player |
| Joining a team | 10 per minute per player |
| Starting a lab | 5 per minute per player or team |
