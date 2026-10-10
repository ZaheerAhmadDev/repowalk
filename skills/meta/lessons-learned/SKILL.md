---
name: lessons-learned
description: Log of mistakes made while building in this repo and the rules extracted from them. Use after any rework, failed verification, wrong assumption, or unnecessary change — to record the mistake and decide whether it becomes a skill.
---

# Lessons Learned — the LEARN stage

Feedback loop: **BUILD → LEARN → REFINE** (see `agent.md`).
Mistake → record an entry → extract a rule → promote to a skill if it recurs.
An entry with no rule is noise; the rule is the product.

## When to write an entry

- A verify command failed and you had to rework
- You assumed instead of asking (wrong interpretation, hidden confusion)
- You overcomplicated something a senior engineer would call too big
- You touched code the request didn't ask for
- You skipped naming success criteria and got lost

## How to record (conflict-free)

Create **one new file** — never edit another agent's entry, never edit this file:

```
skills/meta/lessons-learned/entries/YYYY-MM-DD-<short-slug>.md
```

```markdown
# YYYY-MM-DD — Short title
- **Mistake:** what was done
- **Cost:** time lost / what broke
- **Root cause:** why it happened
- **Rule:** the rule going forward
```

Find all entries: `ls skills/meta/lessons-learned/entries/` (date-prefixed, chronological).

## Promotion rule (the REFINE stage)

Same mistake twice, or a rule that would fire on many tasks → create
`skills/<compartment>/<skill-name>/SKILL.md` (frontmatter `name` +
`description` starting with "Use when…"), then report it to the main agent —
only the main agent edits `skills/index.md` (single writer).

Entries: `entries/` directory.
