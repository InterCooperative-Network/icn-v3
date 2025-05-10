#!/usr/bin/env bash
set -euo pipefail
# ────────────────────────────────────────────────────────────────────────────────
# ICN Devnet Bootstrap Script
# Spins up a 3-node federation, generates keys, seeds a demo proposal, and
# prints handy URLs.  Intended for *local* developer use only.
#
# Assumptions
#   • You're in the repo root.
#   • Docker + docker-compose v2 installed.
#   • `icn-cli` binary is in PATH (or adjust $CLI below).
#   • devnet/federation.toml & sample_proposal.ccl already exist.
# ────────────────────────────────────────────────────────────────────────────────

CLI="cargo run -q -p icn-cli --"   # ↳ replace with `icn-cli` after installation
COMPOSE_FILE="devnet/docker-compose.yml"
FED_TOML="devnet/federation.toml"
PROPOSAL="devnet/examples/sample_proposal.ccl"

# ╭──────────────────────── Utility helpers ───────────────────────╮
green () { printf '\033[0;32m%s\033[0m\n' "$*"; }
err   () { printf '\033[0;31m%s\033[0m\n' "$*" >&2; }

wait_for_port () {
  local host=$1 port=$2
  printf "⏳ Waiting for %s:%s " "$host" "$port"
  until nc -z "$host" "$port" >/dev/null 2>&1; do printf "."; sleep 1; done
  echo
}

# ╭──────────────────────── Federation Key-gen ────────────────────╮
bootstrap_keys () {
  green "🔑 Generating dev-federation keys …"
  KEYS_DIR="devnet/examples/federation_keys"
  mkdir -p "$KEYS_DIR"

  $CLI federation keygen --output "$KEYS_DIR/federation.json"
  for N in node-1 node-2 node-3; do
    $CLI node keygen --node-id "$N" --output "$KEYS_DIR/$N.json"
  done
}

# ╭──────────────────────── Docker Compose Up ─────────────────────╮
start_compose () {
  green "🐳 Launching containers …"
  docker compose -f "$COMPOSE_FILE" up -d --build
}

# ╭──────────────────────── DAG Genesis/Bootstrap ─────────────────╮
bootstrap_federation () {
  green "📜 Bootstrapping federation DAG …"
  $CLI federation init \
       --config   "$FED_TOML" \
       --keys     devnet/examples/federation_keys/federation.json \
       --node-api http://localhost:7001

  for N in node-1 node-2 node-3; do
    $CLI node register \
         --node-id "$N" \
         --keys    "devnet/examples/federation_keys/$N.json" \
         --node-api http://localhost:7001
  done
}

# ╭──────────────────────── Seed Demo Proposal  ───────────────────╮
seed_demo_proposal () {
  green "📤 Submitting sample proposal …"
  $CLI coop propose \
       --file "$PROPOSAL" \
       --api  http://localhost:8080
}

# ╭──────────────────────── Execution Flow  ───────────────────────╮
bootstrap_keys
start_compose

# Wait for runtime node-1 API & Agoranet
wait_for_port localhost 7001
wait_for_port localhost 8080

bootstrap_federation
seed_demo_proposal

green "✅ Devnet ready!

Explorer:     http://localhost:8080
Verifier API: http://localhost:8090
Runtime APIs: http://localhost:7001 7002 7003

Next steps:
  • Run: ${CLI} coop list --node http://localhost:7001
  • Hack away 🚀
" 