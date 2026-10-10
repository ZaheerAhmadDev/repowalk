---
name: board-schema-tests
description: Use when writing or running tests over board/tasks/*.json — schema invariants worth asserting, and how to validate a directory of shared JSON while other agents write to it concurrently. Companion to board-filestore-contract (store side).
---

# Board schema tests

`board/tests/test_tasks.py` is the reference implementation (stdlib unittest,
no deps). One test method per schema rule; failure message names the file.

## Invariants worth testing

Assert only what the task file contract states — never derived politeness:

1. **Valid JSON**, top-level object, file parses.
2. **Required fields**: `id, title, phase, skill, status, assignee, description,
   artifacts, created, updated`.
3. **`id` == filename stem** (e.g. `TST-1.json` → `"TST-1"`).
4. **Enums**: `phase ∈ {requirements, design, implementation, testing,
   release, maintenance}`; `status ∈ {todo, in_progress, done, blocked}`.
5. **Types**: `assignee` null or string; `artifacts` list of strings.
6. **Unique ids** across the directory (pairs with rule 3; stems are unique
   in a directory, so duplicates mean a wrong `id`).

Missing-key rules don't need duplicate checks in per-field tests — a dedicated
required-fields test covers them; per-field tests skip absent keys.

## Testing a shared directory under concurrent writers

- **Retry once on read**: a truncating writer can be caught mid-write →
  `JSONDecodeError`/`FileNotFoundError`. Re-read once before declaring a
  defect; a persistently invalid file still fails both attempts.
- **No cascade**: only the JSON-validity test reports parse errors; other
  tests skip unreadable files (they'd otherwise fail 8× for one transient).
- **Fresh snapshot per test**: glob + read inside each test method so no test
  depends on another's stale view; deterministic — no sleeps, no ordering.
- **Never "fix" a red suite by editing another agent's task file** — re-run
  once; if it's still red on a syntactically valid file, it's a real defect:
  report it, don't mask it.
- **Negative-check the suite** in `/tmp/opencode` (copy tests + mutated task
  fixtures) to prove each test fails on its defect; never mutate `board/tasks/`.

Verify: `python3 -m unittest discover -s board/tests -v`
