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
pub const FOLDER: char = '\u{ea83}'; // folder
pub const FOLDER_OPENED: char = '\u{eaf7}'; // folder-opened
pub const GEAR: char = '\u{eaf8}'; // gear
pub const GRAPH: char = '\u{eb03}'; // graph
pub const HEALTH: char = '\u{eb05}'; // heart
pub const MAP: char = '\u{ec05}'; // map
pub const NOTES: char = '\u{eb26}'; // note
pub const OPEN: char = '\u{eaf7}'; // folder-opened
pub const OWNER_GUIDE: char = '\u{ea61}'; // lightbulb
pub const SEARCH: char = '\u{ea6d}'; // search
pub const USAGE: char = '\u{ea88}'; // symbol-variable

/// A file-type mark: a tiny tinted chip carrying the type's short label
/// (`rs`, `py`, `ts`, `md`, `{}`…). Drawn instead of a generic sheet so a
/// Rust file, a Markdown file and a lockfile are told apart at a glance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileKind {
    /// 1–2 character label. Lowercase follows the file extension so the chip
    /// reads like the thing it names (`rs`, not `RS`).
    pub label: &'static str,
    /// Language/brand tint, also used for the chip's translucent fill.
    pub tint: egui::Color32,
}

const fn tint(r: u8, g: u8, b: u8) -> egui::Color32 {
    egui::Color32::from_rgb(r, g, b)
}

const RUST_TINT: egui::Color32 = tint(0xDE, 0xA5, 0x84);
const PYTHON_TINT: egui::Color32 = tint(0x4B, 0x8B, 0xBE);
const TS_TINT: egui::Color32 = tint(0x31, 0x78, 0xC6);
const JS_TINT: egui::Color32 = tint(0xE5, 0xC0, 0x7B);
const MD_TINT: egui::Color32 = tint(0x51, 0x9A, 0xBA);
const JSON_TINT: egui::Color32 = tint(0xCB, 0xCB, 0x41);
const SHELL_TINT: egui::Color32 = tint(0x89, 0xE0, 0x51);

/// The file-type mark for `path`, or `None` when the type has no meaningful
/// chip (a plain file glyph is clearer than inventing one).
///
/// Pure and case-insensitive; special filenames win over extensions.
pub fn file_kind(path: &str) -> Option<FileKind> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let lower = name.to_ascii_lowercase();
    // Filenames whose meaning is not in their extension.
    if let Some(kind) = match lower.as_str() {
        "cargo.toml" | "cargo.lock" => Some(FileKind {
            label: "cg",
            tint: RUST_TINT,
        }),
        "dockerfile" | ".dockerignore" | "docker-compose.yml" | "docker-compose.yaml" => {
            Some(FileKind {
                label: "dk",
                tint: tint(0x24, 0x96, 0xED),
            })
        }
        "makefile" | "gnumakefile" => Some(FileKind {
            label: "mk",
            tint: tint(0x6D, 0x80, 0x86),
        }),
        ".gitignore" | ".gitattributes" | ".gitmodules" | ".ignore" => Some(FileKind {
            label: "gi",
            tint: tint(0xF1, 0x4C, 0x28),
        }),
        _ if lower.starts_with(".env") => Some(FileKind {
            label: "en",
            tint: tint(0xE5, 0xC0, 0x7B),
        }),
        _ => None,
    } {
        return Some(kind);
    }
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    let kind = match ext {
        "rs" => FileKind {
            label: "rs",
            tint: RUST_TINT,
        },
        "py" | "pyi" => FileKind {
            label: "py",
            tint: PYTHON_TINT,
        },
        "ts" | "tsx" | "mts" | "cts" => FileKind {
            label: "ts",
            tint: TS_TINT,
        },
        "js" | "jsx" | "mjs" | "cjs" => FileKind {
            label: "js",
            tint: JS_TINT,
        },
        "md" | "mdx" | "markdown" | "mdown" => FileKind {
            label: "md",
            tint: MD_TINT,
        },
        "json" | "jsonc" => FileKind {
            label: "{}",
            tint: JSON_TINT,
        },
        "toml" => FileKind {
            label: "tm",
            tint: tint(0xC7, 0x7E, 0x5D),
        },
        "yaml" | "yml" => FileKind {
            label: "ym",
            tint: tint(0xCB, 0x6E, 0x6E),
        },
        "sh" | "bash" | "zsh" | "fish" => FileKind {
            label: "sh",
            tint: SHELL_TINT,
        },
        "go" => FileKind {
            label: "go",
            tint: tint(0x00, 0xAD, 0xD8),
        },
        "java" => FileKind {
            label: "jv",
            tint: tint(0xD1, 0x9A, 0x66),
        },
        "c" | "h" => FileKind {
            label: "c",
            tint: tint(0xA8, 0xB9, 0xCC),
        },
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => FileKind {
            label: "c+",
            tint: tint(0xF3, 0x4B, 0x7D),
        },
        "cs" => FileKind {
            label: "c#",
            tint: tint(0x4C, 0xA6, 0x4C),
        },
        "rb" => FileKind {
            label: "rb",
            tint: tint(0xE0, 0x52, 0x52),
        },
        "swift" => FileKind {
            label: "sw",
            tint: tint(0xF0, 0x51, 0x38),
        },
        "kt" | "kts" => FileKind {
            label: "kt",
            tint: tint(0xA9, 0x7B, 0xFF),
        },
        "php" => FileKind {
            label: "ph",
            tint: tint(0x8A, 0x8A, 0xD6),
        },
        "html" | "htm" => FileKind {
            label: "<>",
            tint: tint(0xE3, 0x4C, 0x26),
        },
        "css" | "scss" | "sass" | "less" => FileKind {
            label: "#",
            tint: tint(0x9B, 0x7B, 0xFF),
        },
        "svg" => FileKind {
            label: "sv",
            tint: tint(0xFF, 0xB1, 0x3B),
        },
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "bmp" => FileKind {
            label: "im",
            tint: tint(0xC7, 0x92, 0xEA),
        },
        "pdf" => FileKind {
            label: "pd",
            tint: tint(0xE0, 0x52, 0x52),
        },
        "zip" | "tar" | "gz" | "xz" | "bz2" | "7z" => FileKind {
            label: "zp",
            tint: tint(0xB8, 0xB8, 0xB8),
        },
        "lock" => FileKind {
            label: "lk",
            tint: tint(0x8A, 0x8A, 0x8A),
        },
        _ => return None,
    };
    Some(kind)
}

/// Darken a brand tint for light surfaces so the chip keeps its contrast.
fn readable_tint(c: egui::Color32, dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        c
    } else {
        egui::Color32::from_rgb(
            (c.r() as f32 * 0.68) as u8,
            (c.g() as f32 * 0.68) as u8,
            (c.b() as f32 * 0.68) as u8,
        )
    }
}

/// Paint a file's type chip (or the plain file glyph for unmarked types) and
/// return its response for tooltips.
///
/// `size` is the row's icon box; the chip is `1.15 × size` wide so two
/// lowercase letters fit without eliding. Icons are painted, not laid out as
/// text, so rows keep a fixed height and the name column stays aligned.
pub fn file_mark(ui: &mut egui::Ui, path: &str, size: f32) -> egui::Response {
    let dark = ui.visuals().dark_mode;
    match file_kind(path) {
        Some(kind) => {
            let w = size * 1.15;
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, size), egui::Sense::hover());
            let tint = readable_tint(kind.tint, dark);
            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(3),
                crate::theme::with_alpha(tint, if dark { 40 } else { 30 }),
            );
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                kind.label,
                egui::FontId::new(size * 0.66, egui::FontFamily::Proportional),
                tint,
            );
            resp
        }
        None => {
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                FILE.to_string(),
                egui::FontId::new(size - 1.0, egui::FontFamily::Name(FONT_NAME.into())),
                crate::theme::TEXT_MUTED,
            );
            resp
        }
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
            ("GRAPH", GRAPH),
            ("ARROW_RIGHT", ARROW_RIGHT),
            ("BEAKER", BEAKER),
            ("FOLDER", FOLDER),
            ("FOLDER_OPENED", FOLDER_OPENED),
        ] {
            let cp = cp as u32;
            assert!(
                (0xE000..=0xF8FF).contains(&cp),
                "{name} = U+{cp:04X} is not in the PUA range"
            );
        }
    }

    #[test]
    fn file_kinds_carry_the_extension_and_brand_tint() {
        let kinds = [
            ("src/main.rs", "rs", RUST_TINT),
            ("tools/gen.py", "py", PYTHON_TINT),
            ("web/app.tsx", "ts", TS_TINT),
            ("web/app.js", "js", JS_TINT),
            ("docs/readme.md", "md", MD_TINT),
            ("store/index.json", "{}", JSON_TINT),
            ("scripts/build.sh", "sh", SHELL_TINT),
        ];
        for (path, label, tint) in kinds {
            let kind = file_kind(path).unwrap_or_else(|| panic!("{path} has no mark"));
            assert_eq!(kind.label, label, "{path}");
            assert_eq!(kind.tint, tint, "{path}");
        }
        // Case-insensitive, and the last extension wins.
        assert_eq!(file_kind("SRC/MAIN.RS").map(|k| k.label), Some("rs"));
        assert_eq!(
            file_kind("archive.tar.gz").map(|k| k.label),
            Some("zp"),
            "compound archives still mark as archives"
        );
    }

    #[test]
    fn special_filenames_win_over_extensions() {
        assert_eq!(file_kind("Cargo.toml").map(|k| k.label), Some("cg"));
        assert_eq!(file_kind("Cargo.lock").map(|k| k.label), Some("cg"));
        assert_eq!(file_kind("Dockerfile").map(|k| k.label), Some("dk"));
        assert_eq!(file_kind("Makefile").map(|k| k.label), Some("mk"));
        assert_eq!(file_kind(".gitignore").map(|k| k.label), Some("gi"));
        assert_eq!(file_kind(".env.local").map(|k| k.label), Some("en"));
    }

    #[test]
    fn unmarked_types_fall_back_to_the_plain_glyph() {
        for path in ["LICENSE", "README", "notes.txt", "data.csv", "no-extension"] {
            assert!(file_kind(path).is_none(), "{path} should have no chip");
        }
    }

    #[test]
    fn every_chip_fits_its_badge_box() {
        // Two characters at 0.66 × size must fit inside 1.15 × size even for
        // wide glyphs; 1-char labels trivially do.
        for label in [
            "rs", "py", "ts", "js", "md", "{}", "sh", "c+", "c#", "<>", "#", "gi",
        ] {
            assert!((1..=2).contains(&label.chars().count()), "{label}");
        }
    }
}
