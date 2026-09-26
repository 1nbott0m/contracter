# Economy: rules the game-mechanics document fixes

What `CONTRACTER_GAME_MECHANICS.md` decides, where each decision lives in
the code, and what is still open. Written after aligning the code to that
document; the arguments are recorded because several of them constrain
future work.

## Contract inputs: four to ten

`crates/economy-core/src/tradeup.rs` (`INPUT_RANGE`), enforced again in
`finalize_contract` (migration 0018).

The count was never only a guard. It divides the collection weights and
the output float average, so it had to become the submitted count rather
than a constant. The weight invariant still holds for any count in range,
because the per-collection input counts sum to exactly that divisor.
Leaving a hard-coded ten would have made every shorter contract's weights
fail to sum to one and biased every output float toward Factory New.

Distinctness is enforced separately from the range. A bare length check
would let one item be passed five times and spent as if it were five.

## Collection odds follow input composition

Three inputs from A and one from B gives A exactly 75% and B exactly 25%,
as rationals rather than rounded percentages. This is asserted at four
inputs and at ten, and it is the invariant that shapes everything below.

## Scarcity damps within a collection, never between them

`apply_outcome_scarcity`, keyed by SKU.

The document says scarcity changes the weights of eligible outcomes
*inside* the chosen collection, and separately that collection odds follow
input composition. Together those forbid scarcity from moving probability
between collections — which the earlier implementation did, by
renormalising across the whole outcome set: damping A raised B's odds.

A single multiplier per collection cannot express within-collection
variation at all; scaling every outcome of a collection by the same factor
and renormalising inside it is the identity. So the multiplier is keyed by
SKU, which is also where stock actually lives.

The arithmetic is exact: each collection is damped over a common
multiplier denominator, then the whole distribution is scaled by the
lowest common multiple of the collections' damped totals. Nothing rounds,
so no share drifts.

**Still open.** `collection_scarcity_snapshot_items` aggregates scarcity
to the collection. Producing per-SKU multipliers is BACKEND-08's work.
Until then nothing supplies these multipliers, and `economy-core` has no
production caller in any case.

## Zero stock excludes, it does not refuse

An out-of-stock candidate is dropped and the rest of its collection
renormalised, with the collection's own share untouched. Refusing the
whole contract — the earlier behaviour — meant one out-of-stock skin made
every contract touching its collection impossible.

A collection left with nothing in stock is still refused, as
`MissingOutput`. Silently handing its share to another collection would
break the tie between collection odds and input composition, so there is
no correct answer to give.

## House edge 8%, target EV 92%

`HOUSE_EDGE_BPS`, with `TARGET_EV_BPS` derived from it so the two cannot
drift apart. A contract costs `market_value / 0.92`, so the player's
expected return is 92% of what they paid.

**Taken by price, not by weights.** Margin in the weights would mean
collection probability no longer follows input composition and scarcity
becomes a hidden margin dial — with both mixed into one number, neither
can be audited. The document is explicit that scarcity is not a personal
adjustment, and this is what keeps that true.

Rounded up, so the realised edge is never below the configured one.

## The buyback spread is a different layer

`BUYBACK_PERCENT` = 85. Selling an item for credits pays 85% of its
verified market value.

This is not the house edge and is deliberately not folded into it. The
edge applies when a player runs a contract; the spread applies when a
player leaves the item economy. The document's own loop — top up, buy
skins, contract, reveal, new skin, contract again — pays the edge once per
cycle and never touches the spread.

A player who contracts and then immediately sells the result faces both,
compounded: `1 - 0.85 × 0.92 = 0.218`. That figure is the cost of leaving,
not the cost of playing, and the document does not mention sell-back at
all. Raising the buyback toward 100% to make 8% the only number would
make the item↔credit round trip free and open the obvious arbitrage, so
the two layers stay separate and are written down here instead.

A rebate is priced at the buyback rate for the same reason: paid at face
value it would be a spread-free exit, strictly better than selling.

## Minimum item value: 20 credits

`MINIMUM_ITEM_VALUE_MICROCREDITS` = 20_000_000, checked inside
`quote_adjustment_microcredits` — on the path a quote actually takes,
rather than defined and never consulted, which is how it first shipped.

The floor exists so rounding, spreads and fees stay small relative to the
amounts they act on. On a one-microcredit item every one of them would
dominate the price.

## Money is integer microcredits

One credit is 1,000,000 microcredits. No `f32` or `f64` appears anywhere
in `economy-core`; probabilities are exact `u64`/`u128` rationals, and
`rust_decimal` is fixed-point and used only for wear floats and ratio
reporting.

Exact decimals cross the HTTP boundary as strings, because a JSON number
is read back as a binary float by most clients — for an item whose
identity turns on its float, that is silent corruption.

## The formula is versioned

`QuotePrice` carries the house edge, buyback spread and item floor it was
computed under. The document requires the formula to be deterministic,
versioned and fixed in the quote before acceptance; without a version, a
quote priced before a parameter change and one priced after are
indistinguishable in storage and no dispute can be settled. Derived from
the constants rather than maintained by hand, because a version someone
has to remember to bump goes stale, and a stale one asserts something
false.

## Not yet wired

`economy-core` is not a dependency of `db`, `api`, `application` or
`server`. Everything above is enforced in that crate and its tests, and
takes effect in a running request path when BACKEND-06 and BACKEND-07
build quotes and contracts on it. Nothing here changes the behaviour of
any endpoint that exists today.
