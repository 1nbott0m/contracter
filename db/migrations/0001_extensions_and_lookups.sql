\set ON_ERROR_STOP on

BEGIN;

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- Mutable business vocabularies are rows, never PostgreSQL enums. Task 3
-- installs environment-specific rows; tests insert only their own fixtures.
CREATE TABLE IF NOT EXISTS critical_action_types (
    code text PRIMARY KEY,
    requires_dual_approval boolean NOT NULL DEFAULT true,
    description text,
    CHECK (code = lower(code) AND code ~ '^[a-z][a-z0-9_]*$')
);

CREATE TABLE IF NOT EXISTS ledger_account_kinds (
    code text PRIMARY KEY,
    description text,
    CHECK (code = lower(code) AND code ~ '^[a-z][a-z0-9_]*$')
);

CREATE TABLE IF NOT EXISTS rarities (
    code text PRIMARY KEY,
    rank smallint NOT NULL UNIQUE,
    is_covert boolean NOT NULL DEFAULT false,
    CHECK (code = lower(code) AND code ~ '^[a-z][a-z0-9_-]*$'),
    CHECK (rank >= 0)
);

CREATE TABLE IF NOT EXISTS inventory_event_kinds (
    code text PRIMARY KEY,
    description text,
    CHECK (code = lower(code) AND code ~ '^[a-z][a-z0-9_]*$')
);

CREATE TABLE IF NOT EXISTS seed_event_kinds (
    code text PRIMARY KEY,
    description text,
    CHECK (code = lower(code) AND code ~ '^[a-z][a-z0-9_]*$')
);

CREATE TABLE IF NOT EXISTS quote_statuses (
    code text PRIMARY KEY,
    is_terminal boolean NOT NULL DEFAULT false,
    description text,
    CHECK (code = lower(code) AND code ~ '^[a-z][a-z0-9_]*$')
);

CREATE TABLE IF NOT EXISTS contract_statuses (
    code text PRIMARY KEY,
    is_terminal boolean NOT NULL DEFAULT false,
    description text,
    CHECK (code = lower(code) AND code ~ '^[a-z][a-z0-9_]*$')
);

CREATE TABLE IF NOT EXISTS sale_validity_reasons (
    code text PRIMARY KEY,
    is_valid boolean NOT NULL,
    description text,
    CHECK (code = lower(code) AND code ~ '^[a-z][a-z0-9_]*$')
);

CREATE TABLE IF NOT EXISTS price_halt_reasons (
    code text PRIMARY KEY,
    description text,
    CHECK (code = lower(code) AND code ~ '^[a-z][a-z0-9_]*$')
);

CREATE TABLE IF NOT EXISTS currencies (
    code text PRIMARY KEY,
    minor_unit_scale smallint NOT NULL,
    CHECK (code ~ '^[A-Z]{3}$'),
    CHECK (minor_unit_scale BETWEEN 0 AND 8)
);

CREATE TABLE IF NOT EXISTS grant_kinds (
    code text PRIMARY KEY,
    description text,
    CHECK (code = lower(code) AND code ~ '^[a-z][a-z0-9_]*$')
);

COMMIT;
