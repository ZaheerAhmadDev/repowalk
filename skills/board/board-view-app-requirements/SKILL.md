---
name: board-view-app-requirements
description: Use when writing requirements for a local file-backed, view-only dashboard (agent board or similar) — user stories, Given/When/Then checks, and scope fences for browser views over JSON files with no auth, no DB, no network writes.
---

# Requirements pattern — local file-backed view-only board

Applies when the system under requirements is: a localhost HTTP page that
renders state from JSON files on disk, written by agents, watched by humans,
with **no** auth, edit UI, database, or outbound network.

## Requirement shapes that recur

- **View story** (human in browser): AC must name an HTTP check for the page
  (`curl -w "%{http_code}" …/` → `200`), a DOM check (one card per task file,
  card count == `ls board/tasks/*.json | wc -l`), and a column check (card's
  parent column == the file's `status`).
- **Mutation story** (agent claims/completes): AC mirrors disk → API: after
  rewriting the file, `curl …/api/tasks` shows the new `status`/`assignee`
  within one refresh interval. Never require a write endpoint.
- **Anti-mutation AC**: mutating verbs (`POST/PUT/DELETE`) must be non-2xx and
  the file's `md5sum` unchanged — this is how "read-only" becomes testable.
  Check the real code first: an unimplemented method on stdlib
  `BaseHTTPRequestHandler` returns **501**, not 404/405.
- **Live-update story**: AC names the interval literal in the served page
  (e.g. `setInterval(tick, 2000)`), a ≤2×interval bound for a change to appear,
  and a no-full-reload proof (page-global `window.__probe` survives).
- **Broken-input story**: invalid JSON must surface as a visible placeholder,
  never be silently dropped. AC: `/api/tasks` still `200`, entry carries the
  filename + error marker, card visible in DOM. Note in the doc that testing
  this needs a throwaway fixture file — never mutate another writer's task file.

## Grounding before you write

Verify, don't trust the README: `curl` `/`, `/api/tasks`, `/api/stats`, and a
`POST` probe; `grep setInterval` on the served page; `ls board/tasks/*.json | wc -l`.
README vs. reality mismatches become **open questions**, not silent assumptions.

## Scope fences to state explicitly

auth · multi-user edit UI · database · network/outbound writes · HTTP mutation ·
websockets/SSE (polling suffices) · notifications · multi-board federation.

## Testability rule

Every Must criterion = one independently runnable check whose command or DOM
assertion is written into the criterion itself (status code, grep, md5sum,
count equality, time bound). Ambiguity → open question with owner + blocking flag.
