# Production reference deployment

Only Caddy publishes host ports. The application, PostgreSQL and Prometheus are
reachable only on the internal Compose network. Copy the example env files,
create `secrets/` files with restrictive permissions, and review the domain
before starting. Never commit real secrets or env files.

Required secret files are `database_url`, `quote_signing_key`,
`quote_seed_key`, and `postgres_owner_password`; create them with mode `0600`.
The application database URL must point at the restricted runtime role, not
the schema owner. Run migrations separately with an administrative role.
