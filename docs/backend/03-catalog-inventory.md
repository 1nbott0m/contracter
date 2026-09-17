# BACKEND-03: Catalog + Inventory

Read-only browsing of the game world, and the caller's own items. No
mutations, no market, no contracts, no admin.

## Endpoints

| Method | Path | Auth | Purpose |
|---|---|---|---|
| GET | `/api/v1/catalog/collections` | none | Enabled collections, paged. |
| GET | `/api/v1/catalog/skus` | none | Browsable SKUs, paged, filterable by collection, item, or rarity. |
| GET | `/api/v1/me/inventory` | cookie | The caller's own items, newest first, filterable by collection or rarity. |
| GET | `/api/v1/me/inventory/{item_id}` | cookie | One of the caller's own items. |

The catalog needs no session because nothing in it is account-specific,
and an endpoint that needs no identity cannot leak one account's data to
another. The inventory is addressed as `/me`, not
`/users/{id}/inventory`: the owner comes from `CurrentUser`, which has no
constructor taking a client-supplied id.

## No migration

`contracter_runtime` already had `SELECT` on `collections`,
`catalog_items`, `skus`, `rarities`, `wear_bands`, `inventory_items` and
`inventory_positions` from earlier milestones, and it deliberately still
has none on `inventory_item_locks`. Every query here fits inside those
grants, so this milestone adds no migration and widens no privilege. The
lock state is read through an `EXISTS` subquery the runtime role may
execute, not by reading the guard table.

## Pagination

Keyset, never `OFFSET`. `OFFSET` re-scans what it skips and, worse,
silently skips or repeats rows whenever the data shifts between two page
requests -- which for a live inventory is most of the time.

| Listing | Order | Cursor key |
|---|---|---|
| Collections, SKUs | `public_id` ascending | the public UUID |
| Inventory | `(created_at, public_id)` descending | both |

The inventory needs the composite key because several items can be
created in one transaction and share a `created_at` to the microsecond;
a page boundary landing inside such a group would skip or repeat rows.

The cursor a client receives is **opaque** -- base64url of the sort key.
It is not a security measure; it holds only data the client was already
shown. It keeps the contract honest: a client that parses a cursor
depends on the sort key, which then cannot change without breaking it.

A cursor that does not decode is a `400`, never a silent reset. Answering
a corrupt cursor with page one hands the caller page one while they
believe they are reading page nine, and they never find out.

**Never the sequential id.** A cursor is handed to clients. A sequential
one would tell them roughly how many rows exist and let them walk rows
they were never shown.

## Bounded input

- `limit` is clamped to `1..=200`, default `50`. A limit is an
  instruction to the database about how much work to do, which is
  exactly the kind of client input that must be bounded before it gets
  there. Clamped rather than rejected: there is nothing useful to tell a
  caller who sent `limit=1000000` beyond "you got 200".
- Unknown query parameters are **rejected** (`deny_unknown_fields`). A
  typo'd `?colection=` would otherwise be answered with the unfiltered
  list, which for a filter is the most dangerous possible default.
- Filters reach SQL as bound parameters compared against `NULL` to mean
  "unfiltered". No part of any query is assembled from caller input; the
  shared inventory projection is a macro over `concat!` rather than
  `format!`, so every query stays a compile-time literal instead of
  reaching for SQLx's `AssertSqlSafe`.

## Ownership

Ownership is a predicate inside the query, not a check performed after
the fact. `list_owned_inventory` and `find_owned_inventory_item` both
take the owner as a bound parameter, and `application::inventory` has no
function that accepts an owner from the request -- "read someone else's
inventory" is not an operation this layer can express.

An item owned by someone else returns `404`, byte-identical to one that
never existed. Distinguishing them would make the endpoint an oracle for
which UUIDs are real, and there is nothing a caller can legitimately do
with that answer.

## Locked and retired

The owner's view is deliberately **not** `available_user_inventory`.
That view hides a locked item, which is right for "what can be spent"
and wrong for "what do I own" -- an item silently vanishing while it is
reserved in a quote reads as theft. A locked item is listed with
`locked: true`. A retired item is excluded outright: those are gone, not
reserved.

## Exact decimals

`canonical_float`, `min_float` and `max_float` are JSON **strings**.
They are exact `numeric(9,8)` values in the database, and a JSON number
would be read back as a binary float by most clients -- for an item whose
identity turns on its float, that is a silent corruption. The same rule
that keeps money in integer microcredits applies here.

## Tests

| Layer | File | Count | Covers |
|---|---|---|---|
| db | `crates/db/tests/catalog.rs` | 2 new | Paging by public id, disabled rows hidden, the enabled chain, filters applied in SQL |
| db | `crates/db/tests/inventory.rs` | 3 new | Locked shown / retired hidden, cross-owner reads, paging without skip or repeat |
| application | `crates/application/src/pagination.rs` | 6 unit | Limit clamping, both cursor shapes round-tripping, corrupt and wrong-shape cursors, page-end detection |
| application | `crates/application/src/catalog.rs` | 1 unit | A corrupt cursor is refused before any query runs |
| api | `crates/api/tests/catalog_inventory.rs` | 7 | Public catalog, clamping, cursor and unknown-filter rejection, auth required, locked/retired visibility, IDOR, paging, cache headers, malformed ids |

## A note on test fixtures

The HTTP suite's fixtures **commit**, unlike the transaction-scoped ones
in the `db` suite, because the router uses its own pooled connections and
cannot see an uncommitted row. That makes them globally visible, and two
real cross-test failures came out of it during this milestone:

- an *activated* stock policy version with no bands left every
  collection uncovered and broke scarcity publishing in an unrelated
  test; the fixture now leaves policy versions un-activated, since the
  quote needs the foreign key to resolve, not the policy to be in force;
- a `rarities.rank` near the other suites' 90-95 block collided on a
  `UNIQUE`; the fixture now sits far outside every range in use.

Both were caught by the canonical run rather than by the new tests
themselves, which is the argument for running the whole suite on a
pristine database rather than only the tests one just wrote.

## Deferred

Market, contracts, pricing, scarcity and admin endpoints. Warehouse
inventory is readable at the `db` layer but has no endpoint: it is
market-facing, and belongs with BACKEND-04.
