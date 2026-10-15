# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog and this project adheres to Semantic Versioning.

## [Unreleased]
### Added
- Central design-token system `crates/gui/src/theme.rs` (HCI-based: 4pt spacing
  grid, 5-layer dark elevation, WCAG-checked text colors, semantic
  success/warning/danger tokens, deterministic module palette).
- Command palette (`Ctrl+K`) with fuzzy command filtering and shortcut hints.
- Open-repository popup dialog; keyboard-shortcut overlay (`?`) as a
  two-column grid; spinner on the indexing progress overlay.
- Dedicated toolbar (segmented Text/Graph control with active accent border,
  Open…, Export menu, right-aligned metrics) and bottom status bar with
  dismissable error line.

### Changed
- Full GUI restyle on the `gui-redesign` branch: sidebar panels (map, tour,
  search) use uppercase section headers, full-width hover/selection list rows,
  right-aligned metadata, and semantic health colors; text mode file tree uses
  consistent rows with entrypoint markers and line-count badges; editor header
  shows path + dirty state + save; explain card reworked as an inspector
  panel; graph nodes are `surface`-filled with module-colored borders,
  selected/hovered accent halos, dimmed neighborhoods, muted edges, and a
  proper legend strip; graph trace A→B shown as a persistent banner.
- Syntax highlighting colors come from the `egui_code_editor` built-in
  `ColorTheme::AYU_DARK` instead of a handwritten palette.

### Removed
- Dead code: unused `App::module_color` wrapper, unused theme constants,
  `let _ = …` placeholder statements, always-visible repo path row in text
  mode (moved to toolbar/popup).

## [0.1.0] - 2026-10-13
### Added
- Initial public release of repowalk (GUI + CLI + core static analysis).
- Site with HTML/CSS/JS tutorial and GitHub Pages workflow.
- Comprehensive community health files and templates.
- Extensive CI/CD workflows (Rust, linting, security, site validation).
- Agent team orchestration docs (lead, workers, independent verifier).
- CHANGELOG added.

### Changed
- Relicensed to Apache-2.0 only.
- Workspace license set to Apache-2.0.

### Removed
- Dual MIT/Apache licensing files removed.
