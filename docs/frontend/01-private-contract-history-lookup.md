# Private contract history lookup

## Ruling

The existing `GET /api/v1/me/history/contracts` route is an authenticated,
owner-scoped history reader. Looking up a contract ID by downloading that
history is therefore a **private lookup in the current user's history**.

It is not any of the following:

- public contract verification;
- cryptographic or fairness verification;
- a public contract-details endpoint;
- proof that a contract belongs to anyone other than the authenticated user.

The response is only a history summary: `contract_id`, `status`, `created_at`,
and `ledger_transaction_id`. Frontend API and UI names must make that scope
explicit, for example `findMyContractHistoryEntry` and “Найти в моей истории”.
They must not use labels such as “PUBLIC VERIFICATION”, “verify contract”, or
“full contract details” for this route.

## Production boundary

Only a server response may be presented as a committed contract or verified
result. Fixtures from `frontend/src/mocks/dev-data.ts` are permitted only when
Vite is running in development mode **and** `VITE_ENABLE_DEV_FALLBACK=true`.
Fixture-backed previews must be visibly labelled as development data. Outside
that explicit opt-in, the UI must render an unavailable/loading/server-result
state instead of a fixture.

## Future public verification

Public verification requires a separate backend contract before the frontend
can claim it. That contract must define the public identifier, disclosure-safe
response fields, verification evidence, authentication policy, abuse/rate
limits, and not-found behavior. Until that endpoint exists, private history
lookup must remain private in types, names, and user-facing copy.
