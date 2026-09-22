# Atomic Quote Creation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Atomically create owner-bound trade-up quotes with encrypted unrevealed server seeds.

**Architecture:** PostgreSQL owns locks and durable quote state. Rust computes the immutable proposal, encrypts the seed, and exposes only safe API responses.

**Tech Stack:** Rust, SQLx, PostgreSQL, Axum, XChaCha20-Poly1305.

**Spec:** `docs/superpowers/specs/2026-09-22-atomic-quote-creation-design.md`

## Global Constraints

- Inputs are four through ten distinct normal-rarity items.
- Plaintext unrevealed server seeds are never persisted or returned.
- `QUOTE_SEED_KEY` is runtime-only and never logged or committed.
- Any failed check rolls back every reservation and row.

## Review Focus

- Concurrent requests cannot reserve one input or candidate twice.
- A late risk rejection leaves no quote or reservation.
- Foreign, retired, locked, duplicate, and wrong-count inputs fail safely.
- Candidate stock cannot be over-reserved.
- API errors expose no SQL, seed, encrypted data, or internal ID.

### Task 1: PostgreSQL atomic writer

**Files:** Create `db/migrations/0023_atomic_quote_creation.sql`; modify `db/tests/001_invariants.sql`.

- [ ] Write failing SQL tests for successful envelope storage, rollback after risk rejection, and concurrent input/candidate reservation.
- [ ] Run `TEST_DATABASE_URL=... ./db/verify.sh`; expect missing writer failure.
- [ ] Add `seed_secret_envelopes` with 24-byte nonce, ciphertext, and one-to-one commitment reference.
- [ ] Add security-definer `create_tradeup_quote_for_user(...)` that locks inputs, stock, and risk before inserting allocations, quote rows, reservations, and exposures.
- [ ] Re-run `TEST_DATABASE_URL=... ./db/verify.sh`; expect `001_invariants: ok` and rollback.
- [ ] Commit `feat(db): create quotes atomically`.

### Task 2: Typed SQLx writer

**Files:** Modify `crates/db/src/quotes.rs`, `crates/db/src/lib.rs`; test `crates/db/tests/quotes.rs`.

- [ ] Write a failing integration test proving one write reserves inputs and candidates once.
- [ ] Add `CreateTradeupQuote` and `create_tradeup_quote_for_user(executor, request) -> QuoteId`.
- [ ] Run `TEST_DATABASE_URL=... cargo test -p db --test quotes -- --ignored`.
- [ ] Commit `feat(db): expose atomic quote writer`.

### Task 3: Application and API creation

**Files:** Modify `crates/application/src/quote.rs`, `crates/api/src/routes/quote.rs`, `crates/api/src/router.rs`; test `crates/api/tests/quote.rs`.

- [ ] Write a failing endpoint test for duplicate IDs and prohibited client price fields.
- [ ] Add `POST /api/v1/me/quotes`; it accepts only item IDs and client seed.
- [ ] Run `cargo fmt --check`, `cargo test --workspace`, and `cargo clippy --workspace -- -D warnings`.
- [ ] Commit, push, open PR, and merge after checks are clean.
