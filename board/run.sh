#!/usr/bin/env bash
# Start the agent board. BOARD_PORT defaults to 8765 (live demo port).
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export BOARD_PORT="${BOARD_PORT:-8765}"
exec python3 "$DIR/server.py"
