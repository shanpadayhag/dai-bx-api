# GDPR notes

What personal data DaiBX holds for authentication, why, where, for how long, and how it's deleted. This covers milestone 1 (accounts and sign-in). The final terms and privacy policy are written in milestone 12.

## Who holds the data

DaiBX is run by its owner, Shan Padayhag, as an invite-only personal service. The owner is the controller. The services below process data on the owner's behalf under their standard data processing terms.

| Service | What it does | Where |
| --- | --- | --- |
| Supabase (Postgres) | stores everything listed below | Singapore (ap-southeast-1) |
| AWS Lambda | runs the API; keeps request logs in CloudWatch | Singapore (ap-southeast-1) |
| Vercel | serves the web app and forwards `/backend` requests to the API | its global edge network; requests pass through and Vercel keeps its own short-lived request logs |
| Google (Gmail SMTP) | sends verification, reset and email-change links | Google's infrastructure; a Resend account on a DaiBX domain replaces it before launch |
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
| Terms version and when it was shown | recording what the person was shown at sign-up (`users.terms_version`, `users.terms_presented_at`) | accountability: being able to show what each person saw |
| Sessions: IP address, browser user agent, created and last-seen times | keeping the person signed in, and the device list in Settings | contract; legitimate interest in account security |
| Remembered devices: browser user agent and dates | skipping the code step on a trusted browser for 30 days | contract |
| Email links: a hash of the token, and the new address for an email change | verifying an address, resetting a password, confirming an email change | contract |
| Throttle counters, keyed by the SHA-256 of an email address, or by IP address | slowing down password and code guessing | legitimate interest in security |
| Invites: a hash of the code, and which account used it | invite-only sign-up | legitimate interest |
| Security log: event type, time, IP address, user agent, a few non-personal details | detecting and investigating account attacks | legitimate interest in security |

The security log never holds passwords, tokens, codes, secrets or email addresses: detail names are checked against a deny-list in code (`SecurityEventDetails`). Each row points to the account through `users.security_ref`, a random reference used nowhere else, never through the account id or email.

## How long it's kept

| Data | Kept until |
| --- | --- |
| Account, profile, password hash, authenticator secret, recovery codes | the person deletes the account |
| Sessions | ended (sign-out, 7 days idle, or 30 days after sign-in), then deleted by the daily purge |
| Session tokens, code steps, email links, remembered devices, throttle counters, unused invites | they expire (between 5 minutes and 30 days), then deleted by the daily purge |
| Security log | 12 months, then deleted by the daily purge |
| Lambda request logs in CloudWatch | AWS's log retention setting for the function |

The purge is `purge_expired_auth_data()`, which runs daily at 03:17 UTC through Supabase's `pg_cron` in each environment (`ops/schedule-purge.sql`).

## Erasure

Deleting an account (Settings, then the password, a code and the words "delete my account") is immediate and complete, in one database transaction:
- the account row and everything it owns (sessions, tokens, devices, recovery codes, email links, code steps) is deleted;
- an invite the person used stays, but no longer points to them;
- the security log keeps its rows, and an `account_deleted` row is added. With the account gone, its `security_ref` matches nothing, so the rows can't be tied back to the person. They're deleted after 12 months like any other.

There's no grace period and no undo. Supabase's own backups, and CloudWatch logs, age out on their own schedules.

## Not yet offered

- **Exporting account data:** planned for milestone 10.
- **Notifications after security changes**, such as an email when the password changes: out of scope for milestone 1.
- **The final terms and privacy policy:** written in milestone 12. Accounts created before then record the draft version (`draft-2026-10`) they were shown.
