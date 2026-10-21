//! Flow mode: the workflow of the code as a dynamic storyboard — an animated
//! A→B call trace and an entrypoint wave that fills in layer by layer.
//! Composes sub-tabs, transport, canvas, and the step-detail panel over
//! `repowalk_core::flow::{trace, waves}` (CORE-1). OWNER: TL-GRAPH (GRAPH-1).
pub mod feature;
pub mod playback;
pub mod render;

use eframe::egui;
use repowalk_core::model::{Confidence, EdgeKind, Index};

use crate::app::{App, Selection, TracePick};
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
    if app.flow.tab == FlowTab::Feature {
        feature::picker(ui, &mut app.flow, &idx);
        ui.add_space(theme::SPACE_2);
    }

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
                        app.select(Selection::Symbol(id));
                    }
                    feature::FeaturePick::Symbol(id) => {
                        app.flow.detail = Some(id.clone());
                        app.select(Selection::Symbol(id));
                    }
                }
            }
        }
        Sequence::PickEndpoints => match app.flow.tab {
            FlowTab::Trace => hint(ui, "pick A and B in the sidebar Flow panel"),
            FlowTab::Waves => hint(ui, "click a symbol in Search to set the entrypoint"),
            FlowTab::Feature => hint(ui, "pick a feature entrypoint above"),
        },
        Sequence::NoPath => hint(
            ui,
            format!(
                "no call path from {} to {}",
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

/// Resolve a symbol id to file:line and jump the text pane there.
fn open_source(app: &mut App, idx: &Index, id: &str) {
    let (file, line) = match idx.symbols.iter().find(|s| s.id == id) {
        Some(sym) => (sym.file.clone(), Some(sym.start_line)),
        None => (render::file_of(id).to_string(), Some(render::line_of(id))),
    };
    app.open_file_with(file, line);
}

/// Sub-tab strip: A→B trace / entrypoint waves / feature slice. Clicking a tab
/// rewinds.
fn sub_tabs(ui: &mut egui::Ui, flow: &mut FlowState) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_2;
        if ui
            .selectable_value(&mut flow.tab, FlowTab::Trace, "A→B trace")
            .clicked()
        {
            flow.playback.playhead = 0.0;
            // Each sub-tab replays a different-length sequence: switching
            // tabs rewinds AND pauses (the new tab otherwise auto-plays from
            // the old tab's timeline).
            flow.playback.playing = false;
        }
        if ui
            .selectable_value(&mut flow.tab, FlowTab::Waves, "Entrypoint waves")
            .clicked()
        {
            flow.playback.playhead = 0.0;
            flow.playback.playing = false;
        }
        if ui
            .selectable_value(&mut flow.tab, FlowTab::Feature, "Feature")
            .on_hover_text("Pick an entrypoint and read what it calls, depth by depth")
            .clicked()
        {
            flow.playback.playhead = 0.0;
            flow.playback.playing = false;
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
