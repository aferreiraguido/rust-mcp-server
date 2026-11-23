#!/usr/bin/env bash
# smoke-http.sh — end-to-end check of HTTP Streamable mode at /mcp for rust-mcp-server.
# Starts the server, runs initialize (captures mcp-session-id) → tools/list
# → tools/call → auth stage (401/200), then stops it.
# Usage: ./scripts/smoke-http.sh [--port 18080]
set -euo pipefail

cd "$(dirname "$0")/.."

PORT="${1:-18080}"
if [[ "${1:-}" == "--port" ]]; then PORT="${2:-18080}"; fi

BIN="${BIN:-./target/release/rust-mcp-server}"
if [ ! -f "$BIN" ]; then
  echo "== building release binary =="
  cargo build --release
fi

LOG="$(mktemp)"
KEYS="$(mktemp)"
HDRS="$(mktemp)"
BODY="$(mktemp)"
trap 'kill ${SRV:-} 2>/dev/null || true; rm -f "$LOG" "$KEYS" "$HDRS" "$BODY"' EXIT

echo "== starting HTTP mode (no auth) on :$PORT =="
"$BIN" --mode http --port "$PORT" --host 127.0.0.1 >"$LOG" 2>&1 &
SRV=$!
sleep 2

BASE="http://127.0.0.1:$PORT/mcp"
HDR=(-H "Content-Type: application/json" -H "Accept: application/json, text/event-stream")
FIXTURE="$PWD/tests/fixtures/sample_crate"

FAIL=0
check() {
  local desc="$1" pattern="$2" body="$3"
  if printf '%s' "$body" | grep -q "$pattern"; then
    echo "PASS: $desc"
  else
    echo "FAIL: $desc (pattern: $pattern)"
    FAIL=1
  fi
}

start_session() {
  # $1 = extra curl header args (e.g. Authorization); echoes session id.
  curl -s -D "$HDRS" -o "$BODY" -X POST "$BASE" "${HDR[@]}" "$@" \
    -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke-http","version":"0"}}}'
  grep -i "^mcp-session-id:" "$HDRS" | tr -d '\r' | awk '{print $2}'
}

SID=$(start_session)
check "initialize → serverInfo" 'rust-mcp-server' "$(cat "$BODY")"
[ -n "$SID" ] && echo "PASS: initialize → session id" || { echo "FAIL: initialize → session id"; FAIL=1; }

SESS=(-H "mcp-session-id: $SID")
curl -s -X POST "$BASE" "${HDR[@]}" "${SESS[@]}" -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' >/dev/null

LIST=$(curl -s -X POST "$BASE" "${HDR[@]}" "${SESS[@]}" -d '{"jsonrpc":"2.0","id":3,"method":"tools/list","params":{}}')
check "tools/list → load tool" '"name":"load_rust_project"' "$LIST"
check "tools/list → crates tool" '"name":"list_crates"' "$LIST"

LOAD=$(curl -s -X POST "$BASE" "${HDR[@]}" "${SESS[@]}" -d '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"load_rust_project","arguments":{"source":"'"$FIXTURE"'","alias":"smoke-http"}}}')
check "load → loaded" 'smoke-http' "$LOAD"

CRATES=$(curl -s -X POST "$BASE" "${HDR[@]}" "${SESS[@]}" -d '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"list_crates","arguments":{"name":"smoke-http"}}}')
check "list_crates → sample_crate" 'sample_crate' "$CRATES"

# Stage 2: restart with --api-keys; unauthenticated must 401, Bearer must pass.
kill $SRV 2>/dev/null || true
wait $SRV 2>/dev/null || true
sleep 1

printf '[{"name":"smoke","apiKey":"sk-smoke-123"}]' > "$KEYS"
"$BIN" --mode http --port "$PORT" --host 127.0.0.1 --api-keys "$KEYS" >"$LOG" 2>&1 &
SRV=$!
sleep 2

CODE_ANON=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE" "${HDR[@]}" -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}')
[ "$CODE_ANON" = "401" ] && echo "PASS: no key → 401" || { echo "FAIL: no key → 401 (got $CODE_ANON)"; FAIL=1; }

CODE_BAD=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE" "${HDR[@]}" -H "Authorization: Bearer wrong" -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}')
[ "$CODE_BAD" = "401" ] && echo "PASS: wrong key → 401" || { echo "FAIL: wrong key → 401 (got $CODE_BAD)"; FAIL=1; }

SID2=$(start_session -H "Authorization: Bearer sk-smoke-123")
AUTH_LIST=$(curl -s -X POST "$BASE" "${HDR[@]}" -H "Authorization: Bearer sk-smoke-123" -H "mcp-session-id: $SID2" -d '{"jsonrpc":"2.0","id":3,"method":"tools/list","params":{}}')
check "bearer key → tools/list" '"name":"project_metadata"' "$AUTH_LIST"

if [ "$FAIL" -ne 0 ]; then
  echo "== SMOKE-HTTP FAILED =="
  exit 1
fi
echo "== SMOKE-HTTP OK =="

kill $SRV 2>/dev/null || true
wait $SRV 2>/dev/null || true
SRV=""
