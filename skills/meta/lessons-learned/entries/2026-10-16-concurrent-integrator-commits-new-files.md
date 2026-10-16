# A concurrent integrator commits your new files while you are still writing them

**Date:** 2026-10-16 · **Task:** v2 skill library · **Phase:** refine

**What happened:** Six authoring agents were spawned to write 15 new skills into
the working tree while the v2 integrator session was running its normal
commit cadence. Mid-flight the integrator committed the skill files
(`ecb6fac` "skills: the 13 packs the v2 seats wrote, indexed", then `47eaf68`).
One agent, opening its target file, found it already tracked at HEAD, byte-identical
to what it had just authored, and reported the work as "already present" — which
reads like duplication even though it was simply its own file being committed by
someone else. Meanwhile the integrator's index sweep covered 13 of the 15 packs,
leaving 4 skills (2 of them older than this batch) with no index row at all.

**Cost:** ~10 minutes of one agent's time re-deriving whether its own output was
duplicate work; one near-miss where the export skill was about to be "restored"
from HEAD, discarding a verified accuracy fix; and 4 skills sat unindexed until
the index owner diffed the folder list against `index.md`.

**Root cause:** Two writers with different jobs on one tree — the author (write
files) and the integrator (commit whatever appears). Neither is wrong; the
failure was deciding "is this file mine?" from `git ls-files`/`git log` alone,
without checking authorship and commit ordering, and trusting that "indexed by
the integrator" meant "everything is indexed".

**Rule:** In a repo with a concurrent integrator, (1) when a file you just wrote
is suddenly tracked, check `git log -1 --format='%h %an %s' -- <file>` before
concluding it is someone else's work — it is usually yours, already committed;
(2) never restore a path from HEAD without diffing first, or you discard the
fixes made after that commit; (3) index completeness is proven by set difference,
not by a commit message:

```bash
for d in $(ls -d skills/*/*/ | sed 's#skills/##; s#/$##'); do
  grep -q "$(basename "$d")" skills/index.md || echo "NOT INDEXED: $d"
done
```

**Skill?** The method is promoted to `skills/meta/skill-authoring/SKILL.md`
(gates 1–6, including "Indexed"), which is the step this batch existed to make
repeatable.
