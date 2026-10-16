---
name: repowalk-gui-egui-036
description: Use when writing or reviewing GUI code in crates/gui — panel layout, Frame/Margin/CornerRadius, theme application, virtualized lists, TextEdit, ids, drag-and-drop, repaint, eframe wiring, wgpu backend, persistence, and the graphics adapters (egui_graphs / egui_code_editor). Every signature here was read from the pinned crate source, not memory.
---

# repowalk — GUI: verified egui / eframe 0.36.2 patterns

Ground truth is the **pinned source on disk**, never memory. Before writing any egui call:

```bash
ls -d ~/.cargo/registry/src/index.crates.io-*/egui-0.36.2 \
      ~/.cargo/registry/src/index.crates.io-*/eframe-0.36.2 \
      ~/.cargo/registry/src/index.crates.io-*/egui_graphs-0.32.0 \
      ~/.cargo/registry/src/index.crates.io-*/egui-wgpu-0.36.2
grep -rn "fn <name>" ~/.cargo/registry/src/*/egui-0.36.2/src/
```

A guessed API costs a compile cycle on the critical path; a grep costs seconds.

## Versions and where each was verified

`egui`/`eframe`/`egui-wgpu` **0.36.2** and `egui_graphs` **0.32.0** under `~/.cargo/registry/src/index.crates.io-*/`,
plus `egui_code_editor` **0.4.2** in `vendor/` (patched) and `petgraph` 0.8; line refs below point into them.

## Panels: layout and ordering

`SidePanel` and `TopBottomPanel` **do not exist** in 0.36 — zero matches in `egui-0.36.2/src`.
They are replaced by one `Panel` type (`src/containers/panel.rs`):

```rust
Panel::left(impl Into<Id>) -> Self          // ::right ::top ::bottom (panel.rs:249-274)
    .resizable(bool)                        // 322
    .default_size(f32)                      // 369
    .min_size(f32).max_size(f32)            // NOT min_width/max_width (380,387)
    .show(ui: &mut Ui, |ui| ...)            // Panels take &mut Ui (422)
    .show_collapsible(ui, &mut open: bool, |ui| ...)   // 451
```

`CentralPanel` is `#[derive(Default)]`; `CentralPanel::default().show(ui, ..)` also takes `&mut
Ui` (1212). `Window::show(ctx, ..)` (window.rs:543) and `Area::show(ctx, ..)` (area.rs:406) keep
the `Context` form. Order panels outside-in before the central pane: `app.rs` does toolbar Frame
→ `Panel::left("sidebar")` → `CentralPanel` per mode → status bar; `text/mod.rs` nests
`Panel::left("text-files")` + `Panel::bottom("text-explain")` then the editor fills the rest.
Unique panel ids (`"sidebar"`) also drive resize-state persistence.

## Frame / Margin / CornerRadius

```rust
Frame::new()                          // == Frame::NONE; "no colors/margins/border" (frame.rs:173)
    .fill(Color32).stroke(impl Into<Stroke>)          // 258,267
    .inner_margin(impl Into<Margin>).outer_margin(..) // 248,296 (accepts f32 or Margin)
    .corner_radius(impl Into<CornerRadius>).shadow(Shadow)  // 277,303
    .show(ui, |ui| ...)               // 404
Frame::popup(style)                   // 214 — used by the learn overlay
Margin::symmetric(x: i8, y: i8)       // epaint margin.rs:44 — i8, NOT f32
Margin::same(i8)                      // margin.rs:33
CornerRadius::same(u8)                // epaint corner_radius.rs:59 — u8
CornerRadius::ZERO                    // corner_radius.rs:50
```

`Frame::none()` is gone — use `Frame::new()`. `egui::Rounding` no longer exists; the field is
`corner_radius` (not `rounding`). Repo idiom: `theme::SPACE_3 as i8` as the x margin.

## Theme: style, not `set_style`

```rust
ctx.set_theme(impl Into<ThemePreference>)             // context.rs:2170
let mut style = (*ctx.style_of(Theme::Dark)).clone(); // 2221 — returns Arc<Style>
ctx.set_global_style(style);                          // 2200 ; or set_style_of(theme, style)
```

`ctx.set_style` and `ctx.style()` **do not exist**. `theme.rs::apply_theme` is the single place
tokens become a `Style` (text styles, `spacing.item_spacing`/`button_padding`/`menu_margin`/
`indent`/`scroll.*`, and the whole `visuals` block). Cards never call `set_global_style`; they
call `theme::apply_theme(ctx, mode)` only on a theme toggle (`app.rs` Ctrl+T / View menu).
Verified fields the repo sets: `override_text_color`, `weak_text_color`, `warn_fg_color`,
`error_fg_color`, `panel_fill`, `window_fill`, `extreme_bg_color`, `faint_bg_color`,
`code_bg_color`, `window_corner_radius`, `menu_corner_radius`, `popup_shadow`,
`window_highlight_topmost`, `selection.bg_fill/stroke`, `hyperlink_color`, `slider_trailing_fill`,
`striped`; widget states `widgets.{noninteractive,inactive,hovered,active,open}.{bg_fill,
weak_bg_fill,fg_stroke,bg_stroke,corner_radius}`; `Spacing.scroll.{floating,bar_width,
bar_inner_margin,bar_outer_margin,foreground_color}` (style.rs:495+).

## Virtualized lists: `ScrollArea::show_rows`

```rust
ScrollArea::vertical()/both()/horizontal()   // scroll_area.rs:369-381
    .id_salt(impl AsIdSalt).auto_shrink([false, false])   // 482,605
    .show_rows(ui, row_height_sans_spacing: f32, total_rows: usize,
               |ui, range: Range<usize>| { for i in range { ... } })   // 983
```

Only `range` rows are laid out and painted, so a 10k-file list costs ~viewport-height work per
frame instead of 10k widget allocations. `row_height_sans_spacing` must equal the real row height
(`theme::ROW_HEIGHT = 24.0`); egui adds `item_spacing.y` itself. Used by the CFG picker
(`diagrams/control.rs`); other sidebar lists use plain `.show(..)` over **pre-capped** data
(health `MAX_ROWS=30`, onboard `MODULE_ROW_BUDGET=40`). Use `show_rows` for unbounded lists.

## TextEdit, ids, drag-and-drop

```rust
TextEdit::singleline(&mut dyn TextBuffer)   // builder.rs:113 ; ::multiline (123)
    .hint_text(impl IntoAtoms<'static>)     // 209 — on the BUILDER
    .id(Id).id_salt(impl AsIdSalt)          // 168,181
    .desired_width(f32).desired_rows(usize) // 321,330
    .show(ui) -> TextEditOutput             // 452 ; output.response carries the Response
pub trait AsIdSalt: core::hash::Hash + core::fmt::Debug {}   // id_salt.rs:7
impl<T: Hash + Debug> AsIdSalt for T {}                      // 9 ; Id::new(impl AsIdSalt) 32
resp.dnd_set_drag_payload::<P: Any + Send + Sync>(payload);  // response.rs:499
resp.dnd_hover_payload::<P>() -> Option<Arc<P>>;             // 516
resp.dnd_release_payload::<P>() -> Option<Arc<P>>;           // 532
Sense::drag() / click_and_drag()                             // sense.rs:68,81
```

Wrong (the recorded lesson): `.hint_text()` on the `Response` of `ui.text_edit_singleline`.
Right: build, set `hint_text`, `.show(ui)`; read `resp.response.lost_focus()` +
`ui.input(|i| i.key_pressed(Key::Enter))` (`text/mod.rs`). `id_salt: impl Hash` is **not** enough
— the bound is `Hash + Debug` (hence `theme::list_row`/`sidebar::data_row` take
`impl std::hash::Hash + std::fmt::Debug`); tuples like `("cfg-row", id)` satisfy both. Keep ids
unique per widget per parent (debug builds panic on collisions). The repo has **no** internal DnD
(no `dnd_*`/`Sense::drag`/`dragged` in `crates/gui/src`); the one drop surface is file-drop onto
the editor via `egui_code_editor::push_dropped_files(ui, &mut text) -> bool` (vendor lib.rs:508).

## Folder picking — honest state

`rfd` is **not a dependency**: zero hits in any `Cargo.toml`/`Cargo.lock`. The GUI opens a repo
by **typing a path**: `App::repo_input` + `TextEdit` + `app.start_indexing(PathBuf)`
(`app.rs::open_popup`, `text/mod.rs::open_repository`), reachable via `Open…` / Ctrl+K.
`repowalk-stack` lists `rfd` as an intended CLI picker; if a card adds it, pin it per
`repowalk-stack`, **then grep the pinned source**. Do not write `rfd::FileDialog::…` from memory.

## Repaint and spinners

`ctx.request_repaint()` (context.rs:1821, callable from any thread) and
`ctx.request_repaint_after(Duration)` (1872) schedule frames; `ui.spinner()` (ui.rs:1965) draws one.
The index worker holds a cloned `Context` (via `OnceLock`, `app.rs::APP_CTX`) and repaints per
progress tick, so the spinner animates without busy-looping. Flow playback repaints (`PULSE_MS/4`)
**only while playing** and nothing when idle (`flow/playback.rs`).

## eframe wiring and the wgpu backend

```rust
pub trait App { fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame); ... }  // epi.rs:182
pub fn run_native(app_name: &str, NativeOptions, AppCreator<'_>) -> Result     // lib.rs:288
```

The trait method is **`ui`**, not `update` — `App::update` does not exist in 0.36, and the root
`Ui` has no margin/background. `App::logic(ctx, frame)` (epi.rs:167) is the hidden-window hook.
`AppCreator` returns `Result<Box<dyn App>, _>`; `main.rs` does `Box::new(|cc| Ok(Box::new(
app::App::new(cc))))`. Read `cc.egui_ctx`/`cc.storage` from `CreationContext` (epi.rs:58,64).
`NativeOptions` fields used: `viewport: ViewportBuilder` (`.with_inner_size([1280.0, 800.0])`),
`wgpu_options`, `renderer`, `persistence_path` (epi.rs:303,375,328,383).

```rust
let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();  // egui-wgpu setup.rs:240
if cfg!(windows) { setup.instance_descriptor.backends.set(egui_wgpu::wgpu::Backends::VULKAN, false); }
egui_wgpu::WgpuConfiguration { wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(setup), ..Default::default() }
```

Some Windows Vulkan ICDs segfault during adapter enumeration, so Vulkan is dropped on Windows;
Linux keeps stock behavior. `WGPU_BACKEND=…` still forces a backend — do not re-enable Vulkan.

## Persistence, storage, and view state

`App::save(&mut self, storage: &mut dyn Storage)` runs only with the `persistence` feature;
`auto_save_interval` defaults to 30s (epi.rs:212,234,259). Typed values:

```rust
eframe::get_value::<T: DeserializeOwned>(storage, key) -> Option<T>   // epi.rs:960
eframe::set_value::<T: Serialize>(storage, key, &value)               // 975
eframe::APP_KEY = "app"                                              // 984
```

The repo keeps **transient** view state in egui memory instead of growing `App`:
`ctx.data_mut(|d| d.get_temp::<T>(Id::new(..)))` / `insert_temp` (health cache, notes state,
graph filter/cluster state). `get_persisted`/`insert_persisted` survive restart; `get_temp`/
`insert_temp` do not.

## Keyboard shortcuts and command palette

Global keys are read once per frame with `ctx.input(|i| ...)` (`app.rs::handle_keyboard`):
`i.key_pressed(Key::Tab/Slash/Escape/Questionmark/Enter/J/K/G/S/T)` + `i.modifiers.{none,ctrl,
shift}`. Ctrl+K opens the palette and swallows other keys until closed; `/` focuses search; `j`/
`k` walk the list; `Esc` clears. Shortcuts are shown, not remembered: `resp.on_hover_text(..)`
tooltips plus a `?` overlay built with `Grid::new("shortcut-grid").num_columns(2).spacing([..]).
show(ui, ..)` (grid.rs:341,366,412). Each palette action is a `PaletteAction { label, hint, kind }`.

## Graphics adapters

**egui_graphs 0.32.0** (`graph_view.rs`):

```rust
GraphView::<S, L>::new().with_id(Option<String>)               // 339,389
  .with_interactions(&SettingsInteraction::new().with_node_clicking_enabled(true))  // settings.rs:66
  .with_navigations(&SettingsNavigation::new().with_zoom_and_pan_enabled(true)
                                              .with_fit_to_screen_enabled(true))     // 161,153
  .show(ui, &mut graph) -> GraphViewResponse<Ix>               // 345
// response.changes: Vec<GraphChange>; match GraphChange::NodeClicked { node }
let graph: WalkGraph = Graph::from(&petgraph_stable_graph);    // graph.rs:52
```

Repo call (`graph/render.rs`): `FruchtermanReingoldState` + `LayoutForceDirected<
FruchtermanReingold>`, id `"repowalk-graph"`. `SettingsInteraction` defaults
`node_clicking_enabled=false` — set it explicitly. The graph draws in screen space, not as DOM
widgets; the keyboard duty that follows is in the HCI skill.

**Vendored `egui_code_editor` 0.4.2** (`vendor/egui_code_editor-0.4.2/src/lib.rs`):

```rust
CodeEditor::default().id_source(impl Into<String>)   // 164
    .with_rows(usize).with_fontsize(f32)             // 174,188
    .with_theme(ColorTheme)                          // 181 (ColorTheme::AYU_DARK, themes/ayu.rs:41)
    .with_numlines(bool).with_wrap(bool)             // 212,239
    .show(ui, &mut text, &Syntax) -> (TextEditOutput, Vec<Token>)   // 382
Syntax::rust() / Syntax::python()                    // syntax/rust.rs:7 ; syntax/python.rs:7
```

**Why vendored:** `egui_graphs` 0.32.0 forces `egui/persistence`, under which egui's
`SerializableAny` bound requires `Serialize + Deserialize`. `egui_code_editor`'s published
`serde` feature adds the dep but never uses it, so `CursorState` lacked the derives and
`CodeEditor::show` failed to compile. The vendored copy adds
`#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]` to `CursorState`
(lib.rs:109), wired via `[patch.crates-io]` in the root `Cargo.toml`. Do not "fix" it by
disabling `serde` or editing the lockfile.

## Gotchas (real compile errors / pinned-source checks)

| Wrote from memory | Real in pinned 0.36.2 | Where |
|---|---|---|
| `ctx.set_style(s)` | `ctx.set_global_style(s)` / `set_style_of(theme, s)` | context.rs:2200,2250 |
| `Frame::none()` | `Frame::new()` (== `Frame::NONE`) | frame.rs:173 |
| `Panel::min_width/max_width` | `Panel::min_size/max_size` | panel.rs:380,387 |
| `egui::Rounding` | `egui::CornerRadius`, `same(u8)` | corner_radius.rs:59 |
| `Margin::symmetric(f32,f32)` | `Margin::symmetric(i8,i8)` | margin.rs:44 |
| `ctx.style()` | `ctx.style_of(Theme::Dark)` -> `Arc<Style>` | context.rs:2221 |
| `text_edit_singleline(..).hint_text(..)` | `TextEdit::singleline(..).hint_text(..).show(ui)` | builder.rs:209,452 |
| `egui::max2(a,b)` | `Pos2::max` method | — |
| `id_salt: impl Hash` | `AsIdSalt = Hash + Debug` | id_salt.rs:7 |
| `egui::SidePanel` / `TopBottomPanel` | `egui::Panel::left/right/top/bottom` (removed) | panel.rs:249-274 |
| `App::update(ctx, frame)` | `App::ui(ui, frame)` | eframe epi.rs:182 |
| `CentralPanel::show(ctx, ..)` | `CentralPanel::show(ui, ..)` | panel.rs:1212 |
| `rfd::FileDialog::…` assumed present | `rfd` not a dependency — grep after adding | Cargo.toml |

## Pre-flight before claiming a GUI card done

- Every new call grepped against the pinned source (paste the signature into the PR note).
- Panels ordered outside-in; each panel has a unique id.
- Unbounded lists use `ScrollArea::show_rows`; rows use `theme::ROW_HEIGHT`.
- No raw hex outside `theme.rs` — only `theme::*` tokens (see the HCI skill).
- The real gate is `cargo clippy -D warnings` (not run here — one writer holds the target lock).

## Sources

- <https://docs.rs/egui/0.36.2/egui/>
- <https://docs.rs/eframe/0.36.2/eframe/>
- <https://docs.rs/egui-wgpu/0.36.2/egui_wgpu/>
- <https://docs.rs/egui_graphs/0.32.0/egui_graphs/>
- <https://github.com/emilk/egui/tree/master/examples>
- <https://github.com/p4ymak/egui_code_editor>
