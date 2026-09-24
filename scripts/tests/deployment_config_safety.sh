#!/usr/bin/env sh
set -eu
compose=deploy/docker-compose.prod.yml
caddy=deploy/Caddyfile
grep -q '"80:80"' "$compose"
grep -q '"443:443"' "$compose"
! grep -A8 '^  app:' "$compose" | grep -q 'ports:'
! grep -A8 '^  postgres:' "$compose" | grep -q 'ports:'
grep -q 'internal: true' "$compose"
grep -q 'Strict-Transport-Security' "$caddy"
grep -q 'reverse_proxy app:8080' "$caddy"
echo 'deployment config safety checks: ok'
