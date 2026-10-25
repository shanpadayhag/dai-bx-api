<a id="readme-top"></a>

[![Contributors][contributors-shield]][contributors-url]
[![Forks][forks-shield]][forks-url]
[![Stargazers][stars-shield]][stars-url]
[![Issues][issues-shield]][issues-url]
[![MIT License][license-shield]][license-url]

<br />
<div align="center">
  <h3 align="center">DaiBX API</h3>

  <p align="center">
    The backend for DaiBX: one workspace for nested tasks, a board and a calendar.
    <br />
    <a href="#usage"><strong>Explore the endpoints »</strong></a>
    <br />
    <br />
    <a href="https://github.com/shanpadayhag/dai-bx-api/issues/new?labels=bug">Report Bug</a>
    ·
    <a href="https://github.com/shanpadayhag/dai-bx-api/issues/new?labels=enhancement">Request Feature</a>
  </p>
</div>

<details>
  <summary>Table of Contents</summary>
  <ol>
    <li>
      <a href="#about-the-project">About The Project</a>
      <ul>
        <li><a href="#built-with">Built With</a></li>
      </ul>
    </li>
    <li>
      <a href="#getting-started">Getting Started</a>
      <ul>
        <li><a href="#prerequisites">Prerequisites</a></li>
        <li><a href="#installation">Installation</a></li>
      </ul>
    </li>
    <li><a href="#usage">Usage</a></li>
    <li><a href="#roadmap">Roadmap</a></li>
    <li><a href="#contributing">Contributing</a></li>
    <li><a href="#license">License</a></li>
    <li><a href="#contact">Contact</a></li>
    <li><a href="#acknowledgments">Acknowledgments</a></li>
  </ol>
</details>

## About The Project

DaiBX puts nested tasks, a Trello-like board and a calendar in one place. Starting a task writes a calendar event and finishing it sets the end, so the calendar records what actually happened.

This repository is the API. It's one Axum application that runs as a single AWS Lambda function, with Postgres on Supabase as its only store. The web app ([dai-bx-web](https://github.com/shanpadayhag/dai-bx-web)) reaches it through a `/backend` rewrite on Vercel, so the browser sees one origin.

The code is organised by feature (`src/features/<feature>/`), each with its own router, handlers, actions and data sources. What no single feature owns lives in `src/core/`.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

### Built With

[![Rust][rust-shield]][rust-url]
[![Axum][axum-shield]][axum-url]
[![PostgreSQL][postgres-shield]][postgres-url]
[![AWS Lambda][lambda-shield]][lambda-url]

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Getting Started

### Prerequisites

- Rust 1.99 or later
- [cargo-lambda](https://www.cargo-lambda.info/) to run and build the Lambda function. Homebrew has no prebuilt version for older macOS and falls back to compiling LLVM, so install it with Cargo instead:
  ```sh
  cargo install cargo-lambda --locked
  ```
- PostgreSQL 16 (no Docker needed)
  ```sh
  brew install postgresql@16
  ```
- [Mailpit](https://mailpit.axllent.org/), which catches emails sent during local runs and tests (needed once the API sends email)
  ```sh
  brew install mailpit
  ```

### Installation

1. Clone the repo
   ```sh
   git clone git@github.com:shanpadayhag/dai-bx-api.git
   cd dai-bx-api
   ```
2. Create a local Postgres cluster inside the repo (ignored by git), on port 55440
   ```sh
   LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8 initdb -D .postgres/data -U postgres --auth=trust --locale=C --encoding=UTF8
   ```
3. Start it, and create the database
   ```sh
   LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8 pg_ctl -D .postgres/data -o "-p 55440 -k ''" -l .postgres/log start
   createdb -h localhost -p 55440 -U postgres daibx
   ```
   Stop it with `pg_ctl -D .postgres/data stop -m fast`.
4. Copy the environment file, then replace the `TOTP_KEYS` placeholder with a fresh key. TOTP secrets are encrypted with it; old versions can stay listed (`v1:…,v2:…`) while `TOTP_KEY_CURRENT` names the one used for new secrets.
   ```sh
   cp .env.example .env
   openssl rand -base64 32
   ```
5. Start Mailpit in another terminal, so email tests have an inbox to deliver to:
   ```sh
   mailpit --smtp 127.0.0.1:1025 --listen 127.0.0.1:8025
   ```
   Its inbox is at http://127.0.0.1:8025. Homebrew compiles it from source on older macOS; `GOBIN=$HOME/.local/bin go install github.com/axllent/mailpit@v1.32.0` is quicker if Go is installed.
6. Run the tests. Each test gets its own fresh database on the local cluster.
   ```sh
   cargo test
   ```

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Usage

Run the API locally on port 9000:

```sh
cargo lambda watch --bin daibx-api --env-file .env
```

Then call it:

```sh
curl http://localhost:9000/health
```

| Method | Path | Answers |
| --- | --- | --- |
| GET | `/health` | `200 {"status":"ok"}` when the database answers, `500 {"error":"internal server error"}` when it doesn't |
| POST | `/auth/sign-up` | `{"inviteCode","name","email","password"}` → `202 {"status":"verification_email_sent"}`, also for an email that's already taken |
| POST | `/auth/verify-email` | `{"token"}` from the emailed link → `200 {"next":"set_up_authenticator"}` and a 5-minute challenge cookie |
| POST | `/auth/verify-email/resend` | `{"email"}` → `202 {"status":"verification_email_sent"}` whether or not the account exists; `429` after 3 an hour |
| GET | `/auth/mfa/setup` | with the challenge cookie → `200 {"otpauthUri","manualEntryKey"}` for the authenticator app; `401` without a live setup challenge |
| POST | `/auth/mfa/setup/confirm` | `{"code"}` → `200 {"recoveryCodes":[…10]}` shown once, plus the access and refresh cookies; `422` for a wrong code, `401` after 5 wrong codes or 5 minutes |
| POST | `/auth/sign-in` | `{"email","password"}` → `200 {"next":"enter_code"}` (or `set_up_authenticator`, `verify_email`, or `signed_in` on a remembered device); `401 {"error":"the email or password is incorrect"}` for any wrong pair; `429` after repeated failures |
| POST | `/auth/mfa/verify` | `{"code","rememberDevice"}` with the challenge cookie; an authenticator code or a recovery code → `200 {"next":"signed_in"}` and the session cookies, plus `"remainingRecoveryCodes"` after a recovery code |
| POST | `/auth/refresh` | `{}` with the refresh cookie → `200 {"status":"refreshed"}` and a new cookie pair; `401` when the session is over or the token was reused |
| POST | `/auth/sign-out` | `{}` with a session → `200 {"status":"signed_out"}`; ends this session and keeps the remembered device |
| POST | `/auth/password-reset` | `{"email"}` → `202 {"status":"reset_email_sent"}` whether or not the account exists; `429` after 3 an hour |
| POST | `/auth/password-reset/complete` | `{"token","password"}` → `200 {"next":"enter_code"}` and a challenge cookie; every session and remembered device ends; `422` for a used, expired or refused password |
| GET | `/auth/session` | with the access cookie → `200 {"user":{"name","email","accentColor","theme"}}`; `401` when signed out, idle 7 days, or 30 days after sign-in |
| GET | `/account` | with a session → `200 {"name","email","accentColor","theme","security":{"recoveryCodesRemaining","activeSessionCount"}}` |
| PATCH | `/account` | any of `{"name","accentColor","theme"}` → `200` with the updated profile; `422` for an unknown color or theme |
| POST | `/account/password` | `{"currentPassword","newPassword"}` → `200 {"status":"password_changed"}`; ends every other session; `422` for a wrong current password |
| POST | `/account/email` | `{"password","code","newEmail"}` → `202 {"status":"confirmation_email_sent"}` and a link to the new address |
| GET | `/account/sessions` | with a session → `200 {"sessions":[{"id","createdAt","lastSeenAt","userAgent","isCurrent"}]}`, most recently used first |
| POST | `/account/sessions/end` | `{"sessionId","password"}` → `200 {"status":"session_ended"}`; ends another of your sessions |
| POST | `/account/sessions/end-all` | `{"password"}` → `200 {"status":"signed_out"}`; ends every session and forgets every remembered device |
| POST | `/account/recovery-codes` | `{"password","code"}` → `200 {"recoveryCodes":[…10]}`; every earlier code stops working |
| POST | `/account/authenticator` | `{"password","code"}` (a recovery code works) → `200 {"next":"set_up_authenticator"}` and a challenge cookie for `GET /auth/mfa/setup` |
| POST | `/account/authenticator/confirm` | `{"code"}` from the new app → `200 {"status":"authenticator_replaced"}`; ends the other sessions and every remembered device |
| POST | `/account/delete` | `{"password","code","confirmation":"delete my account"}` → `200 {"status":"account_deleted"}`; deletes everything the account owns |
| POST | `/account/email/confirm` | `{"token"}`, no session needed → `200 {"status":"email_changed"}`; `422` for a used or expired link |

Every error comes back as `{ "error": "<safe message>" }`.

### Invites

Sign-up needs an invite code while DaiBX is invite-only. Create one with the `invite` CLI, which connects as the restricted `invite_operator` database role: it can add and read invites, and nothing else.

1. Once per database, give the role a password as the database owner (in Supabase, the SQL editor):
   ```sql
   ALTER ROLE invite_operator WITH PASSWORD 'a long random password';
   ```
2. Create a code with that role's connection string. On Supabase, use the session pooler address with the user `invite_operator.<project-ref>`:
   ```sh
   INVITE_DATABASE_URL='postgresql://invite_operator.<project-ref>:<password>@<pooler-host>:5432/postgres' cargo run --bin invite -- create
   ```
   It prints a code like `K7QM-2XRD-9FTB-0011`, valid for one sign-up within 7 days. Only its SHA-256 is stored, so this is the only time it's shown.

Every request that changes something (anything but GET, HEAD and OPTIONS) must come from the web app and carry JSON:

- `Sec-Fetch-Site: same-origin`, or, from browsers that don't send it, an `Origin` listed in `APP_ORIGINS`. Otherwise `403`.
- `Content-Type: application/json`. Otherwise `415`.

`APP_ORIGINS` is a comma-separated list. One entry may contain a single `*`, which matches letters, digits and hyphens only, for example `https://dai-bx-web-*-shans-projects-a71057cb.vercel.app` for Vercel preview deployments.

The API sends no CORS headers: the browser reaches it through the web app's `/backend` rewrite, on the same origin.

New passwords are checked against Have I Been Pwned's range API (only the first 5 characters of the password's SHA-1 leave the server). `HIBP_API_BASE_URL` overrides the default `https://api.pwnedpasswords.com`, which tests use to point at a local fake. If the service is slow or down, the check is skipped and logged, and the local rules still apply.

### Security and privacy

- [`docs/security/asvs-l2.md`](docs/security/asvs-l2.md) maps every OWASP ASVS 5.0 Level 2 authentication and session requirement to where it's met, and lists the tests behind every endpoint.
- [`docs/security/gdpr-notes.md`](docs/security/gdpr-notes.md) records what personal data is stored, why, where, for how long, and how deletion works.

### Releases and housekeeping

- Pushing to `release/v1.0.0` deploys the dev Lambda. Pushing a `v1.*` tag deploys prod, once CI passes on the tagged commit. Each environment has its own Supabase project, Lambda, deploy role and secrets.
- Expired sessions, tokens, challenges and old security events are deleted daily by `purge_expired_auth_data()`. Schedule it once per Supabase project by running `ops/schedule-purge.sql` as the database owner (in the SQL editor, or with `psql`). Re-running it replaces the schedule. `ops/check-purge.sql` shows the job and its last runs.
- To end someone's sessions as the operator, run `psql "$DATABASE_URL" -v account_email='person@example.com' -f ops/end-account-sessions.sql`. `ops/end-all-sessions.sql` ends everyone's.

Before committing, run the same checks as CI:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

CI compiles with `SQLX_OFFLINE=true`, so query checks come from the committed `.sqlx/` folder instead of a live database. After adding or changing a query, refresh it with [sqlx-cli](https://crates.io/crates/sqlx-cli) 0.9 and commit the result:

```sh
cargo install sqlx-cli@0.9.0 --locked --no-default-features --features postgres,rustls
cargo sqlx prepare -- --all-targets
```

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Roadmap

- [ ] Authentication: invites, sign-up, email verification, authenticator-app MFA, sessions, account settings and deletion
- [ ] Tasks, with categories
- [ ] Board
- [ ] Calendar and time tracking
- [ ] Workflows
- [ ] Automatic scheduling

See the [open issues](https://github.com/shanpadayhag/dai-bx-api/issues) for a full list of proposed features and known issues.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Contributing

Contributions are welcome. If you have a suggestion that would make this better, fork the repo and create a pull request, or open an issue with the tag "enhancement".

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'feat: add some amazing feature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## License

Distributed under the MIT License. See `LICENSE` for more information.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Contact

Shan Padayhag - shanpadayhag@gmail.com

Project Link: [https://github.com/shanpadayhag/dai-bx-api](https://github.com/shanpadayhag/dai-bx-api)

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Acknowledgments

- [Best-README-Template](https://github.com/othneildrew/Best-README-Template)
- [cargo-lambda](https://www.cargo-lambda.info/)
- [OWASP Application Security Verification Standard](https://owasp.org/www-project-application-security-verification-standard/)

<p align="right">(<a href="#readme-top">back to top</a>)</p>

[contributors-shield]: https://img.shields.io/github/contributors/shanpadayhag/dai-bx-api.svg?style=for-the-badge
[contributors-url]: https://github.com/shanpadayhag/dai-bx-api/graphs/contributors
[forks-shield]: https://img.shields.io/github/forks/shanpadayhag/dai-bx-api.svg?style=for-the-badge
[forks-url]: https://github.com/shanpadayhag/dai-bx-api/network/members
[stars-shield]: https://img.shields.io/github/stars/shanpadayhag/dai-bx-api.svg?style=for-the-badge
[stars-url]: https://github.com/shanpadayhag/dai-bx-api/stargazers
[issues-shield]: https://img.shields.io/github/issues/shanpadayhag/dai-bx-api.svg?style=for-the-badge
[issues-url]: https://github.com/shanpadayhag/dai-bx-api/issues
[license-shield]: https://img.shields.io/github/license/shanpadayhag/dai-bx-api.svg?style=for-the-badge
[license-url]: https://github.com/shanpadayhag/dai-bx-api/blob/main/LICENSE
[rust-shield]: https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white
[rust-url]: https://www.rust-lang.org/
[axum-shield]: https://img.shields.io/badge/Axum-1f2937?style=for-the-badge&logo=rust&logoColor=white
[axum-url]: https://github.com/tokio-rs/axum
[postgres-shield]: https://img.shields.io/badge/PostgreSQL-4169E1?style=for-the-badge&logo=postgresql&logoColor=white
[postgres-url]: https://www.postgresql.org/
[lambda-shield]: https://img.shields.io/badge/AWS_Lambda-FF9900?style=for-the-badge&logo=awslambda&logoColor=white
[lambda-url]: https://aws.amazon.com/lambda/
