#!/usr/bin/env bash
# Smoke test: builds badgery, renders badges via every subcommand (static
# path syntax, endpoint schema, JSONPath query, manifest build), then runs
# the HTTP server against a real directory and asserts on the responses.
# Self-contained: temp dirs + 127.0.0.1 only, no network ever.
set -euo pipefail

cd "$(dirname "$0")/.."

fail() { echo "SMOKE FAIL: $*" >&2; exit 1; }

echo "[smoke] building..."
cargo build --quiet
BIN=target/debug/badgery

WORK=$(mktemp -d "${TMPDIR:-/tmp}/badgery-smoke.XXXXXX")
trap 'rm -rf "$WORK"' EXIT

# --- 1. version/help sanity -------------------------------------------------
"$BIN" --version | grep -q '^badgery 0\.1\.0$' || fail "--version mismatch"
"$BIN" --help | grep -q 'COMMANDS:' || fail "--help missing sections"

# --- 2. static badge: shields path syntax + escapes -------------------------
echo "[smoke] badgery static"
"$BIN" static build-passing-brightgreen > "$WORK/build.svg"
grep -q '>passing</text>' "$WORK/build.svg" || fail "static message missing"
grep -q '#4c1' "$WORK/build.svg" || fail "brightgreen not resolved"
"$BIN" static release--notes-up__to_date-blue --style flat-square > "$WORK/esc.svg"
grep -q '>release-notes</text>' "$WORK/esc.svg" || fail "'--' escape broken"
grep -q '>up_to date</text>' "$WORK/esc.svg" || fail "'__'/'_' escapes broken"

# An invalid spec must fail with exit code 2 (usage error).
if "$BIN" static justoneword 2>/dev/null; then fail "invalid spec accepted"; fi

# --- 3. endpoint schema file -------------------------------------------------
echo "[smoke] badgery endpoint"
cat > "$WORK/coverage.json" <<'EOF'
{"schemaVersion": 1, "label": "coverage", "message": "92%", "color": "green"}
EOF
"$BIN" endpoint "$WORK/coverage.json" --out "$WORK/coverage.svg"
grep -q '>92%</text>' "$WORK/coverage.svg" || fail "endpoint message missing"
grep -q '#97ca00' "$WORK/coverage.svg" || fail "endpoint color missing"

# isError always renders red, even with a green override.
cat > "$WORK/err.json" <<'EOF'
{"schemaVersion": 1, "label": "scan", "message": "failed", "isError": true}
EOF
"$BIN" endpoint "$WORK/err.json" --color brightgreen | grep -q '#e05d44' \
  || fail "isError badge not forced red"

# --- 4. dynamic JSON query ----------------------------------------------------
echo "[smoke] badgery query"
cat > "$WORK/meta.json" <<'EOF'
{"version": "1.4.2", "tests": {"passed": 89}}
EOF
"$BIN" query "$WORK/meta.json" '$.version' --label version --prefix v --color blue \
  | grep -q '>v1.4.2</text>' || fail "query prefix/value missing"
"$BIN" query "$WORK/meta.json" '$.tests.passed' --label tests --suffix ' passed' \
  | grep -q '>89 passed</text>' || fail "query suffix/number missing"
if "$BIN" query "$WORK/meta.json" '$.nope' 2> "$WORK/q.err"; then
  fail "bad query accepted"
fi
grep -q "no member 'nope'" "$WORK/q.err" || fail "query error not descriptive"

# --- 5. manifest build ---------------------------------------------------------
echo "[smoke] badgery build"
cat > "$WORK/badgery.json" <<'EOF'
{
  "outDir": "badges",
  "badges": [
    {"name": "build", "type": "static", "label": "build", "message": "passing", "color": "brightgreen"},
    {"name": "coverage", "type": "endpoint", "file": "coverage.json"},
    {"name": "version", "type": "query", "file": "meta.json", "query": "$.version", "label": "version", "prefix": "v", "color": "blue"}
  ]
}
EOF
"$BIN" build --manifest "$WORK/badgery.json" | tee "$WORK/build.out"
grep -q 'built 3/3 badges' "$WORK/build.out" || fail "build summary wrong"
for name in build coverage version; do
  [ -s "$WORK/badges/$name.svg" ] || fail "badges/$name.svg not written"
done
grep -q '>v1.4.2</text>' "$WORK/badges/version.svg" || fail "built version badge wrong"

# --- 6. HTTP server on 127.0.0.1 ----------------------------------------------
PORT=$(( 21000 + $$ % 20000 ))
ADDR="127.0.0.1:$PORT"
echo "[smoke] badgery serve (foreground, --exit-after 6s, $ADDR)"
"$BIN" serve --addr "$ADDR" --root "$WORK" --exit-after 6s > "$WORK/serve.log" 2>&1 &
SPID=$!
for _ in $(seq 1 40); do
  if curl -s -o /dev/null "http://$ADDR/health"; then break; fi
  sleep 0.1
done
HEALTH=$(curl -s "http://$ADDR/health")
[ "$HEALTH" = "ok" ] || fail "GET /health -> '$HEALTH' (want ok)"
echo "[smoke] GET /health -> ok"
curl -s "http://$ADDR/badge/docs-latest-informational.svg?style=flat-square" > "$WORK/http1.svg"
grep -q '>latest</text>' "$WORK/http1.svg" || fail "HTTP static badge wrong"
curl -s "http://$ADDR/endpoint?file=coverage.json" | grep -q '>92%</text>' \
  || fail "HTTP endpoint badge wrong"
curl -s "http://$ADDR/query?file=meta.json&query=%24.version&label=version&prefix=v" \
  | grep -q '>v1.4.2</text>' || fail "HTTP query badge wrong"
STATUS=$(curl -s -o /dev/null -w '%{http_code}' "http://$ADDR/endpoint?file=../etc/passwd")
[ "$STATUS" = 400 ] || fail "path traversal not refused (got $STATUS)"
echo "[smoke] traversal refused with 400"
# A missing data file must come back as a red error badge, not a broken image.
curl -s "http://$ADDR/endpoint?file=missing.json" | grep -q '#e05d44' \
  || fail "missing file did not render an error badge"
wait "$SPID" || fail "server exited non-zero"
grep -q 'shutting down' "$WORK/serve.log" || fail "server did not shut down cleanly"

echo "SMOKE OK"
