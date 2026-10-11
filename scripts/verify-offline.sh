#!/usr/bin/env bash
# scripts/verify-offline.sh — full offline verification (RSK-27).
# Green tests + clippy with no network access. Exits non-zero on any failure.
# Toolchain override: CARGO="cargo +stable-x86_64-pc-windows-gnu" ./scripts/verify-offline.sh
set -euo pipefail

cd "$(dirname "$0")/.."
CARGO="${CARGO:-cargo}"

echo "== repowalk offline verify =="
export CARGO_NET_OFFLINE=true

echo "-- cargo test (workspace) --"
$CARGO test --workspace

echo "-- cargo clippy (-D warnings) --"
$CARGO clippy --workspace --all-targets -- -D warnings

echo "-- golden file present? --"
test -f fixtures/golden/index.json || {
  echo "FAIL: fixtures/golden/index.json missing (UPDATE_GOLDEN=1 $CARGO test -p repowalk-core --test golden)" >&2
  exit 1
}

echo "== OK: offline verify passed =="
