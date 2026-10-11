#!/usr/bin/env bash
# scripts/bench.sh — index-in-a-second check (RSK-27).
# Times `repowalk map` over this repo N times with a warm cache; reports avg.
set -euo pipefail

cd "$(dirname "$0")/.."
N="${1:-5}"

BIN="target/release/repowalk"
[ -x "$BIN" ] || cargo build --release

echo "== repowalk bench: $N runs on ./ (warm cache) =="
total=0
for i in $(seq 1 "$N"); do
  start=$(date +%s%N)
  "$BIN" map . > /dev/null
  end=$(date +%s%N)
  ms=$(( (end - start) / 1000000 ))
  echo "run $i: ${ms} ms"
  total=$(( total + ms ))
done
echo "avg: $(( total / N )) ms"
