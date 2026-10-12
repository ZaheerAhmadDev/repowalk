# Vendored crates

## egui_code_editor-0.4.2

Unmodified upstream 0.4.2 from crates.io except one change:

- `src/lib.rs` — `CursorState` gains `#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]`.

Why: the crate's published `serde` feature adds the `serde` dependency but never
uses it, so `CursorState` has no serde impls. `egui_graphs` 0.32.0 forces
`egui/persistence`, under which egui's `SerializableAny` bound requires
`Serialize + Deserialize` — making `CodeEditor::show` fail to compile against
eframe/egui 0.36. This patch supplies exactly the missing derive.

Wired up via `[patch.crates-io]` in the workspace root `Cargo.toml`.
Upstream: https://github.com/p4ymak/egui_code_editor (MIT).
