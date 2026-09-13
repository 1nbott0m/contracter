# Contracter 0.5.0 — database and economy core design

## Goal

Build a testable backend foundation for an invite-only CS2 trade-up simulator with virtual credits. The MVP has no deposits, withdrawals, peer-to-peer exchange, cases, upgrades, StatTrak, Souvenir, or knife/glove outcomes.

The system must be transparent and replayable. It must not personalize results from a player's profit history, falsify a “near miss”, reroll unavailable outcomes, or silently alter published probabilities.

## Architecture

The first deliverable is a Rust workspace with:

- a pure `economy-core` library for trade-up probabilities, normalized float calculation, deterministic selection, and pricing;
- PostgreSQL migrations that encode identity, catalog, inventory, append-only value movement, price evidence, quotes/contracts, stock policy, and two-admin approval;
- seed data for stable lookup tables and the ten selected collections;
- local verification that does not require a paid service.

Application/API/UI work is intentionally deferred until these contracts are stable.

## Trade-up rules

An ordinary contract consumes exactly ten eligible normal items of one rarity and produces one normal item at the next rarity. Covert inputs are rejected. Every input collection must expose at least one enabled next-rarity output.

Collection weight is the count of inputs from that collection divided by ten. Within a collection, enabled outputs are equiprobable. Therefore output probability is:

`P(output) = input_count(collection) / 10 / output_count(collection)`.

If any quoted output is unavailable at acceptance time, the whole operation is rejected. Outcomes are never silently removed or rerolled.

For each input:

`normalized_i = (float_i - min_i) / (max_i - min_i)`

Then:

`normalized_average = mean(normalized_i)`

`output_float = output_min + normalized_average * (output_max - output_min)`

All ranges are validated and calculations use decimal/rational-safe representations rather than binary database floats.

## Deterministic selection

A quote contains a server-seed commitment, client seed, nonce, formula version, exact ordered outcome table, valuation snapshot IDs, stock-policy version, and expiry. Acceptance reveals the server seed and derives a digest from a domain-separated encoding of these fields. Replaying the same signed inputs must reproduce the selected outcome.

Fairness uses a two-step protocol. Before the server receives item IDs or the client seed, it allocates the next unused seed commitment from an append-only, monotonically numbered commitment pool and returns its public commitment ID. The client then submits that ID with the ten items and client seed. Commitments are SHA-256 over a versioned, domain-separated, length-prefixed canonical byte encoding. Quotes are authenticated with an HMAC key ID; key rotation never removes the verification metadata required by stored quotes.

The selector uses rejection sampling from SHA-256 digest blocks so mapping to the integer total weight has no modulo bias. A commitment ID and nonce are unique and single-use. Seeds are revealed for accepted quotes immediately after selection and for expired or rejected allocations after expiry, so selective suppression remains auditable. Commitment allocation, quote creation, acceptance, and revelation are append-only events.

Quote expiry is 60 seconds. A changed, expired, or incompletely stocked quote is rejected atomically. Acceptance is idempotent: a retry with the same idempotency key returns the already committed contract result rather than creating or rejecting a second result.

## Pricing and margin

The automated initial price source is Market.CSGO completed-sale evidence. A SKU is valued by a 10% trimmed mean:

- use the latest 7 days when at least 20 valid sales exist;
- otherwise use 30 days when at least 20 valid sales exist;
- otherwise disable quoting for that SKU.

The ledger settles in integer microcredits (one credit is 1,000,000 microcredits); the UI rounds only for display to USD cents. Intermediate expected values use exact integer/rational arithmetic. Each quote publishes its actual post-rounding buyback spread.

For outcome `j`, immediate platform buyback is:

`buyback_j = floor_to_microcredit(0.85 * verified_price_j)`.

For probabilities `p_j`:

`expected_buyback = sum(p_j * buyback_j)`

`quote_total = ceil_to_microcredit(expected_buyback / 0.85)`

`adjustment = quote_total - verified_input_value`

A positive adjustment is a fee; a negative adjustment is a visible rebate. This targets a 15% gross spread only if the user immediately sells the result back. It is not a promise of net profit.

Price movement above 20% over 24 hours halts buy/sell/contracts for the SKU. Automatic recovery requires the next 24-hour comparison to be within 5%; earlier recovery requires two-admin approval.

## Stock and risk controls

Initial target stock per SKU:

- Consumer 167
- Industrial 125
- Mil-Spec 83
- Restricted 42
- Classified 17
- Covert 7

Stock-policy bands are versioned data, not hard-coded business logic. A quote stores the exact version used. Inputs return to the global warehouse and the result transfers out.

Risk values use the same immutable valuation snapshot as the quote. `liquid_reserve` is the system treasury balance available for buybacks plus the verified buyback value of unencumbered warehouse inventory. `stressed_liability` is all user credit balances plus current buyback value of user-held items plus the maximum buyback result and rebate exposure of every outstanding quote. Coverage is `liquid_reserve / stressed_liability`; zero liability has infinite coverage.

Default limits:

- reserve coverage at least 1.25;
- one quote at most 1% of reserve;
- one item at most 10% of liability;
- one collection at most 25% of liability.

One quote exposure is its maximum candidate buyback plus maximum rebate. Item and collection concentration use their stressed liabilities divided by total stressed liability. Quote creation reserves every candidate outcome and adds worst-case exposure. Acceptance or expiry releases unused reservations.

A singleton `risk_state` row stores the snapshot version and aggregate exposures. Quote creation and acceptance lock it before SKU, item, and ledger rows; all buy, sell, and contract paths use the same sorted lock order. The transaction rechecks price-halt state, risk version, coverage, quote exposure, and concentration. Violating a limit rejects the operation with a stable reason code.

## Identity and administration

Test access is invitation-only. Users authenticate with a unique login and Argon2id password; email is not required. Invitations and recovery codes are random, single-use, stored only as hashes.

There are exactly two initial equal administrators. Critical actions use a two-person rule: proposer and approver must differ, payload is immutable, approval expires after 24 hours, and execution is idempotent. This covers economic settings, administrator membership/rights, early market unfreeze, a single credit adjustment over 100 USD, or cumulative adjustments by the same initiator to the same account exceeding 100 USD in a rolling 24-hour window.

Administrative sessions require TOTP. Recovery revokes active sessions, cancels pending approvals, and freezes critical administration for 24 hours. Reconstituting a missing administrator requires the surviving administrator plus the missing administrator's sealed offline recovery share, creates an externally retained audit record, and cannot itself execute an economic action.

Test users may receive 1000 USD in virtual credits through a one-time grant protected by a unique user-and-grant-kind constraint and a fail-closed test-environment database setting. Production has no automatic grant.

## Database invariants

- Internal joins use integer identities; public objects and operations use UUIDs.
- Ledger and inventory history are append-only. Corrections are compensating events.
- Every ledger transaction balances to zero.
- Current balance, inventory position, stock, valuation, and singleton risk-state tables are projections guarded by transactions.
- Business operations retain their valuation evidence even when old aggregates are pruned.
- PostgreSQL lookup tables replace database enums to keep migrations reversible.
- Runtime, migration, and read-only database roles are separated.

Candidate identity is a wear-specific SKU derived from a catalog item and calculated float. Floats are canonical decimal values at scale 8; catalog bounds require `min < max`. Wear bands are half-open at their upper bound except the final band, which includes its upper bound. Quantization uses round-half-even once after the output formula and before wear/SKU lookup.

Raw sale evidence is immutable and includes source, stable external event key, exact variant/currency, gross amount, source timestamp, receive timestamp, and validity reason. Duplicate, cancelled, future-dated, wrong-currency, or wrong-variant records are excluded. The 10% trim removes `floor(n * 0.10)` values from each sorted end and rounds the remaining arithmetic mean half-even to a microcredit. Snapshot windows use source timestamps not later than snapshot time. Price halt is sticky for every input and candidate output until a later complete 24-hour comparison is within 5% or two admins approve an early release.

Quote acceptance locks the singleton risk row, quote, ten item positions, affected stock rows in sorted SKU order, and ledger accounts in sorted ID order. It performs no HTTP, password hashing, or heavy valuation calculation while locks are held.

## Verification

Unit/property tests must cover probability conservation, normalized float bounds and wear boundaries, commitment ordering and eventual revelation, unbiased deterministic replay, integer rounding and effective spread, rejection conditions, zero-sum postings, exact ten-input enforcement, two-admin separation and rolling thresholds, unauthorized journal mutation, idempotent retry, and concurrent risk-limit enforcement. Migration smoke tests run against PostgreSQL when a test URL is supplied; pure SQL/static checks remain runnable offline.

Before a public release, run at least one million simulated contracts across typical and adversarial portfolios and measure market-value RTP, buyback RTP, gross spread, oracle error, reserve coverage, concentration, drawdown, and halt frequency.

## Deferred from the first increment

HTTP API, frontend/admin panel, live scraper scheduling, public registration, real-money flows, and deployment are separate increments. Their schemas are anticipated here, but they must not be simulated by placeholder production behavior.
