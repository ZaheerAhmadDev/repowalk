//! Flow mode: the workflow of the code as a dynamic storyboard — an animated
//! A→B call trace and an entrypoint wave that fills in layer by layer.
//! Composes sub-tabs, transport, canvas, and the step-detail panel over
//! `repowalk_core::flow::{trace, waves}` (CORE-1). OWNER: TL-GRAPH (GRAPH-1).
pub mod feature;
pub mod playback;
pub mod render;

use eframe::egui;
use repowalk_core::model::{Confidence, EdgeKind, Index, SymbolKind};
use repowalk_core::search;

use crate::app::{App, Selection, TracePick};
use crate::sidebar::{data_row, Meta};
use crate::theme;

use playback::Playback;

/// Wave expansion rounds beyond the entrypoint (layer count = this + 1).
const WAVE_MAX_DEPTH: usize = 3;
/// Feature slices start three call hops below the entry.
const FEATURE_DEFAULT_DEPTH: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlowTab {
    #[default]
    Trace,
    Waves,
    Feature,
}

/// Flow-mode state owned by `App`.
pub struct FlowState {
    pub tab: FlowTab,
    /// Trace endpoints — seeded from `app.trace` (the graph trace picker).
    pub trace_a: String,
    pub trace_b: String,
    /// Wave entrypoint — seeded from the current symbol selection.
    pub wave_source: String,
    /// Feature-tab entrypoint symbol id. Never seeded from the selection:
    /// clicking a slice step selects a symbol, which must not re-slice the view.
    pub feature_entry: String,
    /// Feature call-tree depth cap (hops below the entry).
    pub feature_depth: usize,
    /// Fuzzy query for the Feature "any symbol" entry search.
    pub feature_query: String,
    /// Feature coverage scope: `true` = whole index, `false` = the current slice.
    pub feature_scope_all: bool,
    pub playback: Playback,
    /// Which endpoint the sidebar picker is waiting for.
    pub pick: TracePick,
    /// Fuzzy query for the armed endpoint slot.
    pub pick_query: String,
    /// Whether the Waves tab's entrypoint search is open (the entrypoint is
    /// set from the Flow view itself, not from a trip through Search).
    pub wave_pick: bool,
    /// Explicit detail pick (card/chip click); falls back to the current step.
    pub detail: Option<String>,
    /// Last values adopted from the app, so seeding rewinds only on change.
    seeded_trace: Option<(String, String)>,
    seeded_symbol: Option<String>,
}

impl Default for FlowState {
    fn default() -> Self {
        Self {
            tab: FlowTab::default(),
            trace_a: String::new(),
            trace_b: String::new(),
            wave_source: String::new(),
            feature_entry: String::new(),
            feature_depth: FEATURE_DEFAULT_DEPTH,
            feature_query: String::new(),
            feature_scope_all: true,
            playback: Playback::default(),
            pick: TracePick::default(),
            pick_query: String::new(),
            wave_pick: false,
            detail: None,
            seeded_trace: None,
            seeded_symbol: None,
        }
    }
}

impl FlowState {
    /// Adopt externally picked endpoints the way the graph trace picker seeds
    /// itself: `app.trace` drives A→B, the selected symbol seeds the wave
    /// source. A change rewinds the playhead to the start.
    fn sync(&mut self, trace: &Option<(String, String)>, selection: Option<&Selection>) {
        if self.seeded_trace.as_ref() != trace.as_ref() {
            match trace {
                Some((a, b)) => {
                    self.trace_a = a.clone();
                    self.trace_b = b.clone();
                }
                None => {
                    self.trace_a.clear();
                    self.trace_b.clear();
                }
            }
            self.seeded_trace = trace.clone();
            self.detail = None;
            self.playback.playhead = 0.0;
        }
        if let Some(Selection::Symbol(id)) = selection {
            if self.seeded_symbol.as_deref() != Some(id.as_str()) {
                self.wave_source = id.clone();
                self.seeded_symbol = Some(id.clone());
                // Rewind the playhead ONLY for the Waves tab (this seed
                // serves wave slicing). Clicking a Feature step row also
                // selects a symbol — rewinding here used to snap that tab's
                // playhead back to step 1 right after you clicked it.
                if self.tab == FlowTab::Waves {
                    self.playback.playhead = 0.0;
                }
            }
        }
    }
}

/// The step sequence for the active sub-tab: one step per trace symbol, one per
/// wave layer, or the ordered core feature slice. The empty variants are the
/// honest "why not" states.
pub enum Sequence {
    /// No A or B picked (or no wave source / feature entry) yet.
    PickEndpoints,
    /// A and B picked but BFS found no call path between them.
    NoPath,
    /// Wave source picked but it is not an endpoint of any call edge.
    UnknownSource,
    /// Feature entry set but it is not a symbol declared in this index
    /// (`feature::slice` returns `None` for exactly this case).
    UnknownFeatureEntry,
    Path(Vec<String>),
    Waves(Vec<Vec<String>>),
    /// Ordered feature slice (`repowalk_core::feature::slice`).
    Feature(repowalk_core::feature::FeatureSlice),
}

impl Sequence {
    /// Playback length: symbols in a trace, layers in a wave, steps in a slice.
    pub fn step_count(&self) -> usize {
        match self {
            Sequence::Path(p) => p.len(),
            Sequence::Waves(l) => l.len(),
            Sequence::Feature(s) => s.steps.len(),
            Sequence::PickEndpoints
            | Sequence::NoPath
            | Sequence::UnknownSource
            | Sequence::UnknownFeatureEntry => 0,
        }
    }

    /// Symbol shown in the detail panel when the user hasn't clicked one.
    pub fn default_detail(&self, step: usize) -> Option<String> {
        match self {
            Sequence::Path(p) => p.get(step).cloned(),
            // Waves: the entrypoint is the anchor; chips inspect any member.
            Sequence::Waves(l) => l.first().and_then(|src| src.first()).cloned(),
            Sequence::Feature(s) => s.steps.get(step).map(|st| st.symbol.clone()),
            Sequence::PickEndpoints
            | Sequence::NoPath
            | Sequence::UnknownSource
            | Sequence::UnknownFeatureEntry => None,
        }
    }
}

/// Resolve the active tab's picks into a playback sequence via the shared
/// core primitives.
pub fn sequence(idx: &Index, flow: &FlowState) -> Sequence {
    match flow.tab {
        FlowTab::Trace => {
            if flow.trace_a.is_empty() || flow.trace_b.is_empty() {
                return Sequence::PickEndpoints;
            }
            match repowalk_core::flow::trace(idx, &flow.trace_a, &flow.trace_b) {
                Some(path) => Sequence::Path(path),
                None => Sequence::NoPath,
            }
        }
        FlowTab::Waves => {
            if flow.wave_source.is_empty() {
                return Sequence::PickEndpoints;
            }
            let layers = repowalk_core::flow::waves(idx, &flow.wave_source, WAVE_MAX_DEPTH);
            if layers.is_empty() {
                Sequence::UnknownSource
            } else {
                Sequence::Waves(layers)
            }
        }
        FlowTab::Feature => {
            if flow.feature_entry.is_empty() {
                return Sequence::PickEndpoints;
            }
            match repowalk_core::feature::slice(idx, &flow.feature_entry, flow.feature_depth) {
                Some(slice) => Sequence::Feature(slice),
                None => Sequence::UnknownFeatureEntry,
            }
        }
    }
}

/// Central-pane entry: sub-tabs → canvas → transport → step detail.
pub fn ui(ctx: &egui::Context, ui: &mut egui::Ui, app: &mut App) {
    let Some(idx) = app.index.clone() else {
        hint(ui, "open a repository to watch its code flow");
        return;
    };
    app.flow.sync(&app.trace, app.selection.as_ref());

    sub_tabs(ui, &mut app.flow);
    ui.add_space(theme::SPACE_2);
    // The chooser lives where the empty canvas is, not in a sidebar section
    // the reader has to find first: the old hint pointed at a "sidebar Flow
    // panel" that only exists nested inside the Map activity.
    match app.flow.tab {
        FlowTab::Trace => endpoints_card(ui, app, &idx),
        FlowTab::Waves => waves_card(ui, app, &idx),
        FlowTab::Feature => feature::picker(ui, &mut app.flow, &idx),
    }
    ui.add_space(theme::SPACE_2);

    let seq = sequence(&idx, &app.flow);
    let steps = seq.step_count();

    app.flow.playback.advance(ctx, steps);
    app.flow.playback.request_repaint_while_playing(ctx, steps);
    render::transport(ui, &mut app.flow, steps);
    ui.add_space(theme::SPACE_2);

    let (step, progress) = playback::step_pos(app.flow.playback.playhead, steps);
    // Frozen-at-progress while paused; looping PULSE_MS phase while playing.
    let pulse = if app.flow.playback.playing {
        playback::pulse_phase(ctx.input(|i| i.time))
    } else {
        theme::ease_in_out(progress)
    };

    // Canvas: the animated stage strip, or the wave fan filling in per layer.
    match &seq {
        Sequence::Path(path) => {
            if let Some(i) =
                render::stage_strip(ui, &idx, path, step, progress, pulse, app.flow.tab)
            {
                app.flow.playback.goto_step(i, steps);
                app.flow.detail = path.get(i).cloned();
            }
        }
        Sequence::Waves(layers) => {
            if let Some((layer, id)) =
                render::wave_view(ui, &idx, layers, step, progress, app.flow.tab)
            {
                app.flow.playback.goto_step(layer, steps);
                app.flow.detail = Some(id);
            }
        }
        Sequence::Feature(slice) => {
            if let Some(pick) = feature::ui(
                ui,
                &mut app.flow,
                &idx,
                slice,
                app.selection.as_ref(),
                step,
                progress,
            ) {
                match pick {
                    feature::FeaturePick::Step(i, id) => {
                        app.flow.playback.goto_step(i, steps);
                        app.flow.detail = Some(id.clone());
                        // In-view selection: clicking a slice step moves the
                        // playhead and highlights the step, it must not open
                        // a Text tab and hide the flow being read.
                        app.select_in_view(Selection::Symbol(id));
                    }
                    feature::FeaturePick::Symbol(id) => {
                        app.flow.detail = Some(id.clone());
                        app.select_in_view(Selection::Symbol(id));
                    }
                }
            }
        }
        Sequence::PickEndpoints => match app.flow.tab {
            FlowTab::Trace => hint(
                ui,
                "Pick a start (A) and an end (B) in the trace card above to draw the call path.",
            ),
            FlowTab::Waves => hint(
                ui,
                "Pick an entrypoint in the card above; the waves fan out from it.",
            ),
            FlowTab::Feature => hint(ui, "Search for a feature entrypoint in the picker above."),
        },
        Sequence::NoPath => hint(
            ui,
            format!(
                "no call path from {} to {} — try swap in the trace card above",
                render::display_name(&idx, &app.flow.trace_a),
                render::display_name(&idx, &app.flow.trace_b)
            ),
        ),
        Sequence::UnknownSource => hint(
            ui,
            format!(
                "unknown symbol — “{}” is not an endpoint of any call edge",
                render::display_name(&idx, &app.flow.wave_source)
            ),
        ),
        Sequence::UnknownFeatureEntry => hint(
            ui,
            format!(
                "unknown entry — “{}” is not a symbol in this index",
                render::display_name(&idx, &app.flow.feature_entry)
            ),
        ),
    }

    // Step-detail panel for the current (or clicked) step.
    let detail = app.flow.detail.clone().or_else(|| seq.default_detail(step));
    if let Some(id) = detail {
        let incoming = incoming_edge(&idx, &seq, step, &id);
        theme::separator(ui);
        if let Some(open_id) = render::step_detail(ui, &idx, &id, incoming) {
            open_source(app, &idx, &open_id);
        }
    }
}

/// The Trace tab's endpoint chooser: two labelled slots with their own fuzzy
/// search, right where the empty canvas is. Picking A moves straight on to B
/// so the two-step gesture is visible instead of hidden behind a sidebar.
fn endpoints_card(ui: &mut egui::Ui, app: &mut App, idx: &Index) {
    let both = !app.flow.trace_a.is_empty() && !app.flow.trace_b.is_empty();
    egui::Frame::new()
        .fill(theme::SURFACE_E1)
        .stroke(egui::Stroke::new(1.0, theme::BORDER_SUBTLE))
        .corner_radius(theme::CORNER_MD)
        .inner_margin(egui::Margin::symmetric(
            theme::SPACE_3 as i8,
            theme::SPACE_2 as i8,
        ))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Trace a call path")
                        .strong()
                        .color(theme::TEXT_PRIMARY),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if both
                        && ui
                            .small_button("swap")
                            .on_hover_text("Swap the start and end functions")
                            .clicked()
                    {
                        std::mem::swap(&mut app.flow.trace_a, &mut app.flow.trace_b);
                        app.trace = Some((
                            app.flow.trace_a.clone(),
                            app.flow.trace_b.clone(),
                        ));
                        app.flow.detail = None;
                        app.flow.playback.playhead = 0.0;
                    }
                });
            });
            ui.label(
                egui::RichText::new(
                    "Choose where to start (A) and where to end (B) — the storyboard follows the shortest call path between them.",
                )
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
            );
            ui.add_space(theme::SPACE_1);
            endpoint_row(ui, app, idx, "A", "Start (A)", TracePick::PickA);
            endpoint_row(ui, app, idx, "B", "End (B)", TracePick::PickB);
            if !matches!(app.flow.pick, TracePick::None) {
                ui.add_space(theme::SPACE_1);
                let slot = if app.flow.pick == TracePick::PickA {
                    "A"
                } else {
                    "B"
                };
                let what = if slot == "A" { "start" } else { "end" };
                picker_search(
                    ui,
                    app,
                    idx,
                    slot,
                    format!("search functions for the {what} ({slot})…"),
                    assign_endpoint,
                );
            }
            if both {
                ui.label(
                    egui::RichText::new(
                        "Shortest path ready — Space plays it; the left/right arrow keys step through it.",
                    )
                    .size(theme::FONT_SMALL)
                    .color(theme::TEXT_MUTED),
                );
            }
        });
}

/// One labelled endpoint slot: current function or "not set", plus a button
/// that arms the fuzzy search below it.
fn endpoint_row(
    ui: &mut egui::Ui,
    app: &mut App,
    idx: &Index,
    slot: &str,
    label: &str,
    pick: TracePick,
) {
    let armed = app.flow.pick == pick;
    let value = if slot == "A" {
        app.flow.trace_a.clone()
    } else {
        app.flow.trace_b.clone()
    };
    ui.horizontal(|ui| {
        ui.add_sized(
            [74.0, 20.0],
            egui::Label::new(
                egui::RichText::new(label)
                    .size(theme::FONT_SMALL)
                    .color(if armed {
                        theme::ACCENT
                    } else {
                        theme::TEXT_MUTED
                    }),
            ),
        );
        if value.is_empty() {
            ui.label(
                egui::RichText::new("not set")
                    .size(theme::FONT_SMALL)
                    .color(theme::TEXT_MUTED),
            );
        } else {
            ui.label(egui::RichText::new(render::display_name(idx, &value)).strong());
            ui.label(
                egui::RichText::new(render::file_of(&value))
                    .size(theme::FONT_SMALL)
                    .color(theme::TEXT_MUTED),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let text = if armed {
                "cancel"
            } else if value.is_empty() {
                "choose…"
            } else {
                "change"
            };
            if ui.small_button(text).clicked() {
                app.flow.pick = if armed { TracePick::None } else { pick };
                app.flow.pick_query.clear();
            }
        });
    });
}

/// The Waves tab's entrypoint chooser. Selecting here keeps the reader in
/// Flow; the old hint sent them to Search, whose rows navigate to Text.
fn waves_card(ui: &mut egui::Ui, app: &mut App, idx: &Index) {
    let value = app.flow.wave_source.clone();
    let show_search = app.flow.wave_pick || value.is_empty();
    egui::Frame::new()
        .fill(theme::SURFACE_E1)
        .stroke(egui::Stroke::new(1.0, theme::BORDER_SUBTLE))
        .corner_radius(theme::CORNER_MD)
        .inner_margin(egui::Margin::symmetric(
            theme::SPACE_3 as i8,
            theme::SPACE_2 as i8,
        ))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new("Entrypoint waves")
                    .strong()
                    .color(theme::TEXT_PRIMARY),
            );
            ui.label(
                egui::RichText::new(
                    "Pick one function; each ring is one call hop outward from it.",
                )
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
            );
            ui.add_space(theme::SPACE_1);
            ui.horizontal(|ui| {
                ui.add_sized(
                    [74.0, 20.0],
                    egui::Label::new(
                        egui::RichText::new("Entrypoint")
                            .size(theme::FONT_SMALL)
                            .color(theme::TEXT_MUTED),
                    ),
                );
                if value.is_empty() {
                    ui.label(
                        egui::RichText::new("not set")
                            .size(theme::FONT_SMALL)
                            .color(theme::TEXT_MUTED),
                    );
                } else {
                    ui.label(egui::RichText::new(render::display_name(idx, &value)).strong());
                    ui.label(
                        egui::RichText::new(render::file_of(&value))
                            .size(theme::FONT_SMALL)
                            .color(theme::TEXT_MUTED),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let text = if app.flow.wave_pick {
                        "cancel"
                    } else if value.is_empty() {
                        "choose…"
                    } else {
                        "change"
                    };
                    if ui.small_button(text).clicked() {
                        app.flow.wave_pick = !app.flow.wave_pick;
                        app.flow.pick_query.clear();
                    }
                });
            });
            if show_search {
                ui.add_space(theme::SPACE_1);
                picker_search(
                    ui,
                    app,
                    idx,
                    "wave",
                    "search functions for the entrypoint…".to_string(),
                    assign_wave,
                );
            }
        });
}

/// What the endpoint pickers offer: the first functions for an empty query
/// (a freshly opened picker must not read as "no matches"), fuzzy hits
/// otherwise. Shared by the in-canvas Flow cards and the sidebar mirror.
pub(crate) fn picker_hits(idx: &Index, query: &str, limit: usize) -> Vec<(String, String)> {
    if query.trim().is_empty() {
        idx.symbols
            .iter()
            .filter(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
            .take(limit)
            .map(|s| (s.id.clone(), s.file.clone()))
            .collect()
    } else {
        search::search(idx, query, limit)
            .into_iter()
            .map(|h| (h.target, h.path))
            .collect()
    }
}

/// Shared fuzzy picker for both Flow cards: one text field, up to six hits,
/// clicking a hit hands its id to `on_pick`. Unique egui ids per `salt` keep
/// the two cards (and the sidebar picker) from fighting over focus.
fn picker_search(
    ui: &mut egui::Ui,
    app: &mut App,
    idx: &Index,
    salt: &str,
    hint: String,
    mut on_pick: impl FnMut(&mut App, String),
) {
    ui.add(
        egui::TextEdit::singleline(&mut app.flow.pick_query)
            .id(egui::Id::new(("flow-picker-query", salt)))
            .hint_text(hint)
            .desired_width(f32::INFINITY),
    );
    let hits = picker_hits(idx, &app.flow.pick_query, 6);
    if hits.is_empty() {
        let message = if app.flow.pick_query.trim().is_empty() {
            "no functions in this repository"
        } else {
            "no functions match — try a shorter query"
        };
        ui.label(
            egui::RichText::new(message)
                .size(theme::FONT_SMALL)
                .color(theme::TEXT_MUTED),
        );
        return;
    }
    // Cap the picker at a few rows: the Flow canvas below still needs room,
    // especially on the small windows the shell has to survive.
    egui::ScrollArea::vertical()
        .id_salt(("flow-picker-hits", salt))
        .max_height(theme::ROW_HEIGHT * 3.5)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for (id, path) in &hits {
                let name = idx
                    .symbols
                    .iter()
                    .find(|s| s.id == *id)
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| id.clone());
                let row = data_row(
                    ui,
                    false,
                    ("flow-picker-hit", salt, id),
                    name,
                    Some(Meta::muted(path)),
                );
                if row.clicked() {
                    on_pick(app, id.clone());
                }
            }
        });
}

/// Fill the armed trace slot. Picking A when B is still empty moves straight
/// on to B — the two-step gesture is guided instead of left implicit. Also
/// used by the sidebar Flow panel so both pickers behave identically.
pub(crate) fn assign_endpoint(app: &mut App, id: String) {
    match app.flow.pick {
        TracePick::PickA => {
            app.flow.trace_a = id;
            app.flow.pick = if app.flow.trace_b.is_empty() {
                TracePick::PickB
            } else {
                TracePick::None
            };
        }
        TracePick::PickB => {
            app.flow.trace_b = id;
            app.flow.pick = TracePick::None;
        }
        TracePick::None => return,
    }
    app.flow.pick_query.clear();
    app.flow.detail = None;
    app.flow.playback.playhead = 0.0;
    app.flow.playback.playing = false;
    if !app.flow.trace_a.is_empty() && !app.flow.trace_b.is_empty() {
        app.trace = Some((app.flow.trace_a.clone(), app.flow.trace_b.clone()));
    }
}

/// Set the Waves entrypoint without leaving Flow.
fn assign_wave(app: &mut App, id: String) {
    app.flow.wave_source = id;
    app.flow.wave_pick = false;
    app.flow.pick_query.clear();
    app.flow.detail = None;
    app.flow.playback.playhead = 0.0;
    app.flow.playback.playing = false;
}

/// Confidence of the edge feeding the current step — the previous trace hop, a
/// call from the previous wave layer, or a call from the feature step's parent
/// (the deepest earlier step shallower than it). `None` on step/layer 0.
fn incoming_edge(
    idx: &Index,
    seq: &Sequence,
    step: usize,
    id: &str,
) -> Option<(String, Confidence)> {
    match seq {
        Sequence::Path(p) => {
            let caller = p.get(step.checked_sub(1)?)?;
            find_call(idx, id, std::slice::from_ref(caller))
        }
        Sequence::Waves(l) => find_call(idx, id, l.get(step.checked_sub(1)?)?),
        Sequence::Feature(s) => {
            let cur = s.steps.get(step)?;
            let parent = s
                .steps
                .iter()
                .take(step)
                .rev()
                .find(|st| st.depth < cur.depth)?;
            find_call(idx, id, std::slice::from_ref(&parent.symbol))
        }
        Sequence::PickEndpoints
        | Sequence::NoPath
        | Sequence::UnknownSource
        | Sequence::UnknownFeatureEntry => None,
    }
}

/// First `EdgeKind::Call` edge into `callee` from any of `callers`, with the
/// caller's display name and the edge confidence. A name-only caller key that
/// does not match a symbol id simply yields `None` — no silent guess.
fn find_call(idx: &Index, callee: &str, callers: &[String]) -> Option<(String, Confidence)> {
    idx.edges
        .iter()
        .find(|e| {
            e.kind == EdgeKind::Call && e.callee == callee && callers.iter().any(|c| c == &e.caller)
        })
        .map(|e| (render::display_name(idx, &e.caller), e.confidence))
}

/// Resolve a symbol id to file:line and jump the text pane there. The button
/// promises "open source", so it must actually leave Flow and land on the
/// line — `open_file_with` alone loaded the buffer invisibly behind the flow
/// canvas and looked like a dead control.
fn open_source(app: &mut App, idx: &Index, id: &str) {
    let (file, line) = match idx.symbols.iter().find(|s| s.id == id) {
        Some(sym) => (sym.file.clone(), Some(sym.start_line)),
        None => (render::file_of(id).to_string(), Some(render::line_of(id))),
    };
    app.goto_file(file, line, Some(Selection::Symbol(id.to_string())));
}

/// Sub-tab strip: A→B trace / entrypoint waves / feature slice. Clicking a tab
/// rewinds.
fn sub_tabs(ui: &mut egui::Ui, flow: &mut FlowState) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_2;
        if ui
            .selectable_value(&mut flow.tab, FlowTab::Trace, "A to B trace")
            .clicked()
        {
            flow.playback.playhead = 0.0;
            // Each sub-tab replays a different-length sequence: switching
            // tabs rewinds AND pauses (the new tab otherwise auto-plays from
            // the old tab's timeline).
            flow.playback.playing = false;
            flow.pick = TracePick::None;
            flow.pick_query.clear();
            flow.wave_pick = false;
        }
        if ui
            .selectable_value(&mut flow.tab, FlowTab::Waves, "Entrypoint waves")
            .clicked()
        {
            flow.playback.playhead = 0.0;
            flow.playback.playing = false;
            flow.pick = TracePick::None;
            flow.pick_query.clear();
            flow.wave_pick = false;
        }
        if ui
            .selectable_value(&mut flow.tab, FlowTab::Feature, "Feature")
            .on_hover_text("Pick an entrypoint and read what it calls, depth by depth")
            .clicked()
        {
            flow.playback.playhead = 0.0;
            flow.playback.playing = false;
            flow.pick = TracePick::None;
            flow.pick_query.clear();
            flow.wave_pick = false;
        }
    });
}

/// Centered muted line for the honest empty/error states.
fn hint(ui: &mut egui::Ui, text: impl AsRef<str>) {
    ui.add_space(theme::SPACE_6);
    ui.vertical_centered(|ui| {
        ui.label(
            egui::RichText::new(text.as_ref())
                .size(theme::FONT_BODY)
                .color(theme::TEXT_MUTED),
        );
    });
    ui.add_space(theme::SPACE_6);
}
