# Guess the status code without probing — REQ-1

**Date:** 2026-10-10 · **Task:** REQ-1 · **Phase:** requirements

**What happened:** In `board/docs/requirements.md` AC-2.3 I wrote that a
mutating verb (`POST /api/tasks/REQ-1`) returns "404 or 405", assuming a
generic router. I had already grounded `/`, `/api/tasks`, and `/api/stats`, but
not the write path. A quick `curl -X POST` probe showed the real answer: **501**
(Python stdlib `BaseHTTPRequestHandler` answers unimplemented methods with 501).
I corrected the criterion before verification.

**Rule:** If an acceptance criterion names an HTTP status, probe it before
freezing the criterion — including the negative/unimplemented paths. Grounding
is not complete when you have only checked the happy endpoints.

**Skill?** Captured in `skills/board/board-view-app-requirements/SKILL.md`
(anti-mutation AC + 501 note). No separate rule needed.
