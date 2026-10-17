//! Vendored Microsoft VS Code codicons — the icon font the real VS Code uses.
//! OWNER: integrator.
//!
//! `assets/codicon.ttf` is vendored from `@vscode/codicons` 0.0.36 (CC BY 4.0 —
//! full legal text alongside in `assets/codicons-LICENSE.txt`). The font is
//! `include_bytes!`-ed at build time: no runtime file IO, and this stays within
//! the project's no-network contract — the shipped binary makes no HTTP use,
//! exactly like the pinned tree-sitter grammars. Codepoints below are
//! generated from the same release `codicon.csv` mapping, so a name and its
//! glyph can never drift apart.
//!
//! Typical use: `ui.label(icons::glyph(icons::FILES, 15.0))` or a painter text
//! call with `FontId::new(size, FontFamily::Name(icons::FONT_NAME.into()))`.
use eframe::egui;

/// The egui font family this font registers under.
pub const FONT_NAME: &str = "codicons";

/// Install the font family onto the context (called from `theme::apply_theme`
/// next to the rest of the typography). `Context::add_font` is idempotent by
/// font name, so theme toggling never stacks copies.
pub fn register(ctx: &egui::Context) {
    ctx.add_font(egui::epaint::text::FontInsert::new(
        FONT_NAME,
        egui::FontData::from_static(include_bytes!("../assets/codicon.ttf")),
        vec![egui::epaint::text::InsertFontFamily {
            family: egui::FontFamily::Name(FONT_NAME.into()),
            priority: egui::epaint::text::FontPriority::Highest,
        }],
    ));
}

/// Render one icon glyph at `size`. Icons are font glyphs like any other, so
/// they tint with text styling and stay crisp at every scale.
pub fn glyph(icon: char, size: f32) -> egui::RichText {
    egui::RichText::new(icon.to_string()).font(egui::FontId::new(
        size,
        egui::FontFamily::Name(FONT_NAME.into()),
    ))
}

pub const ARROW_RIGHT: char = '\u{ea9c}'; // arrow-right
pub const BEAKER: char = '\u{ea79}'; // beaker
pub const CIRCLE_FILLED: char = '\u{ea71}'; // circle-filled
pub const CLOSE: char = '\u{ea76}'; // close
pub const EXPORT_ICON: char = '\u{ebac}'; // export
pub const FILE: char = '\u{ea7b}'; // file
pub const FILES: char = '\u{eaf0}'; // files
pub const FILE_CODE: char = '\u{eae9}'; // file-code
pub const GEAR: char = '\u{eaf8}'; // gear
pub const GRAPH: char = '\u{eb03}'; // graph
pub const HEALTH: char = '\u{eb05}'; // heart
pub const JSON: char = '\u{eb0f}'; // json
pub const MAP: char = '\u{ec05}'; // map
pub const MARKDOWN: char = '\u{eb1d}'; // markdown
pub const NOTES: char = '\u{eb26}'; // note
pub const OPEN: char = '\u{eaf7}'; // folder-opened
pub const OWNER_GUIDE: char = '\u{ea61}'; // lightbulb
pub const SEARCH: char = '\u{ea6d}'; // search
pub const USAGE: char = '\u{ea88}'; // symbol-variable

/// File-type glyph for a tab / list row: the "correct representation" of what
/// the file is (code, markdown, json…). Falls back on the plain file icon.
pub fn file_icon(path: &str) -> char {
    let name = path.rsplit('/').next().unwrap_or(path);
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "md" | "mdown" | "markdown" => MARKDOWN,
        "json" | "jsonc" | "yaml" | "yml" | "toml" => JSON,
        "rs" | "py" | "ts" | "js" | "mjs" | "cjs" | "go" | "java" | "c" | "h" | "cpp" | "hpp"
        | "rb" | "sh" | "swift" | "kt" | "cs" => FILE_CODE,
        _ => FILE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_covers_every_icon_the_shell_uses() {
        // Every icon must be a Private-Use-Area codepoint (0xE000..0xF8FF) —
        // i.e. generated correctly, not a stray ASCII letter.
        for (name, cp) in [
            ("FILES", FILES),
            ("SEARCH", SEARCH),
            ("MAP", MAP),
            ("USAGE", USAGE),
            ("HEALTH", HEALTH),
            ("NOTES", NOTES),
            ("OWNER_GUIDE", OWNER_GUIDE),
            ("GEAR", GEAR),
            ("CLOSE", CLOSE),
            ("CIRCLE_FILLED", CIRCLE_FILLED),
            ("OPEN", OPEN),
            ("EXPORT", EXPORT_ICON),
            ("FILE", FILE),
            ("FILE_CODE", FILE_CODE),
            ("MARKDOWN", MARKDOWN),
            ("JSON", JSON),
            ("GRAPH", GRAPH),
            ("ARROW_RIGHT", ARROW_RIGHT),
            ("BEAKER", BEAKER),
        ] {
            let cp = cp as u32;
            assert!(
                (0xE000..=0xF8FF).contains(&cp),
                "{name} = U+{cp:04X} is not in the PUA range"
            );
        }
    }

    #[test]
    fn file_icons_pick_the_extension_representation() {
        assert_eq!(file_icon("crates/gui/src/app.rs"), FILE_CODE);
        assert_eq!(file_icon("docs/notes.md"), MARKDOWN);
        assert_eq!(file_icon("store/index.json"), JSON);
        assert_eq!(file_icon("LICENSE"), FILE);
        assert_eq!(file_icon("script.py"), FILE_CODE);
    }
}
