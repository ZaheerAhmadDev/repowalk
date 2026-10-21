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
//! Layout is deterministic (fixed grid) and capped for legibility — and the
//! canvas is interactive: nodes drag, empty canvas pans, wheel zooms
//! (`diagrams::interact`), all on top of the canonical grid.
use eframe::egui;
use egui::{Align2, Color32, FontId, Pos2, Rect, Stroke, Vec2};
use repowalk_core::analysis::{Analysis, TypeEdgeKind, TypeKind, TypeNode};

use crate::app::{App, Selection};
use crate::diagrams::interact;
use crate::theme;

/// egui-memory key for this canvas's interact layer (state survives tab
/// switches within one session; the layout id is global — one type diagram).
const STATE_ID: &str = "diagrams-types-interact";

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

/// Canvas item id of one module group header ("hdr:" keeps it disjoint from
/// any `file:name` type id).
fn header_id(group: &str) -> String {
    format!("hdr:{group}")
}

/// The interact-layer group key of a module — every header and every one of
/// its type boxes share it, so any single drag moves the module as a block.
fn group_id(group: &str) -> String {
    format!("g:{group}")
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

/// Centered empty-state card — the local multi-line variant of
/// `ui_kit::empty_state` (it takes a list of lines), not a "not built yet"
/// placeholder: this view has landed; only the data is thin.
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

fn header(ui: &mut egui::Ui, lay: &TypeLayout, edge_count: usize, reset: &mut bool) {
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
            if ui
                .button("Reset")
                .on_hover_text(
                    "put every type box back on the deterministic grid \
                     (double-click on empty canvas does the same)",
                )
                .clicked()
            {
                *reset = true;
            }
            ui.vertical(|ui| {
                // Dynamic: the explanatory captions are desktop furniture —
                // on a narrow pane they get out of the way (the hover tags
                // carry the same words).
                if ui.available_width() > 560.0 {
                    ui.label(
                        egui::RichText::new(
                            "what you see: a box = a type learned from the code · \
                             arrows = how types relate (solid inherits · dashed impl · \
                             field, variant labelled)",
                        )
                        .size(theme::FONT_SMALL)
                        .color(theme::TEXT_MUTED),
                    );
                    ui.label(
                        egui::RichText::new(
                            "gestures: wheel zoom · drag empty space pans · \
                             drag a box or its module name to move that module together",
                        )
                        .size(theme::FONT_SMALL)
                        .color(theme::TEXT_MUTED),
                    );
                }
            });
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
    let mut reset_requested = false;
    header(ui, &lay, analysis.type_edges.len(), &mut reset_requested);

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
    // box id → position in `lay.boxes` / the placed screen-rect list below.
    let by_id: std::collections::BTreeMap<&str, usize> = lay
        .boxes
        .iter()
        .enumerate()
        .map(|(i, b)| (b.id.as_str(), i))
        .collect();

    egui::Frame::new()
        .fill(theme::BG_BASE)
        .stroke(Stroke::new(1.0, theme::BORDER_SUBTLE))
        .show(ui, |ui| {
            // No ScrollArea (B5-02): pan — dragging empty canvas — replaced
            // scrolling inside the canvas, so the plain wheel is free to zoom
            // about the cursor (the wheel no longer has a scroll to fight).
            let (rect, resp) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            let origin = rect.min;
            let key = interact::canvas_key(STATE_ID, app.index.as_deref());
            let mut state = interact::CanvasState::load(ui.ctx(), key);
            if reset_requested {
                state.reset();
            }
            let s = state.layer.scale;

            // Base items for the interact layer: module headers then type
            // boxes, in paint order. Every item belongs to its module group,
            // so dragging a box — or the module name itself — moves the whole
            // module together, name and all (user-reported mismatch: names
            // used to stay behind while only boxes moved).
            let items: Vec<(String, Rect)> = lay
                .headers
                .iter()
                .map(|(group, pos)| {
                    (
                        header_id(group),
                        Rect::from_min_size(*pos, Vec2::new(lay.width, GROUP_HEAD_H)),
                    )
                })
                .chain(
                    lay.boxes
                        .iter()
                        .map(|b| (b.id.clone(), Rect::from_min_size(b.pos, b.size))),
                )
                .collect();
            let member_of: std::collections::BTreeMap<String, String> = lay
                .headers
                .iter()
                .map(|(group, _)| (header_id(group), group_id(group)))
                .chain(lay.boxes.iter().map(|b| (b.id.clone(), group_id(&b.group))))
                .collect();
            let (hovered, actions) = interact::frame_with(&resp, &mut state, &items, &member_of);
            if actions.double_clicked_empty {
                state.reset();
            }
            let placed: Vec<Rect> = lay
                .boxes
                .iter()
                .map(|b| {
                    interact::placed_rect(
                        Rect::from_min_size(b.pos, b.size),
                        state.node_offset(&b.id) + state.group_offset(&group_id(&b.group)),
                        state.layer,
                    )
                    .translate(origin.to_vec2())
                })
                .collect();

            let focus = selected.as_deref();
            let is_focus = |id: &str| focus == Some(id) || hovered.as_deref() == Some(id);
            let is_nb =
                |id: &str| focus.is_some_and(|f| adjacency.get(f).is_some_and(|v| v.contains(&id)));
            let is_dim = |id: &str| focus.is_some() && !is_focus(id) && !is_nb(id);

            // Group headers ride their module group: a box drag (or a header
            // drag) carries the name with it — own + group offsets then the
            // layer transform, exactly the box placement math.
            for (group, pos) in &lay.headers {
                painter.text(
                    state.layer.to_screen(
                        *pos + state.node_offset(&header_id(group))
                            + state.group_offset(&group_id(group)),
                    ) + origin.to_vec2(),
                    Align2::LEFT_TOP,
                    elide(group, 44),
                    font(theme::FONT_SMALL * s, false),
                    theme::module_color(group),
                );
            }

            // Plain-language answers to "what am I looking at": hover a box or
            // a module header (suppressed while the pointer is dragging).
            if !state.is_dragging() {
                if let Some(pos) = resp.hover_pos() {
                    let tag = match hovered.as_deref() {
                        Some(h) if h.starts_with("hdr:") => {
                            let group = h.trim_start_matches("hdr:");
                            Some((
                                format!("module {group}"),
                                vec![
                                    "drag it — or any of its type boxes —".to_string(),
                                    "to move the module together, name included".to_string(),
                                ],
                            ))
                        }
                        Some(h) => analysis.types.iter().find(|t| t.id == *h).map(|node| {
                            (
                                format!("{} {}", kind_label(node.kind), node.name),
                                vec![
                                    format!(
                                        "{} fields · {} methods · defined at {}:{}",
                                        node.fields.len(),
                                        node.methods.len(),
                                        node.file,
                                        node.line
                                    ),
                                    "arrows = how it relates to other types".to_string(),
                                    "(solid inherits · dashed impl · field, variant)".to_string(),
                                ],
                            )
                        }),
                        None => None,
                    };
                    if let Some((title, lines)) = tag {
                        interact::hover_tag(&painter, pos, &title, &lines);
                    }
                }
            }

            // Edges first, so boxes sit on top. Endpoints follow dragged boxes.
            for e in &analysis.type_edges {
                let (Some(&from), Some(&to)) =
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
                let a = placed[from].center();
                let b = placed[to].center();
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
                        font(theme::FONT_SMALL * s, false),
                        color,
                    );
                }
            }

            // Boxes. `placed` is parallel to `lay.boxes`, offset + layer applied.
            for (bi, b) in lay.boxes.iter().enumerate() {
                let brect = placed[bi];
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
                let pad = Vec2::new(theme::SPACE_2 * s, (theme::SPACE_1 + 1.0) * s);
                painter.text(
                    brect.min + pad,
                    Align2::LEFT_TOP,
                    elide(&node.name, 24),
                    font(theme::FONT_BODY * s, false),
                    name_color,
                );
                painter.text(
                    brect.right_top() + Vec2::new(-theme::SPACE_2 * s, (theme::SPACE_1 + 1.0) * s),
                    Align2::RIGHT_TOP,
                    kind_label(node.kind),
                    font(theme::FONT_SMALL * s, false),
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
                let x = brect.min.x + theme::SPACE_2 * s;
                let mut ty = brect.min.y + 20.0 * s;
                let line_h = LINE_H * s;
                for f in node.fields.iter().take(FIELD_CAP) {
                    painter.text(
                        Pos2::new(x, ty),
                        Align2::LEFT_TOP,
                        elide(&format!("{}: {}", f.name, f.type_name), TEXT_CHARS),
                        font(theme::FONT_SMALL * s, true),
                        text_color(theme::TEXT_SECONDARY),
                    );
                    ty += line_h;
                }
                if node.fields.len() > FIELD_CAP {
                    painter.text(
                        Pos2::new(x, ty),
                        Align2::LEFT_TOP,
                        format!("+{} more fields", node.fields.len() - FIELD_CAP),
                        font(theme::FONT_SMALL * s, false),
                        theme::TEXT_MUTED,
                    );
                    ty += line_h;
                }
                for m in node.methods.iter().take(METHOD_CAP) {
                    painter.text(
                        Pos2::new(x, ty),
                        Align2::LEFT_TOP,
                        elide(&format!("{m}()"), TEXT_CHARS),
                        font(theme::FONT_SMALL * s, true),
                        text_color(theme::INFO),
                    );
                    ty += line_h;
                }
                if node.methods.len() > METHOD_CAP {
                    painter.text(
                        Pos2::new(x, ty),
                        Align2::LEFT_TOP,
                        format!("+{} more methods", node.methods.len() - METHOD_CAP),
                        font(theme::FONT_SMALL * s, false),
                        theme::TEXT_MUTED,
                    );
                }
            }

            // Click a box → shared selection (egui's click-vs-drag threshold
            // keeps an unmoved click a click; a real drag never selects).
            if let Some(id) = actions.clicked {
                if let Some(&bi) = by_id.get(id.as_str()) {
                    if let Some(node) = analysis.types.iter().find(|t| t.id == lay.boxes[bi].id) {
                        let target = click_target(app, node);
                        app.select(Selection::Symbol(target));
                    }
                }
            }

            state.store(ui.ctx(), key);
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
