#!/usr/bin/env bash
# Builds and starts a fresh, empty instance (own compose project + volume),
# runs the end-to-end suite against it, then tears it down. KEEP=1 keeps it running.
set -euo pipefail
cd "$(dirname "$0")/.."
export COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-lhtest}"
export PORT="${PORT:-18080}"
export SWEEP_SECONDS=5

docker compose down -v --remove-orphans >/dev/null 2>&1 || true
# example lab image used by the labs tests
docker build -q -t livehack/example-web:latest examples/labs/web-hello >/dev/null
docker compose up -d --build --quiet-pull >/dev/null

for _ in $(seq 1 90); do
  curl -fs "http://localhost:$PORT/healthz" >/dev/null 2>&1 && break
  sleep 1
done

status=0
python3 -I tests/e2e.py "http://localhost:$PORT" "$COMPOSE_PROJECT_NAME" || status=$?
if [ $status -ne 0 ]; then
  echo "---- app logs (tail) ----"
  docker compose logs app --no-log-prefix | tail -40
fi
if [ -z "${KEEP:-}" ]; then
  docker compose down -v >/dev/null 2>&1
  # labs left by a failed run
  docker ps -aq --filter label=org.livehack.instance | xargs -r docker rm -f >/dev/null 2>&1 || true
  docker network ls -q --filter label=org.livehack.instance | xargs -r docker network rm >/dev/null 2>&1 || true
fi
exit $status
