# BACKEND-02: Auth + Account

Invitation-only authentication and the caller's own account, built on the
identity schema that already existed in
`db/migrations/0002_identity_and_admin.sql`. No parallel auth tables were
created; no existing table was altered.

## What the schema already decided

The approved design (`docs/superpowers/specs/2026-09-13-database-economy-core-design.md`)
and migration 0002 fixed these before this milestone started, and this
milestone implements them rather than re-deciding them:

- **Invitation-only registration.** `invitations` + `invitation_redemptions`,
  the latter with `UNIQUE (invitation_id)` and `UNIQUE (user_id)`. There is
  no open signup, and email is not part of identity.
- **Argon2id passwords.** Named explicitly in the design doc. `users.password_hash`
  is `text`, so it holds a PHC string.
- **Server-side opaque sessions, not JWT.** `user_sessions` stores a
  32-byte `session_token_hash`, with `expires_at` and `revoked_at`. The
  design doc's own wording ("Recovery revokes active sessions") only makes
  sense for revocable server-side sessions, so JWTs were never an option
  here.
- **Secrets stored as hashes only.** Invitations, recovery codes, and
  sessions all store a 32-byte hash and never the raw value.
- **TOTP is an administrator concern.** `administrators.totp_secret_hash`
  exists; `users` has no TOTP. Admin auth is BACKEND-09, not this
  milestone.

## Endpoints

All under `/api/v1`. `/health/*` deliberately stays outside it.

| Method | Path | Auth | Purpose |
|---|---|---|---|
| POST | `/api/v1/auth/register` | none | Redeem an invitation, create an account. Returns `201` with the account's public UUID. Does **not** log the new account in. |
| POST | `/api/v1/auth/login` | none | Exchange credentials for a session cookie. |
| POST | `/api/v1/auth/logout` | cookie (optional) | End this session, clear the cookie. Idempotent, `204`. |
| POST | `/api/v1/auth/logout-all` | cookie (required) | End every session of the calling account; returns how many were revoked. |
| GET | `/api/v1/me` | cookie | The caller's own account. |
| GET | `/api/v1/me/balance` | cookie | The caller's own ledger-derived balance, in integer microcredits. |

There is no `/users/{id}` endpoint in this milestone, by design: with no
identifier in any path, there is nothing for a client to tamper with.

## Layering

```text
POST /api/v1/auth/login
  -> crates/api/src/routes/auth.rs        (HTTP shape, cookie, status mapping)
  -> crates/application/src/auth.rs       (Argon2id, token generation, policy)
  -> crates/db/src/identity.rs            (typed wrappers)
  -> db/migrations/0015_*.sql functions   (SECURITY DEFINER, the only DB reach)
  -> PostgreSQL
```

`crates/db` and `crates/economy-core` contain no Axum types. `crates/api`
touches `db` only for `AppState`'s pooled `Database` handle; all auth
logic goes through `application`.

## Why the database functions are narrow

`db/migrations/0013_security_hardening.sql` (BACKEND/DB work that
preceded this) deliberately narrowed `contracter_runtime`'s `SELECT` on
`users` to a column list that **excludes `password_hash`**. Login still
needs the stored hash, because Argon2id verification cannot happen in
PostgreSQL. Rather than widening that grant, migration 0015 adds
`find_user_credential_by_login`: one `SECURITY DEFINER` function that
returns one login's credential and nothing else, with `EXECUTE` granted
to the runtime role.

The same reasoning produced the rest:

| Function | Why it exists |
|---|---|
| `find_user_credential_by_login` | Login, without a blanket read of `password_hash`. |
| `register_invited_user` | Redeem + create in one atomic call, locking the invitation `FOR UPDATE` first so two concurrent redemptions resolve to one winner with a clean error instead of both racing into the `UNIQUE` constraint. |
| `create_user_session` | One `clock_timestamp()` reading feeds `created_at` and `expires_at`; the table's `CHECK (expires_at > created_at)` has no margin and two separate readings can land microseconds apart. |
| `find_active_user_session` | Resolves a token, or returns no row for unknown/expired/revoked/disabled — indistinguishable by design. |
| `revoke_user_session` / `revoke_all_user_sessions` | Idempotent revocation; the latter is also the building block recovery will need. |
| `find_account_by_public_id` | `/me`, addressed by public id only. |
| `find_user_credit_balance` (0016) | `/me/balance`. `contracter_runtime` has no `SELECT` on `ledger_accounts`/`ledger_balances` and still does not. |

`db/tests/001_invariants.sql` asserts this boundary executably: the
runtime role can execute exactly these functions, still cannot read
`password_hash`, and has no direct table access to sessions, invitations,
redemptions, or recovery codes.

## Sessions and cookies

A session token is 32 bytes from the OS CSPRNG, carried as URL-safe
base64. Only its SHA-256 is stored, so a database read cannot reconstruct
a usable session. The raw token is returned exactly once, in `Set-Cookie`,
and never appears in a response body (asserted by test).

Cookie: `__Host-contracter_session`, `HttpOnly`, `SameSite=Lax`,
`Path=/`, `Max-Age` from the session TTL (14 days by default), `Secure`,
and no `Domain`.

**The `__Host-` prefix is load-bearing, not decoration.** An
app-specific name alone gives no protection against cookie shadowing:
any host under the registrable domain — a sibling subdomain, a stale
CNAME, a forgotten staging box — can set `contracter_session=<attacker
token>; Domain=example.com; Path=/api/v1`, and browsers send more
specific paths first, so a first-match parser would authenticate the
victim's browser as the attacker's account. `__Host-` is what makes the
cookie unwritable by anyone else: the browser refuses to store such a
cookie unless it is `Secure`, has `Path=/`, and carries no `Domain`. A
`Secure` deployment therefore reads *only* the prefixed name, and
ignores the unprefixed one even when it carries a genuinely valid token.

On top of that, a session cookie name appearing more than once in a
request is rejected outright rather than resolved by position.
Duplicates are not something a well-behaved client produces; they are
what shadowing looks like in flight, and any "first wins" or "last wins"
rule only changes which half of the attack succeeds.

**CSRF.** All four state-changing endpoints are POST, and `SameSite=Lax`
means a cross-site POST never carries the cookie. `register` and `login`
additionally require a JSON body; `logout` and `logout-all` do not, so
the body is not part of the argument for them. With no cross-origin
policy configured (the default), a cross-origin `fetch` cannot reach
these endpoints at all. If `CORS_ALLOWED_ORIGINS` is configured, the
listed origins are trusted by the operator. No separate CSRF token is
introduced; if a future milestone adds cookie-authenticated `GET` side
effects, this decision needs revisiting.

**Caching.** Every `/api/v1` response carries `Cache-Control: no-store`
and `Vary: Cookie`, applied as a layer so a route added later cannot
forget it. Without `Vary: Cookie` a shared cache in front of this
service is entitled to serve one account's `/me` to the next caller,
because the requests differ only in a header it was never told mattered.

**`Secure`.** On by default. `INSECURE_COOKIES=true` (exact value, any
other value leaves it on) turns it off for local HTTP development —
which also drops back to the unprefixed name, because a browser rejects
a `__Host-` cookie that is not `Secure`. The server logs a warning at
startup when it does.

**CORS.** When origins are configured, the layer allows `GET` and `POST`
and sets `allow_credentials`, without which a browser frontend on
another origin cannot send the session cookie and so cannot use the API
at all. This is safe only because the origin list is explicit: the CORS
spec forbids pairing credentials with a wildcard, and the layer is
omitted entirely rather than widened when nothing is configured.

## Identity

`CurrentUser` (`crates/api/src/extract.rs`) is the only way a handler
learns who is calling, and it has no constructor taking a client-supplied
id. A handler that takes it is authenticated; one that does not is
public. `logout-all` identifies the account from the session, never from
the body.

## What is deliberately indistinguishable

| Situation | Response |
|---|---|
| Wrong password / unknown login / disabled account | `401`, identical message, no cookie set |
| No cookie / unknown / tampered / empty / foreign-named cookie | `401`, identical message |
| Expired session / revoked session / disabled account's session | `401`, identical message |
| Spent invitation / expired invitation / invitation that never existed | `403`, identical message |

An unknown login still costs a full Argon2id verification against a real
decoy hash, so timing does not separate it from a wrong password.

What is *not* hidden, deliberately: a taken login returns `409`. Since
registration already requires possessing a valid unused invitation, the
enumeration surface is gated behind that, and a would-be user has to be
told why their chosen login failed.

## Cost of an unauthenticated request

Argon2id's cost is the point of it, and that cost is paid in memory: the
default parameters allocate 19 MiB per operation. Tokio's blocking pool
defaults to 512 threads, so an unbounded `spawn_blocking` per login is
roughly 9.7 GB of resident memory that any unauthenticated client can
ask for, on two endpoints with no rate limit. The request timeout does
not help — it cancels the *future*, not the blocking task, so an
abandoned request keeps its allocation until the hash finishes.

Three things bound it:

- Concurrent hashing is limited by a semaphore sized to the machine's
  parallelism, with the permit held across the whole blocking task so it
  accounts for memory actually resident, including for a caller who has
  already gone away. Extra concurrency would not make hashing finish
  sooner anyway; there are only so many cores to run it on.
- A request waits at most 5 seconds for a permit, then gets `503` with a
  truthful "at capacity" message. Saturation sheds load instead of
  deepening a queue.
- `login` rejects a password outside the accepted length *before*
  hashing. Such a password cannot be any account's password, so this
  costs an honest caller nothing and is not an oracle: the answer
  depends only on what the caller typed, never on whether the login
  exists.

This is a ceiling, not a rate limit. Per-IP and per-account rate
limiting is still owed (see **Residual risks**).

## Password policy

Length only: 12 to 1024 characters. Length rather than composition rules
follows NIST SP 800-63B; the upper bound stops a hostile client making
the server hash megabytes. This is a security floor, not a product rule.
Login validation (3–64 characters, no surrounding whitespace) mirrors the
`users` table's own `CHECK` so a bad login is a clean `422` rather than a
constraint violation.

## Money

`/me/balance` returns `balance_microcredits` as a JSON integer, matching
the ledger exactly. No float appears anywhere in this path. An account
with no ledger account yet (nothing creates one at registration) reads as
`0`, not as a missing resource.

## Tests

| Layer | File | Count | Covers |
|---|---|---|---|
| db | `crates/db/tests/identity.rs` | 6 | Registration/redemption, reuse and expiry rejection, case-insensitive login uniqueness, session lifecycle (revoked/expired/disabled), logout-all scoping |
| db (races) | `crates/db/tests/concurrency.rs` | 2 new | Two real connections racing one invitation, and racing one login |
| application | `crates/application/src/auth.rs` | 8 unit | Token entropy/redaction, validation bounds, Argon2id round trip, fresh salt, decoy hash, unparsable hash fails closed |
| application | `crates/application/tests/auth.rs` | 10 | Use-case round trip and every indistinguishability property |
| api | `crates/api/src/session_cookie.rs` | 9 unit | Cookie parsing, shadowing and duplicate rejection, attribute construction |
| api | `crates/api/tests/auth.rs` | 12 | Full HTTP flows, cookie attributes, shadowing rejection, IDOR/isolation, leak sweep, sanitized 500, cache headers |

`scripts/verify.sh` now runs the whole workspace's `--ignored` tests
rather than only `db`'s, so these are part of the canonical run. It still
runs `db/verify.sh` first, which requires a pristine database — the Rust
suites commit fixtures on purpose, so re-running invariants after them
would fail on the singleton treasury account.

## Residual risks

Known and accepted for this milestone, each with the reason it is not
fixed here rather than a claim that it does not matter:

| Risk | Why it is still open |
|---|---|
| No per-IP or per-account rate limiting on `/auth/*` | Rate limiting is its own milestone with its own storage question (in-process vs. shared). The hashing bound above caps the damage to `503`s rather than memory exhaustion, but it does not stop password guessing. |
| Registration reveals a taken login (`409`) without consuming the invitation | Enumeration is gated behind possessing a valid unused invitation, and a user has to be told why their chosen login failed. Consuming the invitation on a name clash would punish the honest case. |
| `x-request-id` is accepted from the client | Convenient for tracing through a trusted proxy; a hostile client can forge or collide ids. It is used for correlation only, never for authorization. |
| Database functions are owned by the superuser | `SECURITY DEFINER` functions run as their owner. A dedicated lower-privileged owner role is a deployment change, not a code change, and belongs with the provisioning work. |
| Password material is not zeroized in memory | Rust moves `String`s freely, so zeroizing is only meaningful with a type that owns the buffer end to end. Worth doing; not worth doing halfway. |

## Deferred

Everything else: catalog, inventory, market, contracts, pricing/scarcity,
admin, rate limiting, OpenAPI. Recovery codes exist in the schema but no
recovery flow is implemented here. Administrator authentication and TOTP
are BACKEND-09.
