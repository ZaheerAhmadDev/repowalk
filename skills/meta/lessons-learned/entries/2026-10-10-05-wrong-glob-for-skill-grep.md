# 2026-10-10 — grep tool include glob missed nested SKILL.md files

**Mistake:** Used the Grep tool with `include: "*/SKILL.md"` under `skills/`
to find skill descriptions; it matched nothing, because the files are nested
two levels deep (`skills/<compartment>/<skill>/SKILL.md`). agent.md already
gives the exact command (`grep -r "^description:" skills/*/*/SKILL.md`); I
reimplemented it with the wrong pattern instead of running it.

**Cost:** one wasted tool call.

**Rule:** When agent.md prescribes an exact command, run that command via bash
— do not paraphrase it into a tool call with a subtly different glob.
