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

The v2 query surface (each command learns on first use, then reads the cache):

```bash
repowalk report ./repowalk                 # architecture report + guardrails
repowalk refs build_index ./repowalk       # every use site, resolved vs name-only
repowalk complexity --by file ./repowalk   # per-file complexity roll-up
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

The complete understanding suite. The **CLI** steps below run today (F14); the GUI
panels marked **in progress (card X)** have an owner card and are still being
filled. Never show an in-progress panel as if it works.

Setup (once):

```bash
cargo build -p repowalk-cli
cp -r fixtures/sample /tmp/e2e        # the repo every command below uses
```

1. **Learn** — walk → parse → resolve → analyze → cache, written to
   `.repowalk/analysis.json` plus the per-file incremental stamp cache. Every v2
   query command below learns on first use (or reuses the cache).

   ```bash
   repowalk report /tmp/e2e            # learns + caches, prints the report
   ```

   In progress (card DSN-4): the learn-progress view and the **Onboarding
   report** panel.

2. **Inventory** — LOC, languages, entrypoints, complexity, hotspots, god files,
   cycles.

   ```bash
   repowalk map /tmp/e2e                     # languages, LOC, entrypoints, health line
   repowalk complexity --by function /tmp/e2e   # hotspot ranking
   repowalk complexity --by file /tmp/e2e       # per-file roll-up
   ```

   In progress (card DSN-3): the `health-v2` panel with sortable
   complexity/hotspot tables and graph tinting.

3. **Variables** — per-file table with **reads / writes / total uses**.

   ```bash
   repowalk variables model.rs /tmp/e2e
   ```

   In progress (card TEXT-2): the sortable variables panel and click-to-
   highlight-all-uses.

4. **Control flow** — a CFG per function with a cyclomatic badge.

   ```bash
   repowalk graph --scope cfg --symbol main /tmp/e2e
   ```

   In progress (card GRAPH-2): Diagrams → Control flow.

5. **Refs** — every use site of a symbol, split resolved vs name-only.

   ```bash
   repowalk refs greet /tmp/e2e
   ```

   In progress (card TEXT-3): the refs panel and Ctrl+click navigation.

6. **Export the report** — Mermaid / DOT / SVG plus a self-contained Markdown
   report.

   ```bash
   repowalk report /tmp/e2e > ARCHITECTURE.md
   repowalk graph --format svg --scope modules /tmp/e2e > modules.svg
   ```

   Working in the GUI today: **export ▾ → PNG** and **Tour + health →
   Markdown**. The GUI menu entries for Mermaid/DOT/SVG/report are in progress;
   the CLI exposes them now. A nonzero `report` exit means an `Error`-severity
   rule fired — see `docs/howto-guardrails.md`.

## Minute 5 — determinism close

> "Same repo, same result — every time. Run `repowalk map` twice: identical
> output. That's the verification layer you can check AI claims against."

## Recovery lines

- Indexing slow? The overlay shows live file counts; a warm cache reloads in
  milliseconds.
- Demo repo missing? `repowalk ./repowalk` — dogfooding is the demo.
- Screenshot API hiccups? Markdown export is the fallback (per cut order).
