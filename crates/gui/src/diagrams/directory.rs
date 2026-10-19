//! Diagrams → Directory (B5-05): the real directory tree drawn as an
//! interactive painter canvas. OWNER: integrator.
//!
//! The data is `Index.files` grouped by [`crate::sidebar::explorer::build`]
//! (the same deterministic builder the Explorer panel uses), so every node
//! shows the actual directory/file name. Interaction — drag a node anywhere,
//! drag empty canvas to pan, wheel to zoom, double-click empty canvas or the
//! Reset button to restore the canonical layout — is the shared
//! [`super::interact`] layer, applied strictly on top of a pure `layout` fn.
//! Clicking a file node opens that file through the one selection path.

use eframe::egui;

use super::interact;
use crate::app::Selection;
use crate::sidebar::explorer::{self, DirNode};
use crate::theme;

/// Node cap: huge repos would produce an unreadable wall of boxes; the cap
/// message states the omitted count honestly (the CLI `graph --scope tree`
/// emitter never caps — that is the escape hatch for the full tree).
pub(crate) const MAX_NODES: usize = 360;

const COL_W: f32 = 188.0;
const COL_GAP: f32 = 30.0;
const ROW_GAP: f32 = 8.0;
const H_DIR: f32 = 26.0;
const H_FILE: f32 = 22.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Dir,
    File,
}

/// One placed node of the canonical layout.
pub(crate) struct Box {
    /// Interact id: `d` + directory path, or `f` + file path.
    pub id: String,
    pub label: String,
    pub kind: Kind,
    pub rect: egui::Rect,
    pub entrypoint: bool,
}

/// The canonical layout: boxes plus parent→child containment edges as indices
/// into `boxes`.
pub(crate) struct Layout {
    pub boxes: Vec<Box>,
    pub edges: Vec<(usize, usize)>,
}

/// How many nodes the full tree would produce (root + dirs + files).
pub(crate) fn total_nodes(tree: &DirNode) -> usize {
    1 + count_tree(tree)
}

fn count_tree(tree: &DirNode) -> usize {
    tree.files.len() + tree.dirs.values().map(|d| 1 + count_tree(d)).sum::<usize>()
}

/// Column x position of a depth column.
fn col_x(depth: usize) -> f32 {
    depth as f32 * (COL_W + COL_GAP)
}

/// Place the whole tree in depth-columns (deterministic, pure). A directory
/// header sits in column = its depth; its files and subdirectory headers sit
/// one column right, files first — the same order the Explorer panel prints.
/// Returns the layout and the node count omitted by `cap` (0 below the cap).
pub(crate) fn layout(root: &str, tree: &DirNode, cap: usize) -> (Layout, usize) {
    let total = total_nodes(tree);
    if cap == 0 {
        return (
            Layout {
                boxes: Vec::new(),
                edges: Vec::new(),
            },
            total,
        );
    }

    let mut out = Layout {
        boxes: Vec::new(),
        edges: Vec::new(),
    };
    let mut cursor: Vec<f32> = vec![0.0]; // y cursor per column

    // The repo-root box.
    out.boxes.push(Box {
        id: "d".to_string(),
        label: root.to_string(),
        kind: Kind::Dir,
        rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(COL_W, H_DIR)),
        entrypoint: false,
    });
    cursor[0] = H_DIR + ROW_GAP;

    walk(tree, &mut out, &mut cursor, "", 0, cap);

    let omitted = total - out.boxes.len();
    (out, omitted)
}

/// Place one directory's children; `dir_idx` is the parent box index,
/// `dir_path` its full repo-relative path ("" at the root). Returns `true`
/// once the cap is reached and nothing more should be placed (deterministic:
/// DFS order decides what fits).
fn walk(
    dir: &DirNode,
    out: &mut Layout,
    cursor: &mut Vec<f32>,
    dir_path: &str,
    depth: usize,
    cap: usize,
) -> bool {
    if out.boxes.len() >= cap {
        return true;
    }
    let dir_idx = out
        .boxes
        .iter()
        .position(|b| b.id == format!("d{dir_path}"))
        .unwrap_or(0);

    // Children live in the next column.
    let col = depth + 1;
    while cursor.len() <= col {
        cursor.push(0.0);
    }
    let x = col_x(col);

    // Files first (Explorer order).
    for f in &dir.files {
        if out.boxes.len() >= cap {
            return true;
        }
        let y = cursor[col];
        cursor[col] = y + H_FILE + ROW_GAP;
        let marker = if f.entrypoint { " ★" } else { "" };
        out.boxes.push(Box {
            id: format!("f{}", f.path),
            label: format!("{}{marker}", f.name),
            kind: Kind::File,
            rect: egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(COL_W, H_FILE)),
            entrypoint: f.entrypoint,
        });
        out.edges.push((dir_idx, out.boxes.len() - 1));
    }

    // Then the subdirectories, depth-first.
    let mut stop = false;
    for (name, sub) in &dir.dirs {
        if stop || out.boxes.len() >= cap {
            stop = true;
            continue;
        }
        let y = cursor[col];
        cursor[col] = y + H_DIR + ROW_GAP;
        let full = if dir_path.is_empty() {
            name.clone()
        } else {
            format!("{dir_path}/{name}")
        };
        out.boxes.push(Box {
            id: format!("d{full}"),
            label: name.clone(),
            kind: Kind::Dir,
            rect: egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(COL_W, H_DIR)),
            entrypoint: false,
        });
        out.edges.push((dir_idx, out.boxes.len() - 1));
        stop = walk(sub, out, cursor, &full, depth + 1, cap);
    }
    stop
}

/// Per-panel interaction-state id (egui memory: transforms survive tab
/// switches within one session, die with the app).
const STATE_ID: &str = "diagrams-directory-canvas";

/// Distinct directory count (for the header strip).
fn count_dirs(tree: &DirNode) -> usize {
    tree.dirs.len() + tree.dirs.values().map(count_dirs).sum::<usize>()
}

/// The panel entry: header strip, then the interactive canvas.
pub(crate) fn ui(ui: &mut egui::Ui, app: &mut crate::app::App) {
    let Some(idx) = app.index.clone() else {
        return;
    };

    let mut reset_requested = false;
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(theme::SPACE_3 as i8, 0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Directory structure").strong());
                ui.weak(format!(
                    "{} directories · {} files",
                    count_dirs(&explorer::build(&idx.files)),
                    idx.files.len(),
                ));
                if ui
                    .button(
                        egui::RichText::new("Reset")
                            .size(theme::FONT_SMALL)
                            .color(theme::TEXT_SECONDARY),
                    )
                    .on_hover_text(
                        "Restore the canonical layout (double-click empty canvas also works)",
                    )
                    .clicked()
                {
                    reset_requested = true;
                }
            });
        });
    ui.add_space(theme::SPACE_2);

    let tree = explorer::build(&idx.files);
    if tree.files.is_empty() && tree.dirs.is_empty() {
        ui.weak("the index has no files yet");
        return;
    }

    let root_name = app
        .repo
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repository".to_string());
    let (lay, omitted) = layout(&root_name, &tree, MAX_NODES);

    if omitted > 0 {
        ui.label(
            egui::RichText::new(format!(
                "capped at {} nodes — {} omitted (the CLI `graph --scope tree` prints the full tree)",
                lay.boxes.len(),
                omitted
            ))
            .size(theme::FONT_SMALL)
            .color(theme::WARNING),
        );
        ui.add_space(theme::SPACE_2);
    }

    // No ScrollArea (B5-02): pan — dragging empty canvas — replaced scrolling
    // inside the canvas, so the plain wheel is free to zoom about the cursor.
    let (resp, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
    let state_id = interact::canvas_key(STATE_ID, app.index.as_deref());
    let mut state = interact::CanvasState::load(ui.ctx(), state_id);
    if reset_requested {
        state.reset();
    }
    let origin = resp.rect.min.to_vec2();
    painter.rect_filled(
        resp.rect,
        egui::CornerRadius::same(theme::CORNER_MD),
        theme::SURFACE_E0,
    );

    let items: Vec<(String, egui::Rect)> =
        lay.boxes.iter().map(|b| (b.id.clone(), b.rect)).collect();
    let (hovered_id, actions) = interact::frame(&resp, &mut state, &items);
    if actions.double_clicked_empty {
        state.reset();
    }
    state.store(ui.ctx(), state_id);

    let placed: Vec<egui::Rect> = lay
        .boxes
        .iter()
        .map(|b| {
            interact::placed_rect(b.rect, state.node_offset(&b.id), state.layer).translate(origin)
        })
        .collect();

    // Containment edges first (faint open lines), behind the nodes.
    for (a, b) in &lay.edges {
        painter.line_segment(
            [placed[*a].center(), placed[*b].center()],
            egui::Stroke::new(1.0, theme::with_alpha(theme::TEXT_MUTED, 60)),
        );
    }

    // Nodes on top.
    let small = egui::FontId::new(theme::FONT_SMALL, egui::FontFamily::Proportional);
    for (i, b) in lay.boxes.iter().enumerate() {
        let rect = placed[i];
        let hovered = hovered_id.as_deref() == Some(b.id.as_str());
        let fill = match b.kind {
            Kind::File if b.entrypoint => theme::accent_subtle(38),
            _ => theme::SURFACE_E1,
        };
        painter.rect_filled(rect, egui::CornerRadius::same(theme::CORNER_MD), fill);
        painter.rect_stroke(
            rect,
            egui::CornerRadius::same(theme::CORNER_MD),
            egui::Stroke::new(
                if hovered { 1.6 } else { 1.0 },
                if hovered {
                    theme::ACCENT
                } else {
                    theme::BORDER_SUBTLE
                },
            ),
            egui::StrokeKind::Middle,
        );
        let color = match b.kind {
            Kind::Dir => theme::TEXT_PRIMARY,
            Kind::File => theme::TEXT_SECONDARY,
        };
        painter.text(
            egui::pos2(rect.left() + 6.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            &b.label,
            small.clone(),
            color,
        );
    }

    // A clicked file node opens through the one selection path; directory
    // clicks are a no-op (no collapse in the canvas, by design).
    if let Some(id) = actions.clicked {
        if let Some(file) = id.strip_prefix('f').map(str::to_string) {
            app.select(Selection::File(file));
        }
    }

    // Hover guidance: node details over a node, the gesture cheat-sheet over
    // empty canvas — with the one-line "what am I looking at" sentence.
    let tip = match &hovered_id {
        Some(id) if id.starts_with('f') => {
            let path = &id[1..];
            match idx.files.iter().find(|f| f.path == *path) {
                Some(f) => format!(
                    "file {path} — {} · {} lines\ndrag the box to move it",
                    f.lang, f.lines
                ),
                None => path.to_string(),
            }
        }
        Some(id) => format!(
            "folder {}/ — the files inside sit below it\ndrag to move the folder",
            id.trim_start_matches('d')
        ),
        None => "each box = a real folder or file on disk · drag empty canvas to pan · \
                 wheel to zoom · double-click to reset"
            .to_string(),
    };
    resp.on_hover_text(tip);
}

#[cfg(test)]
mod tests {
    use super::*;
    use repowalk_core::model::FileEntry;

    fn fe(path: &str, lines: usize, entry: bool) -> FileEntry {
        FileEntry {
            path: path.into(),
            lang: "rust".into(),
            lines,
            mtime: 0,
            is_entrypoint: entry,
        }
    }

    fn sample() -> DirNode {
        explorer::build(&[
            fe("src/main.rs", 10, true),
            fe("src/diagrams/types.rs", 12, false),
            fe("README.md", 1, false),
        ])
    }

    #[test]
    fn layout_uses_the_real_names_under_the_real_directories() {
        let (lay, omitted) = layout("repo", &sample(), MAX_NODES);
        assert_eq!(omitted, 0);
        let labels: Vec<&str> = lay.boxes.iter().map(|b| b.label.as_str()).collect();
        assert!(labels.contains(&"repo"));
        assert!(labels.contains(&"src"));
        assert!(labels.contains(&"diagrams"));
        assert!(labels.contains(&"types.rs"));
        assert!(labels.contains(&"main.rs ★"));
        assert!(labels.contains(&"README.md"));
        let total = total_nodes(&sample());
        assert_eq!(total, lay.boxes.len());
        // The root's containment edges reach README.md and src (2 children).
        assert_eq!(lay.edges.iter().filter(|(a, _)| *a == 0).count(), 2);
    }

    #[test]
    fn a_file_sits_in_the_column_right_of_its_parent_directory() {
        let (lay, _) = layout("repo", &sample(), MAX_NODES);
        let src = lay.boxes.iter().find(|b| b.label == "src").unwrap();
        let main = lay.boxes.iter().find(|b| b.label == "main.rs ★").unwrap();
        assert_eq!(src.rect.left() + COL_W + COL_GAP, main.rect.left());
        // …and shares the column with root's other children, ordered files
        // first: README above src, main.rs at the top of the next column.
        let readme = lay.boxes.iter().find(|b| b.label == "README.md").unwrap();
        assert_eq!(readme.rect.left(), src.rect.left());
        assert!(readme.rect.top() < src.rect.top());
        assert!(main.rect.top() < src.rect.top()); // files before the dir box
    }

    #[test]
    fn the_cap_truncates_deterministically_and_reports_the_omitted_count() {
        let big = explorer::build(
            &(0..20)
                .map(|i| fe(&format!("d{i}/f{i}.rs"), i, false))
                .collect::<Vec<_>>(),
        );
        let (a, omitted) = layout("repo", &big, 10);
        let (b, again) = layout("repo", &big, 10);
        assert_eq!(a.boxes.len(), 10);
        assert_eq!(a.boxes.len(), b.boxes.len());
        assert_eq!(omitted, again);
        assert_eq!(
            a.boxes.first().map(|x| x.label.clone()),
            Some("repo".into())
        );
        assert_eq!(total_nodes(&big), 10 + omitted);
        // Cap 0: nothing placed, everything accounted for.
        let (none, all_of_it) = layout("repo", &big, 0);
        assert!(none.boxes.is_empty());
        assert_eq!(all_of_it, total_nodes(&big));
    }
}
