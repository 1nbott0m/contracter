# SQLx Database Crate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a production-safe Rust `db` crate that connects to PostgreSQL and applies the existing schema through SQLx.

**Architecture:** Keep `db/migrations` as the single migration source for both SQLx and psql verification. The crate owns redacted connection configuration, `PgPool`, health checks, migration execution, and transaction startup; business calculations remain in `economy-core`.

**Tech Stack:** Rust 1.98+, SQLx 0.9, Tokio, rustls, PostgreSQL 16+

**Spec:** `docs/superpowers/specs/2026-09-13-database-economy-core-design.md`

## Global Constraints

- Do not add Aiven credentials or a real service URI to Git.
- Do not add backend or frontend crates.
- Preserve existing PostgreSQL invariants and psql verification.
- Use `db/migrations` as the only migration source.

---

### Task 1: SQLx connection boundary

**Files:**
- Create: `crates/db/Cargo.toml`
- Create: `crates/db/build.rs`
- Create: `crates/db/src/lib.rs`
- Create: `crates/db/src/config.rs`
- Create: `crates/db/src/database.rs`
- Test: `crates/db/tests/config.rs`

**Interfaces:**
- Produces: `DatabaseConfig`, `Database`, `DatabaseError`, `MIGRATOR`.

- [ ] Write failing tests proving credentials are redacted and invalid pool limits are rejected.
- [ ] Run tests and capture failure because crate `db` does not exist.
- [ ] Implement configuration, PostgreSQL pool, migration, health-check, and transaction APIs.
- [ ] Run crate tests and Clippy.

### Task 2: Shared SQLx and psql migrations

**Files:**
- Modify: `db/migrations/*.sql`
- Modify: `db/seeds/0001_reference_data.sql`
- Modify: `db/verify.sh`
- Create: `crates/db/tests/postgres.rs`

**Interfaces:**
- Consumes: `MIGRATOR` and an isolated `TEST_DATABASE_URL`.
- Produces: SQLx-compatible transactional migrations with unchanged database invariants.

- [ ] Write an ignored integration test that applies migrations twice and verifies health and rollback.
- [ ] Remove psql-only commands and transaction wrappers from migration and seed files.
- [ ] Make psql apply each migration and seed with `--single-transaction`.
- [ ] Run psql SQL suites and the SQLx integration test on temporary PostgreSQL.

### Task 3: Unified verification and documentation

**Files:**
- Modify: `Cargo.toml`
- Modify: `scripts/verify.sh`
- Modify: `README.md`

**Interfaces:**
- Produces: one offline command and one database-enabled command.

- [ ] Add `crates/db` to the workspace and document safe Aiven usage.
- [ ] Run formatting, Clippy, all Rust tests, psql suites, secret scan, and Git diff checks.
- [ ] Commit the verified database-only change.
