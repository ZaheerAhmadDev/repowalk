# 2026-10-10 — pkill -f killed its own shell, twice

- **Mistake:** Restarted the board with `pkill -f 'board/server.py'` — the pattern matched the shell's own command line (same string inside it), killing the shell mid-sequence; the "fix" using `board/ser[v]er.py` self-matched too because the start command in the same line held the literal path.
- **Cost:** two 20 s timeouts, live server left dead, diagnosis needed before the demo could proceed.
- **Root cause:** kill pattern also appears in the invoking command; kill + start mixed in one sequence.
- **Rule:** kill by port (`fuser -k <port>/tcp`) instead of name-matching; keep kill and start in separate calls; assume `-f` patterns match your own command line.
