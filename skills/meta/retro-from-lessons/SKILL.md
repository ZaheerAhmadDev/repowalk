---
name: retro-from-lessons
description: Use when running a retrospective from lesson entries plus task-board state — gathering evidence, computing metrics, building the bug-triage table, and writing action items with owners and destinations.
---

# Retro from lessons + board state

**Goal:** turn scattered lesson entries and task JSONs into a short, fully-cited
retro artifact (`board/docs/retro.md` for this repo). Companion that *executes*
the retro format defined in `skills/sdlc/sdlc-maintenance/SKILL.md`.

## 1. Gather evidence (before writing a word)

- `ls skills/meta/lessons-learned/entries/` — count first, then read all entries.
  The count you report must equal this `ls`, taken *before* you add your own.
- Read every `board/tasks/*.json` (statuses, assignees, artifacts) — that's
  "who did what". Grep plan/promise docs (`requirements.md`, `design.md`)
  section-by-section; never read them in full.
- Run live spot-checks yourself (scratch port only, never the live port) so the
  retro carries first-hand facts, not second-hand claims.

## 2. Metrics table

At minimum: tasks done vs total (by counting JSON statuses — say how you
counted), lessons logged (the `ls` count), skills created this wave (use
`stat -c '%y' skills/*/*/SKILL.md` to separate *seeded* skills from
*session-created* ones — same-day mtimes lie), test suite result, and
verify catches vs escaped problems.

## 3. Worked / failed / surprised

- **Worked (keep):** rules that held, verify steps that caught things.
- **Failed (stop):** rework, false premises, missing tests — cite the lesson
  entry or file that proves each.
- **Surprised (investigate):** assumptions the session falsified.

## 4. Bug-triage table

One row per issue, columns: issue · reproduce (an executable command or
explicit `needs-info` — never guess a repro) · severity
(blocker/major/minor/cosmetic per sdlc-maintenance) · status + fix-forward.
Sources: lesson entries **plus** fresh observations from your spot-checks —
a retro that only paraphrases existing entries is not a retro.

## 5. Action items

Each row: action · **owner** · **destination** (skill update / backlog item /
process change). Destination must name a real file or task to be created — and
if the destination is a file you don't own (e.g. `agent.md`, `skills/index.md`),
mark it *reported*, not edited.

## 6. Close

- Near the end, **re-read** any still-moving task you want to report on (release
  results change minute-to-minute); if it isn't `done`, mark it in-flight with a
  timestamp instead of guessing.
- **Verify:** every claim cites a path that exists (spot-check them); counts
  match fresh `ls`; then close your own task JSON (valid JSON, `artifacts`,
  `updated=now`).
