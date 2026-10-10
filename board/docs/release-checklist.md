# Board demo — pre-release checklist

Every item is a command with an exact expected output. Run from the repo root
(`/home/zaheerahmad/Documents/Project`). Live demo port is **8765 — never touch**; verify on a scratch
port (8877+, this run used 8899). Kill the scratch server by captured PID or
`fuser -k 8899/tcp` — never `pkill -f` a pattern from your own command line.

## 1. Python compile check

```sh
python3 -m py_compile board/server.py && echo "py_compile OK"
```

Expected output:

```
py_compile OK
```

(exit 0, no traceback)

## 2. Unittest suite (8 tests)

```sh
python3 -m unittest discover -s board/tests -v
```

Expected output (tail):

```
Ran 8 tests in 0.0xxs

OK
```

All 8 named tests `ok`: `test_valid_json`, `test_required_fields_present`,
`test_id_matches_filename`, `test_phase_enum`, `test_status_enum`,
`test_assignee_null_or_string`, `test_artifacts_list_of_strings`,
`test_unique_ids`. Overall result `OK`, exit 0.

## 3. Start run script on a scratch port

```sh
BOARD_PORT=8899 ./board/run.sh > /tmp/opencode/rel1-server.log 2>&1 &
echo "PID=$!"
```

Expected output: `PID=<n>` where `<n>` is the live server PID — **capture it**;
it is the only safe way to stop the server later.

## 4. Root page `GET /` → 200

```sh
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8899/
```

Expected output:

```
200
```

## 5. `GET /api/tasks` → 200

```sh
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8899/api/tasks
```

Expected output:

```
200
```

## 6. `GET /api/stats` → 200, valid JSON

```sh
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8899/api/stats
curl -s http://localhost:8899/api/stats | python3 -m json.tool
```

Expected output: status line `200`, then a JSON object with exactly the keys
`total` (int), `statuses` (object status→count), `phases` (object
phase→count). Example shape (counts drift as tasks move; keys do not):

```
{
    "phases": {"design": 1, "implementation": 3, "maintenance": 1,
               "release": 1, "requirements": 1, "testing": 1},
    "statuses": {"done": 4, "in_progress": 2, "todo": 2},
    "total": 8
}
```

## 7. Zero third-party imports in server.py

```sh
grep -nE '^(import|from) ' board/server.py
```

Expected output — only Python stdlib modules, nothing else:

```
11:import json
12:import os
13:from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
14:from pathlib import Path
```

(`fetch(` matches in the file are in-browser JS inside the HTML page string,
not Python network code.)

## 8. One-writer / no-network confirmation

```sh
grep -cE 'urllib|requests|http\.client|socket\.create_connection|aiohttp' board/server.py; echo $?
```

Expected output:

```
0
1
```

Zero matches; grep exits 1 (not found). The server is a stdlib
`ThreadingHTTPServer` on localhost only (no outbound sockets, no remote
deployment) and read-only over `board/tasks/*.json` — agents remain the one
writer per task file; the server never writes task JSON.

## 9. Stop the scratch server

```sh
kill <PID>          # the PID captured in step 3
curl -s -o /dev/null -w '%{http_code}' --connect-timeout 1 http://localhost:8899/ || echo refused
```

Expected output:

```
000refused
```

Fallback if the PID is lost: `fuser -k 8899/tcp`. Confirm port 8765 was never
touched (`curl -s -o /dev/null -w '%{http_code}' http://localhost:8765/` still
answers the live board).

## Sign-off

All nine items green on a scratch port = demo is shippable. Rollback story:
stop `run.sh` (Ctrl-C / `kill <PID>`), restart with
`./board/run.sh` — task data is plain JSON on disk, untouched by the server.
