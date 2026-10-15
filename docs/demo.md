# repowalk — demo runbook

Five-minute walkthrough, rehearsed twice. Everything is offline; no keys, no
model, no network.

## Before the demo

```bash
cargo build --release          # one-time; binary at target/release/repowalk
scripts/verify-offline.sh      # green tests, offline-proof
```

## Minute 1 — the wall and the pitch

> "AI writes your code. You still have to understand it. repowalk turns an
> unfamiliar repo into a map, a reading order, and honest call graphs — pure
> static analysis, under a second, no AI."

## Minute 2 — CLI orientation

```bash
repowalk map ./some-repo       # languages, LOC, entrypoints, health line
repowalk tour ./some-repo      # numbered reading order with why-lines
repowalk explain build_index ./repowalk   # signature, docs, callers, callees
repowalk find "index json" ./repowalk     # ranked fuzzy hits
```

Point out the health line: `● 2 cycles · 1 god-files · 12% edges unresolved`
— the honesty number.

## Minute 3–4 — GUI walkthrough (dogfood on repowalk itself)

```bash
repowalk ./repowalk
```

1. **Map sidebar**: LOC, languages, entrypoints; health panel with cycles.
2. **Tour panel**: click item 1 — text view opens at the exact line.
3. **Text mode**: edit a buffer, `Ctrl+S` saves; click a caller in the
   explain card — everything follows the selection.
4. **Tab → Graph mode**: force-directed call graph; hover = signature
   preview; click = neighborhood highlight; dashed `?` edges = low
   confidence; legend says size=fan-in, color=module.
5. **Trace A→B**: pick `main`, pick `save` — shortest path lights up, or an
   honest "no call path".
6. **export ▾**: PNG of the window; Markdown tour + health report — show the
   README-ready artifact.

## Flow mode — 60 seconds

1. Open the repo, pick A and B from search or an explain card — Flow loads the
   pair and plays the call path: stage strip lights up, playhead advances,
   active segment pulses.
2. Scrub the timeline or step forward/back; flip 0.5x / 1x / 2x when someone
   says "wait, what just happened?".
3. **Open source** jumps to the exact call line in text mode — same selection,
   no context lost.
4. Wave view: pick an entrypoint and the calls fan out in BFS waves, layer by
   layer.

Honest notes:

- Unreachable pair? Flow says "no path" — same honesty number as Graph's trace.
- Pause and the stage strip stays a plain, readable list of steps — the story
  holds with animation switched off.

## v2 flows — learn → inventory → variables → CFG → refs → export report

The complete understanding suite. **Live** steps run today; **planned (card X)**
steps have an owner card and are not wired into the GUI/CLI yet. Never show a
planned step as if it works.

1. **Learn** — walk → parse → resolve → analyze → cache, written to
   `.repowalk/analysis.json` (plus the per-file stamp cache for incremental
   re-learn).

   ```bash
   # LIVE (engine): learn() runs and caches; a warm re-learn re-analyzes 0 files
   cargo test -p repowalk-core --lib learn -- --nocapture
   # LIVE: print the analyzer counts the learn stage produces
   cargo test -p repowalk-core --test analysis_golden pending_analyzers_are_reported -- --nocapture
   ```

   Planned (card DSN-4): the learn-progress view and the **Onboarding report**
   panel.

2. **Inventory** — LOC, languages, entrypoints, complexity, hotspots, god files,
   cycles.

   ```bash
   repowalk map ./some-repo     # LIVE (v1): languages, LOC, entrypoints, health line
   ```

   Planned (card DSN-3): the `health-v2` panel with sortable complexity/hotspot
   tables and graph tinting.

3. **Variables** — per-file table with **reads / writes / total uses**.

   ```bash
   # LIVE (engine): Rust 7 vars / 19 uses · Python 7 / 21 · TS 13 / 26 (fixtures)
   cargo test -p repowalk-core --test analysis_golden golden_rust_variable_counts -- --ignored
   ```

   Planned (card TEXT-2): the sortable variables panel and click-to-
   highlight-all-uses.

4. **Control flow** — a CFG per function with a cyclomatic badge.

   ```bash
   # LIVE (engine): 3 CFGs (Rust) / 3 (Python) / 6 (TS); TS `classify` = 9 nodes, cyclo 3, nesting 2
   cargo test -p repowalk-core --test analysis_golden golden_rust_cfg -- --ignored
   ```

   Planned (card GRAPH-2): Diagrams → Control flow.

5. **Refs** — every use site of a symbol.

   ```bash
   # LIVE (engine): reference lookups + resolved/name-only split, 8 green tests
   cargo test -p repowalk-core --lib refs
   ```

   Planned (card TEXT-3): the refs panel and `refs <symbol>`. The use-site data
   is collected (`Analysis.uses`) and the core lookups work; only the panel and
   the CLI command are still to come.

6. **Export the report** — Mermaid / DOT / SVG plus a self-contained Markdown
   report.

   ```bash
   # LIVE (engine): 10 green emitter tests, byte-stable
   cargo test -p repowalk-core --test export_emitters
   ```

   LIVE in the GUI today: **export ▾ → PNG** and **Tour + health → Markdown**.
   Planned (card PROD-3): the Mermaid/DOT/SVG/report menu entries and the CLI
   `report`, `graph`, `variables`, `complexity`, `refs` commands.

## Minute 5 — determinism close

> "Same repo, same result — every time. Run `repowalk map` twice: identical
> output. That's the verification layer you can check AI claims against."

## Recovery lines

- Indexing slow? The overlay shows live file counts; a warm cache reloads in
  milliseconds.
- Demo repo missing? `repowalk ./repowalk` — dogfooding is the demo.
- Screenshot API hiccups? Markdown export is the fallback (per cut order).
