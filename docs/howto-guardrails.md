# How-to: architecture guardrails (F15)

**What it answers:** *Does this repo still obey the architecture rules we agreed
on?* repowalk reads a small, hand-written rule file, checks the index + analysis
against it, and reports **findings**. Hard rule violations (`Error`) make
`repowalk report` exit nonzero, so the check drops straight into CI.

## Where the rules live

`crates/core/src/rules.rs` loads, in order:

1. `<repo>/.repowalk/rules.json` (preferred — `.repowalk/` is generated and
   git-ignored);
2. `<repo>/rules.json` (fallback — the sample committed at this repo's root).

A missing, unreadable or corrupt file yields an **empty rule set** — the check
never panics and never invents findings. Every field is optional:

```json
{
  "forbid_imports": [["core", "gui"]],
  "max_cyclomatic": 40,
  "max_file_loc": 5000,
  "no_cycles": ["crates/**"]
}
```

## The four rule kinds

| Rule | Shape | Fires on | Severity |
|---|---|---|---|
| `forbid_imports` | `[[from_glob, to_glob], …]` | every import edge whose *from* side matches the from-glob **and** whose *to* side matches the to-glob | **Error** |
| `max_cyclomatic` | `N` | every analyzed function with cyclomatic complexity `> N` | Warning |
| `max_file_loc` | `N` | every indexed file with more than `N` lines | Warning |
| `no_cycles` | `[glob, …]` | for each glob, every import cycle among the module-graph nodes it selects (via `modules::cycles`) | **Error** |

Patterns are matched against **labels**, not bare paths: for the importing file
the repo-relative path, its ancestor directories, the crate directory name and the
repo directory name; for the imported target the raw target, a normalized
`/`-separated form, its first segment, a crate-directory alias, and (when it
resolves to a file) the same path labels. Glob syntax is dependency-free and
case-sensitive: `?` = one non-`/` character, `*` = one path segment (no `/`),
`**` = any run including `/`.

Findings are deterministic — ordered by `(severity, file, line, rule, message)`.

## How to run the check

The rules run inside `repowalk report` (Markdown **and** `--json`); there is no
separate command.

```bash
repowalk report ./some-repo
```

With no rules file (or no violations) the report ends with:

```
## Guardrails

_No architecture-rule findings._
```

## What an Error does to the exit code

`Error`-severity findings are printed and then the command **exits 1**. Warnings
are reported but do not change the exit code. Captured on a copy of
`fixtures/sample` with a deliberately failing rule
(`{"forbid_imports":[["a.rs","b.rs"]]}` — `a.rs` imports `b.rs`):

```
$ repowalk report ./some-repo
…
## Guardrails

| severity | rule | file | line | message |
|---|---|---|---|---|
| Error | forbid_imports[0] a.rs -> b.rs | `a.rs` | 0 | forbidden import 'crate::b' in a.rs (rule 'a.rs' -> 'b.rs') |

$ echo $?
1
```

`report --json` carries the same data in its `guardrails` object and exits 1 the
same way:

```
"guardrails":{"errors":1,"warnings":0,"findings":[{"rule":"forbid_imports[0] a.rs -> b.rs","severity":"Error","file":"a.rs","line":0,"message":"forbidden import 'crate::b' in a.rs (rule 'a.rs' -> 'b.rs')"}]}
```

So a CI job is simply `repowalk report . > ARCHITECTURE.md` — it writes the
report and fails the build when an architecture rule is broken.

## Status at this release (v2.0.0, 2026-10-16)

- **working:** `rules.rs` (load + `check`) and the `## Guardrails` section in both
  the Markdown and `--json` report, with the nonzero exit on `Error`. This repo's
  own `rules.json` (at the repo root; `forbid_imports: core→gui, gui→cli`,
  `max_cyclomatic: 50`, `max_file_loc: 5000`, `no_cycles: crates/**`) was checked
  with `repowalk report .` and reported **0 findings, exit 0**.
