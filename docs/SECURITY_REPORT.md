# Security hardening report

Дата: 2026-09-24. Искусственный security score не используется.

## Threat model

Клиент считается полностью контролируемым атакующим. Нельзя доверять cookie,
UUID, `user_id`, цене, вероятности, редкости, выбранному результату, балансу,
состоянию инвентаря или idempotency key. Runtime database role также не должна
иметь права владельца таблиц/схемы. За пределами репозитория доверенными
компонентами остаются production secret manager, reverse proxy/TLS и PostgreSQL
администрирование.

## CRITICAL

Подтверждённых неисправленных CRITICAL-проблем в проверенных путях нет.

### Абсолютный экономический cap можно было обойти на write boundary

- Риск: CRITICAL
- Файл/функция: `db/migrations/0033_quote_value_limits.sql`,
  `assert_quote_value_limits`
- Эксплуатация: записать outcome выше 15,000 RUB через серверный/DB write path.
- Влияние: неограниченное обязательство платформы и нарушение экономики.
- Исправление: insert/update constraint trigger с абсолютным значением
  `15,000,000,000` microcredits; точный нижний предел входа также находится в БД.
- Regression test: `db/tests/006_quote_value_limits.sql`.
- Статус: FIXED, VERIFIED IN FRESH DATABASE SUITE.

### Подпись quote не покрывала полный экономический документ

- Риск: CRITICAL
- Файл/функция: `crates/db/src/quotes.rs`,
  `CreateTradeupQuote::signature_digest`, `verify_signature`,
  `create_tradeup_quote`
- Эксплуатация: изменить неподписанное экономическое поле или передать отдельно
  подготовленный digest DB writer-у.
- Влияние: quote мог перестать быть криптографическим доказательством именно тех
  входов, исходов, цен и сроков, которые сохраняются.
- Исправление: канонический domain-separated SHA-256 охватывает owner/allocation/
  quote ids, версии, сроки, totals, selection, ordered inputs и outcomes. DB
  writer сам пересчитывает digest и проверяет Ed25519 до SQL.
- Regression test: unit-тест DB payload меняет outcome buyback и quote total и
  доказывает изменение digest/отказ проверки; signing tests отклоняют tampering.
- Статус: FIXED, VERIFIED BY UNIT AND WORKSPACE TESTS.

## HIGH

### Пользовательский ledger balance мог стать отрицательным

- Риск: HIGH
- Файл/функция: `db/migrations/0035_nonnegative_user_ledger_balances.sql`,
  `enforce_nonnegative_user_ledger_balance`
- Эксплуатация: конкурентный или будущий ошибочный writer списывает больше
  доступного остатка.
- Влияние: пользователь создаёт долг/необеспеченную покупку.
- Исправление: deferrable constraint trigger проверяет итоговый balance каждого
  `user_credit` account при commit; atomic writers дополнительно блокируют строки.
- Regression test: ignored ledger integration test отклоняет отрицательный
  пользовательский баланс; market/acceptance и concurrency suites проходят.
- Статус: FIXED, VERIFIED IN FRESH INTEGRATION DATABASE.

### Не было application-level rate limiting

- Риск: HIGH
- Файл/функция: `crates/api/src/router.rs`, `router_with_config`
- Эксплуатация: высокий поток auth/public/private запросов истощает CPU/DB.
- Влияние: деградация или отказ сервиса, усиление credential guessing.
- Исправление: общий peer-IP token bucket с конфигурируемым положительным burst;
  исчерпание возвращает `429`.
- Regression test: `configured_rate_limit_returns_429_for_the_same_peer` и
  `rate_limit_burst_is_positive_and_configurable`.
- Статус: FIXED IN APPLICATION; production proxy/source-IP behaviour requires
  manual deployment verification.

### Owner isolation и replay/idempotency требовали HTTP-доказательства

- Риск: HIGH
- Файл/функция: `crates/api/tests/quote.rs`, quote acceptance route
- Эксплуатация: известный UUID чужого quote или повтор/смена idempotency key.
- Влияние: чужое исполнение контракта или двойное финансовое/инвентарное
  движение.
- Исправление: identity берётся из authenticated session; DB predicate включает
  owner; acceptance writer атомарен и сохраняет idempotent result.
- Regression test: `quote_acceptance_is_owner_bound_and_idempotent_over_http`,
  SQL lifecycle and concurrency tests.
- Статус: FIXED, VERIFIED.

## MEDIUM

### Seed material мог быть раскрыт или сохранён небезопасно

- Риск: MEDIUM
- Файл/функция: allocation route/state and seed protection implementation
- Эксплуатация: получить nonce/ciphertext/key/server seed из публичного ответа
  либо запустить production без protector.
- Влияние: предсказание результата до acceptance и потеря provable fairness.
- Исправление: response содержит только allocation id и commitment; production
  startup требует key-backed XChaCha20-Poly1305 protector и иначе fail-closed;
  envelope owner-bound через associated data.
- Regression test: allocation HTTP tests проверяют отсутствие secret fields и
  безопасный `503` без secure configuration.
- Статус: FIXED, VERIFIED.

### HSTS применялся не ко всем ответам

- Риск: MEDIUM
- Файл/функция: `crates/api/src/router.rs`, `security_response_headers`
- Эксплуатация: публичный маршрут/ошибка не объявляет HTTPS policy.
- Влияние: неодинаковая browser transport policy.
- Исправление: response-wide middleware добавляет
  `Strict-Transport-Security: max-age=31536000; includeSubDomains`.
- Regression test: `every_response_advertises_the_production_https_policy`.
- Статус: FIXED IN APP; TLS termination/redirect remains deployment-owned.

## LOW

### Integration DB имела неверную owner-role модель

- Риск: LOW
- Файл/функция: `scripts/prepare_integration_db.sh`, `.github/workflows/ci.yml`
- Эксплуатация: тесты случайно проходят с правами владельца и не доказывают
  production-like grants.
- Влияние: ложноположительный CI и скрытый privilege bypass.
- Исправление: отдельная fresh integration database создаётся с owner
  `anonymous`; verifier проверяет обе базы и ignored suites.
- Regression test: CI PostgreSQL verification job and local fresh-DB run.
- Статус: FIXED, VERIFIED.

## Passed checks

- Authentication rejects missing/tampered/foreign cookies.
- Owner is derived from the session, not request fields.
- Secret allocation material is not returned by the API.
- Economic values/candidates are resolved server-side.
- Quote signature is recomputed and verified before persistence.
- Runtime DB access uses narrow functions and revoked PUBLIC grants.
- Atomic market/acceptance operations enforce ownership, stock, balance,
  inventory movement and idempotency.
- Exact money arithmetic uses checked integers/rationals until defined rounding.
- Full workspace format, Clippy, tests, dependency audit and fresh PostgreSQL
  verifier passed during the audit.

## Failed checks

No test in the executed verification set failed at the recorded final run. This
does not convert deployment-only checks into passed checks.

## Manual checks required

- Verify certificate chain, TLS versions/ciphers, HTTPS redirect and HSTS domain
  suitability at the real reverse proxy.
- Verify the proxy preserves a trustworthy socket peer for limiter keys; do not
  trust client-supplied forwarding headers without an allow-listed proxy chain.
- Exercise secret rotation, backup restore, alerting, audit-log retention and
  incident response.
- Load-test auth and expensive quote/market paths with production-like data.
- Confirm runtime credentials cannot become table/schema owner or disable
  triggers.

## Impossible to verify without production environment

- Real network segmentation, firewall/WAF and cloud IAM.
- Production secret custody and operator access.
- Real traffic denial-of-service capacity and autoscaling behaviour.
- Correctness/freshness of live price, reserve, stock and risk calibration data.
- External provider behaviour and contractual security guarantees.

## Objective conclusion

The confirmed code-level CRITICAL/HIGH findings listed above have fixes and
regression evidence. The application remains dependent on correctly configured
TLS/reverse proxy, restricted PostgreSQL roles and operational controls; those
dependencies are explicit rather than being counted as passed.
