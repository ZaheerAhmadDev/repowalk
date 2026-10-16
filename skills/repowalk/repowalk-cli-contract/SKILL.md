---
name: repowalk-cli-contract
description: Use when adding, changing, or reviewing a repowalk CLI subcommand in crates/cli — its clap 4 args, `--json` shape, exit codes, stdout/stderr split, colour handling, or deterministic ordering.
---

# repowalk CLI contract

Source of truth: `crates/cli/src/main.rs` (the `Cmd` enum) plus one renderer per
command in `crates/cli/src/commands/`. Owner card: **PROD-3**. The CLI is a *thin
renderer over `repowalk-core`* — no analysis or rendering logic belongs here.

## Subcommand tree (as actually built)

| Command | Positionals / flags | `--json` | Backing core |
|---|---|---|---|
| `map <path>` | — | no | `pipeline::build_index` + `metrics::compute` |
| `tour <path>` | — | no | `tour::tour(&idx, 20)` |
| `explain <symbol> <path>` | — | no | `pipeline::build_index` |
| `find <query> <path>` | — | no | `search::search(idx, query, 20)` |
| `flow [a] [b] [path]` | `--from <sym>`, `--depth` (default `6`), `--md` | no (`--md` for Markdown) | `flow::trace` / `flow::waves` |
| `report <path>` | `--md`, `--json` | repeated | `export::markdown_report` + `rules::check` |
| `graph <path>` | `--format mermaid\|dot\|svg`, `--scope call\|cfg\|types\|modules`, `--symbol <id>` | no | `export::{mermaid,dot,svg}` |
| `variables <file> <path>` | `--json` | yes | `Analysis::variables` |
| `complexity <path>` | `--by function\|file`, `--top N` (default `20`, `0` = all), `--json` | yes | `complexity::{hotspots,per_file}` |
| `refs <target> <path>` | `--json` | yes | `refs::references_ex` |

There is **no `artifacts` subcommand**. `crates/cli/src/commands/artifacts.rs` is a
shared loader (`artifacts::load(path) -> Result<Artifacts>`) used by every v2
query command: it reuses the cached `.repowalk/analysis.json`, else runs
`learn::learn` once, and errors honestly ("no analysis yet") when analyzers
produced nothing.

Map/tour/explain/find use the **v1 spine** `pipeline::build_index`; the v2 query
commands all go through `artifacts::load`. Do not assume language parity between
the two paths (card CORE-8 covers the v1 TS/JS gap).

## clap 4 derive patterns used

```rust
#[derive(Parser)]
#[command(name = "repowalk", version, about = "Offline codebase understanding")]
pub struct Cli { #[command(subcommand)] cmd: Cmd }

#[derive(Subcommand)]
pub enum Cmd {
    /// Orientation in one screen: metrics, tree, health   // → short help
    Map { path: PathBuf },
    Flow {
        a: Option<String>,                       // positional, optional
        #[arg(long)] from: Option<String>,
        #[arg(long, default_value_t = 6)] depth: usize,
        #[arg(long)] md: bool,                   // presence flag
        path: Option<PathBuf>,
    },
    Graph {
        #[arg(long, value_enum, default_value = "mermaid")] format: GraphFormat,
        #[arg(long, value_enum, default_value = "call")] scope: GraphScope,
        #[arg(long)] symbol: Option<String>,
        path: PathBuf,
    },
}
```

- `#[command(name, version, about)]` supplies `--help` and `--version`; `version`
  reads the Cargo package version (`0.1.0` in `crates/cli/Cargo.toml`).
- Variant doc comments (`///`) become subcommand help text — keep them one line.
- `value_enum` enums derive `clap::ValueEnum`: `GraphFormat`, `GraphScope`
  (`commands/graph.rs`), `ComplexityBy` (`commands/complexity.rs`).
- `default_value_t` for typed numeric defaults; `default_value = "..."` for enums.
- **No** clap `requires`/`conflicts`/`ArgGroup` are declared. Cross-argument rules
  are enforced at runtime and are part of the contract:
  `--scope cfg` requires `--symbol`; `--symbol` is rejected for any other scope.
- `flow` re-seats positionals by hand: with `--from`, clap fills the single
  trailing positional into `a`, so `run` maps it back to the repo path.

## `--json` output contract

`crates/cli/src/json.rs` is a **dependency-free** minimal writer (no
`serde_json`), escaping per RFC 8259: `escape`, `string`, `opt_string`, `array`.
It guarantees:

- One JSON object per invocation, printed with a **trailing `\n`**.
- Keys in **fixed construction order** (not sorted) — stable for a given build.
- **No ANSI escapes**; tests assert `!out.contains('\u{1b}')`.
- `null` for absent optionals; compact arrays (`[a,b]`), still valid JSON.

| Command | Top-level shape |
|---|---|
| `report --json` | `repo`, `overview`, `complexity`, `unused_variables`, `types`, `top_references`, `guardrails` |
| `variables --json` | `file`, `variables[]` (`id,name,kind,type,reads,writes,uses,decl_line,scope,enclosing`) |
| `complexity --json` | `by` (`function`\|`file`), `rows[]` |
| `refs --json` | `target`, `count`, `resolved[]`, `by_name[]` |

There is **no `version` field in CLI JSON.** Schema versions live in the cache
artifacts (`.repowalk/analysis.json` → `ANALYSIS_VERSION = 1`; `.repowalk/index.json`
→ `SCHEMA_VERSION = 1`); `repowalk --version` is clap's package version. `graph`
has no `--json`: it prints a Mermaid/DOT/SVG document to stdout.

## Exit codes

| Code | When | Where the message goes |
|---|---|---|
| `0` | success (including `--json`) | stdout |
| `1` | any `anyhow` error — bad arg, missing repo, unknown/ambiguous target, `report` guardrail `Severity::Error` | stderr (`Error: <msg>`) |
| `2` | clap usage/parse error — unknown subcommand/flag, missing required positional | stderr |

`main() -> anyhow::Result<()>`; a `?` error is printed by the runtime as
`Error: <msg>` and exits `1`. clap's `exit_code()` returns `USAGE_CODE = 2` for
errors that print to stderr, `0` for `--help`/`--version`.

## stdout vs stderr discipline

- Human data and JSON go to **stdout** with `print!` — never `println!` extras.
- Errors go to **stderr** via `anyhow` only; render nothing before a failure
  (load first, then render — see `report::run`, `graph::run`).
- Progress callbacks are deliberately no-ops in the CLI (`&|_| {}`); the GUI owns
  live progress.

## Colour handling (`colored` 3)

`crates/cli/src/render.rs` wraps `colored::Colorize` (`head` = bold+cyan, `dim`,
`warn` = yellow). coloured 3.1.1 decides colour in this priority:

1. `CLICOLOR_FORCE` set and not `"0"` → **colour on** (even when piped).
2. else `NO_COLOR` set (any value) → **colour off**.
3. else `CLICOLOR` (default on) **and** `stdout().is_terminal()` → colour on.

A manual override exists: `colored::control::set_override(true|false)`. Because a
piped/redirected stdout is not a tty, unit tests and `| cat` see plain text —
which is exactly why the tests can assert on plain strings, and why any `--json`
or `--md` path must avoid the render helpers entirely.

## Deterministic ordering

- Directory tree uses `BTreeMap` (`commands/map.rs::render_module_tree`).
- Sorts carry an explicit tiebreak, then a dedup:
  variables by `(decl_line, name)`; types by `(file, name)`; explain
  callers/callees `sort()` then `dedup()`; refs split resolved vs name-only.
- Caps go through `.take(n)` / `--top` **after** ordering.
- Never iterate a `HashMap` into output; that is the classic non-determinism bug.

## Error text via `anyhow`

Messages name the offending argument so a script can grep them:

- `unknown symbol: {a}` · `ambiguous symbol: {arg} ({n} matches: …)`
- `no such file in index: {file}` · `unknown symbol or variable: {arg}`
- `repo path not found or not a directory: {path}`
- `--scope cfg requires --symbol <id>` · `--symbol "x" is only valid with --scope cfg`

## Add a new subcommand — recipe

1. `crates/cli/src/main.rs`: add an enum variant with a one-line `///` doc comment,
   then add a match arm that calls your `run(...)`.
2. `crates/cli/src/commands/<name>.rs`: write
   `pub fn run(<args>) -> anyhow::Result<()>`. If it reads the analysis, start with
   `let art = artifacts::load(path)?;`. Keep the pure formatting in a separate
   `pub fn render_*` so it is unit-testable.
3. `crates/cli/src/commands/mod.rs`: add `pub mod <name>;` (the list is alphabetical).
4. JSON: build rows with `crate::json::{string, opt_string, array}` and
   `print!(...)` — no serde, trailing `\n`.
5. Add an inline `#[cfg(test)] mod tests` covering the render function: content,
   ordering, the empty case, and (for JSON) `"\u{1b}"`-free + parseable.
6. Add an end-to-end assertion in `crates/cli/tests/cli_v2_e2e.rs`
   (`run(&["<name>", …])` and `run_err(...)` for the honest-error path).
7. Verify:

```bash
cargo test -p repowalk-cli
cargo run -p repowalk-cli -- <name> --help
cargo run -p repowalk-cli -- <name> . | python3 -m json.tool   # when --json
```

## Pre-done checklist

- [ ] `--help` lists the subcommand; an unknown flag exits `2`.
- [ ] Success data on stdout, errors on stderr as `Error: …`, exit `1`.
- [ ] `--json` parses (`python3 -m json.tool`) and contains no ESC byte.
- [ ] Output is byte-identical on two runs over a warm cache.
- [ ] No new dependency added to `crates/cli/Cargo.toml`.
- [ ] `cargo test -p repowalk-cli` green and `cargo fmt`/`clippy -D warnings` clean.

## Sources

- clap 4 derive tutorial — https://docs.rs/clap/latest/clap/_derive/index.html
- Command Line Interface Guidelines — https://clig.dev/
- The Twelve-Factor App (config & CLI conventions) — https://12factor.net/
- colored 3 docs (colour control) — https://docs.rs/colored/latest/colored/
- NO_COLOR standard — https://no-color.org/
