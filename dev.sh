#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

# Production config comes from the environment; these are dev defaults only.
# Port 80 by default so a Zyxel router can call the webhook (it cannot handle
# explicit ports in its DDNS update URL); override with DDNSER_PORT=8080 to
# avoid the sudo/setcap step.
export DDNSER_PORT=${DDNSER_PORT:-80}
if [ "$DDNSER_PORT" = 80 ]; then
  export DDNSER_PUBLIC_URL=${DDNSER_PUBLIC_URL:-http://localhost}
else
  export DDNSER_PUBLIC_URL=${DDNSER_PUBLIC_URL:-http://localhost:$DDNSER_PORT}
fi
export DDNSER_DATABASE_URL=${DDNSER_DATABASE_URL:-postgres://ddnser:ddnser@localhost:5432/ddnser}
export DDNSER_OIDC_ISSUER=${DDNSER_OIDC_ISSUER:-http://localhost:5556/dex}
export DDNSER_OIDC_CLIENT_ID=${DDNSER_OIDC_CLIENT_ID:-ddnser}
export DDNSER_OIDC_CLIENT_SECRET=${DDNSER_OIDC_CLIENT_SECRET:-dev-secret}
export DDNSER_ADMIN_GROUP=${DDNSER_ADMIN_GROUP:-authors}
export DDNSER_SESSION_SECRET=${DDNSER_SESSION_SECRET:-dev-session-secret-at-least-32-chars}
export DDNSER_WEBHOOK_USERNAME=${DDNSER_WEBHOOK_USERNAME:-router}
export DDNSER_WEBHOOK_PASSWORD=${DDNSER_WEBHOOK_PASSWORD:-router}
# Syncs fail against Vercel until you export a real token; the UI still works.
export VERCEL_TOKEN=${VERCEL_TOKEN:-invalid-dev-token}
export RUST_LOG=${RUST_LOG:-info,sqlx::query=debug}

run() {
  cargo build
  # Rebuilds replace the binary and drop file capabilities, so re-grant when
  # binding a privileged port.
  if [ "$DDNSER_PORT" -lt 1024 ] && ! getcap target/debug/ddnser | grep -q cap_net_bind_service; then
    echo "granting cap_net_bind_service to target/debug/ddnser (needs sudo)"
    sudo setcap cap_net_bind_service=+ep target/debug/ddnser
  fi
  ./target/debug/ddnser
}

case "${1:-}" in
  frontend)
    docker compose up -d --wait
    pnpm --dir web install
    pnpm --dir web run dev &
    VITE_PID=$!
    trap 'kill "$VITE_PID" 2>/dev/null' EXIT
    export DDNSER_DEV_FORWARD=http://localhost:5173
    run
    ;;
  backend)
    docker compose up -d --wait
    pnpm --dir web install
    pnpm --dir web run build
    run
    ;;
  *)
    echo "Usage: dev.sh {frontend|backend}" >&2
    echo "  frontend  postgres+dex in docker, vite dev server with HMR, app via cargo" >&2
    echo "  backend   postgres+dex in docker, embedded frontend build, app via cargo" >&2
    exit 1
    ;;
esac
