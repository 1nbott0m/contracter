# Contracter Production Infrastructure Design

Date: 2026-09-24  
Status: approved for implementation planning

## 1. Purpose

This increment turns the verified backend into a reproducible production
reference deployment. It must provide a trusted TLS edge, preserve the real
client IP without trusting arbitrary forwarding headers, load secrets without
placing their values in the repository or process arguments, expose internal
monitoring, prove PostgreSQL backup restoration, and provide repeatable load
tests. The platform's internal ledger currency is `CC` (Contracter Coins),
stored as integer micro-CC. Rubles are only an external funding input: a future
payment writer must convert a provider-confirmed RUB amount using a
server-recorded, versioned exchange rate and must never accept a client-supplied
rate or balance adjustment.

The repository will contain deployment artifacts and executable verification,
but it will not claim that an unknown production host, DNS record, certificate,
secret manager, firewall, or backup destination has been configured. Those
facts can only be verified against the eventual deployment environment.

## 2. Chosen architecture

The reference deployment uses Docker Compose and Caddy:

```text
Internet
  -> host firewall: 80/443 only
  -> Caddy: HTTP redirect, TLS, trusted forwarding headers
  -> private compose network
  -> contracter-server: HTTP, application rate limit, metrics
  -> private compose network
  -> PostgreSQL: runtime role, no public host port

Prometheus -> private metrics endpoint
Grafana    -> Prometheus
postgres_exporter -> restricted monitoring role
backup job -> pg_dump custom archive + SHA-256 manifest
```

Caddy and the application are the only components on the edge network. The
application and PostgreSQL share a separate private network. PostgreSQL,
Prometheus, Grafana, exporter, and application metrics are not published on a
host interface by the production Compose file. Caddy is the only service with
host ports.

Kubernetes and a host-specific Nginx/systemd setup are intentionally excluded:
they add operational choices that cannot be validated on the current machine
and are unnecessary for a single-host reference deployment.

### Currency boundary

All wallet, quote, market, ledger and API money values are denominated in `CC`.
The existing `*_microcredits` storage names remain for migration compatibility;
they mean micro-CC, not RUB, USD or fiat. API money responses include
`currency_code: "CC"`. External sale evidence may retain its source currency
metadata, but it is converted into the internal valuation before it can affect
CC balances.

RUB top-ups are a separate payment boundary. The eventual payment webhook must
authenticate the provider event, persist `amount_rub_kopecks`, a versioned
`rub_to_cc_rate`, and the resulting `amount_microcredits` in one idempotent
ledger operation. The user request cannot choose the rate, target account,
credit amount, or currency. No payment provider or conversion rate is invented
by this repository until one is selected.

## 3. TLS and reverse proxy

Caddy receives the required `CONTRACTER_DOMAIN` value at deployment time. No
domain is invented or committed. It must:

- publish ports 80 and 443 only;
- redirect cleartext HTTP to HTTPS;
- obtain and renew a public certificate for the configured domain;
- proxy to `contracter-server:8080` over the private edge network;
- replace, rather than append untrusted client values for, forwarding headers;
- pass `X-Forwarded-For`, `X-Forwarded-Proto`, and `X-Forwarded-Host`;
- apply conservative request/header timeouts and body limits compatible with
  the application's 256 KiB limit;
- deny access to the internal metrics route;
- retain application HSTS while avoiding contradictory duplicate policies.

TLS certificate issuance and public DNS are manual production gates. Local
verification uses Caddy's internal CA or configuration validation where
available; it is not evidence of a publicly trusted certificate.

## 4. Trusted client IP and rate limiting

The current socket-peer limiter would see only the reverse proxy. The backend
will gain an explicit trusted-proxy policy:

- `TRUSTED_PROXY_CIDRS` is an optional comma-separated list of CIDRs.
- With an empty list, the socket peer is the client identity and all forwarding
  headers are ignored.
- With a configured list, forwarding headers are considered only if the socket
  peer belongs to a trusted CIDR.
- The parser walks `X-Forwarded-For` from right to left, discards trusted proxy
  hops, and selects the first untrusted address as the client.
- Malformed, empty, ambiguous, or non-IP entries fail closed to the socket peer;
  they never disable limiting.
- The chosen address is stored in request extensions and used by the
  application limiter. It is not accepted from a request body or query string.

The production Compose network CIDR is explicit and is the only proxy range in
the reference environment. Tests must prove that a direct client cannot spoof
`X-Forwarded-For`, a trusted proxy can convey the client address, multi-hop
chains resolve correctly, malformed input falls back safely, and two clients
behind one proxy receive independent buckets.

## 5. Production secrets

The server accepts every sensitive setting through either the existing variable
or a corresponding file variable:

- `DATABASE_URL` or `DATABASE_URL_FILE`;
- `QUOTE_SIGNING_KEY` or `QUOTE_SIGNING_KEY_FILE`;
- `QUOTE_SEED_KEY` or `QUOTE_SEED_KEY_FILE`.

Rules:

- exactly one source is allowed for each secret;
- a file is read once at startup, trimmed only for one trailing newline, and
  rejected if empty, unreadable, oversized, or not a regular file;
- errors identify the variable, never its value or file content;
- `Debug` and logs remain redacted;
- Compose mounts external Docker secrets read-only under `/run/secrets`;
- no production secret value or default credential is committed;
- rotation is performed by replacing a secret and recreating the application;
  key rotation that affects stored cryptographic material requires an explicit
  application migration and is not silently attempted.

The database runtime credential must use `contracter_runtime`, must not own the
database/schema/tables, must not be superuser, and must not have `BYPASSRLS`,
`CREATEROLE`, or `CREATEDB`. A verification script will query these invariants
and fail when connected with an over-privileged runtime identity.

## 6. Monitoring and logging

The application exposes Prometheus-format metrics on a separate internal
listener, configured by `METRICS_HOST` and `METRICS_PORT`. Production defaults
bind the metrics listener to the container's private interface; it is never
routed through Caddy.

Required metrics:

- HTTP requests by method, normalized route, and status;
- request duration histogram;
- in-flight requests;
- rate-limit rejections;
- readiness state;
- process uptime;
- database pool size, idle connections, and acquisition failures where the
  current SQLx API exposes them without high-cardinality labels.

Labels must not contain user IDs, UUIDs, cookies, tokens, raw paths, SQL text,
or secrets. Unknown paths use a bounded `unmatched` label.

Production logs are newline-delimited JSON with request IDs and sanitized error
messages. Local development may retain compact human-readable logs through an
explicit `LOG_FORMAT=pretty` setting.

Prometheus scrapes the application and PostgreSQL exporter on the private
monitoring network. Grafana is optional through a Compose profile and has no
committed administrator password. Alert rules cover readiness failure, elevated
5xx ratio, sustained high latency, backup age, and PostgreSQL exporter failure.

## 7. Backup and restore

Backup artifacts are logical PostgreSQL custom-format archives created with
`pg_dump --format=custom --no-owner --no-acl`. The backup process:

1. receives its connection secret through a file or `PGPASSFILE`;
2. writes to a temporary file in the destination filesystem;
3. validates the archive with `pg_restore --list`;
4. computes a SHA-256 checksum and writes a manifest containing timestamp,
   database name, PostgreSQL client version, archive filename, size, and hash;
5. atomically renames the archive and manifest into place;
6. applies retention only after a valid new backup exists;
7. exports success/failure timestamp metrics for monitoring.

The restore drill never targets the configured application database. It
requires a distinct explicit database name with a fixed safe prefix, creates
that database, restores the archive, applies repository migrations, runs
database verification/reconciliation queries, and drops the drill database only
when explicitly requested. It refuses a remote host unless an additional
operator acknowledgement is provided.

Backups are not considered proven until a restore drill succeeds. Encryption at
rest belongs to the selected backup storage/KMS and must be documented at
deployment; this repository does not invent an encryption key.

## 8. Load testing

The repository provides k6 scenarios with bounded defaults:

- `smoke`: liveness, readiness, public catalog and market reads;
- `steady`: representative read traffic with latency/error thresholds;
- `authenticated`: owner-bound read endpoints using a supplied test session;
- `write-concurrency`: market/idempotency and quote acceptance against an
  explicitly named disposable load-test database only.

Every scenario requires `BASE_URL`; authenticated/write scenarios also require
test credentials or a session cookie. The scripts refuse a production hostname
for write tests unless a separate explicit acknowledgement is present. No test
secret is committed. Thresholds are versioned, results may be exported as JSON,
and a smoke test is suitable for CI when k6 is available.

The current workstation lacks Docker, Caddy, Prometheus, Grafana, and k6.
Therefore source/configuration tests and native PostgreSQL backup/restore can be
verified here; full container startup and k6 execution remain `NOT VERIFIED`
until those tools are installed or CI supplies them.

## 9. Verification strategy

Implementation follows test-first boundaries:

- unit tests for secret source exclusivity, file validation, redaction, CIDR
  validation and trusted client-IP extraction;
- router tests proving independent rate-limit buckets behind a trusted proxy;
- tests for bounded metrics labels and internal metrics listener configuration;
- shell syntax and negative safety tests for backup/restore scripts;
- a real local `pg_dump -> pg_restore -> invariants` drill on a disposable
  PostgreSQL database;
- Compose config and Caddy validation when their binaries are available;
- Prometheus rule validation when `promtool` is available;
- k6 smoke when k6 is available;
- existing `scripts/verify.sh`, all-target/all-feature Rust checks, dependency
  audit, and clean-worktree review before completion.

Skipped tool-dependent checks must be printed as `NOT VERIFIED` with the exact
missing binary or production dependency. They cannot be silently counted as
passing.

## 10. Deliverables

- `deploy/compose.production.yml` and a secret-free environment example;
- `deploy/caddy/Caddyfile`;
- Prometheus configuration/rules and optional Grafana provisioning;
- trusted proxy/client-IP implementation and tests;
- `_FILE` secret loading and tests;
- internal metrics endpoint and structured logging configuration;
- safe `scripts/backup_postgres.sh`, `scripts/restore_drill.sh`, and tests;
- `load/k6/*.js` scenarios and safety wrapper;
- CI validation for deploy configuration, scripts, Rust code, and available
  integration drills;
- `docs/PRODUCTION_RUNBOOK.md` covering provisioning, deployment, rotation,
  recovery, monitoring, load tests, rollback, and the `NOT VERIFIED` gates.

## 11. Acceptance criteria

The increment is complete only when:

1. the application cannot be reached through a published production host port;
2. only Caddy publishes 80/443 in the reference Compose file;
3. direct clients cannot spoof limiter identity and trusted proxies preserve
   distinct real client buckets;
4. production secrets are external files and never appear in committed config,
   logs, command arguments, or generated test artifacts;
5. runtime DB-role verification rejects ownership or privilege escalation;
6. metrics are private and bounded-cardinality, with alert rules present;
7. a fresh backup restores successfully into a disposable local database and
   passes integrity checks;
8. load tests are repeatable, bounded by default, and guarded against accidental
   production writes;
9. existing backend and PostgreSQL verification remains green;
10. every environment-dependent gap is explicitly reported as `NOT VERIFIED`.

## 12. Explicit non-goals

- purchasing/configuring a real domain or cloud host;
- generating or committing production credentials;
- choosing a cloud-specific KMS, object store, firewall, or IAM provider;
- executing destructive tests against shared or production databases;
- claiming public TLS, backup off-site durability, alert delivery, or production
  capacity without evidence from the actual deployment.
