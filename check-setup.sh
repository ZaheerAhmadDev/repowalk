#!/usr/bin/env bash
# check-setup.sh — validate this laptop can build and demo repowalk (RSK-26).
# Checks: toolchain, warm cargo cache (pinned crates), a clap build smoke test.
# Toolchain override: CARGO="cargo +stable-x86_64-pc-windows-gnu" ./check-setup.sh
set -uo pipefail

cd "$(dirname "$0")"
CARGO="${CARGO:-cargo}"
fail=0

check() { # check <label> <cmd...>
  local label="$1"; shift
  if "$@" > /dev/null 2>&1; then
    echo "  ok    $label"
  else
    echo "  FAIL  $label"
    fail=1
  fi
}

echo "== repowalk setup check (cargo: $CARGO) =="

check "rustc present"          rustc --version
check "$CARGO present"         $CARGO --version
check "cargo cache: clap"      sh -c 'ls ~/.cargo/registry/cache/*/clap-4* >/dev/null 2>&1 || cargo metadata --format-version 1 >/dev/null'
check "cargo cache: eframe"    sh -c 'ls ~/.cargo/registry/src/*/eframe-0.36* >/dev/null 2>&1 || cargo metadata --format-version 1 >/dev/null'
check "cargo cache: egui_graphs"    sh -c 'ls ~/.cargo/registry/src/*/egui_graphs-0.32* >/dev/null 2>&1 || cargo metadata --format-version 1 >/dev/null'
check "cargo cache: egui_code_editor" sh -c 'ls ~/.cargo/registry/src/*/egui_code_editor-0.4.2* >/dev/null 2>&1 || cargo metadata --format-version 1 >/dev/null'
check "cargo cache: petgraph"  sh -c 'ls ~/.cargo/registry/src/*/petgraph-0.8* >/dev/null 2>&1 || cargo metadata --format-version 1 >/dev/null'
check "golden file"            test -f fixtures/golden/index.json

echo "-- clap build smoke test --"
if $CARGO build -p repowalk-cli > /dev/null 2>&1; then
  echo "  ok    repowalk-cli builds"
else
  echo "  FAIL  repowalk-cli build"
  fail=1
fi

if [ "$fail" -eq 0 ]; then
  echo "== setup OK =="
else
  echo "== setup INCOMPLETE — fix the FAIL lines above =="
fi
exit "$fail"
