# Agent Board — demo v0

Locally hosted board for the software development cycle. Humans watch it in a
browser; agents take tasks by editing files. No dependencies beyond Python 3.

## Run

```bash
python3 board/server.py        # http://localhost:8765  (BOARD_PORT to change)
# or
./board/run.sh
```

## Endpoints

| Path | Returns |
|---|---|
| `/` | board page (auto-refreshes every 2s) |
| `/api/tasks` | all task files as JSON array |
| `/api/stats` | `{total, statuses, phases}` counts |

## Tasks

Source of truth: `board/tasks/*.json` — one file per task, one writer per task.
Claim protocol and schema: [`../skills/meta/agent-board/SKILL.md`](../skills/meta/agent-board/SKILL.md).

Statuses (the four columns): `todo → in_progress → done` (or `blocked`).

## SDLC skills

Each task's `skill` field points at the governing skill in
[`../skills/`](../skills/index.md) (`sdlc-requirements` → `sdlc-design` →
`sdlc-implement` → `sdlc-test` → `sdlc-release` → `sdlc-maintenance`).

## Tests

```bash
python3 -m unittest discover -s board/tests -v
```
