# Database and Economy Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a tested Rust economy library and PostgreSQL schema that form the stable backend foundation for Contracter 0.5.0.

**Architecture:** Keep deterministic trade-up and pricing calculations in a pure Rust crate with no database or network dependencies. Keep durable identity, catalog, inventory, ledger, price evidence, quote, contract, stock, and dual-admin invariants in ordered PostgreSQL migrations. Provide one reproducible verification entry point for a less experienced collaborator.

**Tech Stack:** Rust 1.98+, Cargo, `rust_decimal`, `sha2`, `thiserror`, `proptest`, PostgreSQL 16+, POSIX shell.

**Spec:** `docs/superpowers/specs/2026-09-13-database-economy-core-design.md`

## Global Constraints

- The MVP has no deposits, withdrawals, peer-to-peer exchange, cases, upgrades, StatTrak, Souvenir, or knife/glove outcomes.
- Do not personalize outcomes, falsify near-misses, reroll unavailable outputs, or hide published probabilities.
- A contract accepts from four through ten eligible normal inputs of one rarity and rejects Covert inputs.
- Quote expiry is 60 seconds; administrative approval expiry is 24 hours.
- Unused commitment allocation expires after 15 seconds; each user has at most one active allocation or quote.
- Buyback is `floor(0.85 * verified_price)`; quote total is `ceil(expected_buyback / 0.85)`.
- Ledger settlement uses integer microcredits; the UI rounds only for display to USD cents; float values use decimal-safe types.
- Price movement above 20% in 24 hours halts a SKU; automatic recovery requires movement within 5%.
- Credit adjustments strictly above 100 USD require approval from a different administrator.
- Ledger and inventory history are append-only; corrections use compensating entries.
- Production code is written only after a relevant test has failed for the expected reason.

---

### Task 1: Pure Rust trade-up and pricing engine

**Files:**
- Create: `Cargo.toml`
- Create: `crates/economy-core/Cargo.toml`
- Create: `crates/economy-core/src/lib.rs`
- Create: `crates/economy-core/src/tradeup.rs`
- Create: `crates/economy-core/src/pricing.rs`
- Test: `crates/economy-core/tests/tradeup.rs`
- Test: `crates/economy-core/tests/pricing.rs`

**Interfaces:**
- Consumes: no earlier implementation.
- Produces: `build_outcomes(&[InputItem], &[OutputItem]) -> Result<Vec<WeightedOutcome>, TradeupError>`, `calculate_output_float(&[InputItem], Decimal, Decimal) -> Result<Decimal, TradeupError>`, `server_seed_commitment(&[u8; 32]) -> [u8; 32]`, `select_outcome(&[WeightedOutcome], &[u8; 32], &[u8], u64) -> Result<Selection, TradeupError>`, `trimmed_mean_microcredits(&[i64]) -> Result<i64, PricingError>`, `buyback_microcredits(i64) -> Result<i64, PricingError>`, and `quote_adjustment_microcredits(i64, &[PricedOutcome]) -> Result<QuotePrice, PricingError>`.

- [ ] **Step 1: Add trade-up behavior tests before implementation**

Create literal fixtures proving: three and eleven inputs are rejected; four and ten inputs are accepted; mixed input rarities fail; Covert input fails; a 7/3 collection split with respectively two/one outputs yields weights 35%, 35%, 30%; weights sum exactly to one; normalized per-input floats produce the hand-calculated output; unavailable output fails; the seed commitment differs from the seed; identical seed/client-seed/nonce/outcome ordering replays the same result.

- [ ] **Step 2: Run the trade-up tests and capture RED**

Run: `cargo test -p economy-core --test tradeup`

Expected: compilation failure because the public interfaces do not exist.

- [ ] **Step 3: Implement the minimum trade-up module**

Use `Decimal` for float bounds and rational integer weights with a common denominator. Validate all input/output ranges. Sort outcomes by stable SKU identifier before hashing. Hash a domain-separated length-prefixed encoding of server seed, client seed, nonce, and the ordered outcome identifiers with SHA-256; map the first 128 digest bits into the total integer weight without floating-point conversion.

- [ ] **Step 4: Run trade-up tests and capture GREEN**

Run: `cargo test -p economy-core --test tradeup`

Expected: all trade-up tests pass with no warnings.

- [ ] **Step 5: Add pricing behavior tests before implementation**

Use literal cases proving: a 10% trimmed mean drops two values from each end of a twenty-sale sample; fewer than 20 sales are rejected by the valuation-window selector; 1,001,000 microcredits buy back for 850,850 microcredits; negative prices fail; exact rational expected buyback is rounded upward only at final quote-total settlement; actual effective spread is returned; positive adjustment is a fee; negative adjustment is a rebate; probabilities must sum exactly to one; checked arithmetic rejects overflow.

- [ ] **Step 6: Run pricing tests and capture RED**

Run: `cargo test -p economy-core --test pricing`

Expected: compilation failure because pricing interfaces do not exist.

- [ ] **Step 7: Implement the minimum pricing module**

Use checked integer arithmetic and numerator/denominator weights. Return a signed `adjustment_microcredits` and expose fee and rebate as mutually exclusive derived values.

- [ ] **Step 8: Run all crate tests**

Run: `cargo test -p economy-core`

Expected: all unit, integration, and property tests pass with no warnings.

- [ ] **Step 9: Commit**

Run: `git add Cargo.toml Cargo.lock crates/economy-core && git commit -m "feat: add deterministic economy core"`

### Task 2: PostgreSQL schema and database-enforced invariants

**Files:**
- Create: `db/migrations/0001_extensions_and_lookups.sql`
- Create: `db/migrations/0002_identity_and_admin.sql`
- Create: `db/migrations/0003_catalog_inventory_ledger.sql`
- Create: `db/migrations/0004_prices_stock_quotes_contracts.sql`
- Create: `db/migrations/0005_append_only_guards.sql`
- Create: `db/tests/001_invariants.sql`
- Create: `db/verify.sh`

**Interfaces:**
- Consumes: monetary settlement and probability conventions from Task 1.
- Produces: ordered idempotency-safe migrations, database constraints/triggers, and `db/verify.sh` accepting `TEST_DATABASE_URL`.

- [ ] **Step 1: Write the failing invariant test transaction**

Create a SQL test script that applies migrations then proves by expected SQLSTATE or explicit assertions: duplicate login fails; reused invitation fails; proposer cannot approve their own critical action; approval after 24 hours fails; a 10001-cent credit adjustment requires dual approval; unbalanced ledger transaction fails; an eleventh or ninth contract input fails finalization; UPDATE/DELETE on journal tables fails; duplicate quote acceptance fails.

- [ ] **Step 2: Run the database test and capture RED**

Run: `TEST_DATABASE_URL="$TEST_DATABASE_URL" ./db/verify.sh`

Expected: failure because migrations and required objects are absent. If no PostgreSQL test URL exists, record this integration test as environment-blocked and run `sh -n db/verify.sh` after the script exists.

- [ ] **Step 3: Implement ordered migrations**

Use bigint identity primary keys internally and UUID public IDs. Use lookup tables rather than PostgreSQL enums. Store password, invitation, recovery, and session tokens only as hashes. Store money as bigint microcredits and floats as `numeric(9,8)` with range checks.

Create append-only event tables and current projections for inventory, balances, seed commitments/revelations, and singleton risk state. Enforce one active allocation or quote per user, 15-second allocation expiry, input locks, candidate reservations, and idempotent result lookup. A deferred constraint trigger verifies each ledger transaction sums to zero. Security-definer functions own posting and finalization while runtime roles cannot mutate journals directly. Finalization validates four through ten locked inputs. Critical-action approval enforces distinct active administrators, immutable payload hash, 24-hour expiry, rolling 24-hour credit thresholds, and idempotent execution key.

Create immutable normalized sale evidence, current valuation, changed-only snapshots, daily aggregates, sticky price halts and anomaly checks, versioned stock policies, quote inputs/outcomes and reservations, contracts, stored valuation references, and risk exposure aggregates. Snapshot publication invalidates older active quotes and releases their locks and reservations while holding the singleton risk lock. Add indexes for login, usable inventory, stock, recent sale evidence, quote expiry, and pending approvals.

- [ ] **Step 4: Add append-only role guards**

Create trigger functions and grants that reject direct INSERT where privileged procedures are required and all UPDATE/DELETE for ledger postings, inventory transfer events, seed events, contract inputs/outcomes, admin approval events, and price evidence. Document runtime grants in SQL comments without embedding credentials.

- [ ] **Step 5: Run migration verification**

Run: `sh -n db/verify.sh`

Run when available: `TEST_DATABASE_URL="$TEST_DATABASE_URL" ./db/verify.sh`

Expected: shell syntax passes; PostgreSQL invariant transaction passes with zero failed assertions.

- [ ] **Step 6: Commit**

Run: `git add db && git commit -m "feat: add PostgreSQL economy schema"`

### Task 3: Seed configuration and one-command collaborator verification

**Files:**
- Create: `db/seeds/0001_reference_data.sql`
- Create: `scripts/verify.sh`
- Create: `.env.example`
- Create: `README.md`
- Modify: `.gitignore`
- Test: `crates/economy-core/tests/policy.rs`

**Interfaces:**
- Consumes: database lookup identifiers from Task 2 and public Rust interfaces from Task 1.
- Produces: deterministic reference seeds and `./scripts/verify.sh` as the documented single verification command.

- [ ] **Step 1: Write policy tests before seed implementation**

Add literal tests for initial per-SKU targets 167/125/83/42/17/7 and risk defaults 1.25 coverage, 1% quote, 10% item, and 25% collection. Test zero-liability coverage, worst-case outstanding-quote exposure, and rejection of overlapping stock bands or ratios outside 0..=1.

- [ ] **Step 2: Run policy tests and capture RED**

Run: `cargo test -p economy-core --test policy`

Expected: compilation failure because policy types do not exist.

- [ ] **Step 3: Implement the minimum versioned policy types**

Add immutable policy value objects and validation to `economy-core`. Keep stock-band multipliers supplied by database seed data rather than compiled constants.

- [ ] **Step 4: Seed stable reference data**

Seed rarities, wear bands, action/status codes, initial stock targets, risk-policy version, and these collection slugs: `control`, `ancient`, `havoc`, `2021-dust-2`, `2021-mirage`, `2021-vertigo`, `canals`, `norse`, `2021-train`, `2018-nuke`. Do not invent item/SKU mappings or float bounds without a verified catalog source.

- [ ] **Step 5: Add one-command verification and onboarding**

`scripts/verify.sh` must run formatting check, Clippy with warnings denied, all Cargo tests, shell syntax checks, and database integration only when `TEST_DATABASE_URL` is set. `.env.example` contains placeholders only. README gives Zed-friendly clone/open/test steps in Russian and distinguishes local tests from shared Aiven testing.

- [ ] **Step 6: Run full verification**

Run: `./scripts/verify.sh`

Expected: offline checks pass; output explicitly says whether PostgreSQL integration ran or was skipped.

- [ ] **Step 7: Commit**

Run: `git add .gitignore .env.example README.md Cargo.toml Cargo.lock crates db scripts && git commit -m "docs: add seeded local development workflow"`
