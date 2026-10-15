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

## Live messages (coordination)

The task files say *what*; messages say *what is happening right now*. The board
shows the feed live and the whole log is stored in
`board/messages/messages.jsonl` (append-only, one JSON object per line).

```bash
python3 board/messages.py post --from <seat> --to <seat|team|all> --re <task-id> \
  --kind <claim|progress|handoff|question|answer|blocked|done|note> --body "…"
python3 board/messages.py read --to <seat>     # your inbox (team + broadcasts included)
python3 board/messages.py read --re <task-id>  # everything about one card
python3 board/messages.py stats
```

Post at these five moments — this is the handoff currency between seats:

1. **claim** — before touching a file: `--kind claim --re <card> --body "starting <file>"`.
2. **progress** — at a milestone (interface frozen, first test green) so dependents can plan.
3. **handoff** — when you finish something someone else needs: name the interface,
   the file, and how to verify it. Address the *consumer* seat, not `all`.
4. **question / blocked** — when you need something: `--kind question` to a specific
   seat, `--kind blocked` with the exact reason and what would unblock you.
5. **done** — when your card is done: the card gets `status: done` **and** a `done`
   message with one line of evidence (command + observed result).

Etiquette:

- One writer per file, always — messages are for coordination, never for grabbing a file.
- Keep messages to one or two sentences; the card description holds the long form.
- Never claim a card whose `depends-on` message (a `handoff`) has not arrived.
- The sequence number is permanent: quote it (`see #42`) when you answer a message.
