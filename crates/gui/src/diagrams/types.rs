//! Type-structure diagram view. OWNER: GRAPH-3.
//!
//! Reads `analysis.types` / `analysis.type_edges` — never re-derives edges; the
//! core `types::build` already drops dangling endpoints and duplicate edges.
//!
//! One box per declared type (name, kind badge, fields with their types, and
//! methods), grouped by module/file and tinted with [`theme::module_color`].
//! Edges are styled by kind — `Inherit` solid, `Impl` dashed, `Field`/`Variant`
//! labelled — and the selected type plus its direct neighbourhood are
//! emphasised. Clicking a box drives the shared selection (`Selection::Symbol`).
//! Layout is deterministic (fixed grid) and capped for legibility.
use eframe::egui;
use egui::{Align2, Color32, FontId, Pos2, Rect, Stroke, Vec2};
use repowalk_core::analysis::{Analysis, TypeEdgeKind, TypeKind, TypeNode};

use crate::app::{App, Selection};
use crate::theme;

// ── Geometry: a fixed grid, so identical input always lays out identically ──
const BOX_W: f32 = 200.0;
const BOX_HEAD_H: f32 = 42.0;
const LINE_H: f32 = 15.0;
const GAP_X: f32 = 32.0;
const GAP_Y: f32 = 22.0;
const GROUP_HEAD_H: f32 = 26.0;
/// Rows shown per box before the rest are summarised as "+N more".
const FIELD_CAP: usize = 6;
const METHOD_CAP: usize = 5;
/// Max characters per rendered text line (painter has no clipping here).
const TEXT_CHARS: usize = 30;

/// Legibility cap: draw at most this many type boxes; the header says so.
pub const NODE_CAP: usize = 200;

// ── Layout model (pure, unit-testable) ──────────────────────────────────────

/// One placed type box, in canvas coordinates (top-left origin).
#[derive(Debug, Clone, PartialEq)]
pub struct TypeBox {
    pub id: String,
    pub group: String,
    pub pos: Pos2,
    pub size: Vec2,
    /// Total fields / methods on the type (the box shows a capped prefix).
    pub fields: usize,
    pub methods: usize,
}

/// Deterministic placement of a capped prefix of the type list.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeLayout {
    pub boxes: Vec<TypeBox>,
    /// Group header label and its canvas position.
    pub headers: Vec<(String, Pos2)>,
    pub width: f32,
    pub height: f32,
    /// Boxes drawn / boxes present (cap honesty).
    pub shown: usize,
    pub total: usize,
}

/// Group key: the type's `module_path` when the parser filled it, else the
/// file's directory — never an empty string, so the header always reads.
fn group_key(node: &TypeNode) -> String {
    let module = node.module_path.trim();
    if module.is_empty() {
        App::module_of(&node.file)
    } else {
        module.to_string()
    }
}

/// Box height from the capped field/method rows (plus one "+N more" row each).
fn box_height(node: &TypeNode) -> f32 {
    let fields = node.fields.len().min(FIELD_CAP) + usize::from(node.fields.len() > FIELD_CAP);
    let methods = node.methods.len().min(METHOD_CAP) + usize::from(node.methods.len() > METHOD_CAP);
    BOX_HEAD_H + (fields + methods) as f32 * LINE_H + theme::SPACE_2
}

/// Place `types` in a `columns`-wide grid, grouped by module then sorted by id.
///
/// Deterministic: the output depends only on `types` and `columns`. Only the
/// first [`NODE_CAP`] boxes are placed; `total` still reports the full count so
/// the UI can state the cap instead of silently hiding types.
pub fn layout(types: &[TypeNode], columns: usize) -> TypeLayout {
    let columns = columns.max(1);
    let mut order: Vec<usize> = (0..types.len()).collect();
    order.sort_by(|&a, &b| {
        group_key(&types[a])
            .cmp(&group_key(&types[b]))
            .then_with(|| types[a].id.cmp(&types[b].id))
    });
    let total = order.len();
    order.truncate(NODE_CAP);

    let mut boxes = Vec::with_capacity(order.len());
    let mut headers = Vec::new();
    let mut y = 0.0f32;
    let mut width = 0.0f32;
    let mut i = 0usize;
    while i < order.len() {
        let group = group_key(&types[order[i]]);
        headers.push((group.clone(), Pos2::new(0.0, y)));
        y += GROUP_HEAD_H;
        let mut col = 0usize;
        let mut row_h = 0.0f32;
        while i < order.len() && group_key(&types[order[i]]) == group {
            let node = &types[order[i]];
            let size = Vec2::new(BOX_W, box_height(node));
            let x = col as f32 * (BOX_W + GAP_X);
            boxes.push(TypeBox {
                id: node.id.clone(),
                group: group.clone(),
                pos: Pos2::new(x, y),
                size,
                fields: node.fields.len(),
                methods: node.methods.len(),
            });
            width = width.max(x + BOX_W);
            row_h = row_h.max(size.y);
            col += 1;
            i += 1;
            if col >= columns {
                col = 0;
                y += row_h + GAP_Y;
                row_h = 0.0;
            }
        }
        if col != 0 {
            y += row_h + GAP_Y;
        }
        y += GAP_Y;
    }

    TypeLayout {
        boxes,
        headers,
        width,
        height: y,
        shown: order.len(),
        total,
    }
}

/// Edge style per kind: `(color, width, dashed, label)`. Distinct by both
/// stroke and label so the difference survives a grayscale / colorblind view.
fn edge_style(kind: TypeEdgeKind) -> (Color32, f32, bool, &'static str) {
    match kind {
        TypeEdgeKind::Inherit => (theme::ACCENT, 2.0, false, "inherits"),
        TypeEdgeKind::Impl => (theme::INFO, 1.6, true, "impl"),
        TypeEdgeKind::Field => (
            theme::with_alpha(theme::TEXT_SECONDARY, 210),
            1.0,
            false,
            "field",
        ),
        TypeEdgeKind::Variant => (theme::WARNING, 1.3, true, "variant"),
    }
}

fn kind_label(kind: TypeKind) -> &'static str {
    match kind {
        TypeKind::Struct => "struct",
        TypeKind::Enum => "enum",
        TypeKind::Trait => "trait",
        TypeKind::Class => "class",
        TypeKind::Interface => "interface",
    }
}

/// Honest empty state: `Some(reason)` when the pipeline produced no types.
pub fn empty_notice(a: &Analysis) -> Option<String> {
    a.types.is_empty().then(|| {
        format!(
            "analysis ready, but the type pass produced 0 types ({} type edge(s)) — the \
             pipeline has not filled Analysis.types yet (card CORE-5)",
            a.type_edges.len()
        )
    })
}

// ── Rendering ───────────────────────────────────────────────────────────────

/// The selection id to hand to [`App::select`]: the matching index symbol
/// (`file:line`) when one exists, so the click also opens source, else the
/// type's own id (`file:name`).
fn click_target(app: &App, node: &TypeNode) -> String {
    if let Some(idx) = &app.index {
        if let Some(sym) = idx
            .symbols
            .iter()
            .find(|s| s.file == node.file && s.name == node.name)
        {
            return sym.id.clone();
        }
    }
    node.id.clone()
}

fn font(size: f32, mono: bool) -> FontId {
    FontId::new(
        size,
        if mono {
            egui::FontFamily::Monospace
        } else {
            egui::FontFamily::Proportional
        },
    )
}

fn elide(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn box_center(b: &TypeBox) -> Pos2 {
    b.pos + b.size * 0.5
}

fn arrow_head(painter: &egui::Painter, from: Pos2, to: Pos2, stroke: Stroke) {
    let dir = to - from;
    if dir.length_sq() < 1.0 {
        return;
    }
    let dir = dir.normalized();
    let back = to - dir * 9.0;
    let perp = Vec2::new(-dir.y, dir.x) * 4.0;
    let s = Stroke::new(stroke.width, stroke.color);
    painter.line_segment([to, back + perp], s);
    painter.line_segment([to, back - perp], s);
}

/// Centered empty-state card (does NOT use `ui_kit::not_ready`, whose text says
/// the view "lands with" a card — this view has landed; only the data is thin).
fn notice(ui: &mut egui::Ui, title: &str, lines: &[String]) {
    ui.add_space(theme::SPACE_8);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(theme::FONT_HEADING)
                .color(theme::TEXT_SECONDARY),
        );
        ui.add_space(theme::SPACE_2);
        for line in lines {
            ui.label(
                egui::RichText::new(line)
                    .size(theme::FONT_SMALL)
                    .color(theme::TEXT_PRIMARY),
            );
        }
    });
}

fn header(ui: &mut egui::Ui, lay: &TypeLayout, edge_count: usize) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{} type(s) · {} edge(s)", lay.total, edge_count))
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_SECONDARY),
        );
        if lay.shown < lay.total {
            ui.label(
                egui::RichText::new(format!("showing first {} (cap {NODE_CAP})", lay.shown))
                    .size(theme::FONT_SMALL)
                    .color(theme::WARNING),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new("Inherit solid · Impl dashed · Field/Variant labelled")
                    .size(theme::FONT_SMALL)
                    .color(theme::TEXT_MUTED),
            );
        });
    });
    theme::separator(ui);
}

/// Draw the type-structure diagram for the current analysis.
pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    // Arc clone only — the analysis itself is never deep-cloned per frame.
    let Some(analysis) = app.analysis.clone() else {
        notice(
            ui,
            "Type diagrams",
            &["no analysis yet — press Learn (or run `repowalk learn`) first".to_string()],
        );
        return;
    };
    if let Some(msg) = empty_notice(&analysis) {
        notice(ui, "Type diagrams", &[msg]);
        return;
    }

    let selected = match &app.selection {
        Some(Selection::Symbol(id)) => Some(id.clone()),
        _ => None,
    };

    let avail = ui.available_width();
    let columns = (((avail + GAP_X) / (BOX_W + GAP_X)).floor() as usize).max(1);
    let lay = layout(&analysis.types, columns);
    header(ui, &lay, analysis.type_edges.len());

    // Borrowed lookups built once per frame (no clones of the node vec).
    let drawn: std::collections::BTreeSet<&str> = lay.boxes.iter().map(|b| b.id.as_str()).collect();
    let mut adjacency: std::collections::BTreeMap<&str, Vec<&str>> =
        std::collections::BTreeMap::new();
    for e in &analysis.type_edges {
        if drawn.contains(e.from.as_str()) && drawn.contains(e.to.as_str()) {
            adjacency
                .entry(e.from.as_str())
                .or_default()
                .push(e.to.as_str());
            adjacency
                .entry(e.to.as_str())
                .or_default()
                .push(e.from.as_str());
        }
    }
    let by_id: std::collections::BTreeMap<&str, &TypeBox> =
        lay.boxes.iter().map(|b| (b.id.as_str(), b)).collect();

    egui::Frame::new()
        .fill(theme::BG_BASE)
        .stroke(Stroke::new(1.0, theme::BORDER_SUBTLE))
        .show(ui, |ui| {
            egui::ScrollArea::both()
                .id_salt("types-diagram")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let canvas = Vec2::new(lay.width.max(400.0), lay.height.max(160.0));
                    let (rect, resp) = ui.allocate_exact_size(canvas, egui::Sense::click());
                    let painter = ui.painter_at(rect);
                    let origin = rect.min;
                    let to_screen = |p: Pos2| origin + p.to_vec2();

                    let hovered = resp.hover_pos().and_then(|p| {
                        let local = p - origin;
                        lay.boxes
                            .iter()
                            .find(|b| {
                                Rect::from_min_size(b.pos, b.size)
                                    .contains(Pos2::new(local.x, local.y))
                            })
                            .map(|b| b.id.as_str())
                    });

                    let focus = selected.as_deref();
                    let is_focus = |id: &str| focus == Some(id) || hovered == Some(id);
                    let is_nb = |id: &str| {
                        focus.is_some_and(|f| adjacency.get(f).is_some_and(|v| v.contains(&id)))
                    };
                    let is_dim = |id: &str| focus.is_some() && !is_focus(id) && !is_nb(id);

                    // Group headers.
                    for (group, pos) in &lay.headers {
                        painter.text(
                            to_screen(*pos),
                            Align2::LEFT_TOP,
                            elide(group, 44),
                            font(theme::FONT_SMALL, false),
                            theme::module_color(group),
                        );
                    }

                    // Edges first, so boxes sit on top.
                    for e in &analysis.type_edges {
                        let (Some(from), Some(to)) =
                            (by_id.get(e.from.as_str()), by_id.get(e.to.as_str()))
                        else {
                            continue;
                        };
                        let (mut color, width, dashed, label) = edge_style(e.kind);
                        let touches = is_focus(e.from.as_str()) || is_focus(e.to.as_str());
                        let dim = focus.is_some() && !touches;
                        if dim {
                            color = theme::dim_alpha(45);
                        } else if touches {
                            color = theme::ACCENT;
                        }
                        let stroke = Stroke::new(if touches { width + 0.6 } else { width }, color);
                        let a = to_screen(box_center(from));
                        let b = to_screen(box_center(to));
                        if dashed {
                            for seg in egui::Shape::dashed_line(&[a, b], stroke, 7.0, 5.0) {
                                painter.add(seg);
                            }
                        } else {
                            painter.line_segment([a, b], stroke);
                        }
                        arrow_head(&painter, a, b, stroke);
                        if !dim {
                            let mid = a + (b - a) * 0.5;
                            painter.text(
                                mid - Vec2::new(0.0, 3.0),
                                Align2::CENTER_BOTTOM,
                                label,
                                font(theme::FONT_SMALL, false),
                                color,
                            );
                        }
                    }

                    // Boxes.
                    for b in &lay.boxes {
                        let brect = Rect::from_min_size(to_screen(b.pos), b.size);
                        let corner = egui::CornerRadius::same(theme::CORNER_MD);
                        let group_color = theme::module_color(&b.group);
                        let sel = focus == Some(b.id.as_str());
                        let nb = is_nb(b.id.as_str());
                        let dim = is_dim(b.id.as_str());

                        let fill = if sel {
                            theme::SURFACE_SELECTED
                        } else {
                            theme::SURFACE_E2
                        };
                        painter.rect_filled(brect, corner, fill);
                        let (border, bw) = if sel || nb {
                            (theme::ACCENT, 2.0)
                        } else if dim {
                            (theme::dim_alpha(40), 1.0)
                        } else {
                            (group_color, 1.5)
                        };
                        painter.rect_stroke(
                            brect,
                            corner,
                            Stroke::new(bw, border),
                            egui::StrokeKind::Middle,
                        );

                        let Some(node) = analysis.types.iter().find(|t| t.id == b.id) else {
                            continue;
                        };
                        let name_color = if dim {
                            theme::TEXT_MUTED
                        } else {
                            theme::TEXT_PRIMARY
                        };
                        let pad = Vec2::new(theme::SPACE_2, theme::SPACE_1 + 1.0);
                        painter.text(
                            brect.min + pad,
                            Align2::LEFT_TOP,
                            elide(&node.name, 24),
                            font(theme::FONT_BODY, false),
                            name_color,
                        );
                        painter.text(
                            brect.right_top() + Vec2::new(-theme::SPACE_2, theme::SPACE_1 + 1.0),
                            Align2::RIGHT_TOP,
                            kind_label(node.kind),
                            font(theme::FONT_SMALL, false),
                            if dim {
                                theme::TEXT_DISABLED
                            } else {
                                group_color
                            },
                        );

                        let text_color = |c: Color32| {
                            if dim {
                                theme::TEXT_DISABLED
                            } else {
                                c
                            }
                        };
                        let x = brect.min.x + theme::SPACE_2;
                        let mut ty = brect.min.y + 20.0;
                        for f in node.fields.iter().take(FIELD_CAP) {
                            painter.text(
                                Pos2::new(x, ty),
                                Align2::LEFT_TOP,
                                elide(&format!("{}: {}", f.name, f.type_name), TEXT_CHARS),
                                font(theme::FONT_SMALL, true),
                                text_color(theme::TEXT_SECONDARY),
                            );
                            ty += LINE_H;
                        }
                        if node.fields.len() > FIELD_CAP {
                            painter.text(
                                Pos2::new(x, ty),
                                Align2::LEFT_TOP,
                                format!("+{} more fields", node.fields.len() - FIELD_CAP),
                                font(theme::FONT_SMALL, false),
                                theme::TEXT_MUTED,
                            );
                            ty += LINE_H;
                        }
                        for m in node.methods.iter().take(METHOD_CAP) {
                            painter.text(
                                Pos2::new(x, ty),
                                Align2::LEFT_TOP,
                                elide(&format!("{m}()"), TEXT_CHARS),
                                font(theme::FONT_SMALL, true),
                                text_color(theme::INFO),
                            );
                            ty += LINE_H;
                        }
                        if node.methods.len() > METHOD_CAP {
                            painter.text(
                                Pos2::new(x, ty),
                                Align2::LEFT_TOP,
                                format!("+{} more methods", node.methods.len() - METHOD_CAP),
                                font(theme::FONT_SMALL, false),
                                theme::TEXT_MUTED,
                            );
                        }
                    }

                    // Click a box → shared selection.
                    if resp.clicked() {
                        if let Some(p) = resp.interact_pointer_pos() {
                            let local = p - origin;
                            if let Some(b) = lay.boxes.iter().find(|b| {
                                Rect::from_min_size(b.pos, b.size)
                                    .contains(Pos2::new(local.x, local.y))
                            }) {
                                if let Some(node) = analysis.types.iter().find(|t| t.id == b.id) {
                                    let target = click_target(app, node);
                                    app.select(Selection::Symbol(target));
                                }
                            }
                        }
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use repowalk_core::analysis::Field;

    fn ty(file: &str, name: &str, kind: TypeKind, module: &str) -> TypeNode {
        TypeNode {
            id: format!("{file}:{name}"),
            name: name.into(),
            kind,
            file: file.into(),
            line: 1,
            fields: vec![Field {
                name: "f".into(),
                type_name: "u32".into(),
                visibility: "pub".into(),
                line: 2,
            }],
            methods: vec!["m".into()],
            doc: String::new(),
            module_path: module.into(),
        }
    }

    #[test]
    fn layout_is_deterministic_and_grouped() {
        let types = vec![
            ty("src/b.rs", "B", TypeKind::Struct, "crate::b"),
            ty("src/a.rs", "A", TypeKind::Struct, "crate::a"),
            ty("src/a.rs", "C", TypeKind::Enum, "crate::a"),
        ];
        let first = layout(&types, 2);
        let second = layout(&types, 2);
        assert_eq!(first, second, "same input must lay out identically");
        assert_eq!(first.total, 3);
        assert_eq!(first.shown, 3);
        // Groups sorted, then ids sorted inside a group.
        assert_eq!(first.headers[0].0, "crate::a");
        assert_eq!(first.headers[1].0, "crate::b");
        assert_eq!(first.boxes[0].id, "src/a.rs:A");
        assert_eq!(first.boxes[1].id, "src/a.rs:C");
        assert_eq!(first.boxes[2].id, "src/b.rs:B");
    }

    #[test]
    fn columns_change_wrapping_but_not_order() {
        let types: Vec<TypeNode> = (0..5)
            .map(|i| ty("src/x.rs", &format!("T{i}"), TypeKind::Struct, "crate::x"))
            .collect();
        let one_col = layout(&types, 1);
        let three_col = layout(&types, 3);
        // Same order, but the 3-wide grid is shorter and wider.
        let ids = |l: &TypeLayout| l.boxes.iter().map(|b| b.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&one_col), ids(&three_col));
        assert!(three_col.height < one_col.height);
        assert!(three_col.width > one_col.width);
    }

    #[test]
    fn layout_caps_boxes_but_reports_total() {
        let types: Vec<TypeNode> = (0..NODE_CAP + 25)
            .map(|i| {
                ty(
                    "src/x.rs",
                    &format!("T{i:04}"),
                    TypeKind::Struct,
                    "crate::x",
                )
            })
            .collect();
        let l = layout(&types, 8);
        assert_eq!(l.shown, NODE_CAP);
        assert_eq!(l.total, NODE_CAP + 25);
        assert_eq!(l.boxes.len(), NODE_CAP);
    }

    #[test]
    fn empty_types_is_an_empty_layout() {
        let l = layout(&[], 3);
        assert!(l.boxes.is_empty());
        assert!(l.headers.is_empty());
        assert_eq!(l.height, 0.0);
        assert_eq!(l.shown, 0);
    }

    #[test]
    fn edge_styles_are_distinct_and_stable() {
        let inherit = edge_style(TypeEdgeKind::Inherit);
        let imp = edge_style(TypeEdgeKind::Impl);
        let field = edge_style(TypeEdgeKind::Field);
        let variant = edge_style(TypeEdgeKind::Variant);
        assert!(!inherit.2, "Inherit is solid");
        assert!(imp.2, "Impl is dashed");
        assert!(!field.2, "Field is solid, but labelled + thinner");
        assert!(variant.2, "Variant is dashed");
        assert_ne!(inherit.3, field.3);
        assert_ne!(imp.3, variant.3);
        // Same input → identical style (no hidden state).
        assert_eq!(edge_style(TypeEdgeKind::Inherit), inherit);
    }

    #[test]
    fn empty_notice_names_the_type_card() {
        let empty = Analysis::new();
        let msg = empty_notice(&empty).expect("notice for an empty type pass");
        assert!(msg.contains("CORE-5"), "message names the type card: {msg}");

        let mut filled = Analysis::new();
        filled
            .types
            .push(ty("src/a.rs", "A", TypeKind::Struct, "crate::a"));
        assert!(empty_notice(&filled).is_none());
    }
}
