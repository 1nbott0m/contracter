# BACKEND-01: Foundation + Error Model

Production-oriented HTTP backend foundation on top of the already-complete
`crates/db` and `crates/economy-core`. No business endpoints exist yet --
this is scaffolding: startup, health, a stable error envelope, request
correlation, and HTTP hardening.

## Architecture and dependency direction

```text
HTTP -> Axum API (crates/api) -> Application (crates/application) -> economy-core + crates/db -> PostgreSQL
                                                                     ^
                                                        crates/server wires everything together
```

- **`crates/api`** -- routing, extractors, responses, middleware, HTTP-level
  error mapping. No PostgreSQL/SQLx types are defined here. It references
  `db::Database` directly only for `AppState` (infrastructure wiring: the
  shared connection pool), never for business orchestration -- that goes
  through `application`.
- **`crates/application`** -- orchestration/use-cases. Deliberately minimal
  for BACKEND-01: only `check_readiness` exists (a thin wrapper around
  `db::Database::health_check`). Business use-cases (auth, market,
  contracts) are out of scope until a dedicated BACKEND task adds them.
- **`crates/economy-core`** -- unchanged. Pure, deterministic economy
  logic, no Axum types, no I/O.
- **`crates/db`** -- unchanged except one additive method
  (`Database::connect_lazy`, see below). The only PostgreSQL boundary.
- **`crates/server`** -- startup/config/wiring/listener/shutdown. The only
  crate with a `main`.

No microservices, Kafka, Redis, GraphQL, gRPC, or Kubernetes-specific
logic. A modular monolith, one binary (`contracter-server`).

## Startup sequence

`crates/server/src/main.rs`:

```text
config (ServerConfig::from_env)
  -> tracing (init_tracing, from LOG_FILTER)
  -> DB connect (db::Database::connect)
  -> required startup check (database.health_check(), fail fast)
  -> AppState
  -> router (api::build_router)
  -> bind (TcpListener)
  -> serve
  -> graceful shutdown (Ctrl+C, or SIGTERM on Unix)
```

A fatal startup error (bad config, unreachable DB, bind failure) prints via
`Display` to stderr and exits non-zero. It never prints `Debug` output or a
raw SQLx error -- `ServerConfig`'s `Debug` impl redacts `database_url`
exactly like `db::DatabaseConfig` already does.

The startup health check exists so a process that can't actually reach
PostgreSQL never claims to be listening for traffic; it fails at boot,
not on the first request.

## Configuration

Read once, at startup, via `ServerConfig::from_env()`:

| Variable | Required | Default | Notes |
|---|---|---|---|
| `DATABASE_URL` | yes | -- | Fails fast (`ConfigError::MissingEnvVar`) if unset. Never logged, never in `Debug` output. |
| `HOST` | no | `127.0.0.1` | |
| `PORT` | no | `8080` | Must parse as `u16`. |
| `LOG_FILTER` | no | `info` | Passed straight to `tracing_subscriber::EnvFilter`. |

No production secrets or defaults are invented. There is no `.env` file
committed or expected in production; local development uses whatever
mechanism the shell/deployment already provides (matching how
`TEST_DATABASE_URL` already works for `crates/db` tests).

## Health semantics

- **`GET /health/live`** -- 200 with `{"status":"live"}`. Independent of
  PostgreSQL by construction (the handler never touches `AppState`'s
  database). A DB outage must not take liveness down -- an orchestrator
  should not kill and restart a process over a problem restarting it
  cannot fix.
- **`GET /health/ready`** -- goes through `application::check_readiness`.
  200 with `{"status":"ready"}` when the DB responds; 503 in the standard
  error envelope, `code: "SERVICE_UNAVAILABLE"`, when it doesn't. Never a
  raw SQLx error, connection string, or stack trace.

Both live under the top-level path, not `/api/v1` -- that prefix is
reserved for future business endpoints (BACKEND-02+).

## Stable error envelope

Every error response, regardless of source, is:

```json
{
  "error": {
    "code": "SERVICE_UNAVAILABLE",
    "message": "Service temporarily unavailable",
    "request_id": "8ac07fa4-22ec-4af8-a31f-2a90cf936df8"
  }
}
```

`code` is one of a fixed set: `BAD_REQUEST` (400), `UNAUTHORIZED` (401),
`FORBIDDEN` (403), `NOT_FOUND` (404), `CONFLICT` (409),
`UNPROCESSABLE_ENTITY` (422), `TOO_MANY_REQUESTS` (429),
`INTERNAL_SERVER_ERROR` (500), `SERVICE_UNAVAILABLE` (503). This is not a
place for one variant per business scenario -- pick the HTTP-semantic
bucket that fits and put the specific, already-sanitized detail in
`message`. `ApiError::Internal`'s message is always the generic "Internal
server error"; a caller with a real cause logs it via `tracing` first and
returns `ApiError::Internal` -- never `format!("{:?}", err)` and never a
raw `sqlx::Error`.

Unknown routes return 404 in this same envelope (`crate::router::
fallback_404`), not Axum's default plain-text 404.

**A status outside the fixed set is normalized.** `tower_http`'s
`RequestBodyLimitLayer` produces a bare 413 on its own; the request-id
middleware (below) rewrites that specific case to 400
(`BAD_REQUEST`/"Request body too large") so the reported HTTP status and
the JSON `code` never disagree. Any other out-of-set status defaults to
500 (server-error range) or 400 (client-error range).

## Request IDs and tracing

`request_id_middleware` (`crates/api/src/request_id.rs`) is the outermost
layer. It:

1. Trusts an incoming `x-request-id` header if it's a valid UUID,
   otherwise generates one.
2. Makes it available to handlers via request extensions
   (`RequestId(Uuid)`).
3. Stamps the `x-request-id` response header on **every** response,
   regardless of what produced it (a handler, a 404 fallback, a timeout, a
   panic).
4. For any 4xx/5xx response, guarantees the JSON body carries the same id
   and matches the envelope shape -- patching an already-correct
   `ApiError` body in place, or rebuilding one from the status code for
   anything that isn't (framework-level failures that never went through
   `ApiError`).

This is why individual failure paths (timeout, panic) don't need to know
the request id themselves -- they only need to produce *a* response in
roughly the right shape (or not even that, for truly framework-level
ones), and the outermost middleware guarantees correctness.

`TraceLayer::new_for_http()` (`tower_http`) logs `method`, path, status,
and latency per request at the `tracing` level, keyed by the same
correlation id implicitly (span-scoped). **Never logged**: `Authorization`
headers, tokens, passwords, recovery secrets, private signing material, or
`DATABASE_URL`/any DB credential. Nothing in this codebase currently logs
request/response bodies, so there is no log-injection surface from
user-controlled body content today; this must be re-verified if a future
BACKEND task adds body logging.

## HTTP hardening

Layer stack, outermost to innermost (see `crates/api/src/router.rs` for
the exact `.layer()` order and why):

1. **request-id** (outermost) -- see above.
2. **tracing** (`TraceLayer`).
3. **CORS** -- see below. Wraps outside timeout/body-limit/panic handling
   so error responses still carry the headers a browser needs to read
   them via `fetch`, not just 2xx ones.
4. **timeout** (`tower::timeout::TimeoutLayer` + `HandleErrorLayer`) --
   default 10s (`RouterConfig::default`), configurable. A request that
   exceeds it gets a 503 in the standard envelope, not a hung connection.
5. **body limit** (`tower_http::limit::RequestBodyLimitLayer`) -- default
   256 KiB. Rejects a request whose declared `Content-Length` exceeds the
   limit immediately, before any inner service runs (413, normalized to
   400 by the request-id middleware). A body sent without `Content-Length`
   that exceeds the limit is only rejected once something actually reads
   it past the limit -- BACKEND-01 has no body-consuming route, so that
   specific sub-case isn't observable end-to-end yet; the Content-Length
   fast path is what's proven by test.
6. **panic boundary** (`tower_http::catch_panic::CatchPanicLayer`,
   innermost, closest to the actual handlers) -- a panicking handler
   becomes a sanitized 500; the real panic payload is logged via
   `tracing::error!`, never returned to the client.

**CORS decision:** disabled by default (no `Access-Control-*` headers
sent at all) until a frontend origin is known. `RouterConfig.
cors_allowed_origins` accepts an explicit list; if empty, no `CorsLayer`
is added. No production domain is invented and no credentialed wildcard
CORS exists anywhere in this codebase.

**No `unwrap`/`expect`/`panic!`/`todo!`/`unimplemented!` on a production
request path.** The one place a panic is *expected* to be possible (a
future handler bug) is exactly what `CatchPanicLayer` exists to contain.
`main.rs`'s own `.expect()`-free startup path uses `Result`/`?` throughout;
the two remaining `.expect()` calls in the whole backend
(`shutdown.rs`'s signal-handler installation) are startup-time,
this-should-never-fail infrastructure setup, not per-request code.

**Trusted-proxy model:** none exists yet. No header (`X-Forwarded-For`,
`X-Real-IP`, etc.) is read or trusted anywhere in this codebase. If a
reverse proxy is introduced later, that decision (and header trust
config) belongs to whichever task adds it.

## Testability

`api::build_router(state, &RouterConfig) -> Router` is independent of any
TCP listener. Tests exercise it via `tower::ServiceExt::oneshot`, no port
binding required (`crates/api/tests/health.rs`).

## Tests and verification

### Commands run this session

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All pass, workspace-wide (including the pre-existing `db`/`economy-core`
suite -- BACKEND-01 changed nothing about their behavior other than adding
`Database::connect_lazy`, an additive method).

### PostgreSQL integration

`db/verify.sh` + `cargo test -p db --tests -- --ignored` were run against
a real PostgreSQL 17 instance this session (`TEST_DATABASE_URL` set), and
the one `#[ignore]`-gated backend test
(`readiness_returns_200_against_a_real_postgresql_instance`) was run the
same way. Both green. Per this repo's own rule, PostgreSQL integration is
only claimed verified when `TEST_DATABASE_URL` is actually set and used --
every other backend test in `crates/api/tests/health.rs` deliberately runs
without it, using `Database::connect_lazy` against an address nothing
listens on to exercise the failure paths deterministically.

### Manual smoke test

The `contracter-server` binary was started against the same real
PostgreSQL instance (`DATABASE_URL` set, default port overridden via
`PORT`) and exercised with `curl`: `/health/live` and `/health/ready` both
200, an unknown route 404 in the standard envelope, `/api/v1/health/live`
correctly 404 (proving the reservation), and every response carried
`x-request-id`. Graceful shutdown uses the standard
`axum::serve(...).with_graceful_shutdown(...)` pattern; SIGTERM delivery
specifically could not be exercised end-to-end in this sandboxed Windows
environment (git-bash's `kill -TERM` does not reliably deliver a real
signal to a native Windows process), so that path is verified by
code/construction (the well-established axum/tokio pattern, `#[cfg(unix)]`
-gated so it only compiles where SIGTERM exists at all) rather than an
empirical process-level test this session.

## Public API rules for future BACKEND tasks

- **External resource identifiers use the existing `PublicId` (UUID)
  wrapper, never an internal `i64` id.** `crates/db`'s typed ids
  (`UserId`, `ContractId`, etc.) already distinguish these; any future
  endpoint that echoes an id to a client uses `PublicId`.
- **Money is always integer microcredits, never a float**, matching
  `crates/db`/`crates/economy-core`'s existing invariant exactly. No new
  money representation is introduced anywhere in the HTTP layer.

## Explicitly deferred

BACKEND-02 and beyond (auth/register/login/JWT/sessions, market/listings/
purchase/sell, wallet endpoints, contract quotes/accept/results, scarcity/
probability exposure, admin API, frontend) are **not implemented**. Do not
start them without a separate task/approval.
