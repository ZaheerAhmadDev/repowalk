---
name: repowalk-gui-hci
description: Use when designing or reviewing any repowalk GUI panel, card, or widget — token use, spacing, contrast, states, keyboard operability, information architecture, and the pre-flight checklist a GUI card must pass.
---

# repowalk — GUI HCI / design-system rules

The GUI is an **offline code-understanding tool**: the job is to reduce what a stranger has to
hold in their head. Every rule below exists because the user is reading unfamiliar code, not
transacting quickly.

## 1. `theme.rs` is the single source of truth

> "Every color, spacing, and type choice in the GUI comes from here — hardcoded hexes outside
> this module are a defect." — `crates/gui/src/theme.rs`

- Cards import `crate::theme` and use its `pub const`s / fns. No `Color32::from_rgb(..)` outside
  `theme.rs`, no literal `4.0`, `11.0`, `24.0` in a panel.
- `theme.rs` unit-tests lock the palette (`dark_palette_matches_legacy_literals`,
  `palettes_have_distinct_roles`, `light_palette_meets_wcag_aa_body_text`). A card that bypasses
  the tokens silently escapes every one of those tests.
- Need a new role? Post the exact hunk to the integrator (PMO) — do not add hexes in a panel.

## 2. Tokens: names and roles

`Palette` (per-theme struct, `DARK` default, `LIGHT` for F13) plus thin aliases used everywhere:

| Token(s) | Role |
|---|---|
| `BG_BASE` (`background`) | app background behind panels |
| `SURFACE_E0` (`surface_e0`) | toolbar/status strips, faint fills |
| `SURFACE_E1` (`surface`) | primary panel fill |
| `SURFACE_E2` (`surface_e2`) | insets, code bg, extreme bg |
| `SURFACE_E3` (`surface_e3`) | windows, open menus |
| `SURFACE_HOVER` / `SURFACE_SELECTED` | row hover / selection |
| `BORDER_SUBTLE` (`border`) / `BORDER_STRONG` (`border_strong`) | hairlines / strong separators |
| `TEXT_PRIMARY` / `TEXT_SECONDARY` / `TEXT_MUTED` / `TEXT_DISABLED` | 4-step text ramp |
| `ACCENT` / `ACCENT_PRESSED` | links, selection, focus |
| `SUCCESS` / `WARNING` / `DANGER` / `INFO` | semantics |
| `DIM` | receded/de-emphasized graph chrome |
| `MODULE_PALETTE[8]` + `module_color(&str)` | deterministic module colours |
| `accent_subtle(a)`, `dim_alpha(a)`, `with_alpha(c,a)` | alpha washes (never hand-roll rgba) |

## 3. Spacing: 4pt grid, 8pt rhythm

| Token | px | Use |
|---|---|---|
| `SPACE_1` | 4 | icon↔label, tight stack, separator breathing |
| `SPACE_2` | 8 | default item gap, row inner x |
| `SPACE_3` | 12 | toolbar item gap, panel padding |
| `SPACE_4` | 16 | between cards/blocks |
| `SPACE_6` | 24 | section breaks, card padding |
| `SPACE_8` | 32 | empty-state top offset |

Fixed metrics: `ROW_HEIGHT=24`, `TOOLBAR_HEIGHT=36`, `STATUS_BAR_HEIGHT=26`,
`SIDEBAR_DEFAULT=272`, `SIDEBAR_MIN=180`, `SIDEBAR_MAX=480`, `TREE_INDENT=14`,
`CORNER_SM=3`, `CORNER_MD=4`.

## 4. Type scale

| Token | px | Role |
|---|---|---|
| `FONT_SMALL` | 11 | meta, captions, section headers, list rows |
| `FONT_BODY` | 13 | body, metric values |
| `FONT_BUTTON` | 13 | buttons/labels |
| `FONT_HEADING` | 17 | panel/view titles |
| `FONT_CODE` | 13 | code, signatures, file:line |

Applied once via `apply_theme` to `style.text_styles[{Small,Body,Button,Heading,Monospace}]`.
Cards set size with `RichText::new(..).size(theme::FONT_*)`; never ad-hoc.

## 5. Colour, contrast, and colour-blind safety

Target: **WCAG 2.2 AA** minimum, AAA where the palette test already proves it.

| Requirement | Criterion | Where it lives |
|---|---|---|
| body/secondary text ≥ 7:1 on surface & background | 1.4.6 (AAA, chosen) | `theme.rs` light test |
| muted text, accent-as-link, semantics ≥ 4.5:1 | 1.4.3 (AA) | `theme.rs` light test |
| non-text UI/focus indicators ≥ 3:1 | 1.4.11 Non-text Contrast | border/border_strong, accent |
| colour is never the only cue | 1.4.1 Use of Color | see below |

Colour-blind-safe encoding is already the repo's habit — keep it:

- unresolved/ambiguous edges render **dashed + `?`** (`explain.rs`), not just red.
- severity shows a **tag** (`error`/`warn`) and a rule name, not only a tint (`health`).
- stale notes say **`stale`** in words (`notes.rs`).
- module identity = colour **plus** glyph (`●`/`▣`/`◆` in `tree.rs`, `onboard.rs`).
- every "selected" state also changes fill, not just hue.

## 6. Elevation

Dark mode: higher elevation is slightly **lighter** (E0 darkest strip → E1 panel → E2 inset →
E3 window). Light mode inverts but keeps five distinct layers; `palettes_have_distinct_roles`
asserts no layer collapses. Menus/windows sit at E3 with `menu_corner_radius`/`window_corner_radius`
(`CORNER_MD`); rows and insets use `CORNER_SM`. `popup_shadow = Shadow::NONE` — elevation is
expressed with fill + border, not drop shadows.

## 7. Motion (DSN-1)

| Token | ms | Use |
|---|---|---|
| `TRANSITION_MS` | 200 | hover/selection transitions |
| `PULSE_MS` | 600 | edge/connector pulse loop |
| `STEP_DWELL_MS` | 800 | flow stage dwell per step |

Helpers: `ease_in_out(t)` (clamped smoothstep), `step_alpha(progress)` (active-stage wash).
Rules: motion **never carries information alone** — the stage strip, edge labels, and selection
states stay legible at zero duration or under reduced motion (WCAG 2.3.3). Animate with
`ctx.request_repaint_after(..)` **only while playing**; idle views request no frames
(`flow/playback.rs`).

## 8. Component patterns (reuse, don't re-invent)

| Primitive | Contract |
|---|---|
| `theme::section_header(ui, "HEALTH")` | SPACE_1 → uppercase muted small strong → SPACE_1 |
| `theme::separator(ui)` | hairline with SPACE_1 breathing room |
| `theme::list_row(ui, selected, id_salt, contents)` | full-width `ROW_HEIGHT` row, hover/selection fill, left-to-right centered contents |
| `theme::paint_row_bg(ui, rect, selected, hovered)` | row background only |
| `sidebar::data_row(ui, selected, id_salt, name, Option<Meta>)` | list_row + elided name (full text on hover) + right-aligned `Meta::{muted,tinted,badge}` |
| `ui_kit::tab_strip(ui, current, &[(T, label)])` | one look for every mode/panel switcher |
| `ui_kit::not_ready(ui, title, card, detail)` | placeholder carrying the owning card + a live fact |

`data_row` elides with `…` and shows the full path on hover — apply the same "truncate, reveal
on hover" to any long identifier.

## 9. Information architecture for code exploration

Each surface answers **one** question; the user should never have to guess which view to open.

| Mode | Question | Sub-parts |
|---|---|---|
| Text | what does this code say? | file tree · editor · explain strip · variables · refs · notes |
| Graph | how is it connected? | force layout · clusters · filters · legend |
| Flow | what happens when it runs? | Trace · Waves · Feature |
| Diagrams | what is the control flow / type / crate shape? | Control flow · Types · Modules |
| Report | give me the whole picture | Markdown + Mermaid/DOT/SVG export |

Sidebar tabs: `Map` (map · tour · flow-picker · search) · `Usage` · `Health` · `Notes` · `101`
(onboard). **One selection drives every pane** (`App::select`): clicking a symbol in any view
updates tree, editor, explain, graph, and notes. Never build a second selection model.

## 10. Progressive disclosure

Show the common path first; reveal depth on demand (Nielsen: "show only what users need now").

- Toolbar exposes Text/Graph/Flow/Diagrams, Open, palette, Export, View. Everything else is a
  menu or the Ctrl+K palette.
- The command palette is the overflow valve (Hick's law): 19 actions, filter-as-you-type,
  every action shows its shortcut `hint` (`app.rs::palette_actions`).
- Panels reveal long content in sections with caps and a `+N more` line (health `MAX_ROWS`,
  onboard `MODULE_ROW_BUDGET`), not an infinite wall.
- Diagnostics are progressive: a clean run says "no findings — every rule passes"; failures
  name the rule and location.

Recognition over recall: shortcuts live in tooltips (`on_hover_text`), in the palette, and in
the `?` overlay — a user should never have to remember a binding that was never shown.

## 11. Empty / loading / error states (never lie)

| State | Pattern |
|---|---|
| no repo | centered card with path field (`text/mod.rs::open_repository`) |
| indexing | `ui.spinner()` + phase + `done/total` + live extraction counts (`progress_overlay`) |
| not built yet | `ui_kit::not_ready` with the owning card id |
| no data vs. missing data | say which: health prints "unavailable — needs the v2 analysis" instead of `0` |
| empty list | invite the first action (`notes.rs::empty_invite`) |
| write failed | roll back and surface the error (`notes.rs` never claims a note saved after a failed write) |
| degraded | `learn` falls back to index-only and says so in the status bar |

Rule: a real `0` is a result; a missing artifact is **not** `0`. Distinguish them in words.

## 12. Focus order and full keyboard operability

Keyboard map (`app.rs::handle_keyboard`, mirrored in the `?` overlay): `Tab` cycle modes ·
`Ctrl+K` palette · `/` search · `j`/`k` list cursor · `Enter` select · `g` graph · `Esc` clear ·
`Ctrl+S` save · `Ctrl+T` theme · `?` shortcuts.

Requirements for any new card:

- Every mouse action has a keyboard path (palette entry + shortcut), or a documented reason.
- Focus is visible: selection uses `SURFACE_SELECTED` fill **and** the accent stroke
  (`selection.stroke`), meeting 1.4.11 non-text contrast.
- Respect the root focus order: toolbar → sidebar → central pane → status bar.
- Avoid trapping: the palette returns focus to the app on Esc/Ctrl+K.
- egui is immediate-mode and re-exports `accesskit` (`egui::accesskit`, `egui/src/lib.rs:434`);
  painted canvases (egui_graphs, diagram canvases) are **not** OS-focusable widgets, so provide a
  widget-based equivalent — the repo does this with the CFG **picker list** beside the canvas and
  search hits beside the graph.

## 13. Readable labelling at any zoom

The Graph is the hard case (plan F3: "always-readable real symbol names", zoom-aware LOD).

- `graph/render.rs` draws labels in `FONT_SMALL` monospace; at low zoom, cluster labels; at high
  zoom, real symbol names. Labels must remain legible, never microscopic.
- Clusters group by module `module_color` + glyph; unresolved edges stay dashed `?`.
- Hover/pick a node selects the symbol so the explain strip (with full name, file:line, callers)
  can carry detail a tiny label cannot.
- Caps are **honest**: the caps banner says "showing N of M nodes — capped at K for legibility"
  rather than silently dropping nodes.

## 14. Onboarding flow (`onboard.rs`, "Repo 101")

Runs once after `learn()` completes; read-only over the cached `Index`/`Analysis` (no recompute).
Five sections, each a plain answer to a beginner question:

1. **What this is** — LOC headline, files/symbols/entrypoint count, language mix.
2. **How it is shaped** — module tree (depth-capped, row-budgeted), or top-level dirs fallback.
3. **Where the risk is** — hot files, hot functions, unused variables, import cycles (each
   row clickable to source).
4. **Where to start** — entrypoints first, then the tour's ranked reading order with `why`.
5. **Status** — learn phase + counts, or an explicit "analysis not loaded".

Every number is read verbatim; when a source is absent the panel says so and falls back rather
than inventing a value.

## 15. Pre-flight checklist (a GUI card must pass all)

- [ ] No hardcoded hex, px, or font size outside `theme.rs` — only `theme::*`.
- [ ] Interactive targets ≥ 24×24 px (WCAG 2.2 SC 2.5.8); rows use `ROW_HEIGHT=24`.
- [ ] Contrast: body/secondary ≥ 4.5:1 (aim 7:1), muted/semantics ≥ 4.5:1, borders/focus ≥ 3:1.
- [ ] Colour is never the sole cue — pair with text/glyph/dash.
- [ ] Focus ring visible; full keyboard path exists; a shortcut is discoverable (tooltip + palette + `?`).
- [ ] Empty, loading, and error states are all present and honest (no fake `0`).
- [ ] Any unbounded list is virtualized (`ScrollArea::show_rows`) or capped with `+N more`.
- [ ] Motion has a static fallback and requests no frames when idle.
- [ ] Long identifiers truncate and reveal the full value on hover.
- [ ] The card edits exactly its owned file and never `theme.rs`/chrome without a handoff.

## Sources

- <https://www.w3.org/TR/WCAG22/>
- <https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html>
- <https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html>
- <https://www.nngroup.com/articles/progressive-disclosure/>
- <https://m3.material.io/foundations/design-tokens/overview>
- <https://developer.apple.com/design/human-interface-guidelines/>
