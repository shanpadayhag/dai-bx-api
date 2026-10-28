# ASVS 5.0 Level 2: authentication and sessions

This maps every Level 1 and Level 2 requirement in OWASP ASVS 5.0's **V6 Authentication** and **V7 Session Management** chapters, plus the cookie (V3.3) and browser-origin (V3.5) requirements, to where DaiBX meets it, or why it doesn't apply. Requirement numbers and levels come from the OWASP ASVS repository (`5.0/en`, commit `2b47a9d`, 15 October 2026). The requirement summaries below are paraphrased; read the standard for the exact wording.

**Status key**
- **Met:** implemented, with a pointer to the code or the test that proves it.
- **Met, documented deviation:** implemented differently from the letter of the requirement, for a reason written here.
- **Not applicable:** DaiBX has no such feature.

Paths are relative to the API repository unless they start with `web:`.

## Documentation the requirements ask for

### Anti-automation (6.1.1, 6.3.1)

DaiBX never locks an account. It slows attackers down and answers `429` with a `Retry-After` header instead (`src/features/throttling/`).

- **Per account and IP address:** 5 failures are free, then each further failure blocks that pair for 60 seconds, doubling up to 15 minutes. A success clears the count.
- **Per account:** at most 100 failures an hour from all addresses together. A browser holding a valid remembered-device cookie for that account skips this limit, so an attacker spraying an account from many addresses can't lock its owner out of their own devices.
- **Per IP address:** at most 100 failures an hour across all accounts.
- **What counts:** wrong passwords at sign-in and on account pages that ask for the current password share one counter. Wrong authenticator or recovery codes share another.
- **One-time steps:** a code step (`auth_challenges`) allows 5 tries in 5 minutes, then it's deleted.
- **Email sends:** 3 an hour per address for each purpose (verification, password reset, email change).
- **Accounts are keyed by the SHA-256 of the lowercased email**, so throttling behaves the same for real and unknown addresses and reveals nothing.
- **Caveat:** the client IP comes from `x-vercel-forwarded-for`. A caller who skips Vercel and calls the Lambda's Function URL directly can fake it, so per-IP limits are best effort and the per-account limits are the real guard.

### Context-specific words (6.1.2, 6.2.11)

A new password is refused if it contains, ignoring case:
- `daibx`, the product name;
- any word of 3 or more letters or digits from the person's name;
- any word of 3 or more letters or digits from their email address, both before and after the `@`.

The list is built per person in `src/features/password_policy/entities/new_password.rs` (`personal_words`) and checked at sign-up, password change and password reset.

### Sign-in paths (6.1.3, 6.3.4)

Every way in needs the password plus a second factor:

| Path | Proof | Where |
| --- | --- | --- |
| Password and authenticator code | password, then a 6-digit TOTP code within the 5-minute challenge | `POST /auth/sign-in`, `POST /auth/mfa/verify` |
| Password and recovery code | password, then one unused recovery code | the same two endpoints |
| Password on a remembered device | password, plus a 30-day device cookie created only after a successful code on that browser | `POST /auth/sign-in` |
| Password reset | the emailed link (30 minutes, single use), then a code as above; the reset itself never signs anyone in | `POST /auth/password-reset/complete` |
| First setup | the emailed verification link, then a code from the newly scanned authenticator | `POST /auth/verify-email`, `POST /auth/mfa/setup/confirm` |

Changes to an account's own security need fresh proof while signed in: the email address, recovery codes, authenticator and deletion need the password and a code; a password change needs the current password; ending sessions needs the password. There are no other paths: no social sign-in, magic links, SMS or API keys.

### Session lifetimes and concurrency (7.1.1, 7.1.2)

- **Tokens:** an access token lives 15 minutes. The refresh token renews it, rotates on every use, and is good for 7 days. A replaced refresh token is accepted once more within 30 seconds (two tabs renewing at once); after that, any use counts as theft and ends the session.
- **Inactivity:** a session unused for 7 days ends.
- **Absolute limit:** every session ends 30 days after sign-in, whatever its activity.
- **Deviation from NIST SP 800-63B:** its AAL2 guidance asks for re-authentication within hours, not weeks. DaiBX deviates on purpose. It holds one person's own tasks, not money or other people's data. Every session token is server-side and revocable. Any change to the account's security asks for the password and a code again. The person can see and end sessions at any time. Under these conditions a 7-day idle and 30-day absolute window is a reasonable trade for not typing a code daily.
- **Concurrent sessions:** unlimited. Each sign-in is its own session; Settings lists them all with their device and last activity, and any one, or all, can be ended there after re-entering the password.

## V6 Authentication

| # | Level | Requirement (paraphrased) | Status | Where, or why |
| --- | --- | --- | --- | --- |
| 6.1.1 | 1 | Anti-automation controls are documented | Met | "Anti-automation" above |
| 6.1.2 | 2 | A list of context-specific words is documented | Met | "Context-specific words" above |
| 6.1.3 | 2 | All authentication paths are documented | Met | "Sign-in paths" above |
| 6.2.1 | 1 | Passwords are at least 8 characters, 15 recommended | Met | minimum 15: `MIN_PASSWORD_CHARACTERS` in `new_password.rs`; `sign_up_returns_422_for_each_invalid_field` |
| 6.2.2 | 1 | Users can change their password | Met | `POST /account/password`; `tests/features/account/password.rs` |
| 6.2.3 | 1 | Changing it needs the current and new password | Met | `change_password_returns_422_for_a_wrong_current_password` |
| 6.2.4 | 1 | New passwords are checked against at least the top 3,000 that fit the policy | Met | every 15-character-or-longer entry of the NCSC top-100,000 list, plus curated phrases, stored as SHA-256 digests (`common_password_sha256s.txt`); the top 3,000 is a subset |
| 6.2.5 | 1 | No composition rules | Met | only length, the common list, context words and the breach check apply |
| 6.2.6 | 1 | Password fields are masked, with an optional reveal | Met | `web: src/core/components/molecules/password-input.tsx` |
| 6.2.7 | 1 | Paste and password managers work | Met | no paste blocking; `autocomplete` is `current-password` or `new-password` on every field |
| 6.2.8 | 1 | Passwords are verified exactly as typed | Met | never trimmed, case-folded or truncated; over 128 characters is refused, not cut |
| 6.2.9 | 2 | Passwords of at least 64 characters are allowed | Met | up to 128: `MAX_PASSWORD_CHARACTERS` |
| 6.2.10 | 2 | No forced periodic rotation | Met | a password stays valid until the person changes or resets it |
| 6.2.11 | 2 | The context words are enforced | Met | `ContainsPersonalWord` rejection at sign-up, change and reset |
| 6.2.12 | 2 | New passwords are checked against breached passwords | Met | Have I Been Pwned range API with padding (`data_sources/hibp/`); if it's down for 2 seconds, the local checks still apply and the skip is logged |
| 6.3.1 | 1 | Brute-force controls are implemented as documented | Met | `src/features/throttling/`; `sign_in_returns_429_with_retry_after_once_failures_repeat`, `verify_throttles_wrong_codes_across_challenges` |
| 6.3.2 | 1 | No default accounts | Met | accounts exist only through invites; the `invite_operator` database role can't sign in to the app |
| 6.3.3 | 2 | Multi-factor authentication is required | Met | an authenticator is set up before first use; every sign-in needs a code or a remembered device created by a code (see "Sign-in paths") |
| 6.3.4 | 2 | No undocumented paths; strength is consistent | Met | "Sign-in paths" above |
| 6.4.1 | 1 | Initial secrets are random, expire, and can't become the password | Met | invite codes: 80 random bits, single use, 7 days; verification links: 256 bits, single use, 24 hours |
| 6.4.2 | 1 | No password hints or secret questions | Met | none exist |
| 6.4.3 | 2 | Password reset doesn't bypass MFA | Met | reset ends at the code step: `reset_complete_revokes_every_session_and_device_and_still_asks_for_a_code` |
| 6.4.4 | 2 | Replacing a lost factor needs the same proof as enrolment | Met | password plus a recovery code issued at enrolment: `replacing_with_a_recovery_code_swaps_the_secret_and_ends_other_sessions_and_devices` |
| 6.5.1 | 2 | Recovery codes and TOTP codes work once | Met | recovery codes get `used_at`; TOTP steps at or before `totp_last_used_step` are refused: `verify_refuses_an_authenticator_code_that_was_already_used` |
| 6.5.2 | 2 | Secrets under 112 bits are stored with a password hash and salt | Met | recovery codes (60 bits) use Argon2id with a 128-bit salt (`stores_new_codes_as_argon2id_hashes_only`). Sets issued before `v1.0.0-alpha.2` used salted SHA-256 and keep working until regenerated in Settings. Every token of 256 bits uses SHA-256, which the requirement allows |
| 6.5.3 | 2 | Secrets come from a CSPRNG | Met | `src/core/security/random_bytes.rs` (operating-system randomness) for TOTP secrets, recovery codes, invites and tokens |
| 6.5.4 | 2 | Recovery codes have at least 20 bits of entropy | Met | 12 Crockford symbols, 60 bits |
| 6.5.5 | 2 | TOTP codes live 30 seconds; out-of-band codes 10 minutes | Met, documented deviation | a code is accepted one 30-second step early or late, because phone clocks drift; each step still works once. There are no out-of-band authenticators: emailed links never create a session or count as a factor |
| 6.6.1 | 2 | Phone and SMS codes are restricted | Not applicable | no phone or SMS codes |
| 6.6.2 | 2 | Out-of-band codes are bound to their request | Not applicable | no out-of-band authenticators; emailed links are bound to one user and one purpose anyway |
| 6.6.3 | 2 | Out-of-band codes are rate limited | Not applicable | no out-of-band authenticators |
| 6.8.1 | 2 | Identities can't be spoofed across identity providers | Not applicable | no external identity providers |
| 6.8.2 | 2 | Assertion signatures are validated | Not applicable | no JWTs or SAML; session tokens are opaque and looked up server-side |
| 6.8.3 | 2 | SAML assertions are used once | Not applicable | no SAML |
| 6.8.4 | 2 | Authentication strength from an IdP is checked | Not applicable | no external identity providers |

## V7 Session Management

| # | Level | Requirement (paraphrased) | Status | Where, or why |
| --- | --- | --- | --- | --- |
| 7.1.1 | 2 | Idle and absolute limits are documented, with any NIST deviation justified | Met, documented deviation | "Session lifetimes and concurrency" above |
| 7.1.2 | 2 | Concurrent-session behaviour is documented | Met | "Session lifetimes and concurrency" above |
| 7.1.3 | 2 | Federated session coordination is documented | Not applicable | no single sign-on or federation |
| 7.2.1 | 1 | Session tokens are verified by the backend | Met | `CurrentSession` looks up every access token in Postgres: `src/features/sessions/entities/current_session.rs` |
| 7.2.2 | 1 | Session tokens are dynamic, not static secrets | Met | new opaque tokens per session and per renewal |
| 7.2.3 | 1 | Reference tokens are unique, CSPRNG-made, at least 128 bits | Met | 256 bits: `src/core/security/opaque_token.rs` |
| 7.2.4 | 1 | Authentication issues a new token and ends the old one | Met | every sign-in creates a new session and ends the one the browser already held (`signing_in_again_ends_the_session_this_browser_already_had`); renewal rotates both tokens. Step-up confirmations on account pages create no session |
| 7.3.1 | 2 | An inactivity timeout forces re-authentication | Met | 7 days: `get_session_returns_401_for_a_session_idle_over_7_days` |
| 7.3.2 | 2 | An absolute lifetime forces re-authentication | Met | 30 days: `get_session_returns_401_for_a_session_older_than_30_days` |
| 7.4.1 | 1 | Ended sessions can't be used again | Met | `revoked_at` is checked on every request and refresh: `sign_out_revokes_this_session_and_keeps_the_remembered_device` |
| 7.4.2 | 1 | Deleting an account ends all its sessions | Met | sessions cascade with the user row: `deleting_removes_every_owned_row_and_keeps_an_orphaned_security_log`. There is no disabled-account state |
| 7.4.3 | 2 | Other sessions can be ended after an authentication factor changes | Met | done automatically: a password change ends the other sessions, a reset ends all of them and forgets remembered devices, an authenticator replacement ends the others and forgets devices. After regenerating recovery codes, the devices dialog is one click away |
| 7.4.4 | 2 | Sign-out is easy to reach on every authenticated page | Met | the sidebar's account card on every app page: `web: src/features/auth/components/sidebar-account.tsx` |
| 7.4.5 | 2 | Administrators can end a user's sessions or everyone's | Met | DaiBX has no admin accounts; the operator runs `ops/end-account-sessions.sql` or `ops/end-all-sessions.sql` (README) |
| 7.5.1 | 2 | Changing security-relevant attributes needs full re-authentication | Met | email, recovery codes, authenticator and deletion need the password and a code: `ConfirmIdentity` in `src/features/account/actions/` |
| 7.5.2 | 2 | Users can see their sessions and end any after re-authenticating | Met | `GET /account/sessions`, `POST /account/sessions/end` and `/end-all` need the password: `tests/features/account/sessions.rs`; `web: src/features/settings/components/sessions-dialog.tsx` |
| 7.6.1 | 2 | Relying-party and identity-provider sessions behave as documented | Not applicable | no external identity providers |
| 7.6.2 | 2 | Sessions are created only by an explicit user action | Met | only a submitted code, a completed setup, or a password sign-in on a remembered device creates one |

## V3 cookie and origin requirements

| # | Level | Requirement (paraphrased) | Status | Where, or why |
| --- | --- | --- | --- | --- |
| 3.3.1 | 1 | Cookies are `Secure` and prefixed | Met | all four cookies are `__Host-` and `Secure`: `src/core/cookie.rs` |
| 3.3.2 | 2 | Each cookie's `SameSite` fits its purpose | Met | `Lax` for all four: they're first-party (the browser reaches the API through the web app's own `/backend` rewrite), and state-changing requests are separately protected (3.5.1) |
| 3.3.3 | 2 | Cookies use the `__Host-` prefix unless shared | Met | none are shared |
| 3.3.4 | 2 | Script-hidden values are `HttpOnly` and only sent by `Set-Cookie` | Met | session, challenge and device tokens are `HttpOnly` and never appear in response bodies |
| 3.5.1 | 1 | Cross-origin requests to sensitive functions are rejected | Met | non-GET requests need `Sec-Fetch-Site: same-origin`, or an `Origin` listed in `APP_ORIGINS`, and a JSON body: `src/core/middlewares/csrf_protection.rs`; `tests/features/core/csrf_protection.rs` |
| 3.5.2 | 1 | If relying on CORS preflight, it can't be bypassed | Not applicable | DaiBX doesn't rely on CORS; the API sends no CORS headers, and 3.5.1's checks apply instead |
| 3.5.3 | 1 | Sensitive functions use unsafe HTTP methods | Met | every change is `POST` or `PATCH`; `GET` endpoints only read |
| 3.5.4 | 2 | Separate applications live on separate hostnames | Met | v2 is on `dai-bx-web.vercel.app`, v3 on `dai-bx.vercel.app` (and its first alpha address `dai-bx-v3.vercel.app`); each API Lambda has its own Function URL host |
| 3.5.5 | 2 | `postMessage` origins and syntax are checked | Not applicable | the web app listens for no `postMessage` events |

## Endpoint test coverage

Every endpoint has feature tests for success and for each error status it returns. `403` (cross-site request) and `415` (not JSON) come from shared middleware and are tested once in `tests/features/core/csrf_protection.rs`. A request that times out answers `408`, from the shared timeout layer.

| Endpoint | Statuses | Tests |
| --- | --- | --- |
| `POST /auth/sign-up` | 202, 422 | `tests/features/auth/sign_up.rs` |
| `POST /auth/verify-email` | 200, 422 | `tests/features/auth/verify_email.rs` |
| `POST /auth/verify-email/resend` | 202, 422, 429 | `resend_verification_email.rs`, `malformed_email.rs` |
| `GET /auth/mfa/setup` | 200, 401 | `mfa_setup.rs` |
| `POST /auth/mfa/setup/confirm` | 200, 401, 422 | `mfa_setup.rs` |
| `POST /auth/sign-in` | 200, 401, 422, 429 | `sign_in.rs`, `malformed_email.rs` |
| `POST /auth/mfa/verify` | 200, 401, 422, 429 | `mfa_verify.rs` |
| `POST /auth/refresh` | 200, 401 | `refresh.rs` |
| `POST /auth/sign-out` | 200, 401 | `sign_out.rs` |
| `POST /auth/password-reset` | 202, 422, 429 | `password_reset.rs`, `malformed_email.rs` |
| `POST /auth/password-reset/complete` | 200, 422 | `password_reset.rs` |
| `GET /auth/session` | 200, 401 | `session.rs` |
| `GET /account` | 200, 401 | `tests/features/account/profile.rs`, `signed_out.rs` |
| `PATCH /account` | 200, 401, 422 | `profile.rs`, `signed_out.rs` |
| `POST /account/password` | 200, 401, 422 | `password.rs`, `signed_out.rs` |
| `POST /account/email` | 202, 401, 422 | `email.rs`, `signed_out.rs` |
| `POST /account/email/confirm` | 200, 409, 422 | `email.rs` |
| `POST /account/recovery-codes` | 200, 401, 422 | `recovery_codes.rs`, `signed_out.rs` |
| `POST /account/authenticator` | 200, 401, 422 | `authenticator.rs`, `signed_out.rs` |
| `POST /account/authenticator/confirm` | 200, 401, 422 | `authenticator.rs`, `signed_out.rs` |
| `POST /account/delete` | 200, 401, 422 | `deletion.rs`, `signed_out.rs` |
| `GET /account/sessions` | 200, 401 | `sessions.rs`, `signed_out.rs` |
| `POST /account/sessions/end` | 200, 401, 422 | `sessions.rs`, `signed_out.rs` |
| `POST /account/sessions/end-all` | 200, 401, 422 | `sessions.rs`, `signed_out.rs` |
| `GET /health` | 200, 500 | `tests/features/health/` |
