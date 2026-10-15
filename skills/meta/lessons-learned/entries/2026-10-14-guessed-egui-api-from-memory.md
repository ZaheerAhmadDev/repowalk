# Guess library APIs from memory instead of reading the pinned source

**Date:** 2026-10-14 · **Task:** GUI-redesign · **Phase:** implement

**What happened:** While writing the new `crates/gui/src/theme.rs` and the app
shell, I wrote egui calls from memory of older egui versions and guessed wrong
seven times in one file: `ctx.set_style` (real: `set_global_style` /
`set_style_of`), `Frame::none()` (real: `Frame::new()`), `Panel::min_width`/
`max_width` (real: `min_size`/`max_size`), `egui::Rounding` (renamed to
`CornerRadius`, `u8` not `f32`), `Margin::symmetric(f32,f32)` (now `i8`),
`ctx.style()` (real: `style_of(Theme::Dark)`), `.hint_text()` chained on the
`Response` of `ui.text_edit_singleline` (hint_text lives on the `TextEdit`
builder, shown with `.show(ui)`), `egui::max2` (removed; `Pos2::max` method),
and `id_salt: impl Hash` (trait is `AsIdSalt = Hash + Debug`). Every one was a
compile error I then had to grep the registry to fix.

**Cost:** One full extra compile-fix cycle (~10 min) on the critical path of a
parallel redesign; three subagents had to be briefed with a "gotchas" list to
avoid repeating the same errors.

**Root cause:** Wrote against a *remembered* API surface instead of the pinned
one. The crate version is in `Cargo.lock`; its exact source is already on disk
in `~/.cargo/registry/src/.../egui-0.36.2/` — I checked it only *after* the
errors, which inverts the cost.

**Rule:** Before writing any call into a third-party crate, grep the pinned
version's source for the exact signature (`grep -rn "fn <name>" ~/.cargo/registry/src/*/<crate>-<ver>/src/`). The registry copy is ground truth; memory is not. When
briefing subagents on a pinned-dependency stack, include the verified
signatures so they don't re-derive them.

**Skill?** Promote to `skills/repowalk/repowalk-stack/SKILL.md` if a second
egui-API lapse occurs; for now the grep-the-registry rule is logged here.
