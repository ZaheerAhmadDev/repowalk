#!/usr/bin/env python3
"""Agent task board — stdlib only, no dependencies.

Run:      python3 board/server.py          (or ./board/run.sh)
Watch:    http://localhost:8765
Env:      BOARD_PORT (default 8765)

Agents read/write board/tasks/*.json (see skills/meta/agent-board/SKILL.md);
this server is a read-only view over those files.
"""
import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parent
TASKS_DIR = ROOT / "tasks"
PORT = int(os.environ.get("BOARD_PORT", "8765"))

PAGE = """<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Agent Board — SDLC</title>
<style>
  :root { color-scheme: dark; }
  * { box-sizing: border-box; }
  body { margin: 0; font: 14px/1.45 system-ui, -apple-system, sans-serif;
         background: #0f1115; color: #e6e6e6; }
  header { display: flex; align-items: center; gap: 18px; padding: 14px 20px;
           background: #161a21; border-bottom: 1px solid #2a2f3a; position: sticky; top: 0; }
  header h1 { font-size: 15px; margin: 0; font-weight: 650; }
  header h1 span { color: #7aa2f7; }
  #stats { color: #9aa4b2; font-size: 12.5px; font-family: ui-monospace, monospace; }
  #updated { margin-left: auto; font-size: 12px; color: #6b7280; }
  .board { display: grid; grid-template-columns: repeat(4, 1fr); gap: 14px; padding: 18px; }
  .col { background: #141821; border: 1px solid #232838; border-radius: 10px;
         padding: 10px; min-height: 62vh; }
  .col h2 { font-size: 11.5px; text-transform: uppercase; letter-spacing: .09em;
            color: #8b93a7; margin: 2px 4px 12px; display: flex; justify-content: space-between; }
  .col.todo h2 { color: #8b93a7; } .col.in_progress h2 { color: #e0af68; }
  .col.done h2 { color: #9ece6a; } .col.blocked h2 { color: #f7768e; }
  .card { background: #1b2030; border: 1px solid #2c3347; border-left: 3px solid #7aa2f7;
          border-radius: 8px; padding: 10px 12px; margin-bottom: 10px; }
  .col.in_progress .card { border-left-color: #e0af68; }
  .col.done .card { border-left-color: #9ece6a; opacity: .8; }
  .col.blocked .card { border-left-color: #f7768e; }
  .card .row { display: flex; justify-content: space-between; align-items: center;
               font: 12px ui-monospace, monospace; color: #7aa2f7; }
  .card .phase { color: #7d86a0; font-family: system-ui, sans-serif; }
  .card h3 { font-size: 13.5px; margin: 7px 0 6px; font-weight: 600; }
  .card .who { font-size: 12px; color: #9ece6a; }
  .card .who.none { color: #6b7280; font-style: italic; }
  .card .when { font-size: 11px; color: #565f76; margin-top: 6px; }
  .card .arts { font-size: 11px; color: #7d86a0; margin-top: 4px; word-break: break-all; }
  @media (max-width: 900px) { .board { grid-template-columns: repeat(2, 1fr); } }
</style>
</head>
<body>
<header>
  <h1><span>Agent Board</span> — Software Development Cycle</h1>
  <div id="stats">…</div>
  <div id="updated">connecting…</div>
</header>
<main class="board" id="board"></main>
<script>
const COLS = [["todo","Todo"],["in_progress","In Progress"],["done","Done"],["blocked","Blocked"]];
const esc = s => String(s ?? "").replace(/[&<>"]/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;"}[c]));
function render(tasks) {
  document.getElementById("board").innerHTML = COLS.map(([key, label]) => {
    const items = tasks.filter(t => t.status === key);
    return `<section class="col ${key}"><h2><span>${label}</span><span>${items.length}</span></h2>` +
      items.map(t => `<article class="card">
        <div class="row"><span>${esc(t.id)}</span><span class="phase">${esc(t.phase)}</span></div>
        <h3>${esc(t.title)}</h3>
        <div class="who ${t.assignee ? "" : "none"}">${t.assignee ? "claimed by " + esc(t.assignee) : "unclaimed"}</div>
        ${(t.artifacts && t.artifacts.length) ? `<div class="arts">→ ${t.artifacts.map(esc).join("<br>→ ")}</div>` : ""}
        <div class="when">${esc(t.updated || "")}</div>
      </article>`).join("") + `</section>`;
  }).join("");
  document.getElementById("updated").textContent = "refreshed " + new Date().toLocaleTimeString();
}
async function tick() {
  try {
    render(await (await fetch("/api/tasks")).json());
    const r = await fetch("/api/stats");
    if (r.ok) {
      const s = await r.json();
      document.getElementById("stats").textContent =
        s.total + " tasks · " + Object.entries(s.statuses || {}).map(([k, v]) => `${k} ${v}`).join(" · ");
    } else {
      document.getElementById("stats").textContent = "tasks: " + document.querySelectorAll(".card").length;
    }
  } catch (e) {
    document.getElementById("updated").textContent = "server unreachable — retrying";
  }
}
tick();
setInterval(tick, 2000);
</script>
</body>
</html>
"""


def load_tasks():
    tasks = []
    if TASKS_DIR.is_dir():
        for path in sorted(TASKS_DIR.glob("*.json")):
            try:
                tasks.append(json.loads(path.read_text(encoding="utf-8")))
            except (OSError, json.JSONDecodeError) as exc:
                tasks.append({
                    "id": path.stem, "title": f"BROKEN FILE: {exc}",
                    "phase": "?", "skill": "?", "status": "blocked",
                    "assignee": "<fix this file>", "description": "",
                    "artifacts": [], "created": "", "updated": "",
                })
    return tasks


def load_stats():
    tasks = load_tasks()
    stats = {"total": len(tasks), "statuses": {}, "phases": {}}
    for task in tasks:
        status = task.get("status", "?")
        phase = task.get("phase", "?")
        stats["statuses"][status] = stats["statuses"].get(status, 0) + 1
        stats["phases"][phase] = stats["phases"].get(phase, 0) + 1
    return stats


class Handler(BaseHTTPRequestHandler):
    def _send(self, code, body, ctype):
        data = body.encode() if isinstance(body, str) else body
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        path = self.path.split("?", 1)[0]
        if path in ("/", "/index.html"):
            self._send(200, PAGE, "text/html; charset=utf-8")
        elif path == "/api/tasks":
            self._send(200, json.dumps(load_tasks()), "application/json")
        elif path == "/api/stats":
            self._send(200, json.dumps(load_stats()), "application/json")
        else:
            self._send(404, "not found", "text/plain; charset=utf-8")

    def log_message(self, fmt, *args):
        print("%s %s" % (self.address_string(), fmt % args), flush=True)


def main():
    server = ThreadingHTTPServer(("0.0.0.0", PORT), Handler)
    print(f"agent board → http://localhost:{PORT}  (tasks: {TASKS_DIR})", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        server.shutdown()


if __name__ == "__main__":
    main()
