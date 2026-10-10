---
name: board-endpoint-patterns
description: Use when adding or changing a GET endpoint on board/server.py — routing in Handler.do_GET, the _send helper's headers, JSON serialization, or 404 handling.
---

# Board endpoint patterns

`board/server.py` is a stdlib-only `ThreadingHTTPServer`. Every response goes
through `Handler._send(code, body, ctype)`, which sets `Content-Type`,
`Content-Length`, and `Cache-Control: no-store` — never hand-roll headers.

## Add an endpoint

1. Compute the payload in a module-level `load_*()` function next to
   `load_tasks()` (e.g. `load_stats()`), reading via `load_tasks()`.
   Guard missing keys with `task.get("field", "?")` — broken task files get
   `"?"` placeholders from the fallback in `load_tasks()`.
2. Add one `elif path == "/api/<name>":` branch in `do_GET`, after the
   query-string strip (`self.path.split("?", 1)[0]`):

   ```python
   elif path == "/api/stats":
       self._send(200, json.dumps(load_stats()), "application/json")
   ```

3. Leave the trailing `else: self._send(404, "not found", "text/plain; …")`
   untouched — the page JS relies on its 404 fallback.

## Rules

- Stdlib only (`json`, `http.server`, `pathlib`); no deps, no network writes.
- JSON bodies: `json.dumps(...)` with `"application/json"`.
- Never edit `PAGE` HTML unless the task says so.
- Scratch-verify: `python3 -m py_compile board/server.py`, then
  `BOARD_PORT=8877 python3 board/server.py &`, `curl -sf localhost:8877/api/<name>`,
  kill by captured PID or `fuser -k 8877/tcp`. Port 8765 is the live server —
  never touch it.
