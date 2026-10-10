---
name: agent-board
description: How agents claim and complete tasks on the local agent board. Use whenever asked to take a task, report progress, or update task status for work happening on this project.
---

# Agent Board — Claim Protocol

Locally hosted board. Humans watch it in the browser; agents read/write it as files.

- **Server:** `board/server.py` (Python stdlib only) — http://localhost:8765
- **Tasks (source of truth):** `board/tasks/*.json` — one file per task, one writer per task

## Task schema

```jsonc
{
  "id": "REQ-1",                 // must match filename (REQ-1.json)
  "title": "…",
  "phase": "requirements",       // requirements|design|implementation|testing|release|maintenance
  "skill": "sdlc-…",             // skill governing the work
  "status": "todo",              // todo|in_progress|done|blocked
  "assignee": null,              // your agent id once claimed
  "description": "…",
  "artifacts": [],               // files you produced (paths relative to project root)
  "created": "ISO-8601", "updated": "ISO-8601"
}
```

## Claim procedure (strict order)

1. **Fresh read** — open your task file right before editing; never act on a cached copy.
2. **Claim** — set `status: "in_progress"`, `assignee: "<your-id>"`, `updated: now`.
3. **Work** — read your governing skill (`skills/<skill>/SKILL.md`), do only this task.
4. **Finish** — set `status: "done"`, list every produced file in `artifacts`, `updated: now`.
5. **Blocked?** — `status: "blocked"` + the reason in `description`. Never fake done.

## Rules

- Only edit **your own** task file and **your own** artifact files. Never touch another task's file — that's that agent's contract.
- Keep the JSON valid after every edit (a broken file shows as a blocked card on the board).
- Status vocabulary is exactly `todo | in_progress | done | blocked` — the board renders those four columns and nothing else.
- The board is a view, not a task manager: work is done in the repo, status is mirrored here.
