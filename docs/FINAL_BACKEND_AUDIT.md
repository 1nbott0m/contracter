# Финальный аудит backend Contracter

Дата проверки: 2026-09-24. Ветка: `codex/quote-write-v2`.

## EXECUTIVE SUMMARY

Backend реализует защищённые owner-bound API для аутентификации, каталога,
инвентаря, рынка, истории, allocation/quote и acceptance. Критические денежные
операции выполняются атомарными PostgreSQL-функциями; клиент не задаёт цену,
вероятность, владельца, итоговый предмет или баланс.

В ходе работы закрыты подтверждённые разрывы в ограничениях экономики,
криптографической целостности quote, защите пользовательского баланса,
application-level rate limiting и запуске полного PostgreSQL integration suite.
Свежая полная проверка перед финальным статусом должна выполняться командой
`./scripts/verify.sh` с двумя отдельными свежими базами, вторая из которых
принадлежит роли `anonymous`.

Это состояние близко к production-ready на уровне исходного кода и БД, но не
является доказательством готовности конкретного production deployment: TLS,
reverse proxy, реальные секреты, backup/restore, мониторинг и нагрузочные
характеристики проверяются отдельно в целевой инфраструктуре.

## DATA FLOW

```text
HTTP request
  -> Axum middleware (request id, security headers, rate limit)
  -> authenticated session -> server-side user id
  -> application validation and deterministic economy core
  -> owner-bound DB reader / atomic PostgreSQL writer
  -> ledger + inventory + quote/market event in one transaction
  -> public response without secrets or internal ids
```

Quote flow:

```text
allocation -> encrypted seed envelope -> canonical server projections
-> exact weights/prices -> deterministic selection -> complete Ed25519 signature
-> atomic quote persistence -> owner-bound idempotent acceptance
-> inventory movement + ledger settlement + risk release
```

## ISSUES FOUND

- P0: 0 confirmed.
- P1: 5 confirmed: absent server-side 20 CC floor; bypassable 15,000 CC result
  cap; incomplete quote signature boundary; no hard nonnegative invariant for
  user balances; no application-level request throttling.
- P2: 3 confirmed: incomplete history API/readers; integration CI did not create
  the required owner-role database; deployment security contract was
  insufficiently documented.
- P3: 2 confirmed: missing regression coverage for response-wide HSTS and
  configurable limiter validation.

Counts describe confirmed findings handled by this audit, not a claim that no
other defect can exist.

## ISSUES FIXED

- `20 CC` minimum is enforced by PostgreSQL for quote inputs.
- `15,000 CC` is an absolute cap for every persisted quote outcome buyback.
- Internal balances and API money values are explicitly labelled `CC`; storage
  `*_microcredits` means micro-CC. RUB is reserved for a future payment input
  and is never a client-controlled ledger currency.
- Both limits have boundary and bypass regression tests.
- Quote signatures cover ownership, allocation and quote identity, versions,
  expiry, economic totals, selected position, ordered inputs and ordered
  outcomes. The Rust DB writer recomputes and verifies the signature before SQL.
- A deferrable constraint trigger rejects negative `user_credit` balances at
  transaction commit; system/treasury accounts intentionally remain outside
  that user invariant.
- The router applies a configurable peer-IP token-bucket limiter and returns
  `429` after exhaustion.
- HSTS is attached to every API response path, including public health routes.
- Owner-bound contract, ledger, inventory-event and risk-history readers were
  added with typed application/API models.
- HTTP tests cover owner isolation and idempotent quote acceptance; database
  tests cover concurrent ledger/idempotency paths.
- CI provisions two PostgreSQL databases, including an integration database
  owned by `anonymous`, then runs the repository verifier.
- README and `.env.example` document quote secrets, limits, rate limiting and
  the reverse-proxy/TLS responsibility boundary.

## REMAINING ISSUES

- Risk policy activation remains deliberately fail-closed until production
  minimum-notional and dispersion values are calibrated with real data.
- The limiter keys on the socket peer. A reverse proxy must preserve the true
  source peer through a trusted transport mechanism; blindly trusting public
  forwarding headers would weaken spoofing resistance.
- TLS certificate issuance, protocol/cipher configuration and HTTP-to-HTTPS
  redirects belong to the deployment proxy and are not present in this repo.
- No production load test, disaster-recovery exercise, secret rotation drill or
  observability/SLO validation was performed.
- External marketplace/payment integrations are not implemented by this
  backend and were not inferred or fabricated during the audit.

## TEST RESULTS

The following verification set was executed successfully during this audit:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- `cargo audit` (295 dependencies, no reported vulnerability at execution time)
- `TEST_DATABASE_URL=... TEST_DATABASE_URL_INTEGRATION=... ./scripts/verify.sh`
  on fresh databases `contracter_final_sql_v4` and
  `contracter_final_integration_v4`, with the latter owned by `anonymous`
- targeted limiter and global-HSTS regressions after the final HTTP changes

`scripts/verify.sh` covers SQL invariants, migrations, all ignored database/API/
application integration suites, market, quote acceptance, owner isolation,
ledger and concurrency cases. The final handoff must cite a fresh rerun rather
than relying only on this historical record.

## SECURITY

Reviewed categories: authentication/session handling, owner isolation, server
authority over economic values, secret exposure, quote signing, secure seed
storage, idempotency/replay, concurrent writes, SQL privileges, negative
balances, request throttling, response headers and dependency advisories.

Detailed findings and deployment-only checks are recorded in
[`SECURITY_REPORT.md`](SECURITY_REPORT.md).

## DATABASE

- Published migrations were preserved; changes use new migrations.
- SQLx applies migrations idempotently on a fresh database.
- Runtime functions are explicitly revoked from `PUBLIC` and granted narrowly.
- Market and acceptance writers combine ownership, stock, ledger, inventory,
  idempotency and risk transitions atomically.
- Append-only risk history records before/after state and actor/reason context.
- Reconciliation tests detect direct owner-level tampering.
- Quote value limits and nonnegative user balances have database-level
  enforcement, not only API validation.

The database-specific evidence remains in
[`database/FINAL_DATABASE_REPORT.md`](database/FINAL_DATABASE_REPORT.md).

## ARCHITECTURE

The layering is coherent: `api` owns transport, `application` owns use cases and
server-side orchestration, `economy-core` owns exact deterministic arithmetic,
and `db` owns typed persistence boundaries. PostgreSQL is the final authority
for ownership and atomic financial/inventory transitions. This intentionally
duplicates a small set of critical validations across application and database
boundaries to make direct-call bypasses fail closed.

## FILES CHANGED

The branch changes 61 implementation/test/configuration files relative to its
remote base. Principal groups:

- `crates/api`: history/market/quote routes, rate limiting, security headers,
  state wiring and HTTP regressions.
- `crates/application`: allocation, quote construction/signing, market and
  owner-history use cases.
- `crates/db`: typed quote/market/history writers and readers plus integration
  tests.
- `crates/economy-core`: exact pricing/trade-up invariants and boundary tests.
- `db/migrations/0022..0035` and `db/tests`: lifecycle, market, risk history,
  value limits and nonnegative balances.
- `.github/workflows/ci.yml`, `scripts/verify.sh`,
  `scripts/prepare_integration_db.sh`: reproducible PostgreSQL verification.
- `README.md`, `.env.example`, `docs`: operator contract and audit evidence.

## NOT VERIFIED

- Production TLS certificate, cipher suite, redirect and proxy configuration.
- Correct source-IP preservation by the production reverse proxy.
- Production secret manager, rotation and incident-response procedures.
- Backup restoration and point-in-time recovery on the production database.
- Production traffic capacity, latency percentiles and denial-of-service limits.
- Correctness/freshness of real catalog, price, reserve and risk calibration
  data.
- External providers and production network/IAM controls.

## FINAL STATE

The repository has automated proof for the requested backend invariants and a
fresh-database verification path suitable for CI. No confirmed P0 finding
remains in the audited code path. Deployment readiness is conditional on the
manual infrastructure checks listed under `NOT VERIFIED`; therefore this report
does not assign an artificial quality or security score.
