# Design — Agent Task Board (task schema + board architecture)

Phase: design · Task: `board/tasks/DES-1.json` · Governing skill: `skills/sdlc/sdlc-design/SKILL.md`

## 1. Context

Serves requirement task `board/tasks/REQ-1.json` ("Write requirements for the agent
task board", doc: `board/docs/requirements.md` once written). This document is the
hand-off contract for:

- the **task file schema** (`board/tasks/*.json`) — every agent writing a task must
  follow it exactly, per `skills/meta/agent-board/SKILL.md`;
- the **board architecture** — how files written by agents become the four-column
  view at http://localhost:8765.

## 2. Decision

**One JSON file per task (`board/tasks/<ID>.json`), one writer per file, served by a
stdlib-only read-only HTTP server that re-reads the directory on every request; the
browser polls `GET /api/tasks` every 2 seconds and renders four status columns.**
The filesystem is the database: agents own their file (edit in place), the server
never writes, and the browser never writes. Concurrency safety comes from ownership
rules, not locks.

## 3. Alternatives (rejected)

1. **Single `board/tasks.json` containing all tasks** — rejected: every agent editing
   one file collides (last writer wins, torn reads while another agent writes); no
   natural one-writer-per-task ownership, violating the repo's hard rule.
2. **SQLite database** — rejected: adds a dependency + schema migrations + a writer
   process for a read-only view of ~dozens of tasks; agents would need a DB client
   instead of a file editor; overkill (Karpathy §2: no new deps nobody asked for).
3. **HTTP write API (POST/PUT task) with in-memory store** — rejected: needs auth,
   validation endpoints, and a state source that isn't the repo; the board is a
   *view* mirroring repo state, not a task manager — and `agent.md` forbids network
   code doing writes.

## 4. Contract

### 4.1 File location and naming

- One task = one file at `board/tasks/<ID>.json` (e.g. `board/tasks/DES-1.json`).
- **`id` == filename stem rule:** the top-level `"id"` string must equal the file's
  name without `.json`. Rename filename ⇒ update `id` in the same commit.
- JSON only, UTF-8, must stay parseable after *every* edit (see §6 fallback).

### 4.2 Schema (all 10 fields, in file order)

| # | Field | Type | Required | Constraints |
|---|---|---|---|---|
| 1 | `id` | string | yes | Non-empty; equals filename stem (§4.1). Observed pattern: `REQ-1`, `DES-1`, `TST-1`, `DEV-1`, `REL-1`, `MNT-1`, `RSK-1` (`<PREFIX>-<n>`; prefix is convention only, not validated). |
| 2 | `title` | string | yes | One-line human summary (shown as card heading). |
| 3 | `phase` | string (enum) | yes | Exactly one of the **6 phases**: `requirements`, `design`, `implementation`, `testing`, `release`, `maintenance`. |
| 4 | `skill` | string | yes | Skill folder name governing the work (e.g. `sdlc-design`, `sdlc-test`). |
| 5 | `status` | string (enum) | yes | Exactly one of the **4 statuses**: `todo`, `in_progress`, `done`, `blocked`. Board renders exactly these four columns, in this order. |
| 6 | `assignee` | string \| null | yes | `null` until claimed; then the agent id (e.g. `"design-agent"`). |
| 7 | `description` | string | yes | Task body; on `blocked`, it carries the reason. |
| 8 | `artifacts` | array of string | yes | Files produced, paths relative to project root (e.g. `"board/docs/design.md"`). `[]` until something ships. |
| 9 | `created` | string | yes | ISO-8601 UTC, `YYYY-MM-DDTHH:MM:SSZ` (e.g. `"2026-10-10T12:00:00Z"`). Set once at task creation, never changed. |
| 10 | `updated` | string | yes | Same format as `created`. Must be refreshed to "now" on every status/assignee/artifact edit. |

No other top-level keys are defined; unknown keys are tolerated by the server (it
passes parsed JSON through verbatim) but are not part of this contract.

### 4.3 Ownership / single-writer model

- **Exactly one writer per task file:** the agent whose `assignee` claims the task.
  Others may *read* (fresh-read before acting) but never edit someone else's file.
- Non-task files have their own owner: artifacts belong to the task's assignee;
  `skills/index.md` and `agent.md` belong to the main agent only.
- The server process is **read-only**: it opens task files for reading only and
  never creates, edits, or deletes them.

### 4.4 Validation rules (who checks what)

The board server performs *no schema validation* — it trusts the protocol; these
rules are enforced socially by agents and verifiable mechanically:

1. `json.loads(file)` must succeed (else §6 fallback kicks in).
2. `data["id"] == path.stem`.
3. `data["status"] in {"todo","in_progress","done","blocked"}`.
4. `data["phase"] in {"requirements","design","implementation","testing","release","maintenance"}`.
5. `created`/`updated` match `^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$`.
6. `artifacts` is a list; each entry is a repo-relative path to a file that exists
   (only required at `done`).
7. Status transitions follow the claim protocol
   (`skills/meta/agent-board/SKILL.md`): `todo → in_progress → done`, or
   `→ blocked` from any state; never `done` without a green verify.

Quick check command (run from repo root):

```bash
python3 -c "import json,glob,sys
for f in sorted(glob.glob('board/tasks/*.json')):
    d=json.load(open(f))
    assert d['id']==f.split('/')[-1][:-5], f
    assert d['status'] in ('todo','in_progress','done','blocked'), f
    assert d['phase'] in ('requirements','design','implementation','testing','release','maintenance'), f
print('task files valid')"
```

## 5. Data flow

```
[agent]  edit board/tasks/<ID>.json  (one writer; keep JSON valid; updated=now)
   │           writes on disk = single source of truth
   ▼
[server.py] GET /api/tasks  ──► load_tasks():
             for path in sorted(TASKS_DIR.glob("*.json")):   # re-read per request
                 json.loads(path)  ──ok──► append task dict
                              └─OSError/JSONDecodeError──► append synthetic
                                 {id: stem, status: "blocked",
                                  title: "BROKEN FILE: <error>"}
             respond 200, JSON array, Cache-Control: no-store
   ▼
[browser]  page served at GET /  (inline HTML/JS, no build step)
             tick() on load, then setInterval(tick, 2000)   # poll every 2s
             fetch("/api/tasks") → render(): filter tasks by status
             → 4 <section> columns [todo | in_progress | done | blocked]
             → card shows id, phase, title, assignee, artifacts, updated
             also fetches GET /api/stats (currently 404 → falls back to
             counting rendered cards in the header)
```

- The board at http://localhost:8765 is **read-only for everyone**: only `GET /`,
  `GET /api/tasks` exist; every other path → 404.
- Freshness model: write-to-disk → visible within ≤2 s (next poll). No websockets,
  no push.

## 6. Risks & fallbacks

| Risk | What breaks first | Fallback |
|---|---|---|
| Agent leaves invalid JSON | Server can't parse the file | `load_tasks()` catches `JSONDecodeError` and emits a synthetic **blocked** card (`title: "BROKEN FILE: <error>"`, `assignee: "<fix this file>"`) — visible within 2 s; owner fixes the file. Never silent. |
| Concurrent edits to one task | Torn/lost writes | Avoided by design: one writer per file (§4.3). If two agents claim the same task, protocol says second claimant must fresh-read first — no lock needed at this scale. |
| Server reads mid-write (partial file) | One poll cycle shows BROKEN FILE | Self-heals on the next 2 s poll once the writer finishes; agents should finish an edit as one write operation. |
| Server dies / unreachable | "server unreachable — retrying" in header | Client keeps retrying every 2 s; files on disk are unaffected (view loss only, no data loss). Restart `python3 board/server.py` (port 8765). |
| No authentication | Anyone local could read/modify files via shell | **No auth by design**: the server exposes no mutation endpoints; the attack surface is read-only. Acceptable for a localhost hobby board. If it ever binds beyond localhost, revisit (see §7). |

## 7. Not doing (deliberate omissions)

- **No auth / users / sessions** — local single-operator board.
- **No writes over HTTP** — no POST/PUT/DELETE; status changes are file edits only.
- **No database** — the filesystem is the store (§3 alternatives).
- **No websockets/SSE** — 2 s polling is enough for human eyes.
- **No schema validation in the server** — it renders what it parses; validation is
  the writer's job (§4.4).
- **No task ordering UI, filtering, search, pagination, or per-task detail view** —
  sort order is just `sorted(glob("*.json"))`.
- **No guaranteed `/api/stats`** — optional header stat; client already tolerates 404.
- **No remote deployment / TLS / rate limiting** — board is localhost:8765 only.

*Any deviation from §4 updates this document first, then the code/protocol.*
