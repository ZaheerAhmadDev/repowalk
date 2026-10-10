---
name: board-demo-verification
description: Use when proving the agent-board demo is healthy before a release, after a server change, or when a demo may be flaky — exact smoke checks on a scratch port (compile, 8-test suite, / /api/tasks /api/stats, stdlib-only imports, one-writer/no-network, safe kill).
---

# Board demo verification

**Goal:** one green pass that proves the demo works, without ever touching the
live board on port **8765**.

## Rules

- Live port 8765 is the demo — **never bind, kill, or curl-to-restart it**.
- Scratch port = 8877+ (checklist runs used 8899).
- Kill by **captured PID** (`kill $PID`) or `fuser -k <port>/tcp`. Never
  `pkill -f` a pattern that appears in your own command line.
- Every check is a command with expected output — no feelings.

## Smoke sequence (run from repo root)

```sh
python3 -m py_compile board/server.py && echo "py_compile OK"
# → py_compile OK

python3 -m unittest discover -s board/tests -v
# → Ran 8 tests … OK

BOARD_PORT=8899 ./board/run.sh > /tmp/opencode/board-smoke.log 2>&1 &
SRV=$!; sleep 1
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8899/            # → 200
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8899/api/tasks   # → 200
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8899/api/stats   # → 200
curl -s http://localhost:8899/api/stats | python3 -m json.tool
# → object with keys total:int, statuses:{}, phases:{} (counts drift, keys don't)

grep -nE '^(import|from) ' board/server.py
# → only stdlib: json, os, http.server, pathlib

grep -cE 'urllib|requests|http\.client|socket\.create_connection|aiohttp' board/server.py
# → 0 matches (grep exit 1). fetch( hits are in-page JS, not Python.

kill "$SRV"
curl -s -o /dev/null -w '%{http_code}' --connect-timeout 1 http://localhost:8899/ || echo refused
# → 000refused
```

`./board/run.sh` resolves its own directory and defaults `BOARD_PORT=8765`,
so the smoke run **must** set `BOARD_PORT=8899` explicitly.

## Pass/fail

All commands print the expected output → demo healthy. Any deviation → find
the failing step before shipping; full itemized checklist with expected
outputs lives in `board/docs/release-checklist.md`.

## Rollback story (one sentence)

Kill the `run.sh` process and restart `./board/run.sh` — task data is plain
JSON on disk the server never writes.
