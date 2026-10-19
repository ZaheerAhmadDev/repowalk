//! File links — the whole code base as a graph of its FILES (user request).
//!
//! One card per indexed file (the mini-view of what it contains: the real
//! file name in its head, its top symbol names as rows below, its line
//! count), one arrow per aggregated import (`index.imports` — the B5-01
//! directory emitter reads the same source, so "A imports B" agrees
//! everywhere). Hovering a card lights it and everything it is linked to in
//! both directions and dims the rest; the Deep toggle expands that into the
//! whole reachable import chain (the transitive "what is affected" set).
//! Every name on screen is the real path on disk.
//!
//! OWNER: integrator (post-batch B5 user pass). Layout / links / highlight
//! are pure functions with tests; the canvas rides the shared `interact`
//! layer — module headers + their file stacks are groups, so one drag moves
//! a whole module column, exactly like the Types view.

use std::collections::{BTreeMap, BTreeSet};

use eframe::egui;
use egui::{Align2, Color32, FontId, Pos2, Rect, Stroke, Vec2};

use crate::app::{App, Selection};
use crate::diagrams::interact;
use crate::theme;
use repowalk_core::model::{FileEntry, Index};

// ── Tuning knobs ────────────────────────────────────────────────────────────

/// Card canvas width; body rows scale inside it.
const CARD_W: f32 = 150.0;
/// Card header height (the file name strip).
const HEAD_H: f32 = 16.0;
/// One symbol row height.
const ROW_H: f32 = 11.0;
/// The LOC footer strip height.
const FOOT_H: f32 = 11.0;
/// Vertical gap between stacked cards.
const GAP_Y: f32 = 8.0;
/// Horizontal gap between module columns.
const COL_GAP: f32 = 30.0;
/// Module header strip height.
const GROUP_HEAD_H: f32 = 20.0;
/// Cards drawn cap (honesty banner when the repo is bigger).
const CARD_CAP: usize = 400;
/// Rows of the file's real symbols shown on the mini-view.
const SYM_ROWS: usize = 4;
const STATE_ID: &str = "diagrams-files-canvas";

// ── Pure data model ─────────────────────────────────────────────────────────

/// One real file import link, aggregated per ordered pair (count = how many
/// separate imports from A into B the analysis saw).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileLink {
    pub from: String,
    pub to: String,
    pub count: usize,
}

/// Aggregate `index.imports` into file→file links. Only links between two
/// *indexed* files count (external crates / unresolved module paths are
/// skipped, exactly like the B5-01 emitter — never invented). A file
/// importing itself is not a link.
pub fn links(idx: &Index) -> Vec<FileLink> {
    let paths: BTreeSet<&str> = idx.files.iter().map(|f| f.path.as_str()).collect();
    let norm = |p: &str| p.replace('\\', "/");
    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    for imp in &idx.imports {
        let from = norm(&imp.file);
        let to = norm(&imp.target);
        if from == to {
            continue;
        }
        if !paths.contains(from.as_str()) || !paths.contains(to.as_str()) {
            continue;
        }
        *counts.entry((from, to)).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|((from, to), count)| FileLink { from, to, count })
        .collect()
}

/// One file card as laid out (the mini-view of the real file).
#[derive(Debug, Clone, PartialEq)]
pub struct FileCard {
    /// The real path on disk — the card's identity (no ids on screen).
    pub id: String,
    /// The module (directory) this card belongs to.
    pub group: String,
    pub pos: Pos2,
    pub size: Vec2,
    pub lines: usize,
    pub lang: String,
    pub entry: bool,
    /// The file's real symbol names, in line order (the body rows).
    pub symbols: Vec<String>,
}

impl FileCard {
    /// The real file name (the last path segment) for the card head.
    fn file_name(&self) -> String {
        self.id
            .rsplit_once('/')
            .map_or_else(|| self.id.clone(), |(_, n)| n.to_owned())
    }
}

/// The deterministic file-graph layout: module columns, file stacks inside.
#[derive(Debug, Clone, PartialEq)]
pub struct FilesLayout {
    /// In paint order (group headers first, then the cards in module order).
    pub cards: Vec<FileCard>,
    /// Module header label ↔ its canvas position (top of the column).
    pub headers: Vec<(String, Pos2)>,
    pub width: f32,
    pub height: f32,
    /// Cards drawn / files present (cap honesty).
    pub shown: usize,
    pub total: usize,
}

/// The card's height from how many rows it actually draws.
fn card_height(symbol_rows: usize, filler_rows: usize) -> f32 {
    HEAD_H + (symbol_rows + filler_rows) as f32 * ROW_H + FOOT_H
}

/// How many faint filler bars a file gets — a stable visual hint of its size
/// (roughly one per 50 LOC, clamped); deterministic by data, not by hash.
fn filler_rows(lines: usize) -> usize {
    (lines / 50).min(3)
}

/// Group the indexed files into per-module columns (deterministic: modules
/// sorted by path, files sorted within). The body rows are the file's real
/// symbol names in line order, capped at [`SYM_ROWS`] — the honest mini-view,
/// no invented content.
pub fn layout(files: &[FileEntry], symbols: &BTreeMap<String, Vec<String>>) -> FilesLayout {
    let mut ordered: Vec<&FileEntry> = files.iter().collect();
    ordered.sort_by(|a, b| {
        module_of(&a.path)
            .cmp(&module_of(&b.path))
            .then(a.path.cmp(&b.path))
    });
    let total = ordered.len();
    ordered.truncate(CARD_CAP);

    let mut cards = Vec::with_capacity(ordered.len());
    let mut headers: Vec<(String, Pos2)> = Vec::new();
    let mut width = CARD_W;
    let mut height = GROUP_HEAD_H;
    let (mut y, mut x) = (0.0f32, 0.0f32);
    let mut last_group = String::new();
    for f in &ordered {
        let group = module_of(&f.path);
        if group != last_group {
            if !last_group.is_empty() {
                x += CARD_W + COL_GAP;
            }
            headers.push((group.clone(), Pos2::new(x, 0.0)));
            y = GROUP_HEAD_H;
            last_group = group;
        }
        let rows = symbols.get(&f.path).map_or(0, |v| v.len());
        let size = Vec2::new(CARD_W, card_height(rows, filler_rows(f.lines)));
        cards.push(FileCard {
            id: f.path.clone(),
            group: last_group.clone(),
            pos: Pos2::new(x, y),
            size,
            lines: f.lines,
            lang: f.lang.clone(),
            entry: f.is_entrypoint,
            symbols: symbols
                .get(&f.path)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .take(SYM_ROWS)
                .collect(),
        });
        y += size.y + GAP_Y;
        height = height.max(y - GAP_Y);
        width = width.max(x + CARD_W);
    }
    FilesLayout {
        cards,
        headers,
        width,
        height,
        shown: ordered.len(),
        total,
    }
}

/// The module (directory part) of a path — the same rule the GUI uses
/// everywhere (`App::module_of`): `src/main.rs` → `src`, `c.rs` → `(root)`.
fn module_of(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(d, _)| d.to_string())
        .unwrap_or_else(|| "(root)".to_owned())
}

/// What hover/selection lights up, decided once by a pure function.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Focus {
    /// Full-colour nodes: the focus plus (deep) every file reachable from it
    /// through imports — the transitive "affected / affects" set.
    pub lit: BTreeSet<String>,
    /// How many files this one imports (direct out-degree).
    pub out: usize,
    /// How many files import it — changes here directly affect them.
    pub direct_in: usize,
    /// Link pairs that keep full colour (both endpoints lit).
    pub lit_links: BTreeSet<(String, String)>,
}

/// The highlight for a hovered/selected node. Direct (default): the node,
/// its direct neighbours and their links. Deep: the whole import chain both
/// ways (BFS over the undirected link graph).
pub fn focus_sets(node: Option<&str>, links: &[FileLink], deep: bool) -> Focus {
    let Some(node) = node else {
        return Focus::default();
    };
    let mut lit = BTreeSet::from([node.to_owned()]);
    let mut out = 0;
    let mut direct_in = 0;
    let mut lit_links = BTreeSet::new();
    for l in links {
        if l.from == node || l.to == node {
            lit_links.insert((l.from.clone(), l.to.clone()));
            lit.insert(l.from.clone());
            lit.insert(l.to.clone());
            if l.to == node {
                direct_in += 1;
            }
            if l.from == node {
                out += 1;
            }
        }
    }
    if deep {
        let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for l in links {
            adj.entry(l.from.as_str()).or_default().push(l.to.as_str());
            adj.entry(l.to.as_str()).or_default().push(l.from.as_str());
        }
        // The BFS walks with its own seen-set: `lit` already holds the
        // direct neighbours, so enqueueing on inserts into it would stop
        // at exactly those nodes and miss the transitive closure.
        let mut seen: BTreeSet<&str> = BTreeSet::from([node]);
        let mut queue: Vec<&str> = vec![node];
        while let Some(cur) = queue.pop() {
            for next in adj.get(cur).into_iter().flatten().copied() {
                if seen.insert(next) {
                    lit.insert(next.to_owned());
                    queue.push(next);
                }
            }
        }
        lit_links.clear();
        for l in links {
            if lit.contains(&l.from) && lit.contains(&l.to) {
                lit_links.insert((l.from.clone(), l.to.clone()));
            }
        }
    }
    Focus {
        lit,
        out,
        direct_in,
        lit_links,
    }
}

// ── Canvas ──────────────────────────────────────────────────────────────────

/// Item id of a module header (kept disjoint from file paths).
fn header_id(group: &str) -> String {
    format!("hdr:{group}")
}

/// The interact group key of a module — header and every one of its cards.
fn group_id(group: &str) -> String {
    format!("g:{group}")
}

fn font(size: f32, mono: bool) -> FontId {
    FontId::new(
        size.max(4.0),
        if mono {
            egui::FontFamily::Monospace
        } else {
            egui::FontFamily::Proportional
        },
    )
}

/// Real strings only, width-capped for the card rows.
fn elide(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        text.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}

/// Small arrowhead at `b`, oriented by the `a→b` segment.
fn arrow_head(painter: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    let dir = (b - a).normalized() * 5.0;
    let perpendicular = egui::vec2(-dir.y, dir.x) * 0.45;
    let l = b - dir + perpendicular;
    let r = b - dir - perpendicular;
    // Two dashed segments from the base to the tip = the open arrowhead.
    for [p0, p1] in [[l, b], [r, b]] {
        for seg in egui::Shape::dashed_line(&[p0, p1], stroke, 3.0, 2.0) {
            painter.add(seg);
        }
    }
}

/// Draw the file-links diagram for the whole code base. Needs only the index
/// (no Learn pass): files + imports are exactly what it shows.
pub fn ui(ui: &mut egui::Ui, app: &mut App) {
    let Some(idx) = app.index.clone() else {
        notice(
            ui,
            "File links",
            &["no repository indexed yet — open one first".to_string()],
        );
        return;
    };

    let selected = match &app.selection {
        Some(Selection::File(path)) => Some(path.clone()),
        _ => None,
    };

    // The mini-view rows: each file's real symbols in line order.
    // (Keyed by file so the same file always shows the same rows.)
    let mut symbols: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for s in &idx.symbols {
        symbols
            .entry(s.file.clone())
            .or_default()
            .push(s.name.clone());
    }
    for names in symbols.values_mut() {
        names.truncate(SYM_ROWS);
    }

    let link_list = links(&idx);
    let lay = layout(&idx.files, &symbols);
    // The Deep toggle lives in egui temp state (same as the control-view
    // filter): a per-view switch, surviving frames.
    let deep_id = egui::Id::new("diagrams-files-deep");
    let mut deep = ui
        .ctx()
        .data_mut(|d| *d.get_temp_mut_or_default::<bool>(deep_id));
    let mut reset_requested = false;
    header(ui, &lay, &link_list, &mut reset_requested, &mut deep);
    ui.ctx()
        .data_mut(|d| *d.get_temp_mut_or_default::<bool>(deep_id) = deep);

    if lay.total == 0 {
        notice(
            ui,
            "File links",
            &["no indexed files — the links graph needs a repository".to_string()],
        );
        return;
    }

    egui::Frame::new()
        .fill(theme::BG_BASE)
        .stroke(Stroke::new(1.0, theme::BORDER_SUBTLE))
        .show(ui, |ui| {
            let (rect, resp) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            let origin = rect.min;
            let mut state = interact::CanvasState::load(ui.ctx(), egui::Id::new(STATE_ID));
            if reset_requested {
                state.reset();
            }
            let s = state.layer.scale;

            // Interact items: module headers then cards, every id a member of
            // its module group (one drag = the whole module column).
            let items: Vec<(String, Rect)> = lay
                .headers
                .iter()
                .map(|(group, pos)| {
                    (
                        header_id(group),
                        Rect::from_min_size(*pos, Vec2::new(CARD_W, GROUP_HEAD_H)),
                    )
                })
                .chain(
                    lay.cards
                        .iter()
                        .map(|c| (c.id.clone(), Rect::from_min_size(c.pos, c.size))),
                )
                .collect();
            let member_of: BTreeMap<String, String> = lay
                .headers
                .iter()
                .map(|(group, _)| (header_id(group), group_id(group)))
                .chain(lay.cards.iter().map(|c| (c.id.clone(), group_id(&c.group))))
                .collect();
            let (hovered, actions) = interact::frame_with(&resp, &mut state, &items, &member_of);
            if actions.double_clicked_empty {
                state.reset();
            }

            let placed: BTreeMap<&str, Rect> = lay
                .cards
                .iter()
                .map(|c| {
                    (
                        c.id.as_str(),
                        interact::placed_rect(
                            Rect::from_min_size(c.pos, c.size),
                            state.node_offset(&c.id) + state.group_offset(&group_id(&c.group)),
                            state.layer,
                        )
                        .translate(origin.to_vec2()),
                    )
                })
                .collect();

            // What the pointer lights up: hover overrides selection.
            let focus_now = hovered.clone().or_else(|| selected.clone());
            let focus = focus_sets(focus_now.as_deref(), &link_list, deep);
            let is_lit = |id: &str| focus.lit.contains(id);

            // Module headers ride their group; dim when no card of the module
            // is lit. (A path's directory IS the module name.)
            for (group, pos) in &lay.headers {
                let any_lit = focus.lit.iter().any(|l| module_of(l) == *group);
                painter.text(
                    state.layer.to_screen(
                        *pos + state.node_offset(&header_id(group))
                            + state.group_offset(&group_id(group)),
                    ) + origin.to_vec2(),
                    Align2::LEFT_TOP,
                    elide(&format!("{group}/"), 40),
                    font(theme::FONT_SMALL * s, true),
                    if focus_now.is_some() && !any_lit {
                        theme::dim_alpha(50)
                    } else {
                        theme::TEXT_SECONDARY
                    },
                );
            }

            // Links first so cards sit on top. Lit = both endpoints lit; the
            // arrowhead reads A imports B (dependent → dependency).
            let mut drawn_links = 0;
            for l in &link_list {
                let (Some(ra), Some(rb)) = (placed.get(l.from.as_str()), placed.get(l.to.as_str()))
                else {
                    continue;
                };
                let linked = focus.lit_links.contains(&(l.from.clone(), l.to.clone()));
                let dim = focus_now.is_some() && !linked;
                let (color, width_f): (Color32, f32) = if dim {
                    (theme::dim_alpha(38), 1.0)
                } else {
                    (theme::ACCENT, 1.4)
                };
                let stroke = Stroke::new(width_f.max(s.min(1.0)), color);
                let a = Pos2::new(ra.max.x, ra.center().y);
                let b = Pos2::new(rb.min.x, rb.center().y);
                painter.line_segment([a, b], stroke);
                arrow_head(&painter, a, b, stroke);
                if l.count > 1 {
                    let mid = Pos2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
                    painter.text(
                        mid - Vec2::new(0.0, 3.0),
                        Align2::CENTER_BOTTOM,
                        format!("×{}", l.count),
                        font(theme::FONT_SMALL * s, true),
                        color,
                    );
                }
                drawn_links += 1;
                if drawn_links >= 1200 {
                    // Same honesty rule as the other canvases: cap, clearly.
                    painter.text(
                        origin + Vec2::new(8.0, lay.height * s + 20.0),
                        Align2::LEFT_TOP,
                        "link cap reached — showing the first 1200 import links".to_owned(),
                        font(theme::FONT_SMALL, false),
                        theme::WARNING,
                    );
                    break;
                }
            }

            // Cards on top: module-coloured border, name head, real symbol
            // rows, size fillers, LOC/lang footer.
            for c in &lay.cards {
                let crect = placed[&c.id.as_str()];
                let corner = egui::CornerRadius::same(theme::CORNER_MD);
                let group_color = theme::module_color(&c.group);
                let lit = focus_now.is_none() || is_lit(&c.id);
                let selected_now = selected.as_deref() == Some(c.id.as_str())
                    || hovered.as_deref() == Some(c.id.as_str());

                let fill = if selected_now {
                    theme::SURFACE_SELECTED
                } else {
                    theme::SURFACE_E2
                };
                let fill = if focus_now.is_some() && !lit {
                    theme::dim_alpha(34)
                } else {
                    fill
                };
                painter.rect_filled(crect, corner, fill);
                let (border, bw) = if selected_now || (focus_now.is_some() && lit) {
                    (theme::ACCENT, 2.0)
                } else if focus_now.is_some() {
                    (theme::dim_alpha(40), 1.0)
                } else {
                    (group_color, 1.5)
                };
                painter.rect_stroke(
                    crect,
                    corner,
                    Stroke::new(bw, border),
                    egui::StrokeKind::Middle,
                );

                let pad = theme::SPACE_2 * s;
                let mut ty = crect.min.y + 3.0 * s;
                // The head: the real file name (correct name, no ids), stars
                // for entrypoints.
                painter.text(
                    Pos2::new(crect.min.x + pad, ty),
                    Align2::LEFT_TOP,
                    elide(
                        &format!("{}{}", c.file_name(), if c.entry { " ★" } else { "" }),
                        24,
                    ),
                    font(theme::FONT_BODY * s, true),
                    if focus_now.is_some() && !lit {
                        theme::TEXT_MUTED
                    } else {
                        theme::TEXT_PRIMARY
                    },
                );
                ty += HEAD_H.max(15.0) * s.min(1.0) + 1.0 * s;

                // Body rows: the file's real symbols (functions/types).
                let text_color = |c0: Color32| {
                    if focus_now.is_some() && !lit {
                        theme::TEXT_DISABLED
                    } else {
                        c0
                    }
                };
                for (i, name) in c.symbols.iter().enumerate() {
                    painter.text(
                        Pos2::new(crect.min.x + pad, ty),
                        Align2::LEFT_TOP,
                        elide(name, 18),
                        font(theme::FONT_SMALL * s, true),
                        text_color(theme::TEXT_SECONDARY),
                    );
                    ty += ROW_H * s;
                    if i + 1 >= SYM_ROWS {
                        break;
                    }
                }
                // Filler rows: a stable fraction of the real line count.
                for i in 0..filler_rows(c.lines) {
                    let w = (CARD_W - 2.0 * pad) * (0.35 + 0.25 * (i % 3) as f32);
                    painter.rect_filled(
                        Rect::from_min_size(
                            Pos2::new(crect.min.x + pad, ty + 2.0 * s),
                            Vec2::new(w.max(pad), 3.5 * s),
                        ),
                        egui::CornerRadius::same(2),
                        text_color(theme::dim_alpha(70)),
                    );
                    ty += ROW_H * s;
                }

                // Footer: honest numbers.
                painter.text(
                    Pos2::new(crect.max.x - pad, crect.max.y - 3.0 * s),
                    Align2::RIGHT_BOTTOM,
                    format!("{} LOC · {}", c.lines, c.lang),
                    font(theme::FONT_SMALL * s, true),
                    text_color(theme::TEXT_DISABLED),
                );
            }

            // Click a card → shared selection (opens the real file). Header
            // ids ("hdr:…") never match a card id, so they select nothing.
            if let Some(id) = actions.clicked {
                if lay.cards.iter().any(|c| c.id == id) {
                    app.select(Selection::File(id));
                }
            }

            // Plain-language answers: hover a card or a module header.
            if !state.is_dragging() {
                if let Some(pos) = resp.hover_pos() {
                    let tag = match hovered.as_deref() {
                        Some(h) if h.starts_with("hdr:") => {
                            let group = h.trim_start_matches("hdr:");
                            Some((
                                format!("module {group}"),
                                vec![
                                    "drag it — or any of its file cards — to move".to_owned(),
                                    "the whole column together, name included".to_owned(),
                                ],
                            ))
                        }
                        Some(h) => lay.cards.iter().find(|c| c.id == *h).map(|c| {
                            (
                                c.id.clone(),
                                vec![
                                    format!(
                                        "imports {} file(s) · imported by {} file(s)",
                                        card_out_count(h, &link_list),
                                        card_in_count(h, &link_list)
                                    ),
                                    format!(
                                        "{} lines · {}{} · mini-view rows: its {} symbol(s)",
                                        c.lines,
                                        c.lang,
                                        if c.entry { " · ★ entrypoint" } else { "" },
                                        c.symbols.len()
                                    ),
                                    "solid arrow = imports · hover lights both directions"
                                        .to_owned(),
                                    if deep {
                                        "deep: the whole reachable import chain is lit".to_owned()
                                    } else {
                                        "deep checkbox lights the whole transitive chain".to_owned()
                                    },
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

            state.store(ui.ctx(), egui::Id::new(STATE_ID));
        });
}

/// Direct out-degree (imports made by this file).
fn card_out_count(id: &str, links: &[FileLink]) -> usize {
    links.iter().filter(|l| l.from == id).count()
}

/// Direct in-degree (files that import this one).
fn card_in_count(id: &str, links: &[FileLink]) -> usize {
    links.iter().filter(|l| l.to == id).count()
}

/// The header strip: counts, cap honesty, the Deep toggle, Reset, hints.
fn header(
    ui: &mut egui::Ui,
    lay: &FilesLayout,
    links: &[FileLink],
    reset: &mut bool,
    deep: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "{} file(s) · {} import link(s) — one per file pair",
                lay.total,
                links.len()
            ))
            .size(theme::FONT_SMALL)
            .color(theme::TEXT_SECONDARY),
        );
        if lay.shown < lay.total {
            ui.label(
                egui::RichText::new(format!(
                    "showing first {} (cap {CARD_CAP} cards)",
                    lay.shown
                ))
                .size(theme::FONT_SMALL)
                .color(theme::WARNING),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("Reset")
                .on_hover_text(
                    "put every card back on the deterministic layout \
                     (double-click on empty canvas does the same)",
                )
                .clicked()
            {
                *reset = true;
            }
            ui.checkbox(deep, "deep").on_hover_text(
                "hover a card and light its WHOLE import chain both ways \
                     (transitive affected set), not just direct neighbours",
            );
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(
                        "what you see: one card per real file — the name in its head, \
                         its actual symbols below, its line count; arrows = A imports B",
                    )
                    .size(theme::FONT_SMALL)
                    .color(theme::TEXT_MUTED),
                );
                ui.label(
                    egui::RichText::new(
                        "gestures: hover to light the link set · wheel zoom · drag empty \
                         space pans · drag a card or its module name to move the module \
                         · click opens the file",
                    )
                    .size(theme::FONT_SMALL)
                    .color(theme::TEXT_MUTED),
                );
            });
        });
    });
    theme::separator(ui);
}

/// Centered empty-state card (the same local multi-line variant the other
/// diagram panels use — this view has landed; only data is thin).
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

// ── Tests (pure math; no UI, no window) ─────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn fe(path: &str, lines: usize, entry: bool) -> FileEntry {
        FileEntry {
            path: path.to_owned(),
            lang: "Rust".to_owned(),
            lines,
            mtime: 0,
            is_entrypoint: entry,
        }
    }

    fn idx_with_files_and_imports(files: &[FileEntry], imports: &[(String, String)]) -> Index {
        let mut idx = Index::new(String::new());
        idx.files = files.to_vec();
        idx.imports = imports
            .iter()
            .map(|(file, target)| repowalk_core::model::Import {
                file: file.clone(),
                target: target.clone(),
            })
            .collect();
        idx
    }

    #[test]
    fn links_aggregate_per_pair_skip_self_and_external() {
        let idx = idx_with_files_and_imports(
            &[
                fe("src/a.rs", 10, false),
                fe("src/b.rs", 10, false),
                fe("c.rs", 10, false),
            ],
            &[
                ("src/a.rs".to_owned(), "src/b.rs".to_owned()),
                ("src/a.rs".to_owned(), "src/b.rs".to_owned()),
                ("src/a.rs".to_owned(), "std/collections".to_owned()),
                ("src/b.rs".to_owned(), "src/b.rs".to_owned()),
                ("src/b.rs".to_owned(), "c.rs".to_owned()),
            ],
        );
        let got = links(&idx);
        assert_eq!(
            got,
            vec![
                FileLink {
                    from: "src/a.rs".to_owned(),
                    to: "src/b.rs".to_owned(),
                    count: 2
                },
                FileLink {
                    from: "src/b.rs".to_owned(),
                    to: "c.rs".to_owned(),
                    count: 1
                },
            ],
            "aggregated per ordered pair; self and non-indexed targets dropped"
        );
    }

    #[test]
    fn layout_is_deterministic_and_columns_per_module_with_real_row_caps() {
        let files = [
            fe("src/b.rs", 120, false),
            fe("src/a.rs", 10, true),
            fe("c.rs", 30, false),
        ];
        let mut symbols = BTreeMap::new();
        symbols.insert(
            "src/a.rs".to_owned(),
            vec!["one".to_owned(), "two".to_owned(), "three".to_owned()],
        );
        symbols.insert(
            "src/b.rs".to_owned(),
            (0..9).map(|i| format!("s{i}")).collect::<Vec<_>>(),
        );

        let first = layout(&files, &symbols);
        let second = layout(&files, &symbols);
        assert_eq!(first, second, "same input must lay out identically");
        // Groups sorted, then path-sorted inside; rows capped at SYM_ROWS.
        assert_eq!(first.headers[0].0, "(root)");
        assert_eq!(first.headers[1].0, "src");
        let root_card = first
            .cards
            .iter()
            .find(|c| c.id == "c.rs")
            .expect("root card");
        assert_eq!(root_card.group, "(root)");
        let b_card = first
            .cards
            .iter()
            .find(|c| c.id == "src/b.rs")
            .expect("b card");
        assert_eq!(
            b_card.symbols.len(),
            SYM_ROWS,
            "rows are capped, not invented"
        );
        assert_eq!(b_card.symbols[0], "s0", "line order — the real symbols");
        assert_eq!(
            first.total, first.shown,
            "below the cap every file is drawn"
        );

        // Above the cap the honesty banner has a real shown/total split.
        let many: Vec<FileEntry> = (0..CARD_CAP + 5)
            .map(|i| fe(&format!("f{i}.rs"), 1, false))
            .collect();
        let capped = layout(&many, &BTreeMap::new());
        assert_eq!(capped.total, CARD_CAP + 5);
        assert_eq!(capped.shown, CARD_CAP);
    }

    #[test]
    fn focus_is_direct_both_directions_and_deep_is_transitive() {
        let links = vec![
            FileLink {
                from: "a.rs".to_owned(),
                to: "b.rs".to_owned(),
                count: 1,
            },
            FileLink {
                from: "c.rs".to_owned(),
                to: "a.rs".to_owned(),
                count: 1,
            },
            FileLink {
                from: "c.rs".to_owned(),
                to: "d.rs".to_owned(),
                count: 1,
            },
        ];
        // Direct hover on a.rs: a, b (it imports), c (imports it); d stays dim.
        let direct = focus_sets(Some("a.rs"), &links, false);
        assert_eq!(direct.out, 1);
        assert_eq!(direct.direct_in, 1);
        assert_eq!(
            direct.lit,
            BTreeSet::from(["a.rs".to_owned(), "b.rs".to_owned(), "c.rs".to_owned()])
        );
        assert_eq!(direct.lit_links.len(), 2);
        // Deep: the whole chain — through c also d becomes reachable.
        let deep = focus_sets(Some("a.rs"), &links, true);
        assert_eq!(
            deep.lit,
            BTreeSet::from([
                "a.rs".to_owned(),
                "b.rs".to_owned(),
                "c.rs".to_owned(),
                "d.rs".to_owned()
            ])
        );
        assert_eq!(deep.lit_links.len(), links.len(), "every chain link lights");
        // No focus = nothing lit, nothing dim.
        assert!(focus_sets(None, &links, true).lit.is_empty());
        // Untouched focus = only itself.
        assert_eq!(
            focus_sets(Some("x.rs"), &links, true).lit,
            BTreeSet::from(["x.rs".to_owned()])
        );
    }
}
