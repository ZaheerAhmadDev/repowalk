//! Flow mode: the workflow of the code as a dynamic storyboard — an animated
//! A→B call trace and an entrypoint wave that fills in layer by layer.
//! Composes sub-tabs, transport, canvas, and the step-detail panel over
//! `repowalk_core::flow::{trace, waves}` (CORE-1). OWNER: TL-GRAPH (GRAPH-1).
pub mod playback;
pub mod render;

use eframe::egui;
use repowalk_core::model::{Confidence, EdgeKind, Index};

use crate::app::{App, Selection, TracePick};
use crate::theme;

use playback::Playback;

/// Wave expansion rounds beyond the entrypoint (layer count = this + 1).
const WAVE_MAX_DEPTH: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlowTab {
    #[default]
    Trace,
    Waves,
}

/// Flow-mode state owned by `App`.
#[derive(Default)]
pub struct FlowState {
    pub tab: FlowTab,
    /// Trace endpoints — seeded from `app.trace` (the graph trace picker).
    pub trace_a: String,
    pub trace_b: String,
    /// Wave entrypoint — seeded from the current symbol selection.
    pub wave_source: String,
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
                self.playback.playhead = 0.0;
            }
        }
    }
}

/// The step sequence for the active sub-tab: one step per trace symbol, or
/// one per wave layer. The empty variants are the honest "why not" states.
pub enum Sequence {
    /// No A or B picked (or no wave source) yet.
    PickEndpoints,
    /// A and B picked but BFS found no call path between them.
    NoPath,
    /// Wave source picked but it is not an endpoint of any call edge.
    UnknownSource,
    Path(Vec<String>),
    Waves(Vec<Vec<String>>),
}

impl Sequence {
    /// Playback length: symbols in a trace, layers in a wave.
    pub fn step_count(&self) -> usize {
        match self {
            Sequence::Path(p) => p.len(),
            Sequence::Waves(l) => l.len(),
            Sequence::PickEndpoints | Sequence::NoPath | Sequence::UnknownSource => 0,
        }
    }

    /// Symbol shown in the detail panel when the user hasn't clicked one.
    pub fn default_detail(&self, step: usize) -> Option<String> {
        match self {
            Sequence::Path(p) => p.get(step).cloned(),
            // Waves: the entrypoint is the anchor; chips inspect any member.
            Sequence::Waves(l) => l.first().and_then(|src| src.first()).cloned(),
            Sequence::PickEndpoints | Sequence::NoPath | Sequence::UnknownSource => None,
        }
    }
}

/// Resolve the active tab's picks into a playback sequence via the shared
/// core BFS primitives.
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
            if let Some(i) = render::stage_strip(ui, &idx, path, step, progress, pulse) {
                app.flow.playback.goto_step(i, steps);
                app.flow.detail = path.get(i).cloned();
            }
        }
        Sequence::Waves(layers) => {
            if let Some((layer, id)) = render::wave_view(ui, &idx, layers, step, progress) {
                app.flow.playback.goto_step(layer, steps);
                app.flow.detail = Some(id);
            }
        }
        Sequence::PickEndpoints => match app.flow.tab {
            FlowTab::Trace => hint(ui, "pick A and B in the sidebar Flow panel"),
            FlowTab::Waves => hint(ui, "click a symbol in Search to set the entrypoint"),
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

/// Confidence of the edge feeding the current step — the previous trace hop,
/// or any call from the previous wave layer. `None` on step/layer 0.
fn incoming_edge(
    idx: &Index,
    seq: &Sequence,
    step: usize,
    id: &str,
) -> Option<(String, Confidence)> {
    let prev: &[String] = match seq {
        Sequence::Path(p) => std::slice::from_ref(p.get(step.checked_sub(1)?)?),
        Sequence::Waves(l) => l.get(step.checked_sub(1)?)?,
        Sequence::PickEndpoints | Sequence::NoPath | Sequence::UnknownSource => return None,
    };
    idx.edges
        .iter()
        .find(|e| e.kind == EdgeKind::Call && e.callee == id && prev.iter().any(|c| c == &e.caller))
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

/// Sub-tab strip: A→B trace / entrypoint waves. Clicking a tab rewinds.
fn sub_tabs(ui: &mut egui::Ui, flow: &mut FlowState) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE_2;
        if ui
            .selectable_value(&mut flow.tab, FlowTab::Trace, "A→B trace")
            .clicked()
        {
            flow.playback.playhead = 0.0;
        }
        if ui
            .selectable_value(&mut flow.tab, FlowTab::Waves, "Entrypoint waves")
            .clicked()
        {
            flow.playback.playhead = 0.0;
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
