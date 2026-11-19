# GDPR notes

What personal data DaiBX holds, why, where, for how long, and how it's deleted. This covers everything stored through milestone 5d (api v1.6.0): accounts and sign-in, tasks, the board, the calendar, time tracking, schedules, sign-in alerts and backups. The final terms and privacy policy are written in milestone 12. What to do if data is lost or exposed is in `breach-procedure.md`.

## Who holds the data

DaiBX is run by its owner, Shan Padayhag, as an invite-only personal service. The owner is the controller and the data protection officer. The owner never reads anyone's content except at that person's request. The services below process data on the owner's behalf under their standard data processing terms.

| Service | What it does | Where |
| --- | --- | --- |
| Supabase (Postgres) | stores everything listed below | Singapore (ap-southeast-1) |
| AWS Lambda | runs the API and the minute scheduler; keeps their logs in CloudWatch | Singapore (ap-southeast-1) |
| AWS S3 | keeps the nightly encrypted database backups | Tokyo (ap-northeast-1), so a regional outage can't take the database and its backups together |
| GitHub Actions | runs the nightly backup job, which reads the database and uploads the encrypted file | GitHub's runners; the job keeps no copy |
| Vercel | serves the web app and forwards `/backend` requests to the API | its global edge network; requests pass through and Vercel keeps its own short-lived request logs |
| Google (Gmail SMTP) | sends every DaiBX email: verification, reset and email-change links, and sign-in alerts | Google's infrastructure; it stays the sender while DaiBX has no domain of its own |
| Have I Been Pwned | checks new passwords against known breaches | receives only the first 5 characters of the password's SHA-1, padded, never the password or any account detail |

## What's stored, and why

Data minimisation: each item exists because a feature needs it, and nothing is collected for analytics.

| Data | Purpose | Lawful basis |
| --- | --- | --- |
| Name and email | identifying the account, signing in, sending account links | contract |
| Password | stored only as an Argon2id hash | contract |
| Authenticator secret | checking sign-in codes; encrypted with AES-256-GCM, tied to the account id | contract |
| Recovery codes | signing in without the phone; stored only as Argon2id hashes, shown once | contract |
| Accent colour and theme | the person's own display settings | contract |
| Tasks, task groups and categories: titles, notes, dates and order | the task list the person keeps | contract |
| The board: boards, lists, cards with titles and notes, properties the person defines and their values, archived cards, and each card's move history | the board the person keeps | contract |
| The calendar: events with titles, notes, times, repeats and their exceptions | the calendar the person keeps | contract |
| Time tracking: when a task or card was started, paused and finished, kept as calendar events | showing where the person's time went | contract |
| Card schedules, card triggers and group schedules, with a log of each run | creating and moving cards on the person's schedule | contract |
| Terms version and when it was shown | recording what the person was shown at sign-up (`users.terms_version`, `users.terms_presented_at`) | accountability: being able to show what each person saw |
| Sessions: IP address, browser user agent, created and last-seen times | keeping the person signed in, and the device list in Settings | contract; legitimate interest in account security |
| Remembered devices: browser user agent and dates | skipping the code step on a trusted browser for 30 days | contract |
| Known browsers: a hash of the `__Host-daibx_browser` cookie's token, and when it was first and last seen | telling a new browser from one the person has signed in with before | legitimate interest in account security |
| Pending sign-in alerts: the time, browser user agent and sign-in method of a new-browser sign-in | emailing the person about it within a minute or two | legitimate interest in account security |
| Email links: a hash of the token, and the new address for an email change | verifying an address, resetting a password, confirming an email change | contract |
| Throttle counters, keyed by the SHA-256 of an email address, by IP address, or one overall email counter | slowing down password and code guessing, and capping DaiBX's emails at 100 a day | legitimate interest in security |
| Invites: a hash of the code, and which account used it | invite-only sign-up | legitimate interest |
| Security log: event type, time, IP address, user agent, a few non-personal details | detecting and investigating account attacks | legitimate interest in security |

Tasks, notes, calendar events and time entries are free text, and people write health, money and family details into them. DaiBX treats that content as possibly sensitive personal information: it is never logged, never sent to any service except the database and its encrypted backups, and only its owner's signed-in sessions can read it. The app connects to the database as `daibx_app`, a role limited to the app's own tables.

## Cookies

Every cookie is strictly necessary for signing in or keeping an account safe, so none needs consent. All are `__Host-`, `Secure` and `HttpOnly`, so scripts can't read them and they are never sent over plain HTTP.

| Cookie | What it does | Lasts |
| --- | --- | --- |
| `__Host-daibx_access` | proves the session on each request | 15 minutes |
| `__Host-daibx_refresh` | renews the access cookie | the session's life |
| `__Host-daibx_challenge` | carries a sign-in between the password and code steps | 5 minutes |
| `__Host-daibx_device` | a browser the person chose to remember, skipping the code step | 30 days |
| `__Host-daibx_browser` | a browser that has signed in before, so a sign-in from it sends no alert | 1 year |

The browser also keeps a few `daibx:` entries in its own storage: the appearance choice, and the last board, task group and view opened. Signing out, or a session ending, clears all of them except the appearance choice, which holds only a theme and a colour.

The security log never holds passwords, tokens, codes, secrets or email addresses: detail names are checked against a deny-list in code (`SecurityEventDetails`). Each row points to the account through `users.security_ref`, a random reference used nowhere else, never through the account id or email.

## How long it's kept

| Data | Kept until |
| --- | --- |
| Account, profile, password hash, authenticator secret, recovery codes | the person deletes the account |
| Sessions | ended (sign-out, 7 days idle, or 30 days after sign-in), then deleted by the daily purge |
| Tasks, board, calendar, time tracking, schedules and their run logs | the person deletes them, or deletes the account |
| Session tokens, code steps, email links, remembered devices, throttle counters, unused invites | they expire (between 5 minutes and 30 days), then deleted by the daily purge |
| Known browsers | 1 year after the browser was last seen, then deleted by the daily purge |
| Pending sign-in alerts | sent, then deleted; one that still can't be sent after an hour is deleted and recorded in the security log as failed |
| Security log | 12 months, then deleted by the daily purge |
| Lambda and scheduler logs in CloudWatch | 90 days in production, 30 in development |
| Encrypted database backups in S3 | 30 days, then deleted by the bucket's lifecycle rule |

The purge is `purge_expired_auth_data()`, which runs daily at 03:17 UTC through Supabase's `pg_cron` in each environment (`ops/schedule-purge.sql`).

## Erasure

Deleting an account (Settings, then the password, a code and the words "delete my account") is immediate and complete, in one database transaction:
- the account row and everything it owns is deleted: sessions, tokens, devices, known browsers, pending alerts, recovery codes, email links, code steps, and every task, board, card, calendar event, time entry and schedule;
- an invite the person used stays, but no longer points to them;
- the security log keeps its rows, and an `account_deleted` row is added. With the account gone, its `security_ref` matches nothing, so the rows can't be tied back to the person. They're deleted after 12 months like any other.

There's no grace period and no undo. The nightly backups still hold the account until they expire after 30 days, and CloudWatch logs age out on their own schedule. A backup restored within those 30 days would bring a deleted account back, so `ops/restore-backup.md` re-applies every deletion recorded since the backup (`ops/reapply-deletions.sql`) before the copy is used.

## Not yet offered

- **Exporting account data:** planned for milestone 10.
- **Notifications after other security changes**, such as an email when the password changes. Only a sign-in from a new browser sends an email today.
- **The final terms and privacy policy:** written in milestone 12. Accounts created before then record the draft version (`draft-2026-10`) they were shown.
