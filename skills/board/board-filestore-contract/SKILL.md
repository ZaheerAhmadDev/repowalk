---
name: board-filestore-contract
description: Use when changing, validating, or extending the agent board's file-backed task store — schema fields, one-writer-per-file ownership, broken-JSON fallback, or the poll-based data flow. Not for claim protocol (agent-board).
---

# Board Filestore Contract

Full spec: `board/docs/design.md` (DES-1). This is the distilled method.

## Design axioms

- **Filesystem is the database.** One task = one file `board/tasks/<ID>.json`;
  `id` must equal the filename stem. Server is read-only; agents are the only writers.
- **One writer per file.** Concurrency safety comes from ownership, not locks —
  never edit a task file whose `assignee` is another agent (or null-and-unclaimed
  by someone else first).
- **Validate at write, tolerate at read.** Server does no schema checks; it renders
  whatever parses. Invalid JSON is *never* silent: it becomes a synthetic `blocked`
  card (`title: "BROKEN FILE: <error>"`). Keep every edit parseable.

## Schema (10 fields — all required)

`id` (== filename stem) · `title` (string) · `phase` (enum: requirements | design |
implementation | testing | release | maintenance) · `skill` (string) · `status`
(enum: todo | in_progress | done | blocked — exactly these four board columns) ·
`assignee` (string | null) · `description` (string) · `artifacts` (string[] of
repo-relative paths) · `created` / `updated` (ISO-8601 UTC `...Z`; refresh
`updated` on every status/assignee/artifact edit, never touch `created`).

No other top-level keys are part of the contract.

## Data flow (freshness model)

agent writes file → server re-globs `tasks/*.json` per `GET /api/tasks` → browser
polls every 2 s → renders 4 columns. Write-to-visible latency ≤ 2 s. No websockets,
no HTTP writes, no DB — those are deliberate omissions, not gaps.

## Changing the store

Any schema or flow change updates `board/docs/design.md` **first**, then the code
and `skills/meta/agent-board/SKILL.md`. Deviations in either place without the
doc lead drift.
