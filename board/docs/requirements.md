# Requirements — Agent Task Board (local, view-only)

Phase: requirements. Frozen for REQ-1. No design, no code here.

Grounding (state of the system at time of writing, checked 2026-10-10):
`GET /` serves the board page (auto-refresh `setInterval(tick, 2000)` → 2s);
`GET /api/tasks` returns all `board/tasks/*.json` as a JSON array;
`GET /api/stats` currently returns **404** (server falls through to "not found")
even though `board/README.md` documents it; the tasks directory holds **7** JSON
files; the browser has no edit controls (read-only view).

## 1. Problem statement

Several agents and a human cooperate on one project, and right now nobody can
tell at a glance who is working on what: task state lives only inside scattered
JSON files on disk, so the human has to open files or run commands to see which
work is claimed, blocked, or finished, and has no way to notice a malformed or
missing task before it stalls someone. The human wants a single local page,
open in a browser, that always shows the current state of every task — the four
status columns, who claimed each one, and what it produced — and that keeps
itself current without anyone pressing reload, so that watching the board is
reliable enough to coordinate from.

## 2. User stories and acceptance criteria

Each criterion is a single, independently runnable check.

### US-1 — Human views the board in a browser

*As a human project owner, I want to open one local URL and see every task in
its status column, so that I know the state of the project without reading
files.*

- **AC-1.1** Given the server is running, when I `curl -s -o /dev/null -w "%{http_code}" http://localhost:8765/`, then the result is `200`.
- **AC-1.2** Given the page has loaded, when I inspect the rendered DOM, then there is exactly one column per status (`todo`, `in_progress`, `done`, `blocked`) and no fifth column.
- **AC-1.3** Given `board/tasks/REQ-1.json` exists, when I load `/`, then a card is present whose text includes `REQ-1` and the card's parent column matches that file's `status` field.
- **AC-1.4** Given a task with `"assignee": "requirements-agent"`, when I load `/`, then that card displays the string `requirements-agent`.
- **AC-1.5** Given the tasks directory, when I run `ls board/tasks/*.json | wc -l` and compare it to `document.querySelectorAll(".card").length` on the page, then the two counts are equal.

### US-2 — Agent claims and completes a task

*As an agent, I want the board to reflect a claim or completion I make by
editing my own task file, so that my work is visible without any board-side
action.*

- **AC-2.1** Given `board/tasks/REQ-1.json` has `"status": "todo"`, when an agent rewrites it to `"status": "in_progress", "assignee": "<id>"` and keeps the JSON valid, then within one refresh interval `curl -s http://localhost:8765/api/tasks` contains `"status": "in_progress"` and that `<id>` for id `REQ-1`.
- **AC-2.2** Given a task file whose `status` is changed to `done` and whose `artifacts` lists produced files, when I fetch `/api/tasks`, then the returned object for that id shows `"status": "done"` and the artifact paths as array entries.
- **AC-2.3** Given the board is a pure view, when I request `POST /api/tasks/REQ-1` (or any mutating verb) with `curl -s -o /dev/null -w "%{http_code}"`, then the response is a non-2xx status (observed today: `501`; `404`/`405` also acceptable) and the on-disk file's bytes are unchanged (compare `md5sum board/tasks/REQ-1.json` before and after).
- **AC-2.4** Given a valid ISO-8601 `updated` field on a task, when I fetch `/api/tasks`, then the value round-trips unchanged (string equality in the JSON response).

### US-3 — Live updates without reload

*As a human watching the board, I want it to refresh itself, so that a claim or
completion made by an agent appears without me touching the page.*

- **AC-3.1** Given the page is open, when I read the client script, then a timer calls the refresh function on a `2000` ms interval (check: grep the served page for `setInterval(tick, 2000)`).
- **AC-3.2** Given the page is open and untouched, when I append a new valid task file to `board/tasks/`, then within `≤ 4000 ms` (two intervals) a card for it is present — measured by a headless-browser timestamped DOM observation.
- **AC-3.3** Given the page is open, when I change a task's `status` on disk, then within `≤ 4000 ms` that card is in the new column and is absent from the old one.
- **AC-3.4** Given the page is open, when I inspect the DOM after any refresh, then the document was not reloaded (check: a page-global set before the first refresh still exists — `window.__probe` survives).
- **AC-3.5** Given a failed or malformed response, when the refresh runs, then the page does not go blank: the previous cards remain in the DOM (check: temporarily serve a non-JSON body and assert card count is unchanged).

### US-4 — Broken-task visibility

*As a human or agent, I want a malformed or unreadable task file to show up as
obviously broken instead of silently vanishing, so that I can fix it before it
blocks someone.*

*Note on running US-4 checks: they require a malformed fixture in
`board/tasks/`. Use a uniquely-named throwaway file (e.g. `ZZ-BROKEN.json`)
created and deleted inside the check — never edit or delete an existing task
file (one-writer rule).*

- **AC-4.1** Given a file in `board/tasks/` containing invalid JSON, when I fetch `/api/tasks`, then the response still returns HTTP `200` and includes an entry surfacing that file as an error/blocked placeholder (check: entry's filename matches the broken file and it carries an error marker).
- **AC-4.2** Given that broken file, when I load `/`, then a card for it is visible in the `blocked` column (or in an explicitly-labelled error area) rather than absent — check: `document.body.innerText` contains the filename.
- **AC-4.3** Given the broken file, when I fix it to valid JSON, then within `≤ 4000 ms` the error marker is gone and the card renders with its real `status`.
- **AC-4.4** Given a task JSON missing a schema field (e.g. no `assignee`), when I fetch `/api/tasks`, then the entry is present with a null/absent value for that field and the other fields are intact (check: per-field comparison against the file on disk).
- **AC-4.5** Given the README documents `/api/stats`, when I `curl -s -o /dev/null -w "%{http_code}" http://localhost:8765/api/stats`, then either the code is `200` with `{total, statuses, phases}` keys, or the README line is removed — the two must not disagree (check: the pair of commands above).

## 3. Priorities (MoSCoW)

| Priority | Requirement |
|---|---|
| **Must** | US-1 (browser view of all four columns, one card per task file, assignee shown) — AC-1.1–1.5 |
| **Must** | US-2 (disk claim/done mirrored to the API; board rejects writes) — AC-2.1–2.3 |
| **Must** | US-3 (2s auto-refresh, no manual reload, no full page reload) — AC-3.1–3.4 |
| **Must** | US-4 (broken JSON stays visible, never silently dropped) — AC-4.1–4.3 |
| **Should** | AC-3.5 (degrade gracefully on a bad response) |
| **Should** | AC-4.4 (tolerate missing optional fields) |
| **Should** | AC-4.5 (stats endpoint matches its documentation) |
| **Could** | Per-task artifact links rendered as clickable paths |
| **Could** | Phase (`requirements`…`maintenance`) shown as a badge/filter |
| **Won't** | See §4. |

## 4. Out of scope (explicitly excluded)

- **Authentication / authorization of any kind** — no logins, roles, or tokens.
- **Multi-user edit UI** — humans never edit tasks in the browser; the board is read-only and agents edit files directly.
- **Database** — `board/tasks/*.json` files remain the only store; no SQLite/Redis/other.
- **Network writes** — no outbound network code, no remote sync, no webhook, no cloud; localhost only.
- **Task creation/deletion/mutation via HTTP** — GET-only surface.
- **Real-time push (websockets/SSE)** — polling every 2s is sufficient for this cycle.
- **Cross-project or multi-board federation.**
- **Notifications, dashboards beyond this board, metrics history, analytics.**
- **Repowalk application features** — this document covers only the board.

## 5. Open questions

| # | Question | Owner | Blocking? |
|---|---|---|---|
| Q1 | Should `/api/stats` be implemented, or should the README row be deleted? Two documented behaviours disagree today (AC-4.5). | main agent | No — US-1..US-4 work without it; Should-priority |
| Q2 | Ambiguity: for a broken file, does "visible" mean a `blocked`-column card or a separate error area? Both satisfy US-4 as written; pick one before design. | human owner | No for requirements; yes before `sdlc-design` |
| Q3 | Ambiguity: must a *removed* task file disappear from the board within one refresh, and is that a `done`-card disappearance or an explicit "removed" state? Not covered by AC-3.3 (status change only). | human owner | No |
| Q4 | Is `artifacts` free-text paths or must every entry exist on disk (checkable via `test -e`)? | human owner | No |
| Q5 | Should card ordering inside a column be by `updated` (newest first) or by filename? Unspecified. | human owner | No |

---

Freeze rule: every **Must** criterion above is independently checkable (each names
its own command or DOM assertion). A change to any Must restarts the requirements
cycle for that item only.
