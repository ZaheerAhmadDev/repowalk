---
name: repowalk-guardrails-rules
description: Use when reading, writing, or extending repowalk's architecture guardrails (rules.json / crates/core/src/rules.rs) — the rule schema, severities, exit-code contract, CI gating, glob-labelling, and adding a new rule kind.
---

# repowalk — Guardrails (`rules.json`) and the rules engine

A repo declares architectural rules in a small hand-written JSON file; repowalk
evaluates them against the index + analysis and emits deterministic **findings**.
`Error` findings make `repowalk report` exit non-zero, so the gate drops into CI.

> The invariant: **the same `(index, analysis, rules)` always yields the same
> findings, ordered `(severity, file, line, rule, message)`, and every `Error`
> makes `report` exit `1`.** Source of truth: `crates/core/src/rules.rs`.

Read `repowalk-metrics` for the numbers rules consume (not re-derived here) and
`repowalk-no-network-guardrails` for the zero-network invariant — a `rules.json`
fences imports but can never prove "no sockets".

## Schema — `rules.json` (all fields optional)

`Rules` derives `Deserialize` with `#[serde(default)]` on every field, so **no key
is required** and `Rules::default()` is the empty (disabled) rule set. There is no
`version` key. Unknown keys are silently ignored (no `deny_unknown_fields`).

| key | JSON type | default (off) | severity | what it selects |
|---|---|---|---|---|
| `forbid_imports` | array of `["from_glob","to_glob"]` | `[]` | `Error` | every `Index.imports` edge whose **from**-labels match `from_glob` **and** **to**-labels match `to_glob` |
| `max_cyclomatic` | number (`u32`) | `null` | `Warning` | every `Analysis.cfgs` entry with `cyclomatic > N` |
| `max_file_loc` | number (`usize`) | `null` | `Warning` | every `Index.files` entry with `lines > N` |
| `no_cycles` | array of glob strings | `[]` | `Error` | per glob, every import cycle among module-graph nodes whose `id` matches |

This repo's committed sample (`rules.json` at the repo root):

```json
{
  "forbid_imports": [["core", "gui"], ["gui", "cli"]],
  "max_cyclomatic": 50,
  "max_file_loc": 5000,
  "no_cycles": ["crates/**"]
}
```

## What a violation is — `Finding`

`struct Finding { rule, severity, file, line, message }`. `Severity` derives `Ord`
with `Error` first, so **`Error` sorts before `Warning`**; both serialize as `"Error"`/`"Warning"`.

| field | type | meaning |
|---|---|---|
| `rule` | string | stable id — see below |
| `severity` | `"Error"` \| `"Warning"` | `Error` fails the build; `Warning` is advisory |
| `file` | string | repo-relative file path; for `no_cycles` it is the first module-node id |
| `line` | number | 1-based; **`0` when the rule has no line** (`Import` has no line field, so imports/cycles/LOC are `0`) |
| `message` | string | human-readable explanation, includes the symbol/edge |

| rule | id shape | line |
|---|---|---|
| `forbid_imports` | `forbid_imports[i] {from} -> {to}` (e.g. `forbid_imports[0] core -> gui`) | always `0` |
| `max_cyclomatic` | `max_cyclomatic` | the function's line (`complexity::parse_symbol_id`) |
| `max_file_loc` | `max_file_loc` | always `0` |
| `no_cycles` | `no_cycles {glob}` | always `0` |

## Severity effect (the exit-code contract)

- `forbid_imports` and `no_cycles` are **`Error`** — architectural, hard.
- `max_cyclomatic` and `max_file_loc` are **`Warning`** — thresholds, soft.
- `report` counts `Severity::Error` findings; if `> 0` it still prints the whole
  report first, then `anyhow::bail!` → `Error: N Error-severity architecture
  finding(s) in rules.json` on stderr, **exit `1`**. Warnings never change the
  exit code. (Full CLI exit table: `repowalk-cli-contract`.)

## Target selection and globs

Patterns are matched against **labels**, not bare paths, with `rules::glob_match`:

- `?` = exactly one non-`/` character; `*` = one path segment (no `/`);
  `**` = any run **including** `/`; every other char is a literal, case-sensitive
  match. There are **no** character classes, brace groups, or escaping.
- `crates/*/src` matches `crates/core/src` but **not** `crates/a/b/src`;
  `crates/**/src` matches both.

| side | labels offered |
|---|---|
| importing file (`from`) | repo-relative path, every ancestor dir, crate dir name (`crates/core/…` → `core`), repo dir name |
| imported target (`to`) | raw target, brace-stripped head (`a::{b}` → `a`), separator-normalised slash form (`crate::a::b` → `a/b`), its first segment, a crate-directory alias (`repowalk_gui` → `gui`), and — when the target resolves to a known file — the same path/ancestor/crate labels |

A pair matches when **any** from-label matches `from_glob` **and** any to-label
matches `to_glob`. `no_cycles` matches each `ModuleNode.id` (crate ids are names
like `core`; file/dir ids are paths); a set with fewer than two nodes is skipped.

## Engine — entry points, pipeline, cache, errors

Public surface (`crates/core/src/rules.rs`, re-exported via `pub mod rules`):

| fn | signature | notes |
|---|---|---|
| `load` | `fn load(root: &Path) -> Rules` | preferred `root/.repowalk/rules.json`, else `root/rules.json`, else empty |
| `rules_path` | `fn rules_path(root: &Path) -> PathBuf` | `<root>/.repowalk/rules.json` |
| `check` | `fn check(index: &Index, analysis: &Analysis, rules: &Rules) -> Vec<Finding>` | runs all four checks, then sorts |
| `glob_match` | `fn glob_match(pattern: &str, text: &str) -> bool` | pure, iterative, dependency-free |

- **When it runs:** only in `repowalk report` (`crates/cli/src/commands/report.rs`
  calls `rules::check(&art.index, &art.analysis, &rules::load(path))`) and in the
  GUI health panel (`crates/gui/src/health/mod.rs`, `rules::load` + `rules::check`
  cached once per artifact). It is **not** part of `learn` — no `rules` reference
  exists in `crates/core/src/learn.rs`.
- **Cache interaction:** `check` reads only its in-memory inputs. It rebuilds the
  module graph via `modules::build(index)` on every call (pure; no file reads).
  `artifacts::load` supplies the frozen `(index, analysis)` pair from the cache or
  a single `learn` pass. `max_cyclomatic` needs `Analysis.cfgs`; the other three
  work from `Index` alone.
- **Ordering (determinism):** `check` sorts by
  `(severity, file, line, rule, message)`. Consumers must **not** re-sort. Guarded
  by `rules::tests::findings_are_deterministic_regardless_of_input_order`.
- **Malformed config:** `read()` does `.ok()` → `None` → `Rules::default()`; a
  missing/unreadable/corrupt file **never panics** but yields an empty rule set, and
  a corrupt `.repowalk/rules.json` does **not** fall back to the root sample
  (`rules::tests::corrupt_rules_file_yields_empty_rules_and_does_not_panic`).

## How CI consumes it

```bash
# markdown report + gate; nonzero exit iff any Error-severity finding
cargo run -p repowalk-cli --locked -- report . > ARCHITECTURE.md
echo $?            # 0 clean · 1 on any Error finding (message on stderr) · 2 clap usage
cargo run -p repowalk-cli --locked -- report --json . | python3 -m json.tool
```

`report --json` adds a `guardrails` object (same data, machine-readable):

```json
"guardrails":{"errors":1,"warnings":0,"findings":[{"rule":"no_cycles crates/**","severity":"Error","file":"crates/core/src/a.rs","line":0,"message":"import cycle ..."}]}
```

**Honest status:** no committed workflow runs the guardrails today — `.github/
workflows/ci.yml` runs only `fmt · clippy · build · test`, and a repo-wide grep for
`report` under `.github/workflows/` finds nothing. To gate a build, add a step
(GitHub fails a step whose command exits non-zero):

```yaml
- name: Architecture guardrails
  run: cargo run -p repowalk-cli --locked -- report . > ARCHITECTURE.md
```

## Recipe — write this repo's own `rules.json`

Fence the workspace's real crate boundary (`core` has no GUI/CLI deps; `lib.rs`
says so) and keep thresholds above the committed maximum. Every example below is
expressible in the current schema:

| intent | rule | why it matches |
|---|---|---|
| `core` must not import `gui`/`cli` | `["core","gui"]`, `["core","cli"]` | from-label `core` comes from the crate-dir label of `crates/core/…` |
| `gui` must not import `cli` | `["gui","cli"]` | same, `crates/gui/…` → `gui` |
| no import cycles under `crates/` | `"no_cycles": ["crates/**"]` | selects file/dir ids; cross-file `use` cycles trip it |
| thresholds | `"max_cyclomatic": 50`, `"max_file_loc": 5000` | repo max cyclomatic was 41; largest *indexed* file is `Cargo.lock` (~4860) |

```json
{
  "forbid_imports": [["core", "gui"], ["core", "cli"], ["gui", "cli"]],
  "max_cyclomatic": 50,
  "max_file_loc": 5000,
  "no_cycles": ["crates/**"]
}
```

Partially expressible: `["core","std::process::Command"]` fires on a
`use std::process::Command;` line (raw target label) but **not** on a
fully-qualified inline call — only `use_declaration`s become `Import` edges. Not
expressible at all: `unsafe`, `unwrap()`, raw URLs, parameter counts, feature
flags; the schema has no content/regex rule, so those stay with the grep gates.

## Recipe — add a rule kind (checklist)

- [ ] Add the field to `struct Rules` in `crates/core/src/rules.rs` with
      `#[serde(default)]` + a doc comment; update the module-level rule list and
      the JSON example in that doc comment.
- [ ] Add `fn check_<name>(...)` and call it from `check` **before** the final
      `out.sort_by`; give it a stable `rule` id string and a real `message`.
- [ ] Pick a `Severity`: architectural bans → `Error` (fails `report`); thresholds
      → `Warning`. Document the choice where the variant is pushed.
- [ ] Unit tests in `rules::tests`: one finding on a synthetic violating graph,
      none on a clean graph, and determinism under shuffled input. Extend
      `fixtures/sample` only if the rule needs a new artifact shape.
- [ ] Update `docs/howto-guardrails.md` (the four-rule table) and the guardian-flag
      table in `skills/repowalk/repowalk-metrics/SKILL.md`; add a `## Guardrails`
      JSON/exit example if the shape changed.
- [ ] Add a Keep-a-Changelog entry under the next version in `CHANGELOG.md`.
- [ ] `cargo test -p repowalk-core --lib rules` and
      `cargo test -p repowalk-cli` (the e2e rule test lives in
      `crates/cli/tests/cli_v2_e2e.rs::report_exits_nonzero_on_error_severity_rule_finding`).

## Pitfalls

- **Typo'd or unknown keys are silently ignored.** With no `deny_unknown_fields`
  and every field defaulted, `{"max_cyclematic": 5}` parses to an empty rule set —
  the rule simply never fires, exit `0`. Always confirm a rule by watching it fire.
- **Unmatched globs fail silently.** A `no_cycles` set with `< 2` selected nodes
  is skipped with no finding; a `forbid_imports` glob that matches no label never
  fires. Note `crates/**` does **not** select the crate node `core` (its id is a
  name, not a path) — use `["**"]` to cover literally every node.
- **`*` ≠ `**`.** `crates/*/src` is one segment; deep trees need `crates/**/src`.
  No `[]`, `{}`, or escaping exists.
- **Rules that depend on absent analysis.** `max_cyclomatic` reads `Analysis.cfgs`;
  a file whose language has no analyzer (R-27: only rust/python/ts/js) yields no CFG
  and can never trip it. `report` bails early with `no analysis yet` when analyzers
  produced nothing; the GUI degrades to index-only guardrails and says so.
- **Non-deterministic order breaks the contract.** Iterate `BTree*`/sorted inputs
  and never re-sort a returned `Vec<Finding>`; the inline determinism test is the
  regression guard.
- **One corrupt preferred file disables everything.** A broken
  `.repowalk/rules.json` yields `Rules::default()` instead of falling back to root
  `rules.json` — a silent no-op gate. Validate JSON before committing.
- **Thresholds count every indexed file.** `max_file_loc` includes `Cargo.lock` and
  other non-source files; set it above the real maximum, not just source LOC.
- **Do not encode network policy here.** The rules engine matches import labels,
  not code content; it cannot prove the zero-network invariant. Keep the
  `cargo tree` / `cargo deny` / grep gates as the source of truth
  (`repowalk-no-network-guardrails`), and do not add a fence that contradicts the
  pinned stack.

## Verify

```bash
cargo test -p repowalk-core --lib rules                  # engine, loading, globs
cargo test -p repowalk-cli --test cli_v2_e2e report      # nonzero-exit contract
cargo run -p repowalk-cli -- report --json . | grep guardrails
cargo test -p repowalk-core rules::tests::self_rule -- --ignored   # heavy self-rule
```

## Sources

- [Understanding JSON Schema](https://json-schema.org/understanding-json-schema/)
- [serde — container attributes (`#[serde(default)]`)](https://serde.rs/container-attrs.html)
- [serde — derive Deserialize](https://docs.rs/serde/1/serde/derive.Deserialize.html)
- [Cargo book — `cargo test`](https://doc.rust-lang.org/cargo/commands/cargo-test.html)
- [GitHub Actions — workflow syntax (step exit codes)](https://docs.github.com/en/actions/writing-workflows/workflow-syntax-for-github-actions)
- [petgraph 0.8.3 algorithms (Tarjan SCC)](https://docs.rs/petgraph/0.8.3/petgraph/algo/index.html)
