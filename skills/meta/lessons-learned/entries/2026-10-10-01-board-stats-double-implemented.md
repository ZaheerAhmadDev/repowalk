# 2026-10-10 — Implemented an agent's task while seeding the board

- **Mistake:** Wrote `board/server.py` including the `/api/stats` endpoint — the exact feature `DEV-1` was created to implement, leaving the implementation agent no work.
- **Cost:** Caught by the verify step (`curl` returned 200 instead of expected 404); ~10 minutes of rework + cleanup.
- **Root cause:** Built artifacts first, seeded tasks second, never checked each task's premise against what had just been written.
- **Rule:** After seeding tasks, verify each task's premise against current artifacts — re-scope any task whose work already exists before spawning agents.
