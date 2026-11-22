# ASVS 5.0 Level 2 record

This maps every Level 1 and Level 2 requirement in OWASP ASVS 5.0, one table per chapter, to where DaiBX meets it, or why it doesn't apply. Requirement numbers and levels come from the OWASP ASVS repository (`5.0/en`, commit `2b47a9d`, 15 October 2026). The requirement summaries below are paraphrased; read the standard for the exact wording.

**Status key**
- **Met:** implemented, with a pointer to the code or the test that proves it.
- **Met, documented deviation:** implemented differently from the letter of the requirement, for a reason written here.
- **Not applicable:** DaiBX has no such feature.
- **Gap (F*n*):** not met yet; the numbered fix is tracked for this release.

Paths are relative to the API repository unless they start with `web:`.

## Documentation the requirements ask for

### Anti-automation (6.1.1, 6.3.1)

DaiBX never locks an account. It slows attackers down and answers `429` with a `Retry-After` header instead (`src/features/throttling/`).

- **Per account and IP address:** 5 failures are free, then each further failure blocks that pair for 60 seconds, doubling up to 15 minutes. A success clears the count.
- **Per account:** at most 100 failures an hour from all addresses together. A browser holding a valid remembered-device cookie for that account skips this limit, so an attacker spraying an account from many addresses can't lock its owner out of their own devices.
- **Per IP address:** at most 100 failures an hour across all accounts.
- **What counts:** wrong passwords at sign-in and on account pages that ask for the current password share one counter. Wrong authenticator or recovery codes share another.
- **One-time steps:** a code step (`auth_challenges`) allows 5 tries in 5 minutes, then it's deleted.
- **Email requests per address:** 3 an hour for each purpose (verification, including sign-up; password reset; email change).
- **Email requests per IP address:** sign-up, password reset and verification resend share 10 an hour.
- **All account emails:** at most 100 sent in 24 hours, well under Gmail's published limit of 500 a day for a personal account. The cap is read before any account lookup, so a full cap refuses known and unknown addresses alike, and it is counted only for an email really sent, so made-up addresses can't use it up. Reaching it writes `email_cap_reached` to the security log and an error to the application log.
- **Accounts are keyed by the SHA-256 of the lowercased email**, so throttling behaves the same for real and unknown addresses and reveals nothing.
- **Client IP:** it comes from `x-vercel-forwarded-for`, which Vercel sets. The API refuses every call that doesn't carry the website's `ORIGIN_SECRET` (F17), so a caller can't skip Vercel to pick their own IP address. Several people behind one shared address still share its per-IP limits, which is why the per-account limits come first.

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

### Input rules and checks across data (2.1.1, 2.1.2)

Every request body is JSON, at most 64 KB (`DefaultBodyLimit`, `src/lib.rs`), and is read into typed entities: each endpoint uses only the fields it names, and a missing or wrongly typed field is refused. Text is trimmed, and a value that is empty after trimming is refused where a name is required. Lengths are counted in characters, not bytes.

| Value | Rule |
| --- | --- |
| Email | a valid address, at most 254 characters, compared without case |
| Name | 1 to 100 characters |
| Password | 15 to 128 characters, not in the breached-password list, without the person's own words (see above) |
| Codes and tokens | a 6-digit authenticator code; a recovery code in its fixed alphabet; an emailed link token of at most 128 characters |
| Task name, card title | 1 to 500 characters |
| Task group name | 1 to 200 characters |
| Board name, list name | 1 to 100 characters |
| Category, property and option names | 1 to 50 characters |
| Card notes | at most 20,000 characters |
| Card text property | at most 500 characters |
| Card number property | at most 15 whole and 6 decimal digits |
| Planned calendar block title | 1 to 200 characters |
| Repeats | an interval of 1 to 99, at most 999 occurrences, block length at most 1,440 minutes |
| Dates and times | ISO 8601 with a time zone from the IANA database |
| Ids | UUIDs; an id that isn't the caller's own is answered as not found |

Checks across combined data, made in the same transaction as the write:
- a task's parent must be in the same group and the same account, and a move can't put a task under itself or nest deeper than 32 levels;
- a card's list must be on the card's board, and a property value must match its property's type and options;
- a card trigger's target list must be on the same board, and a schedule's category and card must belong to the same account;
- a calendar event ends after it starts; a task or card that is already running can't be started again, and a running event must be finished before its times change or it is deleted.

### Business limits (2.1.3, 2.3.2)

Per account and per call, enforced in code with a test each:

| Limit | Value |
| --- | --- |
| Writes | 600 in 10 minutes per account (`allow_write`); more get `429` |
| Properties per board | 20 |
| Options per property | 50 |
| Schedules per card | 20 |
| Tasks in one group import | 2,000 |
| Archived cards per page, card moves per page, schedule runs per page | 50, 50, at most 100 |
| Scheduler work per minute | 100 due schedules and 100 idle cards per run |
| Account emails overall | 100 in 24 hours (see Anti-automation) |

Sign-in, code and email limits are in Anti-automation above.

### Group import file (5.1.1)

The only upload is the task-group import, `POST /task-groups/import`. It is JSON, at most 1 MB, read into memory and never stored as a file. It holds a group name (1 to 200 characters) and up to 2,000 tasks nested at most 32 deep, each name following the task name rule. A task may point to a card, which must be one of the caller's own. Anything else is refused with the path of the first problem, and nothing is written unless the whole file is valid.

### Access rules (8.1.1, 8.1.2)

- **Who can do what:** DaiBX has one kind of user. A signed-in person can read and change only their own data: every query is scoped by the session's user id, and an id belonging to anyone else is answered as not found, never forbidden. There are no admin or shared resources in the API.
- **Signed out:** only sign-up, sign-in and its steps, the emailed-link endpoints, password reset and `GET /health` work without a session. The route tables in `tests/features/` list every route as signed-in or signed-out, and a route missing from them fails the suite.
- **The operator:** invites are created by the `invite` binary through the `invite_operator` database role, which can only read and add invites. The owner never reads anyone's content except at that person's request.
- **Fields:** a person can change only the fields each endpoint lists. Owner ids, creation times, security fields (password hash, authenticator secret, terms version) and server-set values can't be set through the API; each request entity names exactly the fields it accepts.
- **The database:** the app connects as `daibx_app`, which can read and write the app's tables, and only add and read rows in `security_events`, never change or delete them.

### Key management and cryptography inventory (11.1.1, 11.1.2)

| Use | Algorithm | Key or secret | Where it lives | Replaced |
| --- | --- | --- | --- | --- |
| Passwords | Argon2id, 19 MiB, 2 passes, 1 lane | none | `src/core/security/password_hash.rs` | settings raised when guidance changes; old hashes re-hashed at next sign-in |
| Recovery codes | Argon2id, the same settings | none | `src/features/recovery_codes/` | when the person makes new codes |
| Authenticator secrets | AES-256-GCM, bound to the account id | `TOTP_KEYS`, versioned (`v1:…,v2:…`), `TOTP_KEY_CURRENT` for new secrets | Lambda environment and the password manager | a new version added when one may have leaked; people re-add their authenticator |
| Sign-in codes | HMAC-SHA-1 (TOTP, RFC 6238), 6 digits, 30 seconds | the per-account authenticator secret | encrypted in `users` | when the person replaces the authenticator |
| Session, device, browser and link tokens | 32 random bytes from the operating system; stored only as SHA-256 | none | `src/core/security/` | on every use or at expiry |
| Breached-password check | SHA-1 prefix (k-anonymity) | none | `src/features/password_policy/` | not applicable |
| Request origin | 64 random hex characters, compared in constant time | `ORIGIN_SECRET` | Vercel Production and the API Lambda | at once if Vercel or the Lambda may be exposed (see `breach-procedure.md`) |
| Database connection | TLS, `verify-full` against the Supabase root CA | the `daibx_app` password | Lambda environment | at once if it may have leaked |
| Email | TLS to Gmail SMTP | the Gmail app password | Lambda environment | at once if it may have leaked |

Rules: keys come only from a cryptographic random source, never from code or the repository; each key has one purpose; a secret is never logged or printed; the owner is the only person with access, and every key is in the password manager. D4 covers the two SHA-1 uses.

### Services DaiBX talks to (13.1.1, 13.2.4, 13.2.5)

| From | To | Why | How |
| --- | --- | --- | --- |
| Browser | `dai-bx.vercel.app` | pages and `/backend` calls | HTTPS only (HSTS) |
| Vercel (`web/src/proxy.ts`) | the API Lambda's Function URL | forwarding `/backend` | HTTPS, with `ORIGIN_SECRET` |
| API and scheduler Lambdas | Supabase session pooler, `aws-0-ap-southeast-1.pooler.supabase.com:5432` | the database | TLS `verify-full` |
| API and scheduler Lambdas | `smtp.gmail.com` | sending email | TLS, app password |
| API Lambda | `api.pwnedpasswords.com` | the breached-password check | HTTPS, 2-second connect timeout, no redirects |
| EventBridge | the scheduler Lambda | the minute run | AWS-internal invoke |
| GitHub Actions | Supabase, AWS Lambda | migrations, deploys | TLS; short-lived AWS role for deploys |

That table is the allowlist. The API and scheduler connect only to the database, SMTP and breached-password hosts, each fixed by configuration (`DATABASE_URL`, `SMTP_HOST`, `HIBP_API_BASE_URL`). No request can name a host, URL or file for the server to fetch, and the HTTP client follows no redirects, so a request can't send the server anywhere else.

### Data classification (14.1.1, 14.1.2, 14.2.4)

| Level | What | Required protection |
| --- | --- | --- |
| Secret | passwords, codes, tokens, authenticator secrets, keys, connection strings | never stored in plain form (hashed or encrypted as in the crypto table), never logged, never returned after creation, never in a URL query |
| Sensitive | tasks, notes, cards, calendar events, time tracking, schedules | only the owner's sessions can read it; never logged; `Cache-Control: no-store` on every API response; deleted with the account |
| Personal | name, email, IP addresses, user agents, sign-in times | kept only as long as the GDPR notes say; IP addresses and user agents only in sessions and the security log; email never in the security log |
| Public | the changelog, landing page and app code | none beyond integrity |

Every level travels only over TLS. Logs follow the log inventory below, and retention follows `gdpr-notes.md` (CloudWatch 90 days in production, 30 in development).

### Dependencies (15.1.1, 15.1.2)

- **Inventory:** `Cargo.lock` and `web:package-lock.json` are committed and are the full list, with exact versions. CI installs only from them.
- **Checks:** `cargo audit` and `npm audit` run in each CI on every push and fail it on a known advisory. Dependabot checks both repos and the GitHub Actions they use weekly and opens update requests.
- **Fix times:** a critical or high advisory in code that runs in production is fixed or replaced within 7 days; a moderate one within 30 days; one only in build or test tools, or in code that isn't compiled, by the next release. An advisory with no fix, or in code that is never compiled, is recorded with its reason and a review date in the audit configuration (`.cargo/audit.toml`, `web:` `audit:dependencies`) and checked again by that date.

### Resource-heavy functions (15.1.3)

| Function | Cost | Guard |
| --- | --- | --- |
| Password hashing (sign-up, sign-in, change, reset, delete) | about 19 MiB and tens of milliseconds each | runs off the async workers; sign-in limits in Anti-automation |
| Breached-password check | one outbound HTTPS call | 2-second connect timeout; the check is skipped, not failed, when the service is down |
| Sending email | one SMTP session | per-address and per-IP limits, and the 100-a-day cap; alerts sent by the scheduler, not during sign-in |
| Group import | up to 1 MB of JSON and 2,000 inserts | the limits above, one transaction |
| Calendar range reads | expanding repeating events | at most 7 days per request, 999 occurrences per series |
| The minute scheduler | due schedules, idle cards and alerts | 100 schedules and 100 cards per run; alerts within a 20-second budget |
| Every request | Lambda time and memory | a request timeout, 64 KB bodies, and the write limit |

### Log inventory (16.1.1, 16.2.3)

| Log | Written by | Holds | Kept | Goes to |
| --- | --- | --- | --- | --- |
| API logs | `daibx-api-*` through `tracing`, as JSON | refused requests (route, status), internal errors, skipped breached-password checks; never bodies, cookies, tokens or content | 90 days prod, 30 dev | CloudWatch Logs, `ap-southeast-1` |
| Scheduler logs | `daibx-scheduler-*` | each run's counts, alert send failures, the email cap being reached | 90 days prod, 30 dev | CloudWatch Logs, `ap-southeast-1` |
| Security log | the API, table `security_events` | account security events with time, IP, user agent and a pseudonymous `security_ref` | 12 months | Supabase |
| Vercel request logs | Vercel | request lines for pages and `/backend` | Vercel's own retention | Vercel |
| CI logs | GitHub Actions | build, test and deploy steps; secrets masked | GitHub's retention | GitHub |

Logs go nowhere else: no third-party log service, no email of log content. The API's log calls are the only writers, and their fields are fixed in code.

## V1 Encoding and Sanitization

| # | Level | Requirement (paraphrased) | Status | Where, or why |
| --- | --- | --- | --- | --- |
| 1.1.1 | 2 | Input is decoded to its canonical form once, before use | Met | axum's extractors decode paths and query strings once, and each body is parsed once as JSON |
| 1.1.2 | 2 | Output is encoded as the last step before its interpreter | Met | React escapes at render, emails escape while building the HTML (`src/features/email/entities/account_email.rs`), and SQL values are bound |
| 1.2.1 | 1 | HTTP, HTML and similar output is encoded for its context | Met | API bodies are serialized JSON, the web app renders through React, and email values pass through `escape_html` |
| 1.2.2 | 1 | Dynamic URLs encode untrusted data and allow only safe protocols | Met | `web:src/features/calendar/data_sources/backend/calendar-event-source.ts` encodes path and query values; note links pass the Markdown URL filter in `web:src/features/board/components/card-notes.tsx` |
| 1.2.3 | 1 | JavaScript and JSON built dynamically are escaped | Met | JSON comes from `serde` and `JSON.stringify`; the inline scripts in `web:src/app/layout.tsx` are constants |
| 1.2.4 | 1 | Database queries are parameterized | Met | every query is a static `sqlx` statement with bound values, for example `src/features/cards/data_sources/postgres` |
| 1.2.5 | 1 | OS commands can't be injected | Not applicable | the deployed binaries run no OS commands |
| 1.2.6 | 2 | LDAP injection is prevented | Not applicable | no LDAP |
| 1.2.7 | 2 | XPath injection is prevented | Not applicable | no XPath or XML |
| 1.2.8 | 2 | LaTeX is processed safely | Not applicable | no LaTeX |
| 1.2.9 | 2 | Regular expression metacharacters are escaped | Not applicable | the API has no regex dependency and the web app builds no expression from input |
| 1.3.1 | 1 | HTML from rich-text editors is sanitized | Not applicable | there is no HTML editor; notes are Markdown and raw HTML isn't rendered |
| 1.3.2 | 1 | `eval` and similar dynamic code are avoided | Met | neither the API nor `web:src` uses `eval` or `new Function` |
| 1.3.3 | 2 | Data bound for dangerous contexts is made safe first | Met | the contexts in use are SQL, HTML and mail, covered by 1.2.4, 1.2.1 and 1.3.11 |
| 1.3.4 | 2 | User-supplied SVG can't carry scripts | Not applicable | users can't upload SVG, and an image in a note loads through `<img>`, where scripts don't run |
| 1.3.5 | 2 | Markdown and similar user content is sanitized | Met | `web:src/features/board/components/card-notes.tsx` renders notes with `react-markdown`, without raw HTML |
| 1.3.6 | 2 | Server-side request forgery is prevented | Met | the one outbound HTTP call goes to a configured address and carries only a hash prefix: `src/features/password_policy/data_sources/hibp/pwned_passwords_source.rs` |
| 1.3.7 | 2 | Templates aren't built from untrusted input | Met | there is no template engine; email text is compiled in and values are escaped |
| 1.3.8 | 2 | JNDI injection is prevented | Not applicable | no JNDI |
| 1.3.9 | 2 | Content sent to memcache is sanitized | Not applicable | no memcache |
| 1.3.10 | 2 | Format strings can't be abused | Met | Rust format strings are fixed at compile time |
| 1.3.11 | 2 | Mail input can't inject SMTP or IMAP commands | Met | `lettre` builds messages from parsed addresses, and subjects are constants: `src/features/email/actions/send_account_email.rs` |
| 1.4.1 | 2 | Memory-safe operations prevent buffer overflows | Met | safe Rust only: `unsafe_code = "forbid"` in `Cargo.toml` |
| 1.4.2 | 2 | Integer overflow is prevented | Met | numeric input is typed and range-checked, for example `src/core/recurrence.rs` and `src/features/cards/entities/card.rs` |
| 1.4.3 | 2 | Memory is released without dangling references | Met | Rust ownership, with `unsafe` forbidden |
| 1.5.1 | 1 | XML parsers are locked down | Not applicable | no XML parsing |
| 1.5.2 | 2 | Deserialization of untrusted data is safe | Met | `serde` parses into fixed types, and the client can't choose a type: `src/core/recurrence.rs` |

## V2 Validation and Business Logic

| # | Level | Requirement (paraphrased) | Status | Where, or why |
| --- | --- | --- | --- | --- |
| 2.1.1 | 1 | Documentation defines input validation rules | Met | "Input rules and checks across data" above |
| 2.1.2 | 2 | Documentation defines checks across combined data | Met | "Input rules and checks across data" above |
| 2.1.3 | 2 | Business limits are documented, per user and overall | Met | "Business limits" above |
| 2.2.1 | 1 | Input is validated against business expectations | Met | limits and allowed values live in the entities, for example `src/features/cards/entities/card.rs` and `src/features/card_schedules/entities/card_schedule.rs` |
| 2.2.2 | 1 | Validation is enforced at a trusted service layer | Met | the API validates everything; checks in the web app are for convenience only |
| 2.2.3 | 2 | Combinations of related values are checked | Met | `src/features/calendar_events/entities/calendar_event.rs`, `src/features/calendar_events/entities/event_series.rs`, `src/core/recurrence.rs` |
| 2.3.1 | 1 | Flows run in order, without skipped steps | Met | sign-in steps are tied to single-purpose challenges: `src/features/auth_challenges` |
| 2.3.2 | 2 | Business limits are enforced as documented | Met | "Business limits" above |
| 2.3.3 | 2 | Business operations commit or roll back as a whole | Met | actions run in database transactions, for example `src/features/auth/actions/sign_up.rs` |
| 2.3.4 | 2 | Locks stop limited resources being over-allocated | Met | changes take the parent's lock before checking a limit: `src/features/boards/actions/lock_board.rs` |
| 2.4.1 | 2 | Anti-automation limits excessive use | Met | 600 writes in 10 minutes per account, account changes included (F10); `tests/features/account/write_limit.rs` |

## V3 Web Frontend Security

| # | Level | Requirement (paraphrased) | Status | Where, or why |
| --- | --- | --- | --- | --- |
| 3.2.1 | 1 | Responses can't render in the wrong browser context | Met | API responses are JSON with `nosniff` (F1, `tests/features/core/response_headers.rs`); pages get the CSP and `nosniff` (F13, `web:src/security-headers.test.ts`) |
| 3.2.2 | 1 | Text is rendered with safe functions | Met | React renders text nodes, and `dangerouslySetInnerHTML` is used only for a constant in `web:src/app/layout.tsx` |
| 3.3.1 | 1 | Cookies are `Secure` and prefixed | Met | all four cookies are `__Host-` and `Secure`: `src/core/cookie.rs` |
| 3.3.2 | 2 | Each cookie's `SameSite` fits its purpose | Met | `Lax` for all four: they're first-party (the browser reaches the API through the web app's own `/backend` proxy), and state-changing requests are separately protected (3.5.1) |
| 3.3.3 | 2 | Cookies use the `__Host-` prefix unless shared | Met | none are shared |
| 3.3.4 | 2 | Script-hidden values are `HttpOnly` and only sent by `Set-Cookie` | Met | session, challenge and device tokens are `HttpOnly` and never appear in response bodies |
| 3.4.1 | 1 | HSTS is sent for at least a year, including subdomains | Met | Vercel sends a two-year policy with `includeSubDomains` (checked live on 2026-11-18) |
| 3.4.2 | 1 | CORS origins are fixed or allowlisted | Not applicable | no CORS headers are sent: `tests/features/core/cross_origin_headers.rs` |
| 3.4.3 | 2 | A Content Security Policy is sent | Met | `web:src/core/security/security-headers.ts` on every page (F13); `web:src/security-headers.test.ts`, and no violations on a production build (T12) |
| 3.4.4 | 2 | `X-Content-Type-Options: nosniff` is on every response | Met | API: `tests/features/core/response_headers.rs` (F1); pages: `web:src/security-headers.test.ts` (F13) |
| 3.4.5 | 2 | A referrer policy limits leakage | Met | `Referrer-Policy: strict-origin-when-cross-origin` on every page (F13); `web:src/security-headers.test.ts` |
| 3.4.6 | 2 | `frame-ancestors` prevents embedding | Met | `frame-ancestors 'none'` in the CSP (F13); `web:src/security-headers.test.ts` |
| 3.5.1 | 1 | Cross-origin requests to sensitive functions are rejected | Met | non-GET requests need `Sec-Fetch-Site: same-origin`, or an `Origin` listed in `APP_ORIGINS`, and a JSON body: `src/core/middlewares/csrf_protection.rs`; `tests/features/core/csrf_protection.rs` |
| 3.5.2 | 1 | If relying on CORS preflight, it can't be bypassed | Not applicable | DaiBX doesn't rely on CORS; the API sends no CORS headers, and 3.5.1's checks apply instead |
| 3.5.3 | 1 | Sensitive functions use unsafe HTTP methods | Met | every change is `POST` or `PATCH`; `GET` endpoints only read |
| 3.5.4 | 2 | Separate applications live on separate hostnames | Met | DaiBX has one web app, on `dai-bx.vercel.app` (v2 and its site were retired on 2026-10-29); each API Lambda has its own Function URL host |
| 3.5.5 | 2 | `postMessage` origins and syntax are checked | Not applicable | the web app listens for no `postMessage` events |
| 3.7.1 | 2 | Only supported client technologies are used | Met | current browsers' standard features, with no plugins |
| 3.7.2 | 2 | Automatic redirects to other hosts are allowlisted | Met | return paths must stay on the site: `web:src/features/auth/entities/return-path.ts` |

## V4 API and Web Service

| # | Level | Requirement (paraphrased) | Status | Where, or why |
| --- | --- | --- | --- | --- |
| 4.1.1 | 1 | Each response's `Content-Type` matches its body | Met | the API returns `application/json` and Next.js returns HTML with a UTF-8 charset |
| 4.1.2 | 2 | Only user-facing addresses redirect HTTP to HTTPS | Met, documented deviation (D3) | Vercel redirects plain HTTP to HTTPS on every path, including `/backend`. The web app is the only intended caller and always uses HTTPS, session cookies are `__Host-` and `Secure` (`src/core/cookie.rs`), and HSTS keeps browsers on HTTPS. Accepted by the owner on 2026-11-18 |
| 4.1.3 | 2 | Headers set by intermediaries can't be overridden | Met | `web:src/proxy.ts` replaces any `x-daibx-origin-secret` the browser sent, and the API refuses calls without it (F17); `tests/features/core/origin_secret.rs`, `web:src/proxy.test.ts` |
| 4.2.1 | 2 | All components agree on HTTP message boundaries | Met | Vercel and Lambda parse HTTP, and the API receives whole requests through `lambda_http` |
| 4.3.1 | 2 | GraphQL query cost is limited | Not applicable | no GraphQL |
| 4.3.2 | 2 | GraphQL introspection is off in production | Not applicable | no GraphQL |
| 4.4.1 | 1 | WebSockets use TLS | Not applicable | no WebSockets |
| 4.4.2 | 2 | WebSocket handshakes check `Origin` | Not applicable | no WebSockets |
| 4.4.3 | 2 | WebSockets use compliant session tokens | Not applicable | no WebSockets |
| 4.4.4 | 2 | WebSocket tokens come from the authenticated session | Not applicable | no WebSockets |

## V5 File Handling

| # | Level | Requirement (paraphrased) | Status | Where, or why |
| --- | --- | --- | --- | --- |
| 5.1.1 | 2 | Documentation defines allowed files and sizes | Met | "Group import file" above |
| 5.2.1 | 1 | Only files of a manageable size are accepted | Met | group imports stop at 1 MiB (`src/features/tasks/entities/group_import.rs`, `src/features/tasks/handlers/post_task_group_import.rs`) |
| 5.2.2 | 1 | File extension and contents are checked | Met | the picker asks for JSON (`web:src/features/tasks/components/import-group-dialog.tsx`), and the contents must parse and fit the import shape (`web:src/features/tasks/entities/group-import-preview.ts`, `src/features/tasks/entities/group_import.rs`) |
| 5.2.3 | 2 | Archives are checked before unpacking | Not applicable | archives aren't accepted |
| 5.3.1 | 1 | Uploaded files can't execute from public folders | Not applicable | the API stores and serves no uploaded files |
| 5.3.2 | 1 | File paths don't come from user file names | Not applicable | the API doesn't work with file paths |
| 5.4.1 | 2 | User file names are ignored or validated on download | Not applicable | the API stores and serves no uploaded files; the recovery-code download is made in the browser under a fixed name |
| 5.4.2 | 2 | Served file names are encoded | Not applicable | the API stores and serves no uploaded files |
| 5.4.3 | 2 | Untrusted files are scanned for malware | Not applicable | the API stores and serves no uploaded files |

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

## V8 Authorization

| # | Level | Requirement (paraphrased) | Status | Where, or why |
| --- | --- | --- | --- | --- |
| 8.1.1 | 1 | Documentation defines function and data access rules | Met | "Access rules" above |
| 8.1.2 | 2 | Documentation defines field-level access rules | Met | "Access rules" above |
| 8.2.1 | 1 | Functions are limited to permitted consumers | Met | every signed-in route refuses a second account and a signed-out caller (F5); `tests/features/core/cross_account_access.rs` |
| 8.2.2 | 1 | Data items are limited to permitted consumers | Met | another account's ids, in the path or the body, are answered as not found (F5); `tests/features/core/cross_account_access.rs` |
| 8.2.3 | 2 | Fields are limited to permitted consumers | Met | each person reaches only their own records, and responses are explicit types, for example `src/features/cards/responses/card_detail_response.rs` |
| 8.3.1 | 1 | Authorization is enforced at a trusted service layer | Met | the API decides from the server-side session: `src/features/sessions/entities/current_session.rs` |
| 8.4.1 | 2 | One tenant's actions can't affect another | Met | each account is its own tenant, and every query is scoped by its user id (F5); `tests/features/core/cross_account_access.rs` |

## V9 Self-contained Tokens

| # | Level | Requirement (paraphrased) | Status | Where, or why |
| --- | --- | --- | --- | --- |
| 9.1.1 | 1 | Token signatures are validated before use | Not applicable | sessions use random opaque tokens stored as hashes, so DaiBX issues and accepts no self-contained tokens: `src/core/security/opaque_token.rs` |
| 9.1.2 | 1 | Only allowlisted token algorithms are accepted | Not applicable | sessions use random opaque tokens stored as hashes, so DaiBX issues and accepts no self-contained tokens: `src/core/security/opaque_token.rs` |
| 9.1.3 | 1 | Token keys come from trusted sources | Not applicable | sessions use random opaque tokens stored as hashes, so DaiBX issues and accepts no self-contained tokens: `src/core/security/opaque_token.rs` |
| 9.2.1 | 1 | Token validity times are enforced | Not applicable | sessions use random opaque tokens stored as hashes, so DaiBX issues and accepts no self-contained tokens: `src/core/security/opaque_token.rs` |
| 9.2.2 | 2 | Token type and purpose are checked | Not applicable | sessions use random opaque tokens stored as hashes, so DaiBX issues and accepts no self-contained tokens: `src/core/security/opaque_token.rs` |
| 9.2.3 | 2 | Token audience is checked | Not applicable | sessions use random opaque tokens stored as hashes, so DaiBX issues and accepts no self-contained tokens: `src/core/security/opaque_token.rs` |
| 9.2.4 | 2 | Tokens for different audiences are distinguishable | Not applicable | sessions use random opaque tokens stored as hashes, so DaiBX issues and accepts no self-contained tokens: `src/core/security/opaque_token.rs` |

## V10 OAuth and OIDC

| # | Level | Requirement (paraphrased) | Status | Where, or why |
| --- | --- | --- | --- | --- |
| 10.1.1 | 2 | Tokens go only to components that need them | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.1.2 | 2 | Authorization responses match the started flow | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.2.1 | 2 | The code flow resists request forgery | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.2.2 | 2 | Clients resist mix-up attacks | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.3.1 | 2 | Access tokens are checked for audience | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.3.2 | 2 | Delegated claims are enforced | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.3.3 | 2 | Users are identified by claims that can't be reassigned | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.3.4 | 2 | Authentication strength in tokens is verified | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.4.1 | 1 | Redirect URIs match an exact allowlist | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.4.2 | 1 | Authorization codes work once | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.4.3 | 1 | Authorization codes are short-lived | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.4.4 | 1 | Clients get only the grants they need | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.4.5 | 1 | Refresh token replay is mitigated | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.4.6 | 2 | PKCE is required | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.4.7 | 2 | Dynamic client registration is controlled | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.4.8 | 2 | Refresh tokens have an absolute expiry | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.4.9 | 2 | Users can revoke tokens | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.4.10 | 2 | Confidential clients authenticate | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.4.11 | 2 | Clients get only the scopes they need | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.5.1 | 2 | ID token replay is prevented | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.5.2 | 2 | Users are identified by stable ID token claims | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.5.3 | 2 | Authorization server metadata is verified | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.5.4 | 2 | ID token audience is checked | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.5.5 | 2 | Back-channel logout is protected | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.6.1 | 2 | Only safe response types are allowed | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.6.2 | 2 | Forced logout is prevented | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.7.1 | 2 | Users consent to each authorization | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.7.2 | 2 | Consent screens are clear | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |
| 10.7.3 | 2 | Users can review and revoke consent | Not applicable | DaiBX has its own sign-in and is neither an OAuth client, resource server, authorization server nor OpenID provider |

## V11 Cryptography

| # | Level | Requirement (paraphrased) | Status | Where, or why |
| --- | --- | --- | --- | --- |
| 11.1.1 | 2 | A key management policy is documented | Met | "Key management and cryptography inventory" above |
| 11.1.2 | 2 | A cryptographic inventory is kept | Met | "Key management and cryptography inventory" above |
| 11.2.1 | 2 | Industry-validated crypto implementations are used | Met | maintained RustCrypto and `getrandom` crates: `src/core/security` |
| 11.2.2 | 2 | Algorithms and parameters can be changed | Met | encryption keys are versioned (`src/core/security/totp_secret_cipher.rs`), and password hashes are upgraded at sign-in (`src/core/security/password_hash.rs`) |
| 11.2.3 | 2 | Primitives give at least 128 bits of security | Met | AES-256-GCM, SHA-256 and a 160-bit authenticator key |
| 11.3.1 | 1 | Insecure block modes and padding are unused | Met | AES-GCM only: `src/core/security/totp_secret_cipher.rs` |
| 11.3.2 | 1 | Only approved ciphers and modes are used | Met | AES-256-GCM: `src/core/security/totp_secret_cipher.rs` |
| 11.3.3 | 2 | Encrypted data is protected from modification | Met | authenticated encryption, bound to the user id: `src/core/security/totp_secret_cipher.rs` |
| 11.4.1 | 1 | Only approved hash functions are used | Met, documented deviation (D4) | HMAC-SHA-1 is the authenticator-app standard for six-digit codes (`src/core/security/totp.rs`), and the breached-password service looks passwords up by SHA-1 (`src/features/password_policy/entities/new_password.rs`). Neither use relies on collision resistance, and stored passwords use Argon2id. Accepted by the owner on 2026-11-18 |
| 11.4.2 | 2 | Passwords use an approved slow hash | Met | Argon2id at 19 MiB, 2 passes: `src/core/security/password_hash.rs` |
| 11.4.3 | 2 | Hashes used for integrity are collision resistant | Met, documented deviation (D4) | HMAC-SHA-1 is the authenticator-app standard for six-digit codes (`src/core/security/totp.rs`), and the breached-password service looks passwords up by SHA-1 (`src/features/password_policy/entities/new_password.rs`). Neither use relies on collision resistance, and stored passwords use Argon2id. Accepted by the owner on 2026-11-18 |
| 11.4.4 | 2 | Keys derived from passwords use stretching | Not applicable | no key is derived from a password |
| 11.5.1 | 2 | Unguessable values come from a CSPRNG with 128 bits or more | Met | tokens are 32 bytes from the operating system; invite codes are 130 bits (F9); `src/features/invites/entities/invite_code.rs` tests |
| 11.6.1 | 2 | Key generation and signatures use approved algorithms | Met | keys are random bytes from the operating system (`src/core/security/random_bytes.rs`), and nothing is signed |

## V12 Secure Communication

| # | Level | Requirement (paraphrased) | Status | Where, or why |
| --- | --- | --- | --- | --- |
| 12.1.1 | 1 | Only current TLS versions are enabled | Met | the site and the API address refuse TLS 1.0 and 1.1 and accept 1.2 and later (checked live on 2026-11-18) |
| 12.1.2 | 2 | Only recommended cipher suites are enabled | Met | Vercel manages them; it refused the older non-forward-secret and SHA-1 suites tried on 2026-11-18 |
| 12.1.3 | 2 | mTLS client certificates are validated | Not applicable | no mTLS |
| 12.2.1 | 1 | External HTTP services use TLS without fallback | Met | the API address doesn't answer plain HTTP, and the site redirects it to HTTPS (see 4.1.2) |
| 12.2.2 | 1 | External services use publicly trusted certificates | Met | Vercel's and AWS's certificates validated against the public trust store on 2026-11-18 |
| 12.3.1 | 2 | All inbound and outbound connections are encrypted | Met | database TLS `verify-full` (F6), SMTP and HTTPS through rustls; `src/core/database.rs` tests |
| 12.3.2 | 2 | TLS clients validate server certificates | Met | the database is checked against the Supabase root CA built into every binary (F6); SMTP and HTTPS use the platform verifier; `src/core/database.rs` tests |
| 12.3.3 | 2 | Internal HTTP connections use TLS without fallback | Met | the web app reaches the API over HTTPS only |
| 12.3.4 | 2 | Internal TLS uses trusted certificates | Met | the API address has a public AWS certificate |

## V13 Configuration

| # | Level | Requirement (paraphrased) | Status | Where, or why |
| --- | --- | --- | --- | --- |
| 13.1.1 | 2 | All communication needs are documented | Met | "Services DaiBX talks to" above |
| 13.2.1 | 2 | Backend connections use individual service accounts | Met, documented deviation | separate database roles: `daibx_app` for the app (F7), `invite_operator` for invites, the owner role only for migrations; secrets stay in encrypted environment settings (D2) |
| 13.2.2 | 2 | Backend connections use least privilege | Met | the app connects as `daibx_app`, with table reads and writes only (F7); `tests/schema/app_database_role.rs`, `tests/features/core/app_database_role.rs` |
| 13.2.3 | 2 | Service credentials aren't defaults | Met | Supabase and Gmail issue no default passwords |
| 13.2.4 | 2 | Outbound destinations are allowlisted | Met | "Services DaiBX talks to" above |
| 13.2.5 | 2 | The server can reach only allowlisted resources | Met | "Services DaiBX talks to" above |
| 13.3.1 | 2 | Backend secrets live in a secrets manager | Met, documented deviation (D2) | secrets are kept in encrypted Lambda settings and GitHub secrets, outside the code and build output, rather than in a separate vault. Accepted by the owner on 2026-11-18, to be revisited when anyone else gets AWS access |
| 13.3.2 | 2 | Access to secrets follows least privilege | Met | each function holds only its own settings (`docs/scheduler-setup.md`), and its AWS role can only write logs |
| 13.4.1 | 1 | Source control metadata isn't deployed or reachable | Met | a compiled binary and the Next.js build are deployed; `/.git/config` returned 404 on 2026-11-18 |
| 13.4.2 | 2 | Debug modes are off in production | Met | release builds: the `build` job in `.github/workflows/ci.yml` |
| 13.4.3 | 2 | Directory listings aren't exposed | Met | neither host serves directories |
| 13.4.4 | 2 | HTTP TRACE isn't supported | Met | returned 405 on 2026-11-18 |
| 13.4.5 | 2 | Documentation and monitoring endpoints aren't exposed | Met | API docs and metrics paths returned 404 on 2026-11-18; `/health` says only `ok` |

## V14 Data Protection

| # | Level | Requirement (paraphrased) | Status | Where, or why |
| --- | --- | --- | --- | --- |
| 14.1.1 | 2 | Sensitive data is identified and classified | Met | "Data classification" above |
| 14.1.2 | 2 | Each protection level has documented requirements | Met | "Data classification" above |
| 14.2.1 | 1 | Sensitive data stays out of URLs | Met | emailed links carry the token in the `#` fragment (F11); `src/features/email/actions/send_account_email.rs` tests and the web verify, reset and confirm-email page tests |
| 14.2.2 | 2 | Sensitive data isn't cached by server components | Met | `Cache-Control: no-store` on every API response (F1); `tests/features/core/response_headers.rs` |
| 14.2.3 | 2 | Sensitive data doesn't reach untrusted parties | Met | the web app has no analytics or trackers: `web:package.json` |
| 14.2.4 | 2 | Protection rules for sensitive data are applied | Met | "Data classification" above |
| 14.3.1 | 1 | Authenticated data is cleared from the client at session end | Met | sign-out and a session ending clear every `daibx:` storage key but the appearance choice (F14); `web:src/features/auth/components/sidebar-account.test.tsx`, `web:src/features/auth/components/session-gate.test.tsx` |
| 14.3.2 | 2 | Anti-caching headers stop browser caching | Met | `Cache-Control: no-store` on every API response (F1); `tests/features/core/response_headers.rs` |
| 14.3.3 | 2 | Browser storage holds no sensitive data | Met | browser storage holds only view choices and the appearance choice; the pending email is removed once verified (F14); `web:src/core/browser-data.test.ts`, `web:src/features/auth/pages/verify-email-page.test.tsx` |

## V15 Secure Coding and Architecture

| # | Level | Requirement (paraphrased) | Status | Where, or why |
| --- | --- | --- | --- | --- |
| 15.1.1 | 1 | Update and fix time frames are documented | Met | "Dependencies" above |
| 15.1.2 | 2 | A dependency inventory is kept | Met | "Dependencies" above |
| 15.1.3 | 2 | Resource-heavy functions are documented | Met | "Resource-heavy functions" above |
| 15.2.1 | 1 | Components are within their update time frames | Met | CI fails on known issues: `.github/workflows/ci.yml`, `.cargo/audit.toml`, `web:audit-ignore.json` |
| 15.2.2 | 2 | Resource-heavy functions are protected | Met | request timeout and body limit (`src/lib.rs`), write limit (`src/features/throttling/entities/write_session.rs`), sign-in limits (`src/features/throttling/entities/throttle_attempt.rs`) |
| 15.2.3 | 2 | Production has no test or development functions | Met | only the API and scheduler binaries are built and deployed: `.github/workflows/ci.yml` and `.github/workflows/deploy.yml` |
| 15.3.1 | 1 | Only the required fields are returned | Met | responses are explicit types, for example `src/features/cards/responses/card_detail_response.rs` |
| 15.3.2 | 2 | Backend calls don't follow redirects unintentionally | Met | the HTTP client follows no redirects (F8); `refuses_a_redirect_without_following_it` in `src/features/password_policy/data_sources/hibp/pwned_passwords_source.rs` |
| 15.3.3 | 2 | Mass assignment is prevented | Met | each endpoint reads a fixed request type and updates named columns |
| 15.3.4 | 2 | The client IP comes from trusted fields | Met | the client IP comes from `x-vercel-forwarded-for`, and only Vercel can reach the API (F17); `tests/features/core/origin_secret.rs` |
| 15.3.5 | 2 | Types and comparisons are strict | Met | Rust's type system, and strict TypeScript on the web |
| 15.3.6 | 2 | JavaScript avoids prototype pollution | Met | the web app doesn't copy user-chosen keys onto plain objects |
| 15.3.7 | 2 | HTTP parameter pollution is prevented | Met | path, query and body values are read separately into fixed types |

## V16 Security Logging and Error Handling

| # | Level | Requirement (paraphrased) | Status | Where, or why |
| --- | --- | --- | --- | --- |
| 16.1.1 | 2 | A log inventory exists | Met | "Log inventory" above |
| 16.2.1 | 2 | Log entries carry when, where, who and what | Met | JSON log lines with time, route, status and `security_ref` (F2, F3); `src/core/logging.rs` tests, `tests/features/core/refusal_logging.rs` |
| 16.2.2 | 2 | Log times are synchronized and in UTC | Met | UTC timestamps from the Lambda clock (F3); `src/core/logging.rs` tests |
| 16.2.3 | 2 | Logs go only to documented destinations | Met | "Log inventory" above |
| 16.2.4 | 2 | Logs use a format the processor can read | Met | one JSON object per line, read by CloudWatch (F3); `src/core/logging.rs` tests |
| 16.2.5 | 2 | Sensitive data is logged only as its level allows | Met | the breached-password error drops the hash prefix and mail errors drop the server's text (F4); `keeps_the_hash_prefix_out_of_its_errors`, `keeps_the_mail_servers_words_out_of_its_error` |
| 16.3.1 | 2 | Authentication attempts are logged | Met | `src/features/security_log/entities/security_event.rs` |
| 16.3.2 | 2 | Failed authorization is logged | Met | every 4xx is logged with its route template and status, never ids (F2); `tests/features/core/refusal_logging.rs` |
| 16.3.3 | 2 | Security events and bypass attempts are logged | Met | refusals and security events are logged (F2) as well as recorded in `security_events`; `tests/features/core/refusal_logging.rs` |
| 16.3.4 | 2 | Unexpected errors and control failures are logged | Met | `src/core/error.rs` |
| 16.4.1 | 2 | Log data is encoded against injection | Met | a line break in a value stays inside one JSON entry (F3); `keeps_a_value_with_a_line_break_inside_one_entry` in `src/core/logging.rs` |
| 16.4.2 | 2 | Logs are protected from access and change | Met | the app role can only add and read security log rows (F7); CloudWatch is readable only by the owner's AWS account; `tests/schema/app_database_role.rs` |
| 16.4.3 | 2 | Logs go to a logically separate system | Met | CloudWatch is separate from the functions, whose AWS role can only write to it |
| 16.5.1 | 2 | Errors return a generic message | Met | `src/core/error.rs` |
| 16.5.2 | 2 | The app stays secure when external services fail | Met | a mail failure is retried (`src/features/sign_in_alerts/actions/send_pending_sign_in_alerts.rs`), and database failures refuse the request |
| 16.5.3 | 2 | Failures don't open access | Met | errors end the request and roll back its transaction: `src/core/error.rs` |

## V17 WebRTC

| # | Level | Requirement (paraphrased) | Status | Where, or why |
| --- | --- | --- | --- | --- |
| 17.1.1 | 2 | TURN can't reach reserved addresses | Not applicable | DaiBX has no WebRTC, TURN or media features |
| 17.2.1 | 2 | DTLS keys are managed by policy | Not applicable | DaiBX has no WebRTC, TURN or media features |
| 17.2.2 | 2 | Media servers use approved DTLS suites | Not applicable | DaiBX has no WebRTC, TURN or media features |
| 17.2.3 | 2 | SRTP authentication is checked | Not applicable | DaiBX has no WebRTC, TURN or media features |
| 17.2.4 | 2 | Malformed SRTP doesn't stop media | Not applicable | DaiBX has no WebRTC, TURN or media features |
| 17.3.1 | 2 | Signaling survives floods | Not applicable | DaiBX has no WebRTC, TURN or media features |
| 17.3.2 | 2 | Signaling survives malformed messages | Not applicable | DaiBX has no WebRTC, TURN or media features |

## Endpoint test coverage

Every endpoint has feature tests for success and for each error status it returns. `403` (cross-site request) and `415` (not JSON) come from shared middleware and are tested once in `tests/features/core/csrf_protection.rs`; the `403` for a call without the website's origin secret is tested once in `tests/features/core/origin_secret.rs`. A request that times out answers `408`, from the shared timeout layer.

| Endpoint | Statuses | Tests |
| --- | --- | --- |
| `POST /auth/sign-up` | 202, 422, 429 | `tests/features/auth/sign_up.rs`, `email_limits.rs` |
| `POST /auth/verify-email` | 200, 422 | `tests/features/auth/verify_email.rs` |
| `POST /auth/verify-email/resend` | 202, 422, 429 | `resend_verification_email.rs`, `email_limits.rs`, `malformed_email.rs` |
| `GET /auth/mfa/setup` | 200, 401 | `mfa_setup.rs` |
| `POST /auth/mfa/setup/confirm` | 200, 401, 422 | `mfa_setup.rs` |
| `POST /auth/sign-in` | 200, 401, 422, 429 | `sign_in.rs`, `malformed_email.rs` |
| `POST /auth/mfa/verify` | 200, 401, 422, 429 | `mfa_verify.rs` |
| `POST /auth/refresh` | 200, 401 | `refresh.rs` |
| `POST /auth/sign-out` | 200, 401 | `sign_out.rs` |
| `POST /auth/password-reset` | 202, 422, 429 | `password_reset.rs`, `email_limits.rs`, `malformed_email.rs` |
| `POST /auth/password-reset/complete` | 200, 422 | `password_reset.rs` |
| `GET /auth/session` | 200, 401 | `session.rs` |
| `GET /account` | 200, 401 | `tests/features/account/profile.rs`, `signed_out.rs` |
| `PATCH /account` | 200, 401, 422, 429 | `profile.rs`, `signed_out.rs`, `write_limit.rs` |
| `POST /account/password` | 200, 401, 422 | `password.rs`, `signed_out.rs` |
| `POST /account/email` | 202, 401, 422, 429 | `email.rs`, `signed_out.rs` |
| `POST /account/email/confirm` | 200, 409, 422 | `email.rs` |
| `POST /account/recovery-codes` | 200, 401, 422 | `recovery_codes.rs`, `signed_out.rs` |
| `POST /account/authenticator` | 200, 401, 422 | `authenticator.rs`, `signed_out.rs` |
| `POST /account/authenticator/confirm` | 200, 401, 422 | `authenticator.rs`, `signed_out.rs` |
| `POST /account/delete` | 200, 401, 422 | `deletion.rs`, `signed_out.rs` |
| `GET /account/sessions` | 200, 401 | `sessions.rs`, `signed_out.rs` |
| `POST /account/sessions/end` | 200, 401, 422 | `sessions.rs`, `signed_out.rs` |
| `POST /account/sessions/end-all` | 200, 401, 422 | `sessions.rs`, `signed_out.rs` |
| `GET /health` | 200, 500 | `tests/features/health/` |
