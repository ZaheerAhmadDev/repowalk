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

## Minute 5 — determinism close

> "Same repo, same result — every time. Run `repowalk map` twice: identical
> output. That's the verification layer you can check AI claims against."

## Recovery lines

- Indexing slow? The overlay shows live file counts; a warm cache reloads in
  milliseconds.
- Demo repo missing? `repowalk ./repowalk` — dogfooding is the demo.
- Screenshot API hiccups? Markdown export is the fallback (per cut order).
