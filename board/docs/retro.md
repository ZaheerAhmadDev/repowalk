# Retrospective — first demo build of the agent board

Phase: maintenance · Task: `board/tasks/MNT-1.json` · Governing skill: `skills/sdlc/sdlc-maintenance/SKILL.md`
Date: 2026-10-10 · Scope: the parallel wave that produced `board/` (REQ-1, DES-1, DEV-1, TST-1, REL-1) plus this maintenance pass.

Evidence base: `skills/meta/lessons-learned/entries/*.md` (4 entries), `board/tasks/*.json` (8 files),
`board/docs/requirements.md`, `board/docs/design.md`, plus live spot-checks run for this retro (commands below).

## 1. Metrics

| Metric | Value | Source / command |
|---|---|---|
| Tasks done vs total | At retro start: **4 of 8 done** (REQ-1, DES-1, DEV-1, TST-1), 4 open. After REL-1 (08:24:43Z) and this task close: **6 of 8 done**; 2 todo (RSK-1, RSK-3 — repowalk, next cycle) | `python3` count over `board/tasks/*.json` |
| Board-demo scope | 6 of 6 demo tasks done when MNT-1 closes (REL-1 done re-read at 08:46:37Z; see §7) | same count, RSK-* excluded |
| Lessons logged (before this retro) | **4** | `ls skills/meta/lessons-learned/entries/` → 4 files |
| Skills created this wave | **6 session-created**: 5 board skills (board-view-app-requirements 12:58, board-filestore-contract 12:59, board-endpoint-patterns 13:04, board-schema-tests 13:07, board-demo-verification 13:27 by release-agent) + 1 by this task (`meta/retro-from-lessons`); 18 `SKILL.md` on disk, 12 seeded before the wave | `stat -c '%y' skills/*/*/SKILL.md` vs seeding batch ≤12:40 |
| Test suite | **8/8 green** | `python3 -m unittest discover -s board/tests -v` → OK |
| Requirements coverage | 19 ACs (14 Must) written; only the schema subset automated | `board/docs/requirements.md` §2–3 vs `board/tests/test_tasks.py` |
| Verify catches | **4 of 4** session mistakes caught in-session by verify/grounding (none escaped) | entries 01–04 |
| Escaped problems reaching the demo | **0 known** — live board healthy at retro time | spot-check below |
| Rework cost | ~10 min (stats double-implement) + ~40 min + diagnosis (pkill ×2) + two pre-freeze corrections (501 probe, skip-hole) | entries 01–04 |

Spot-check (scratch port 8888, live 8765 untouched): `GET /` → 200, `GET /api/tasks` → 200,
`GET /api/stats` → `{"total": 8, ...}`, `POST /api/tasks/REQ-1` → 501.

## 2. What worked (keep doing)

- **One-writer-per-file held.** 8 task files written concurrently by 6 agents; zero collisions, zero torn reads, all valid JSON, suite green (`board/tests/test_tasks.py`).
- **Every mistake was caught before it escaped.** Verify steps did their job: `curl` caught the pre-implemented endpoint (entry 01), the status-code probe corrected AC-2.3 before freeze (entry 03), and a re-read caught the vacuously-green test before its first run (entry 04).
- **Rules propagated immediately.** The pkill rule landed in `agent.md` (line 51, kill-by-port) and the 501-grounding rule in `skills/board/board-view-app-requirements/SKILL.md` — entries became permanent within the same session.
- **Minimal design paid off.** The read-only, stdlib-only, no-auth, no-DB choice (`board/docs/design.md` §3, §7) meant the release needed no dependency or network work at all.
- **Each wave-1 task shipped a reusable skill** alongside its artifact (4 board skills), so the second build of anything board-shaped starts warm.

## 3. What failed (stop)

- **Task premises were not checked after seeding.** `/api/stats` already existed when DEV-1 was created with "404s" in its description — a false premise that cost ~10 min of rework (entry 01).
- **`pkill -f` self-kill happened twice** (entry 02), leaving the live server dead mid-demo-prep. The rule existed in spirit; the discipline didn't yet.
- **Seeded `created` timestamps are wrong:** 6 of 8 task files have `created` (12:00:00Z — local wall time mislabeled as UTC) *later* than `updated` (07:57–08:09Z), violating `design.md` §4.2 ("created set once, updated refreshed to now"). No test asserts `created ≤ updated`.
- **Requirement ACs are mostly untested.** The suite validates task schema only; the 14 Must ACs in `requirements.md` (DOM columns, 2s polling, broken-file visibility) have no automated runner — a broken demo could still show a green suite.
- **Kill-without-verify recurred (this task).** My own scratch-server kill killed a wrapper PID while `python3 board/server.py` kept running on port 8888; caught only when re-checking the port. Same defect class as entry 02.

## 4. What surprised (investigate)

- **Concurrency anxiety was unfounded at this scale** — lock-free ownership worked with 6 parallel writers; the rejected single-`tasks.json` alternative (`design.md` §3) would have been the actual hazard.
- **Python stdlib answers unimplemented methods with 501**, not the 404/405 the requirements guessed — grounding missed the negative path (entry 03).
- **A vacuously-green suite was easy to write by accident** — the skip-cascade hole appeared in the *first* draft of the test suite (entry 04), not under pressure.
- **`board/` accumulates runtime clutter** (`server.log`, `__pycache__/`) with no cleanup rule; cosmetic today, invisible debt if the tree is ever shared.

## 5. Bug triage

Severity per `sdlc-maintenance`: blocker (demo-breaking) / major (core feature) / minor / cosmetic.

| # | Issue | Reproduce | Severity | Status / fix-forward |
|---|---|---|---|---|
| B1 | `/api/stats` pre-implemented while seeding → DEV-1 premise false | Pre-fix: `curl localhost:8765/api/stats` → 200 despite DEV-1 saying 404 | Major | **Fixed** in verify (re-scoped; endpoint now really implemented by DEV-1 — 200 on scratch port). Class fix → AI-1 |
| B2 | `pkill -f 'board/server.py'` matched the invoking shell, killed it twice | Run `pkill -f 'board/server.py'` from a command line containing that string | **Blocker** | **Fixed**: kill by port (`fuser -k <port>/tcp`) per `agent.md:51`. Recurrence risk high → AI-2 |
| B3 | AC-2.3 guessed 404/405; real answer 501 | `curl -s -o /dev/null -w "%{http_code}" -X POST localhost:8765/api/tasks/REQ-1` → 501 | Minor | **Fixed** before freeze (probe + criterion corrected); rule in `skills/board/board-view-app-requirements/SKILL.md` |
| B4 | Skip-hole: `[]` file passes `test_valid_json`, skipped by `test_required_fields` → suite green on broken file | Put `[]` in a fixture task file, run suite → all "ok" (pre-fix) | Major | **Fixed** in first draft before any run; named-reporter + negative-check now in `skills/board/board-schema-tests/SKILL.md` |
| B5 | `created` > `updated` on 6/8 task files | `python3` loop comparing `d['created'] > d['updated']` → DES-1, DEV-1, MNT-1, REL-1, REQ-1, TST-1 | Minor | **Open** → AI-3 (fix seeded values; add `created ≤ updated` test) |
| B6 | 14 Must ACs have no automated runner; suite covers schema only | Read `board/tests/test_tasks.py` vs `requirements.md` §2 | Major | **Open** → AI-4 (acceptance-check runner, backlog) |
| B7 | Scratch-server kill left server running (killed wrapper PID, python survived on 8888) | Background a server, `kill $!`, then `fuser 8888/tcp` → PID still listed | Major | **Fixed** this session (`fuser -k 8888/tcp`, port confirmed free); own lesson logged in `skills/meta/lessons-learned/entries/` → AI-2 covers the class |
| B8 | Runtime clutter in `board/` (`server.log`, `__pycache__/`) | `ls board/` | Cosmetic | **Open** → AI-5 (backlog: cleanup + ignore rule) |

No `needs-info` items — all reproduce steps above were executed for this retro.

## 6. Action items

| # | Action | Owner | Destination |
|---|---|---|---|
| AI-1 | After seeding tasks, re-verify each task's premise against current artifacts; re-scope before spawning agents | main agent | Process change (seed step in `agent.md` / `skills/meta/agent-board/SKILL.md`) — **reported, not edited by this task** |
| AI-2 | Kill-by-port only, and **confirm the port is free after every kill** (`fuser <port>/tcp` → empty) before proceeding; never trust `$!` alone | all agents | Process change (`agent.md` verify-after-kill clause) + rule captured in this session's lesson entry |
| AI-3 | Correct seeded `created` timestamps (true UTC, ≤ `updated`) and add a `created ≤ updated` assertion to the suite | main agent (seeded files) + qa-agent (test) | Backlog → `board/tests/test_tasks.py` extension |
| AI-4 | Build an acceptance-check runner for the 14 Must ACs (curl-based checks scriptable now; DOM checks need a headless-browser decision) | qa-agent | Backlog (new task, phase: testing) |
| AI-5 | Remove/ignore `board/server.log` + `board/__pycache__/` | main agent | Backlog (repo hygiene) |

## 7. Release status (REL-1, re-read at end of this task)

**Done** (re-read `board/tasks/REL-1.json` at 2026-10-10T08:46:37Z):
`status: done`, `updated: 2026-10-10T08:24:43Z`,
`artifacts: ["board/run.sh", "board/docs/release-checklist.md"]` — both confirmed
on disk (`run.sh` executable). Release result: the demo ships with a one-command
run script and a pre-release checklist covering compile check, the 8/8 unittest
suite, and `/api/tasks` + `/api/stats` 200s. First demo build of the board is
complete: 5 of 6 board-demo tasks done (this one included), 0 known escaped problems.

## 8. Hand-off

New/changed needs → `sdlc-requirements` (AI-4 acceptance runner; RSK-1/RSK-3 begin the repowalk cycle).
Method used to build this retro → `skills/meta/retro-from-lessons/SKILL.md`.
