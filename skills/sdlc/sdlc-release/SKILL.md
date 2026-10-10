---
name: sdlc-release
description: Release and deployment phase of the software development cycle. Use when shipping, tagging, publishing, building artifacts, or preparing a demo of the software.
---

# SDLC — Release

**Goal:** a reproducible artifact that works on a clean machine, with a way back.

## Pre-release checklist

- [ ] All tests + lints green (`cargo test && cargo clippy -D warnings`, or project equivalent)
- [ ] Feature freeze respected — nothing merged that wasn't on the frozen list
- [ ] Version bumped; changelog entry written (user-facing: added/changed/fixed)
- [ ] Built from a clean tree: `git status` clean, build reproducible
- [ ] Smoke test on the built artifact (run *it*, not the source tree)
- [ ] Offline/no-secret check if the product promises it (no network, no keys)

## Procedure

1. Freeze scope — no new features after this point, fixes only.
2. Build the artifact (e.g. `cargo build --release`); never demo from an unbuilt tree.
3. Run the checklist above; each item is a command with output, not a feeling.
4. Tag + write changelog; keep the previous tag as the rollback target.
5. Smoke test: the demo script, end to end, once, on the artifact.
6. If the demo can fail: record a fallback recording first (insurance before polish).

## Rollback

Previous tag + previous artifact. If you can't state the rollback in one sentence, you haven't released — you've just moved.

## Hand-off

Shipped → `sdlc-maintenance` (triage, retro, debt log).
