# Production reference deployment

Only Caddy publishes host ports. The application, PostgreSQL and Prometheus are
reachable only on the internal Compose network. Copy the example env files,
create `secrets/` files with restrictive permissions, and review the domain
before starting. Never commit real secrets or env files.
