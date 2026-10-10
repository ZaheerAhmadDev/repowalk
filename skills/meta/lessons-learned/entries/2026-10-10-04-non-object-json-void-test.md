# Skip-hole makes a schema suite vacuously green — TST-1

**Date:** 2026-10-10 · **Task:** TST-1 · **Phase:** testing

**What happened:** First draft of `board/tests/test_tasks.py` had
`test_required_fields_present` skip any file whose top-level JSON wasn't an
object (`if reason or not isinstance(data, dict): continue`). A task file
containing `[]` or `"x"` would then pass `test_valid_json` (it *is* valid
JSON) and be silently skipped by every other test — the whole suite green on a
broken file. Caught it while re-reading the draft, before the first run.

**Rule:** In a loop-based test that skips unparseable/wrong-shaped inputs,
exactly one test must *own* each failure class. If `test_valid_json` accepts a
shape that `test_required_fields` skips, the suite has a hole: every skip
condition needs a named reporter. Then negative-check: mutate fixtures in
`/tmp/opencode` and confirm every test fails on its own defect.

**Skill?** Yes — `skills/board/board-schema-tests/SKILL.md` ("no cascade"
rule + negative-check step).
