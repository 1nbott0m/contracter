# Atomic quote creation

## Purpose

Create an active trade-up quote without trusting client calculations and
without leaving partially reserved inventory, risk, or fairness data after a
failure. A quote uses four through ten normal-rarity input items. Knives,
gloves, and Covert inputs remain excluded.

## Boundary

The server calculates the immutable quote proposal from current catalog,
valuation, stock, and risk data. PostgreSQL is the final authority for
ownership, availability, reservations, and the durable quote record. The API
returns only the public quote representation; it never returns a server seed,
encrypted seed, or runtime key.

## Seed lifecycle

1. The application generates a fresh 32-byte server seed and derives its
   commitment before it receives item IDs or a client seed.
2. `EnvironmentSeedProtector` encrypts the seed with the runtime-only
   `QUOTE_SEED_KEY`, producing a unique XChaCha20-Poly1305 nonce and
   ciphertext.
3. The allocation transaction stores the commitment plus the nonce and
   ciphertext in a table keyed one-to-one to the commitment, then returns an
   owner-bound public allocation ID that expires after fifteen seconds.
4. A later controlled reveal decrypts the ciphertext and inserts the existing
   append-only revelation event. Plaintext seed is never persisted before that
   event.

## Atomic creation transaction

A new security-definer PostgreSQL function consumes a previously allocated,
unexpired commitment and accepts server-derived immutable fields in one
transaction:

1. validates exactly one owner and four through ten distinct owned input IDs;
2. locks eligible inventory rows and rejects retired, locked, or foreign items;
3. locks every candidate warehouse stock row and reserves one unit for each
   possible outcome only when stock is available;
4. locks and increases global, SKU, and collection risk exposure only if the
   active risk policy still permits the proposal;
5. inserts the quote, inputs, outcomes, candidate reservations, and exposure
   records; it never creates or replaces a commitment;
6. returns the new quote ID.

Any rejected check raises an error and rolls back every preceding insert or
reservation. A unique active allocation per user remains the concurrency gate.

## Expiry and acceptance

Expiry releases candidate stock and quote exposure, unlocks inputs, and marks
the seed allocation released. Acceptance consumes only the selected candidate,
releases all unselected reservations, converts the selected reservation into a
contract result, and records a reveal event. Existing finalization must be
aligned so it never decrements a reservation that was not created.

## API and application

`POST /api/v1/me/quote-allocations` creates a commitment before receiving
request details. `POST /api/v1/me/quotes` accepts that allocation ID plus only
ordered public inventory IDs and a client seed. The server computes the
result; clients cannot provide prices, probabilities, candidates, commitments,
signatures, or reserve values. Candidate inventory must have exactly the
computed canonical float; otherwise quote creation fails closed. The existing
active-quote read and accept endpoints remain owner-bound.

## Error handling

Ownership conflicts, unavailable items, insufficient candidate stock, an
existing active quote, expired inputs, and risk-policy rejection map to stable
client-safe conflict responses. SQL text, encryption errors, secret material,
and internal IDs never leave the API.

## Verification

PostgreSQL integration tests cover success, rollback after a late failure,
simultaneous creation attempts, foreign/duplicate/locked inputs, unavailable
candidate stock, risk rejection, stable input/outcome order, and absence of a
plaintext server seed. Rust tests cover the server calculation boundary and
API validation. Full workspace format, test, and Clippy checks are required.
