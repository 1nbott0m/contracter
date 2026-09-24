# Contracter Production Infrastructure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and verify a reproducible single-host production reference deployment with a trusted TLS edge, correct client-IP rate limiting, file-backed secrets, private monitoring, proven PostgreSQL restore, and guarded load tests.

**Architecture:** Caddy is the only public service and proxies to the Rust application on a private Compose network. The Rust server derives limiter identity from forwarding headers only when the socket peer belongs to an explicit trusted CIDR, loads secrets from mutually exclusive environment/file sources, emits bounded Prometheus metrics on an internal listener, and logs structured JSON. PostgreSQL remains private and is accessed by a least-privileged runtime role; backup, restore, monitoring, and load-test tooling are repository-owned and fail closed.

**Tech Stack:** Rust 2024, Axum 0.8, tower/tower-governor, SQLx/PostgreSQL 16+, Caddy 2, Docker Compose, Prometheus, postgres_exporter, Grafana provisioning, POSIX shell, k6 JavaScript.

**Spec:** `docs/superpowers/specs/2026-09-24-production-infrastructure-design.md`

## Global Constraints

- Do not edit published SQL migrations; schema changes require a new migration.
- Do not commit domains, credentials, private keys, database URLs, session cookies, generated backups, or load-test result files.
- The production application and PostgreSQL must not publish host ports; only Caddy publishes 80/443.
- Forwarding headers are trusted only from `TRUSTED_PROXY_CIDRS`; invalid input falls back to the socket peer.
- Every sensitive setting accepts exactly one of direct environment value or `_FILE` source.
- Metrics labels must never contain user identifiers, public UUIDs, secrets, raw paths, SQL, or unbounded values.
- Restore and write-load tests must refuse production-like targets unless an explicit acknowledgement is supplied.
- Tool-dependent checks absent from the workstation must report `NOT VERIFIED`, never `PASS`.
- Production database runtime roles must not own schema objects and must not have superuser, `BYPASSRLS`, `CREATEROLE`, or `CREATEDB`.
- `CC` is the only internal ledger/API currency; current `*_microcredits` columns mean micro-CC for migration compatibility.
- RUB appears only at a future payment boundary and is converted by a server-recorded exchange-rate/payment event; clients never supply a rate or direct ledger amount.

## Review Focus

- A direct client sends a forged `X-Forwarded-For`: limiter identity must remain the socket peer; Task 2 adds this test.
- A trusted proxy sends a malformed or mixed IPv4/IPv6 chain: extraction must fail closed or choose the first untrusted hop deterministically; Task 2 adds these tests.
- Both `SECRET` and `SECRET_FILE` are set, or a secret file is empty/non-regular/oversized: startup must fail without leaking content; Task 1 adds these tests.
- A metric request contains arbitrary UUID paths or secrets: labels must remain normalized and bounded; Task 3 adds this test.
- A restore or write load test points at the configured application database/unknown remote host: the command must stop before a destructive operation; Tasks 6 and 7 add negative safety tests.

---

### Task 1: File-backed production secrets

**Files:**
- Modify: `crates/server/src/config.rs`
- Modify: `.env.example`
- Modify: `.gitignore`

**Interfaces:**
- Consumes: environment lookup closure already used by `ServerConfig::from_values`.
- Produces: `read_secret(name, value_lookup) -> Result<String, ConfigError>` behavior supporting `NAME` or `NAME_FILE`; unchanged public `ServerConfig` accessors.

- [ ] **Step 1: Add failing secret-source tests**

Add tests in `crates/server/src/config.rs` proving:

```rust
#[test]
fn secret_must_have_exactly_one_nonempty_source() {
    // direct only succeeds; file only succeeds; both fail; neither fails.
}

#[test]
fn secret_file_rejects_empty_directory_and_oversized_content_without_leaking() {
    // use a unique std::env::temp_dir child and remove it after the assertion.
}

#[test]
fn config_debug_never_contains_direct_or_file_backed_secrets() {
    // assert neither secret value nor DATABASE_URL occurs in Debug output.
}
```

Use files containing one trailing newline and a 65 KiB fixture to pin trimming
and the 64 KiB maximum. Error assertions check variants, not secret text.

- [ ] **Step 2: Run tests and confirm the new contract fails**

Run:

```bash
cargo test -p server config::tests::secret -- --nocapture
```

Expected: compilation failure because the new variants/helper do not exist, or assertion failure because `_FILE` is unsupported.

- [ ] **Step 3: Implement fail-closed secret resolution**

In `config.rs`, add:

```rust
const MAX_SECRET_BYTES: u64 = 64 * 1024;

fn required_secret(
    name: &'static str,
    value: &mut impl FnMut(&str) -> Option<String>,
) -> Result<String, ConfigError>;
```

It constructs `format!("{name}_FILE")`, rejects both sources, validates file
metadata with `is_file()`, rejects `len() == 0` or `len() > MAX_SECRET_BYTES`,
reads UTF-8 once, removes exactly one `\n` and optional preceding `\r`, and
rejects an empty result. Add sanitized variants:

```rust
ConflictingSecretSources(&'static str),
InvalidSecretFile(&'static str),
UnreadableSecretFile(&'static str),
```

Use it for `DATABASE_URL`, `QUOTE_SIGNING_KEY`, and `QUOTE_SEED_KEY`.

- [ ] **Step 4: Document source names and ignore generated secret artifacts**

Update `.env.example` with commented `_FILE` alternatives and a warning that
each pair is mutually exclusive. Add `deploy/secrets/`, `backups/`, and
`load/results/` to `.gitignore` before later tasks create those directories.

- [ ] **Step 5: Verify Task 1**

Run:

```bash
cargo fmt --all -- --check
cargo test -p server config::tests
cargo clippy -p server --all-targets -- -D warnings
git diff --check
```

- [ ] **Step 6: Commit Task 1**

```bash
git add crates/server/src/config.rs .env.example .gitignore
git commit -m "feat(server): load production secrets from files"
```

### Task 2: Trusted proxy client-IP extraction and limiter keys

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/api/Cargo.toml`
- Create: `crates/api/src/client_ip.rs`
- Modify: `crates/api/src/lib.rs`
- Modify: `crates/api/src/router.rs`
- Modify: `crates/server/src/config.rs`
- Modify: `crates/server/src/main.rs`
- Modify: `.env.example`

**Interfaces:**
- Consumes: Axum `ConnectInfo<SocketAddr>`, `X-Forwarded-For`, configured CIDRs.
- Produces: `TrustedProxyConfig::parse(&str)`, `resolve_client_ip(peer, header) -> IpAddr`, and a tower-governor key extractor returning the resolved `IpAddr`.

- [ ] **Step 1: Add failing pure extraction tests**

Create `client_ip.rs` tests for:

```rust
direct_peer_ignores_forged_forwarded_for();
trusted_proxy_uses_first_untrusted_hop_from_the_right();
trusted_multi_hop_chain_discards_known_proxies();
malformed_empty_or_non_ip_chain_falls_back_to_peer();
ipv4_and_ipv6_cidrs_are_supported();
invalid_cidr_configuration_is_rejected();
```

Example invariant: trusted peer `172.30.0.2`, header
`203.0.113.7, 172.30.0.3` and trusted range `172.30.0.0/24` resolves to
`203.0.113.7`; untrusted peer `198.51.100.4` resolves to itself regardless of
the header.

- [ ] **Step 2: Run the pure tests and observe failure**

```bash
cargo test -p api client_ip -- --nocapture
```

Expected: module/types are absent.

- [ ] **Step 3: Implement the resolver with `ipnet`**

Add workspace dependency `ipnet = "2"`. Implement immutable
`TrustedProxyConfig { networks: Vec<IpNet> }`, reject empty elements and invalid
CIDRs, and keep an empty configuration valid. Parse the header only when the
peer is trusted. Reject the entire header when any element is empty, contains a
port, or is not an `IpAddr`.

- [ ] **Step 4: Integrate the resolver with tower-governor**

Implement a custom `KeyExtractor` in `client_ip.rs` that reads
`ConnectInfo<SocketAddr>` and the header, resolves the client address, and
returns a stable IP key. Extend `RouterConfig` with
`trusted_proxies: TrustedProxyConfig`; production supplies it, in-process tests
default to empty.

Replace the default governor extractor in `router.rs` with this extractor. No
middleware may overwrite `ConnectInfo`.

- [ ] **Step 5: Add router regressions for spoofing and bucket isolation**

Extend router tests with two requests behind trusted peer `172.30.0.2` and
different forwarded clients; both first requests succeed with burst `1`.
Repeated requests from one forwarded client return `429`. A request from an
untrusted peer with changing headers stays in the same bucket and returns
`429` on its second request.

- [ ] **Step 6: Parse production CIDRs at startup**

Add `trusted_proxy_cidrs: TrustedProxyConfig` to `ServerConfig`, sourced from
`TRUSTED_PROXY_CIDRS` with an empty default. Invalid configuration returns
`ConfigError::InvalidTrustedProxyCidrs` without starting the listener. Wire it
into `RouterConfig` and document an example in `.env.example`.

- [ ] **Step 7: Verify Task 2**

```bash
cargo fmt --all -- --check
cargo test -p api client_ip
cargo test -p api router::tests::configured_rate_limit
cargo test -p server config::tests
cargo clippy --workspace --all-targets -- -D warnings
```

- [ ] **Step 8: Commit Task 2**

```bash
git add Cargo.toml Cargo.lock crates/api crates/server .env.example
git commit -m "feat(api): trust proxy client IPs explicitly"
```

### Task 3: Private Prometheus metrics and structured logging

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/api/Cargo.toml`
- Create: `crates/api/src/metrics.rs`
- Modify: `crates/api/src/lib.rs`
- Modify: `crates/api/src/router.rs`
- Modify: `crates/server/Cargo.toml`
- Modify: `crates/server/src/config.rs`
- Modify: `crates/server/src/main.rs`
- Create: `crates/server/src/metrics_server.rs`
- Modify: `.env.example`

**Interfaces:**
- Consumes: normalized Axum matched path, method/status/duration, database pool size, server start instant.
- Produces: a Prometheus `Handle`, HTTP instrumentation middleware, and a second internal listener serving only `GET /metrics` plus `GET /health/live`.

- [ ] **Step 1: Add failing bounded-label middleware tests**

Tests must issue requests to `/api/v1/me/inventory/<two different UUIDs>` and
unknown paths, then render metrics and assert that no UUID/raw path/cookie value
appears. Assert route labels are the matched template or `unmatched`, and status
and method labels are bounded.

- [ ] **Step 2: Run the tests and observe failure**

```bash
cargo test -p api metrics -- --nocapture
```

- [ ] **Step 3: Implement a single Prometheus recorder and middleware**

Add compatible `metrics` and `metrics-exporter-prometheus` dependencies. Install
one recorder during server startup and pass its handle into API state or router
configuration. Record counters/histograms/gauges with fixed metric names and
`MatchedPath` templates only. Increment the explicit rate-limit rejection
counter in a response middleware when status is `429`.

- [ ] **Step 4: Add the private metrics listener**

Add `METRICS_HOST` (default `127.0.0.1`) and `METRICS_PORT` (default `9090`) to
`ServerConfig`. `metrics_server::serve` binds a separate listener, exposes only
metrics and liveness, and shares the existing shutdown future. If the metrics
listener cannot bind, startup fails instead of silently running blind.

- [ ] **Step 5: Add structured logging selection**

Add `LOG_FORMAT=json|pretty`, default `json`. JSON output includes timestamp,
level, target, fields and request ID emitted by existing tracing. Invalid values
fail startup. Tests prove valid variants and reject any other string. Ensure no
secret-bearing config field is added to tracing.

- [ ] **Step 6: Verify Task 3**

```bash
cargo fmt --all -- --check
cargo test -p api metrics
cargo test -p server
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

- [ ] **Step 7: Commit Task 3**

```bash
git add Cargo.toml Cargo.lock crates/api crates/server .env.example
git commit -m "feat(observability): add private metrics and json logs"
```

### Task 4: Least-privilege runtime role verification

**Files:**
- Create: `scripts/verify_runtime_role.sh`
- Create: `db/tests/007_runtime_role_ownership.sql`
- Modify: `scripts/verify.sh`
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `RUNTIME_DATABASE_URL`, PostgreSQL catalogs.
- Produces: exit 0 only when the connected role is `contracter_runtime` and owns no application database/schema/table/function while lacking elevated role flags.

- [ ] **Step 1: Write catalog assertions that fail for an owner**

The SQL test must assert:

```sql
NOT rolsuper AND NOT rolbypassrls AND NOT rolcreaterole AND NOT rolcreatedb;
current_user = 'contracter_runtime';
NOT pg_has_role(current_user, schema_owner, 'MEMBER');
zero owned application relations and functions;
no CREATE privilege on schema public;
```

It must also prove the granted runtime API functions remain executable.

- [ ] **Step 2: Add the shell wrapper and negative safety run**

`verify_runtime_role.sh` requires `RUNTIME_DATABASE_URL`, calls `psql -X
--set=ON_ERROR_STOP=1`, and never echoes the URL. Run it first with an owner URL
and confirm nonzero exit.

- [ ] **Step 3: Provision a login runtime role in CI and verify it**

CI creates a unique test-only login credential, grants membership/access
without ownership, passes its URL through the job environment, and executes the
script after migrations. The credential must not be a repository constant used
outside CI.

- [ ] **Step 4: Verify Task 4**

```bash
sh -n scripts/verify_runtime_role.sh scripts/verify.sh
TEST_DATABASE_URL=postgresql:///contracter_role_test ./db/verify.sh
RUNTIME_DATABASE_URL="$RUNTIME_TEST_DATABASE_URL" ./scripts/verify_runtime_role.sh
```

Expected: owner negative test fails; restricted-role run succeeds.

- [ ] **Step 5: Commit Task 4**

```bash
git add scripts/verify_runtime_role.sh scripts/verify.sh db/tests/007_runtime_role_ownership.sql .github/workflows/ci.yml
git commit -m "test(db): verify production runtime privileges"
```

### Task 5: Caddy and Compose production reference

**Files:**
- Create: `deploy/compose.production.yml`
- Create: `deploy/production.env.example`
- Create: `deploy/caddy/Caddyfile`
- Create: `deploy/prometheus/prometheus.yml`
- Create: `deploy/prometheus/alerts.yml`
- Create: `deploy/grafana/provisioning/datasources/prometheus.yml`
- Create: `deploy/grafana/provisioning/dashboards/provider.yml`
- Create: `deploy/grafana/dashboards/contracter-overview.json`
- Create: `scripts/validate_deployment.sh`
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: prebuilt server image, `CONTRACTER_DOMAIN`, external Docker secrets, explicit networks/volumes.
- Produces: one public TLS edge and private application/database/monitoring services.

- [ ] **Step 1: Add a failing static deployment validator**

`validate_deployment.sh` must inspect the rendered Compose config when Docker is
available and otherwise perform conservative source assertions. It fails unless:

- only `caddy` has `ports`;
- Caddy publishes exactly `80:80` and `443:443`;
- application, database, Prometheus, exporter and Grafana have no host ports;
- database/app secrets use external Compose secrets;
- containers use `read_only`, `security_opt: no-new-privileges:true`, dropped
  capabilities, health checks, restart policy, and explicit networks where
  compatible with their images;
- application `TRUSTED_PROXY_CIDRS` equals the explicit edge subnet;
- metrics has no Caddy route.

Run the validator before creating the Compose file and confirm failure.

- [ ] **Step 2: Implement the production Compose topology**

Create explicit `edge`, `app_db`, and `monitoring` networks. Do not publish the
application or PostgreSQL. Declare `database_url`, `quote_signing_key`,
`quote_seed_key`, PostgreSQL runtime/admin credential files and Grafana admin
password as external secrets. Use an externally supplied immutable server image
tag, not `latest` and not a production build containing source secrets.

- [ ] **Step 3: Implement Caddy policy**

The Caddyfile uses `{$CONTRACTER_DOMAIN:?required}`, reverse proxies only normal
application routes, overwrites forwarding headers, sets timeouts/body limits,
does not expose metrics, and relies on automatic HTTPS. It never embeds an
email, domain, or credential.

- [ ] **Step 4: Add Prometheus/Grafana provisioning and alerts**

Prometheus scrapes only private DNS names. Alerts cover target down/readiness,
5xx ratio, p95 latency, missing recent backup success and PostgreSQL exporter
down. Grafana provisioning references Prometheus by internal service name and
contains no credential. The dashboard uses only bounded labels.

- [ ] **Step 5: Add optional tool-aware validation**

The validator runs `docker compose config`, `caddy validate`, and `promtool
check rules` when available. Missing tools produce
`NOT VERIFIED: missing binary docker`, `NOT VERIFIED: missing binary caddy`, or
`NOT VERIFIED: missing binary promtool`; structural checks must still pass.

- [ ] **Step 6: Verify Task 5**

```bash
sh -n scripts/validate_deployment.sh
./scripts/validate_deployment.sh
rg -n '(password|secret|postgresql://)' deploy --glob '!*.example' && exit 1 || true
git diff --check
```

- [ ] **Step 7: Commit Task 5**

```bash
git add deploy scripts/validate_deployment.sh .github/workflows/ci.yml
git commit -m "feat(deploy): add private Caddy production topology"
```

### Task 6: Safe PostgreSQL backup and restore drill

**Files:**
- Create: `scripts/lib/postgres_safety.sh`
- Create: `scripts/backup_postgres.sh`
- Create: `scripts/restore_drill.sh`
- Create: `scripts/tests/backup_restore_safety.sh`
- Create: `deploy/backup/README.md`
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `BACKUP_DATABASE_URL_FILE`, `BACKUP_DIR`, `RESTORE_DATABASE_URL`, archive and manifest.
- Produces: validated custom archive, SHA-256 manifest, backup success metric, and verified disposable restore database named `contracter_restore_drill_*`.

- [ ] **Step 1: Write failing shell safety tests**

The test script stubs `pg_dump`, `pg_restore`, and `psql` in a temporary PATH and
asserts no destructive command is called when:

- URL/secret file is absent;
- backup destination is `/`, `$HOME`, repository root, or a symlink;
- restore database lacks prefix `contracter_restore_drill_`;
- restore database equals the source database;
- restore host is non-local without `ALLOW_REMOTE_RESTORE_DRILL=yes`;
- manifest hash mismatches the archive.

- [ ] **Step 2: Run tests and observe failure**

```bash
sh scripts/tests/backup_restore_safety.sh
```

- [ ] **Step 3: Implement shared URL and path safety**

`postgres_safety.sh` must parse database name and host via `psql` queries rather
than printing credentials, reject broad/symlink destinations, create files with
`umask 077`, and expose functions used by both scripts. It must never use
`eval`, unquoted globs for deletion, or URLs in diagnostic output.

- [ ] **Step 4: Implement atomic backup and manifest**

Write the dump to a `mktemp` file inside the validated destination, use
`--format=custom --no-owner --no-acl`, validate with `pg_restore --list`, compute
SHA-256 using `shasum -a 256` or `sha256sum`, write a manifest, fsync where the
platform supports it, then atomically rename. Retention removes only files that
match the strict Contracter backup filename regex and only after success.

- [ ] **Step 5: Implement guarded restore verification**

Verify checksum, create an explicitly named disposable database, restore with
`--no-owner --no-acl --exit-on-error`, run SQLx migrations through the existing
server/database tooling, execute database invariant/reconciliation checks, and
leave the database available for inspection by default. `DROP_AFTER_VERIFY=yes`
may drop only the validated prefixed database.

- [ ] **Step 6: Run a real local backup/restore drill**

Create a new source database with an explicit task-specific name, migrate/seed
it using repository scripts, back it up, restore to
`contracter_restore_drill_$(date -u +%Y%m%d%H%M%S)`, verify, then explicitly
drop only those two task databases after recording success. No shared or
production URL is used.

- [ ] **Step 7: Add CI restore drill**

After PostgreSQL verification, CI creates a disposable source, executes backup
and restore, checks the restored database, and removes job artifacts. Uploading
database content as a CI artifact is forbidden.

- [ ] **Step 8: Verify and commit Task 6**

```bash
sh -n scripts/lib/postgres_safety.sh scripts/backup_postgres.sh scripts/restore_drill.sh scripts/tests/backup_restore_safety.sh
sh scripts/tests/backup_restore_safety.sh
git diff --check
git add scripts deploy/backup .github/workflows/ci.yml
git commit -m "feat(ops): add verified PostgreSQL recovery drill"
```

### Task 7: Guarded k6 load scenarios

**Files:**
- Create: `load/k6/lib/config.js`
- Create: `load/k6/smoke.js`
- Create: `load/k6/steady.js`
- Create: `load/k6/authenticated.js`
- Create: `load/k6/write_concurrency.js`
- Create: `scripts/run_load_test.sh`
- Create: `scripts/tests/load_test_safety.sh`
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `BASE_URL`, optional `SESSION_COOKIE`, `LOAD_TEST_MODE`, `LOAD_TEST_DATABASE`, explicit write acknowledgement.
- Produces: bounded k6 runs and optional ignored JSON summaries under `load/results/`.

- [ ] **Step 1: Write failing wrapper safety tests**

Stub `k6` and prove the wrapper rejects absent/invalid HTTPS base URLs (localhost
HTTP is allowed), userinfo/query/fragment URLs, unbounded duration/VUs, missing
session for authenticated mode, write mode without a database containing
`load_test`, and any non-local write target without
`ALLOW_REMOTE_WRITE_LOAD_TEST=yes`.

- [ ] **Step 2: Implement strict configuration and wrapper**

Defaults: smoke `1 VU/30s`, steady ramp `1 -> 20 -> 1` over five minutes,
authenticated max `10 VUs`, write concurrency max `5 VUs/60s`. Shell validates
integer caps before invoking k6. JavaScript reads values through `__ENV`, never
contains a cookie/password literal, and emits no response bodies on failure.

- [ ] **Step 3: Implement scenarios and thresholds**

- Smoke: `/health/live`, `/health/ready`, catalog and market reads; zero failed
  checks and p95 below 500 ms locally.
- Steady: weighted public reads; `http_req_failed < 1%`, p95 below 750 ms.
- Authenticated: `/me`, balance, inventory, history and active quote; no write.
- Write concurrency: repeats one idempotency key and distinct keys against
  disposable fixtures, then checks response equivalence/conflict semantics.

Use tags with fixed route names, never raw URLs containing UUIDs.

- [ ] **Step 4: Add tool-aware CI smoke**

CI may install a pinned k6 version and run smoke against a locally started
server with disposable PostgreSQL/secrets. If installation is intentionally
omitted, deployment verification must print `NOT VERIFIED: k6`.

- [ ] **Step 5: Verify and commit Task 7**

```bash
sh -n scripts/run_load_test.sh scripts/tests/load_test_safety.sh
sh scripts/tests/load_test_safety.sh
./scripts/run_load_test.sh smoke  # expected NOT VERIFIED locally when k6 is absent
git add load scripts .github/workflows/ci.yml
git commit -m "test(load): add guarded production load scenarios"
```

### Task 8: Production runbook and unified verification

**Files:**
- Create: `docs/PRODUCTION_RUNBOOK.md`
- Create: `scripts/verify_production_readiness.sh`
- Modify: `README.md`
- Modify: `docs/FINAL_BACKEND_AUDIT.md`
- Modify: `docs/SECURITY_REPORT.md`
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: all artifacts from Tasks 1-7.
- Produces: one operator workflow and one non-destructive readiness command with explicit `PASS`, `FAIL`, and `NOT VERIFIED` outcomes.

- [ ] **Step 1: Create the unified readiness verifier**

The script runs:

```text
Rust format/check/clippy/test
dependency audit
shell syntax and safety tests
deployment structural validation
runtime role validation when URL is supplied
backup/restore drill only when explicit drill variables are supplied
Caddy/Compose/promtool/k6 checks when binaries are available
secret-pattern scan limited to tracked source/config files
git diff --check
```

It exits nonzero on any required failure, prints each environment-dependent
skip as `NOT VERIFIED: reason`, and never dumps environment variables.

- [ ] **Step 2: Write the operator runbook**

Document prerequisites, DNS/firewall, secret generation and mounting, database
role provisioning, first deployment, certificate verification, trusted-IP test,
metrics/alerts, backup schedule, restore drill, safe load tests, secret rotation,
rollback and incident checks. Commands use placeholders such as
`api.example.invalid` only where visibly marked and never claim execution.

- [ ] **Step 3: Update repository status documents**

README links the production runbook. Audit reports distinguish newly verified
source/local controls from public TLS, real alert delivery, off-site durability
and production capacity, which remain `NOT VERIFIED` until deployed.

- [ ] **Step 4: Run the complete local verification**

Create two new task-specific PostgreSQL databases, including an integration DB
owned by `anonymous`, then run:

```bash
TEST_DATABASE_URL=postgresql:///contracter_prod_readiness_sql \
TEST_DATABASE_URL_INTEGRATION=postgresql:///contracter_prod_readiness_integration \
./scripts/verify.sh
cargo check --workspace --all-targets --all-features
cargo test --workspace --all-features
cargo audit
./scripts/verify_production_readiness.sh
git diff --check
git status --short
```

Read every exit code. Record missing Docker/Caddy/Prometheus/k6 checks as
`NOT VERIFIED`; do not reinterpret them as successful.

- [ ] **Step 5: Perform the acceptance audit**

Map all ten acceptance criteria in the spec to current files and command output.
Confirm that each is either proven or, only where the spec explicitly requires
real production state, accurately marked `NOT VERIFIED`. Inspect tracked files
for secret material and confirm no generated backup/result is staged.

- [ ] **Step 6: Commit Task 8**

```bash
git add docs README.md scripts/verify_production_readiness.sh .github/workflows/ci.yml
git commit -m "docs: add production operations and readiness gate"
```

- [ ] **Step 7: Final branch review**

```bash
git log --oneline --decorate -12
git diff --stat origin/codex/quote-write-v2...HEAD
git status --short --branch
```

Report local evidence, exact `NOT VERIFIED` items, and the fact that no external
deployment, push, merge, paid service, real secret, or production database was
changed.
