# v2 acceptance verification — independent battery + external dogfood

**Seat:** VER-3 (TL-VER, posting as VER-08) · **Tree:** branch `prerelease-v2` (uncommitted), verified
2026-10-16 00:0x–01:2x UTC · **Gate crates:** `repowalk-core`, `repowalk-cli`, `repowalk-gui`
(all three green). Both halves are verified: the CORE+CLI acceptance (§2–§7) and the **GUI half**
(§8, previously PENDING — the panels have landed and the whole workspace compiles).

This is the independent acceptance pass. Nothing here is a producer's own test: every claim is
re-derived from the public API and, where the CLI is the real surface, from the **persisted
artifact** the CLI reads. Each result below is a command that was actually run, with its output.

Budget/timing runs use the release binary `target/release/repowalk-cli`; correctness runs use the
`v2_acceptance` batteries. No production code was written by this seat; every fault found is a card
(§6) left to its owner.

---

## 1. Gates owned

```
$ cargo fmt -p repowalk-core -p repowalk-cli -- --check
FMT_EXIT=0
$ cargo clippy -p repowalk-core -p repowalk-cli --all-targets -- -D warnings
CLIPPY_EXIT=0
$ cargo test -p repowalk-core -p repowalk-cli
TEST_EXIT=0
```

Full pasted output: `§1.1` below (tail). The full-workspace gates (including `crates/gui/**`) are in
§8.1; §1.1 is the CORE+CLI-only run from the first pass.

### 1.1 raw gate output

```
$ cargo fmt -p repowalk-core -p repowalk-cli -- --check
FMT_EXIT=0
$ cargo clippy -p repowalk-core -p repowalk-cli --all-targets -- -D warnings
CLIPPY_EXIT=0
$ cargo test -p repowalk-core -p repowalk-cli
     Running unittests src/main.rs (repowalk_cli)            test result: ok. 37 passed; 0 failed
     Running tests/cli_e2e.rs                                 test result: ok.  6 passed; 0 failed
     Running tests/cli_v2_e2e.rs                              test result: ok.  9 passed; 0 failed
     Running tests/v2_acceptance.rs (cli)                     test result: ok.  3 passed; 0 failed
     Running unittests src/lib.rs (repowalk_core)            test result: ok. 165 passed; 0 failed; 2 ignored
     Running tests/analysis_golden.rs                         test result: ok. 10 passed; 0 failed
     Running tests/contract.rs                                test result: ok.  6 passed; 0 failed
     Running tests/export_emitters.rs                         test result: ok. 10 passed; 0 failed
     Running tests/golden.rs                                  test result: ok.  1 passed; 0 failed
     Running tests/grammar_abi.rs                             test result: ok.  4 passed; 0 failed
     Running tests/v2_acceptance.rs (core)                    test result: ok. 15 passed; 0 failed; 1 ignored
     Running tests/walk_metrics.rs                            test result: ok.  2 passed; 0 failed
   Doc-tests repowalk_core                                    test result: ok.  1 passed; 0 failed
TEST_EXIT=0
```

(fmt exit 0; clippy `Finished` exit 0; every suite 0 failed. The core lib's 2 ignored tests and the
core battery's 1 ignored test are the repo's pre-existing ignored items / this seat's timing harness,
not new failures.)

---

## 2. Battery files (written by this seat, not by any producer)

| file | tests | default result |
|---|---|---|
| `crates/core/tests/v2_acceptance.rs` | 16 | 15 passed, 1 ignored (a measurement harness), 0 failed |
| `crates/cli/tests/v2_acceptance.rs` | 3 | 3 passed, 0 failed |

Run:
```
$ cargo test -p repowalk-core --test v2_acceptance
$ cargo test -p repowalk-cli  --test v2_acceptance
```

The one ignored core test (`measured_learn_budgets`) is a **timing harness**, not a parked defect —
it needs `VER3_BIG_REPO` + `--release`, and it is what produced §4. The two `#[ignore]`-parked
defect repros that lived here during the pass (CORE-11) are now **un-parked**: the fix landed, so
they run in the default gate (§3.6).

---

## 3. Acceptance results

### 3.1 CORE-10 — producer-revision cache invalidation

Claim: a producer (pipeline) change invalidates the whole per-file analysis cache; a warm re-learn
is a hit; a corrupt sidecar is a miss, never a panic; and the module graph (CORE-9) is repopulated.

All four core tests green by default:
`core10_stale_revision_forces_full_reanalysis_and_repopulates_modules`,
`core10_absent_revision_forces_full_reanalysis`,
`core10_warm_hit_then_one_file_edit_reanalyzes_exactly_one`,
`core10_garbage_sidecar_body_is_a_miss_not_a_panic`.

Holds: rewriting `analysis-stamps.json` `revision:1` (or deleting the field) re-analyzes **every**
analyzable file and repopulates `Analysis.modules`; the next learn analyzes **0**; a one-file edit
analyzes exactly **1** and the persisted artifact carries the edited file's data. `LEARN_REVISION`
is now `2`.

### 3.2 R-14 — input staleness

Claim: a same-second rewrite, a size change with a preserved mtime, and an edit of a file that is
not the newest must each be a cache **miss** — at both the index layer (`store`) and the learn layer
(the artifact the CLI serves).

Six regression tests green by default:
learn-level `r14_same_second_rewrite_is_a_miss`, `r14_size_change_with_preserved_mtime_is_a_miss`,
`r14_edit_of_non_newest_file_is_a_miss`; index-level `r14_index_cache_same_second_rewrite_is_a_miss`,
`r14_index_cache_size_change_with_preserved_mtime_is_a_miss`,
`r14_index_cache_non_newest_edit_is_a_miss`.

**Provenance:** this seat found the *learn-level* residual (learn.rs stamped `(mtime_seconds, lines)`,
so both strict cases were warm HITs). Handed to CORE-03 (card CORE-8); fixed to `(mtime_nanos,
size)` (`STAMP_VERSION 1→2`, `store` gains `cache-header.json` per file). Both tests moved from
failing to passing under this pass.

### 3.3 R-27 — language dispatch

Claim: a TS/JS-only tree extracts non-zero symbols, and an unsupported language is reported, never
silently 0.

Core (green):
`r27_ts_js_only_tree_reports_nonzero_symbols` — `pipeline::build_index_with_report` on a `.ts`+`.js`
dir extracts `greet`, `Greeter`, `helper`, `unsupported` empty.
`r27_unsupported_language_is_reported_explicitly` — a `.rb` file is still indexed and named in
`IndexReport.unsupported` with summary `"1 files not analyzed (rb)"`.

CLI surface (probe with the release binary). A TS/JS-only dir now shows a call edge where the
pre-fix code showed `no call edges found`:
```
$ repowalk map rw-tsonly
repowalk map — rw-tsonly
Languages: javascript (1 files), typescript (1 files)
LOC: 12
Entrypoints (0): none
Module tree
  ./
    main.ts
    util.js
Top files
  main.ts · 9 lines
  util.js · 3 lines
Hottest symbols (fan-in)
  main.ts:1 · 1 callers
● 0 cycles · 0 god-files · 0% edges unresolved
```
The TS/JS half is genuinely fixed. **Residual (carded PROD-5):** the unsupported *count* is computed
but no surface consumes it — `grep -rn 'unsupported_summary\|\.unsupported\b\|build_index_with_report'
crates/cli/src crates/gui/src` → **0 hits**. A mixed dir lists `rb (1 files)` but never says it was not
analyzed:
```
$ repowalk map rw-rbprobe
Languages: javascript (1 files), rb (1 files), rust (1 files), typescript (1 files)
...
$ repowalk report rw-rbprobe    # module map row: | `script.rb` | File | 3 | 0 |  (0 symbols, no note)
```

### 3.4 Emitter fidelity — one shared model

`emitters_agree_on_call_types_and_module_counts` (core) and
`report_json_aggregates_match_the_emitters` (CLI) green: Mermaid, DOT and the Markdown report agree
node-for-node and edge-for-edge for the call, module and type diagrams; `Analysis.modules.len()`
equals the module-node count; the SVG node count equals Mermaid/DOT; the report embeds the diagram
bodies verbatim (no second renderer); and the CLI's `report --json` aggregates equal the CLI's own
emitters' node counts. `report_is_byte_identical_on_a_warm_artifact` green (deterministic round-trip).

### 3.5 Frozen contract — version present, unknown version refused

`frozen_contract_version_present_and_unknown_version_refused` green: the written `analysis.json`
carries `version == ANALYSIS_VERSION`; a `version+1` file makes `analysis::load` return `None`; an
unknown `index.json` version makes `store::cache_valid` false.

### 3.6 CORE-11 (found this pass, fixed during it) — unloadable artifact must force re-analysis

**Found by this seat**: an unloadable `analysis.json` (missing / corrupt / `version:99`) with a
current stamp sidecar was **not** forced to re-analyze — the first read errored and rewrote a
version-current but **empty** artifact, and the next read served `variables:0/types:0` with exit 0
(a misread that falsifies R-21). Carded **CORE-11**, fixed by CORE-07 (guard at `learn.rs:186–194`).
End-to-end proof on three fresh copies of `fixtures/sample` (poison → read1 → read2), release binary:
```
### absent  read1_exit=0  read2: "variables":5,"types":1
### garbage read1_exit=0  read2: "variables":5,"types":1
### version read1_exit=0  read2: "variables":5,"types":1
```
Both regression tests (`anti_unloadable_artifact_must_force_full_reanalysis` in core,
`unknown_version_artifact_is_not_misread_as_empty` in CLI) now run **in the default gate** and pass.

---

## 4. Budgets — claimed vs measured

Targets are from `board/docs/risks-v2.md` §Budgets. Timings are `/usr/bin/time -f '%e s / %M KB'`
on the release binary.

| budget (target) | corpus | measured | verdict |
|---|---|---|---|
| Cold learn, ~10k-LOC external repo (**< 5 s**) | external `@ai-sdk` TS/JS, 143 files / **15,278 LOC** | `map` **0.41 s** / 10,280 KB; `learn` (report) **0.33 s** / 13,864 KB; warm **0.01 s** | **MET** (~12×) |
| Incremental re-learn after 1 file touched (**1** re-parsed) | internal copy, 582 files / 77,742 LOC | functional: exactly **1** re-analyzed (test + `reanalyzed()==1`); wall-clock `report` 6.88 s, then warm 1.84 s | **MET** (functional) |
| Memory, 100k-LOC repo (**< 1.5 GB** RSS) | internal copy 77,742 LOC | peak **83,988 KB** (82 MB) cold learn; 26,100 KB map | **MET** (≈14× headroom at 100k) |
| Idle UI CPU / repaints (**~0 %**, **0** repaints) | GUI, headless xvfb, `fixtures/sample` | idle windows **0.000 %** CPU, **0** voluntary ctxt switches; driver-path windows spun (§8.7) | **MET** (code-level + measured) |

Commands:
```
$ /usr/bin/time -f 'COLD_MAP wall=%e rss_kb=%M' repowalk map rw-cold
COLD_MAP wall=5.60 rss_kb=26100            # internal 77,742 LOC
$ /usr/bin/time -f 'COLD_REPORT(learn) wall=%e rss_kb=%M' repowalk report --json rw-learn
COLD_REPORT(learn) wall=10.41 rss_kb=83988
$ /usr/bin/time -f 'EXT_MAP wall=%e rss_kb=%M' repowalk map ext-ai-sdk
EXT_MAP(10.2k LOC) wall=0.41 rss_kb=10280
$ /usr/bin/time -f 'EXT_COLD_LEARN wall=%e rss_kb=%M' repowalk report --json ext-ai-sdk
EXT_COLD_LEARN wall=0.33 rss_kb=13864
```
(All corpora are copies under `/tmp/opencode` — the CLI is never pointed at a source tree it might write
into. `ext-ai-sdk` is a copy of a real third-party TS/JS package; it reports 15,278 CLI-lines / 143 files.)

**Honest note (correcting an earlier read of this pass):** the cold-learn budget is written for a
**~10k-LOC external repo** and its literal hook is `repowalk map` — on that basis it is **MET**
(0.41 s). The earlier "10.27 s vs < 5 s" reading compared the wrong corpus (this 78k-LOC repo) and
the wrong command (`learn`, not `map`). Both are still reported here so the number is not hidden:
this repo's own cold `map` is **5.60 s** / cold `learn` **10.41 s** at **77,742 LOC** (≈0.72 s and
1.34 s per 10k LOC — inside budget, but a larger repo will feel it). No perf card is raised: the
target is met on the corpus the target names. Scale footage: a one-file edit then `report` on the
78k-LOC copy costs 6.88 s, dominated by the 15 MB `analysis.json` round-trip.

---

## 5. External dogfood

Beyond the budget corpus, a vendored third-party crate (`vendor/egui_code_editor-0.4.2`, not
`fixtures/`) was learned cold — no crash, no zero-count section:
```
$ repowalk report --json dogfood-vendor
"overview":{"files":26,"lines":3900,"symbols":163,"call_edges":719,"modules":33,"types":10,"variables":310,...}
```
The external `@ai-sdk` TS/JS tree (F9 end-to-end) is non-zero on every section:
```
"overview":{"files":143,"lines":15278,"symbols":353,"call_edges":198,"modules":181,"types":248,"variables":673,...}
```

---

## 6. Defects carded by this pass (no production code written by VER-3)

| card | severity | summary | owner |
|---|---|---|---|
| **CORE-11** | high | unloadable `analysis.json` rewritten as an empty v1 artifact then served silently (R-21) | CORE-07 — **fixed**, regression tests un-parked (§3.6) |
| **PROD-4** | high | query commands serve a **mixed-revision** report after an edit: index fresh, analysis stale, exit 0 | PROD-05 — **fixed**, re-confirmed (§8.9) |
| **PROD-5** | low | `IndexReport.unsupported` / `unsupported_summary()` computed but surfaced nowhere | PROD-05 (+ GUI) — **fixed**, re-confirmed (§8.9) |
| **PROD-7** | low | GUI Control-flow + Variables no-analysis states still render the `ui_kit::not_ready` scaffold ("this view lands with card …") | GRAPH-02 / TEXT-02 — found by the GUI pass (§8.3) |

**PROD-4 repro** (release binary; `m.rs` = `pub fn f() { let alpha = 1; }`):
```
1. report --json (cold) -> lines 3, symbols 1, variables 1
2. edit m.rs: add `let gamma = 2;` + a new `pub fn g() -> i32 { 7 }`   (8 lines, 2 symbols)
3a. report --json       -> "lines":8,"symbols":2   (FRESH)  "variables":1 (STALE, want 2)
3b. complexity --by function -> "1 functions"      (STALE, want 2: f and g)
3c. variables m.rs           -> "1 declared"       (only alpha; gamma missing)   exit 0 throughout
```
Root cause: `artifacts::load` trusts a *loadable* (version-current) `analysis.json` and never
re-learns, while it *does* rebuild the index fresh; there is no `learn` subcommand and no path that
refreshes a present artifact, so only deleting `.repowalk` drops the stale data.

---

## 7. Honest-state cases (no path / no analysis / unknown symbol / wrong scope)

The CLI's documented non-zero paths were re-run on a copy of `fixtures/sample` and hold:
```
$ repowalk map /tmp/definitely-not-a-repo-xyz                 exit=1  Error: ... No such file or directory (os error 2)
$ repowalk graph --scope cfg <dir>                            exit=1  Error: --scope cfg requires --symbol <id>
$ repowalk graph --scope cfg --symbol definitely_missing <dir> exit=1  Error: unknown symbol: definitely_missing
$ repowalk graph --scope types --symbol greet <dir>           exit=1  Error: --symbol "greet" is only valid with --scope cfg (this scope is Types)
$ repowalk variables nope.rs <dir>                            exit=1  Error: no such file in index: nope.rs
$ repowalk refs definitely_missing <dir>                      exit=1  Error: unknown symbol or variable: definitely_missing
```
A `.repowalk/rules.json` of `{"no_cycles":["*.rs"]}` on the sample's `a.rs↔b.rs` cycle makes `report`
print the Guardrails table and exit non-zero:
```
## Guardrails
| Error | no_cycles *.rs | `a.rs` | 0 | import cycle in module set '*.rs': a.rs -> b.rs |
exit=1   stderr: Error: 1 Error-severity architecture finding(s) in rules.json
```
The CORE-11 fix (§3.6) strengthens the "no analysis yet" state: a refused artifact now triggers a full
rebuild rather than a one-shot error followed by silent zeros. Animation is never the only carrier —
the GUI half is verified in §8.

---

## 8. GUI half — VERIFIED (previously PENDING)

The GUI panels have landed and the whole workspace compiles. This half was verified against the
`prerelease-v2` tree (the GUI panels are committed; the same content is now committed through
`126052d`, which also carries the PROD-4/5/6 fixes). Every claim below is a command+output or an
exact `file:line` citation.

### 8.1 Gates (whole workspace, GUI included)

```
$ cargo fmt --all -- --check
FMT_EXIT=0
$ cargo clippy --workspace --all-targets -- -D warnings
Finished `dev` profile … CLIPPY_EXIT=0
$ cargo test --workspace --locked
     repowalk-cli  unit 43 · cli_e2e 6 · cli_v2_e2e 9 · prod4_revision 2 · prod5_unsupported 2 · v2_acceptance 3   all ok
     repowalk-core lib 167 (2 ignored) · analysis_golden 10 · contract 6 · export_emitters 10 · golden 1 ·
                   grammar_abi 4 · v2_acceptance 15 (1 ignored) · walk_metrics 2 · doctest 1                     all ok
     repowalk-gui  unit 78                                                                                        ok
=> 359 passed · 3 ignored · 0 failed    TEST_EXIT=0
```

GUI-only: `cargo test -p repowalk-gui` → **78 passed / 0 failed**;
`cargo clippy -p repowalk-gui --all-targets -- -D warnings` → 0;
`cargo fmt -p repowalk-gui -- --check` → 0.

### 8.2 Learn progress + live counts (claim 1) — MET

- `IndexProgress` carries the live counters + degradation note: `app.rs:49-67`
  (`phase`/`done`/`total`/`variables`/`cfgs`/`types`/`uses`/`warning`).
- The worker copies every `LearnProgress` tick into them: `app.rs:181-194`
  (`s.variables = p.variables; s.cfgs = p.cfgs;`), fed by `core::learn::LearnProgress` (`learn.rs:82-90`).
- **Read-only fallback**: when `learn()` fails, the worker reroutes through
  `pipeline::build_index(&repo, &|_: usize| {})` and sets
  `warning = "analysis unavailable ({e:#}) — showing the index only"` (`app.rs:196-211`);
  `p.warning = warning` (`app.rs:215`) surfaces it as the status line via `poll_progress`
  (`app.rs:254-256`).
- **Overlay counts**: `"{phase} · {done}/{total} files…"` (`app.rs:882-888`) and the running
  `"{variables} variables · {cfgs} control-flow graphs so far"` (`app.rs:891-900`). Completion line
  `"learned {files} files · {V} variables · {C} cfgs · {T} types"` (`app.rs:242-249`).

### 8.3 Every panel reachable and core-populated (claim 2) — MET (one scaffold caveat, PROD-7)

Sidebar tabs (`sidebar/mod.rs:16-23` enum, `:25-35` labels, `:45-66` dispatch):

| tab | panel file | core data it reads |
|---|---|---|
| **Map** | `sidebar/map.rs:19` | `metrics::compute` (+ `tour::tour`, `search::search`) |
| **Usage → Variables** | `usage/variables.rs` | `Analysis::variables`; `complexity::unused_variables` |
| **Usage → References** | `usage/refs.rs:115-131` | `refs::references_for_symbol` / `references_ex`; `RefSet::by_file` (`core/refs.rs:196,75`) |
| **Health** | `health/mod.rs:142-158` | `metrics::compute` + `compute_v2`, `rules::load`+`rules::check`, `modules::build` (`:591`) |
| **Notes** | `notes.rs:81-124,215-223` | `<repo>/.repowalk/notes.json` (`:64-66`) |
| **101 (Onboard)** | `onboard.rs` | `tour::tour` (`:399`), `complexity::{per_file,hotspots,unused_variables}` (`:538-550`), `metrics::compute`, `modules::cycles` (`:560`) |

Modes (`app.rs` `Mode` `:17-24`; `Tab` cycles `:373-380`; toolbar segments `:571-582`; central-pane
dispatch `:938-963`):

| mode | panel file | core data it reads |
|---|---|---|
| **Text** | `text/mod.rs` | `Index` + editable buffer |
| **Graph** | `graph/mod.rs` | `Index` edges (`graph/render.rs:670` `metrics::god_files`) |
| **Flow** | `flow/mod.rs:166,175,186` | `flow::trace`, `flow::waves`, `feature::slice` |
| **Diagrams → Control flow** | `diagrams/control.rs` | `analysis.cfgs` (`FunctionCfg`) |
| **Diagrams → Types** | `diagrams/types.rs` | `analysis.types` / `type_edges` |
| **Diagrams → Modules** | `diagrams/modules.rs:340` | `analysis.modules` / `module_edges` + `modules::cycles` |

Placeholder grep:
```
$ grep -rn "in progress\|TODO\|stub\|placeholder\|unimplemented" crates/gui/src
ui_kit.rs:8,13                        # doc comment on not_ready
notes.rs:377,382,707                  # `fn placeholder` — honest empty-state text
usage/refs.rs:203,213,229,231,426,427
```
No `todo!()`/`unimplemented!()`/`"in progress"` anywhere. The `placeholder` hits are a *function
named* `placeholder` rendering honest empty-state sentences, not fake panels. **Caveat:**
`ui_kit::not_ready` (`ui_kit.rs:14-37`) still renders `"this view lands with card {card}"` and is
still reached by the no-analysis state of two panels — `diagrams/control.rs:48-55` and
`usage/variables.rs:64-72` — so those two empty states wrongly imply the view has not shipped.
Carded **PROD-7** (low). The panels themselves render real `Analysis` data on every learned repo
(the `analysis.is_none()` guard is the only remaining `not_ready` caller).

### 8.4 Export ▾ writes real files (claim 3) — MET

Every `Export ▾` entry (`app.rs:596-654`) and the core function it calls:

| menu entry | method | emitter |
|---|---|---|
| Window → PNG | `export_png` `app.rs:471-480` | `crate::export::png` (viewport screenshot; no CLI analog) |
| Tour + health → Markdown | `export_markdown` `app.rs:483-499` | `crate::export::markdown` on `metrics::compute`/`tour::tour` (GUI-only file) |
| Architecture report → Markdown | `export_architecture_report` `app.rs:522-532` | **`core::export::markdown_report`** (`app.rs:526`) |
| Diagrams → Mermaid (.mmd) × {Call,Types,Modules} | `export_diagram(…,"mermaid","mmd")` | **`core::export::mermaid`** (`app.rs:541`) |
| Diagrams → DOT (.dot) × {Call,Types,Modules} | `export_diagram(…,"dot","dot")` | **`core::export::dot`** (`app.rs:542`) |
| Diagrams → SVG (.svg) × {Call,Types,Modules} | `export_diagram(…,"svg","svg")` | **`core::export::svg`** (`app.rs:543`) |

The four v2 emitters are the **same functions the CLI calls**: `commands/report.rs:32`
(`export::markdown_report`) and `commands/graph.rs:75-77` (`export::mermaid`/`dot`/`svg`). Because the
emitters are pure (`core/export/mod.rs:33-50`) and both surfaces call them directly, GUI and CLI
cannot drift. Exercised end-to-end through the CLI on `fixtures/sample` (identical code path):

```
$ repowalk report /tmp/opencode/ver08-sample        # -> 176-line Markdown, exit 0
# repowalk architecture report — ver08-sample
| files | 6 | lines of code | 75 | symbols | 14 | call edges | 10 | modules | 7 | types | 1 | variables | 5 |
| language mix | rust (5 files), python (1 file) |
$ repowalk graph --format mermaid --scope call  …sample   # -> .mmd, exit 0, "flowchart LR" + 26 node/edge lines
$ repowalk graph --format dot     --scope call  …sample   # -> .dot,  exit 0, "digraph repowalk {" + edges
$ repowalk graph --format svg     --scope call  …sample   # -> .svg,  exit 0, "<svg … viewBox=…>"
$ types/{mermaid,dot,svg}: 97 / 258 / 1670 bytes;  modules/{mermaid,dot,svg}: 746 / 1431 / 4149 bytes  (all exit 0)
```

### 8.5 Theme — F13 (claim 4) — MET

- Toggle: `Ctrl+T` (`app.rs:350`, `:366-372`) and View ▾ (`app.rs:658-670`) both flip `ThemeMode` and
  call `theme::apply_theme`. Palette + mode: `theme.rs:17-133` (`DARK` `:63-84`, `LIGHT` `:88-109`,
  `ThemeMode` `:113-123`); `apply_theme` installs it (`theme.rs:258-365`).
- **Light palette exists** and is WCAG-asserted: `light_palette_meets_wcag_aa_body_text`
  (`theme.rs:567`) computes WCAG 2.1 contrast from the sRGB bytes (`linear`/`luminance`/`contrast`
  `theme.rs:452-471`) and asserts primary/secondary ≥7:1 (AAA), muted/accent/semantics ≥4.5:1 (AA),
  white-on-`accent_pressed` ≥4.5:1. `palettes_have_distinct_roles` (`theme.rs:528`) asserts the five
  elevation layers and every semantic role stay distinct in both palettes.
- **"Dark is unchanged"** — `dark_palette_matches_legacy_literals` (`theme.rs:480`) compares each
  `DARK` field to its literal pre-F13 RGB byte values *and* re-asserts that every public alias
  (`BG_BASE`, `TEXT_PRIMARY`, `ACCENT`, …) still resolves to `DARK`. It is a value-equality check on
  the palette + aliases, not a rendered-pixel diff; `apply_theme(Dark)` reads `DARK`
  (`theme.rs:258-259`), so dark output is byte-identical by construction.
  `theme_mode_defaults_to_dark_and_selects_palettes` (`theme.rs:617`) pins the default. All four
  tests are in the 78-test GUI run.

### 8.6 Notes persistence (claim 5) — MET

- Path `<repo>/.repowalk/notes.json` (`notes.rs:64-66`).
- **Atomic write** tmp+rename (`notes.rs:116-124`); the round-trip test asserts no `.tmp` remains
  (`notes.rs:1139`).
- **Tolerant load that reports why** (`notes.rs:81-113`): missing → empty store, *no* warning;
  unreadable → empty + "could not read …"; corrupt → empty + "… is not valid notes JSON … — starting
  empty"; version mismatch → loaded best-effort + a version warning. Never panics.
- **Stale targets kept** and marked: `is_stale` (`notes.rs:215-223`) checks file *and* symbol; the
  panel renders a `stale` badge (`:435-439`) and an "N stale" count (`:557-576`).
- Save failures roll the in-memory store back so the list never shows an unsaved note
  (`:466-482`, `:501-529`).
- Display-free tests (all in the 78-test run): `roundtrip_save_load_in_temp_dir`,
  `json_codec_escapes_and_tolerates_junk`, `corrupt_file_loads_empty_without_panicking`,
  `stale_target_detection_marks_but_keeps`, `add_edit_delete_logic`, `filter_and_group_rows`.

### 8.7 Idle-UI budget (claim 6) — measured under xvfb + code-level argument

`xvfb-run` is present (`/usr/bin/xvfb-run`). Headless run against `fixtures/sample`, sampling the
process's own `/proc/<pid>/stat` ticks over consecutive windows:

```
$ xvfb-run -a <probe> target/release/repowalk-gui /tmp/opencode/ver08-idle     # warm 15 s, 5 s windows
window 1: cpu_pct= 0.000 vol_ctxt_delta=0
window 2: cpu_pct= 0.000 vol_ctxt_delta=0
window 3: cpu_pct= 0.000 vol_ctxt_delta=0
window 4: cpu_pct= 1.596 vol_ctxt_delta=39
window 5: cpu_pct= 1.197 vol_ctxt_delta=96
```

In the idle windows every thread was blocked in `ep_poll`/`futex_do_wait` (`/proc/<pid>/task/*/wchan`),
i.e. genuinely waiting for events — **0 % CPU, 0 voluntary context switches**. On *other* headless
runs the same binary spun at 5–13 % CPU (once exiting early); forcing software GL
(`WGPU_BACKEND=gl`) spun at **45–133 %** and died. Those busy windows are the headless Mesa/Vulkan
path (the log shows `MESA-INTEL: warning: Haswell Vulkan support is incomplete`), not an app loop —
the process is event-driven whenever the driver is not spinning.

Code-level argument (exact; the headless driver is too flaky to trust alone): the **only** repaint
requests in the crate are
- `app.rs:192` and `app.rs:225` — `ctx.request_repaint()` from the indexing worker's progress tick and
  its completion; after the index/analysis land, nothing schedules another;
- `flow/playback.rs:104-106` — `ctx.request_repaint_after(PULSE_MS/4)` **only** `if self.playing && steps > 0`.

There is no other `request_repaint*`, timer, or animation. `Playback.playing` defaults `false`
(`playback.rs:66`) and the app opens in `Mode::Text` (`app.rs:123`), where the flow transport is not
rendered at all. A static, unfocused, already-learned window thus requests **0** repaints and no
timer wakes it → budget **MET** (code-level, corroborated by the 0 %/0-switch idle windows above).

Command for a human with a real display (static, unfocused window, 60 s):
```
cargo build --release -p repowalk-gui
./target/release/repowalk-gui fixtures/sample &     # leave it unfocused for 60 s
pidstat -p <pid> 5                                  # %CPU should read ~0.00
```

### 8.8 Budget correction (claim 7) — carried through

§4 already presents the corrected reading: **cold learn 0.33 s on the 15,278-LOC external TS/JS repo
= MET** (`repowalk map` 0.41 s), with the **78k-LOC** numbers (`map` 5.60 s / `learn` 10.41 s)
clearly labelled as a *different, larger corpus* to which the target's own corpus/command does not
apply. This section does not re-state them; §4 is authoritative. No perf card is raised.

### 8.9 PROD-4 / PROD-5 re-confirmed on the current tree

Both were fixed in parallel during this pass; by the end they read `done` and the fixes are committed
(`b77a007` PROD-4, `80c2755` PROD-5), so both repros are now **GREEN**:

- **PROD-4 was RED, now fixed.** `artifacts::load` no longer short-circuits on a loadable artifact —
  it always calls `learn::learn` (`commands/artifacts.rs:40-58`), so index and analysis are one
  revision. Re-ran the exact repro on the current tree (release binary):
  ```
  cold:  m.rs = "pub fn f() { let alpha = 1; }"        -> lines=1 symbols=1 variables=1
  edit:  add `let gamma = 2;` + `pub fn g() -> i32 { 7 }`
  after: report --json                                 -> lines=2 symbols=2 variables=2   (was 1 — FIXED)
         complexity --by function                      -> "2 functions"                    (was 1 — FIXED)
         variables m.rs                                -> "2 declared" (alpha, gamma)      (was 1 — FIXED)
  ```
- **PROD-5**: the unsupported-language count is surfaced by `map`/`report`
  (`commands/map.rs` `render_map`/`health_line` now take the `unsupported` summary), with
  `crates/cli/tests/prod5_unsupported.rs` (2 tests) in the default gate.

### 8.10 GUI residuals

- **PROD-7** (new, low): the `ui_kit::not_ready` scaffold text is still reachable in the Control-flow
  and Variables no-analysis states (§8.3).
- Not re-measured this pass: GUI peak RSS on a ~100k-LOC repo (the R-19 per-frame-clone audit).
  Code shows no per-frame deep `Analysis`/`Index` clone — panels borrow or `Arc`-clone
  (`variables.rs:63-64`); observed RSS on the sample was ~112 MB. No number is faked.

---

## 9. Verdict

- **CORE-10, R-14, R-27 (dispatch), emitter fidelity, frozen contract:** verified, green in the default gate (§3).
- **CORE-11:** found by this pass, **fixed and now gated** (§3.6).
- **GUI half (§8):** verified. Workspace gates green incl. GUI (fmt 0, clippy 0, **359 passed / 3 ignored / 0 failed**; GUI 78/0). Learn progress + live counts, every sidebar tab and all four modes core-populated, Export ▾ on the shared core emitters (exercised via the CLI on `fixtures/sample`), F13 light theme WCAG-checked with dark byte-identical, notes atomic/tolerant/stale-keeping — all hold. Idle-UI budget **MET** (0 %/0-switch idle windows under xvfb + code-level argument, §8.7).
- **Budgets:** cold-learn and memory **MET** on the corpora the targets name; incremental **MET** functionally; idle UI now **MET** (§4, §8.7). §4 carries the corrected 78k-LOC labelling.
- **External dogfood:** two third-party trees learned cold with non-zero output, no crash (§5).
- **Carded by this pass:** PROD-4 + PROD-5 (both **fixed** in parallel, re-confirmed §8.9) and **PROD-7** (new, low — the `not_ready` scaffold in two no-analysis empty states).
