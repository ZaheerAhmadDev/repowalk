# How to use the Health, Notes and 101 panels

Three sidebar tabs have no command-line twin — they exist to answer a question
the moment it occurs to you, without leaving the app. This page covers what each
one shows, what it does when the data is *not* there (it says so), and where it
keeps what you write.

Open a repository first (toolbar **Open…**, or `repowalk ./some-repo`). The
sidebar tab strip is `Map · Usage · Health · Notes · 101`; **Usage** is covered in
[howto-variables.md](howto-variables.md) and [howto-refs.md](howto-refs.md).

---

## Health — "what should worry me?"

`Health` is one scroll of the five things that usually matter, all computed from
the artifact `learn` produced. Nothing here is inferred or guessed; every number
is a rule you can trace back to a file and a line.

| Section | What it lists | Click behaviour |
| --- | --- | --- |
| **Overview** | files, LOC, symbols, cycles for the whole repo | — |
| **Complexity by module** | per-module `Σ total · max · fn` roll-up | opens the module's entry file (`lib.rs` / `main.rs` / `mod.rs` / `index.*`, else its first file) |
| **God files** | files that trip the size rule, each labelled with the rule that named it (`loc · 1617 L · fan-out 133`) | opens the file |
| **Hotspots** | top functions by cyclomatic complexity, `cyc N · nest M` | jumps to the line |
| **Unused variables** | variables with no read site | — |
| **Guardrails** | findings from `rules.json` | — |

Two honesty rules the panel follows:

- **A number never travels alone.** A god file says *why* it was flagged; a
  hotspot says its complexity; a finding carries its rule and `file:line`.
- **Missing data is distinguished from zero data.** With no repository you get
  `no repository loaded`. With an index but no analysis you get the overview plus
  an explicit list of what is unavailable and why — not a screen of plausible
  zeros. A clean guardrail run prints `no findings — every rule passes`; a repo
  with no `rules.json` at all prints a different message, because "nothing is
  configured" and "nothing is wrong" are different facts.

Severity is carried by more than colour: `error` findings are tinted with
`DANGER`, `warning` findings with `WARNING`, and each is prefixed with its kind.

**The same numbers from the terminal** (for a CI job or a pasted report):

```bash
repowalk report ./repo --json        # the underlying data
repowalk complexity --by function --top 20 ./repo
repowalk complexity --by file ./repo
```

`repowalk report` also runs the guardrails and **exits non-zero** when a rule at
`Error` severity fires — see [howto-guardrails.md](howto-guardrails.md).

---

## Notes — "remember this for later"

Notes are bookmarks with a sentence attached. They answer the problem that a
codebase you are reading for the first time is a puzzle you will have to put down
and pick up again.

- **Add a note** to the *current selection* — a file or a symbol. `Add note`
  takes the text, `Save` writes it, the list groups notes by file, and each row
  shows its target and its creation date.
- **Click a note** to jump: the text pane opens the file (at the line, when the
  note was made on a symbol) and the symbol is selected if it still exists.
- **Edit and delete** in place. **Filter** by note text, file or symbol.
- **A note whose target disappears is kept and marked `stale`** — it is never
  silently dropped, because the note is usually the reason you know the thing
  used to exist.

Where they live: `<repo>/.repowalk/notes.json`, next to the analysis caches, so
notes travel with the checkout and are ignored by `.gitignore` in this repo.
Writes are atomic (a temporary file, then a rename) and **every mutation is
persisted before the panel shows it** — if the write fails, the change is rolled
back and the error is shown rather than a note that was never saved.

Reading is deliberately tolerant: a missing, unreadable, corrupt or
wrong-version store loads as *empty* and the panel says why it is empty instead
of showing a silently blank list. Corrupt files are reported, never hidden and
never rewritten behind your back.

---

## 101 — "orient me in thirty seconds"

`101` is the panel to open when you have been handed a repository and nobody is
around to explain it. It is written as five plain questions, in the order people
actually ask them:

1. **What this is** — languages, size, and what the repo appears to be.
2. **Status** — whether the learn pass is still running (`learning · parse · 91/344 files`),
   what it found (`learned · 412 variables · 96 cfgs · 214 types`), and any
   degradation, instead of a spinner that hides progress.
3. **Where to start** — the first entries of the reading order
   ([howto-learn.md](howto-learn.md)), clickable.
4. **How it is shaped** — the module tree, or the top-level directories with
   `module tree not populated yet — showing top-level dirs` when the analysis is
   not there yet.
5. **Where the risk is** — the most complex files and functions, and the unused
   variables, with `hot functions: none (no control-flow data)` when that data is
   genuinely absent.

Honest states are explicit and consistent: `no repository loaded — press Learn`,
`analysis not loaded — press Learn`, `entry points: none detected`,
`languages: unknown`. Each card is clickable into the rest of the app, so 101 is
a starting point rather than a dead end.

---

## See also

- [howto-learn.md](howto-learn.md) — the pass that produces everything above
- [howto-variables.md](howto-variables.md), [howto-refs.md](howto-refs.md) — the Usage tab
- [howto-guardrails.md](howto-guardrails.md) — `rules.json` and the exit contract
- [architecture.md](architecture.md) — where these panels live in the code
