# 2026-10-10 — Killed the wrapper PID, left the scratch server running

- **Mistake:** Backgrounded a scratch board server (`BOARD_PORT=8888 python3 board/server.py &`), captured `$!`, ran `kill <pid>`, and moved on without re-checking the port. The killed PID was a wrapper; `python3 board/server.py` (its own PID, fds pointing at my log) survived and kept listening on 8888.
- **Cost:** ~5 minutes of diagnosis (`ps -o cmd -p`, `/proc/<pid>/fd`) plus cleanup with `fuser -k 8888/tcp`; a stale server could have served my "verify" curls and made the results untrustworthy.
- **Root cause:** trusted `$!`/`kill` success without observing the effect on the resource; same defect class as the pkill entry (kill treated as fire-and-forget).
- **Rule:** after any kill, confirm the *port* is free (`fuser <port>/tcp` → empty) before continuing or re-binding; never trust a PID kill you didn't verify against the resource it held.
