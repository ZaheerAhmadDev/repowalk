//! Graph filters, clustering map, and persistent view state (F3, batch B3).
//!
//! Everything here is pure (no egui): the honest predicates the renderer and
//! tests share, plus the remembered filter/cluster state. That state lives in
//! egui temp memory (see [`crate::graph::render`]) so the view can keep its
//! filters across frames without widening `App`, which this task does not own.
use repowalk_core::model::{Confidence, EdgeKind};
use std::collections::BTreeSet;

/// Hard cap for legibility. Above this the graph is truncated (top fan-in kept)
/// and says so — it never silently drops nodes.
pub const NODE_CAP: usize = 400;

/// Confidence filter tiers surfaced in the legend.
///
/// `All` still draws every non-`Exact` edge **dashed** (honesty rule R-20), it
/// only decides whether they are shown at all; `ExactOnly` hides them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConfidenceFilter {
    #[default]
    All,
    ExactOnly,
}

impl ConfidenceFilter {
    /// Does an edge of this confidence survive the filter?
    pub fn keeps(self, confidence: Confidence) -> bool {
        match self {
            ConfidenceFilter::All => true,
            ConfidenceFilter::ExactOnly => matches!(confidence, Confidence::Exact),
        }
    }
}

/// R-20 honesty rule: anything short of `Exact` is drawn dashed with a `?`.
pub fn is_dashed(confidence: Confidence) -> bool {
    !matches!(confidence, Confidence::Exact)
}

/// Edge-kind toggle (call / import) driven by the legend.
pub fn kind_kept(kind: EdgeKind, calls: bool, imports: bool) -> bool {
    match kind {
        EdgeKind::Call => calls,
        EdgeKind::Import => imports,
    }
}

/// Cluster node id for `module`, or `None` when the module is expanded.
///
/// Deterministic: the id is derived only from the module name, so the same
/// collapse set always maps a module's members to the same super-node.
pub fn cluster_key(module: &str, collapsed: &BTreeSet<String>) -> Option<String> {
    collapsed
        .contains(module)
        .then(|| format!("cluster:{module}"))
}

/// Case-insensitive "does any field contain the query" for search-to-highlight.
/// An empty or whitespace-only query matches everything (nothing is dimmed).
pub fn matches_search(query: &str, fields: &[&str]) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    fields.iter().any(|f| f.to_lowercase().contains(&q))
}

/// How the graph is laid out on the canvas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GraphLayout {
    /// Force-directed (Fruchterman–Reingold): organic clusters, best for
    /// spotting hubs, cycles and tightly-coupled groups.
    #[default]
    Force,
    /// Layered top-down (hierarchical): call/import direction reads as rows,
    /// best for "what sits on top of what".
    Layered,
}

/// Resolve a click on the legend's two layout toggles.
///
/// Clicking the inactive layout switches to it; clicking the active one is a
/// no-op (it does not restage/refit the view). `None` means "keep the current
/// layout", which also covers frames with no click at all.
pub fn resolve_layout_pick(
    current: GraphLayout,
    force_on: bool,
    layered_on: bool,
) -> Option<GraphLayout> {
    let toggled = |want: GraphLayout, on: bool| on != (current == want);
    let picked = if toggled(GraphLayout::Layered, layered_on) {
        GraphLayout::Layered
    } else if toggled(GraphLayout::Force, force_on) {
        GraphLayout::Force
    } else {
        return None;
    };
    (picked != current).then_some(picked)
}

/// Remembered filter/cluster state (egui temp memory; deliberately not `App`).
#[derive(Clone, Debug, Default)]
pub struct GraphUiState {
    pub confidence: ConfidenceFilter,
    /// `Some(module)` restricts the graph to one module.
    pub module: Option<String>,
    pub search: String,
    /// Which layout algorithm positions the nodes.
    pub layout: GraphLayout,
    /// Modules currently collapsed into a single super-node.
    pub collapsed: BTreeSet<String>,
    /// Signature of the graph currently stored in `App::graph_state`; only a
    /// change here forces a rebuild.
    pub last_sig: Option<BuildSig>,
    /// Node counts of the last build, for the honest banners.
    pub total_nodes: usize,
    pub shown_nodes: usize,
    pub truncated: bool,
}

/// Cheap structural signature of everything that changes graph topology.
///
/// `search` is intentionally absent: it only dims/highlights and must not
/// trigger a rebuild while typing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildSig {
    pub repo: String,
    pub indexed_at: String,
    pub edges: usize,
    pub symbols: usize,
    pub calls: bool,
    pub imports: bool,
    pub confidence: ConfidenceFilter,
    pub module: Option<String>,
    pub collapsed: BTreeSet<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_non_exact_is_dashed() {
        assert!(!is_dashed(Confidence::Exact));
        assert!(is_dashed(Confidence::NameOnly));
        assert!(is_dashed(Confidence::Ambiguous));
        assert!(is_dashed(Confidence::Unresolved));
    }

    #[test]
    fn confidence_filter_exact_only_hides_dashed() {
        assert!(ConfidenceFilter::All.keeps(Confidence::NameOnly));
        assert!(ConfidenceFilter::All.keeps(Confidence::Ambiguous));
        assert!(ConfidenceFilter::ExactOnly.keeps(Confidence::Exact));
        assert!(!ConfidenceFilter::ExactOnly.keeps(Confidence::NameOnly));
        assert!(!ConfidenceFilter::ExactOnly.keeps(Confidence::Ambiguous));
    }

    #[test]
    fn kind_toggle_matrix() {
        assert!(kind_kept(EdgeKind::Call, true, false));
        assert!(!kind_kept(EdgeKind::Call, false, true));
        assert!(kind_kept(EdgeKind::Import, false, true));
        assert!(!kind_kept(EdgeKind::Import, true, false));
    }

    #[test]
    fn cluster_key_only_for_listed_modules() {
        let mut collapsed = BTreeSet::new();
        collapsed.insert("src/a".to_string());
        assert_eq!(
            cluster_key("src/a", &collapsed).as_deref(),
            Some("cluster:src/a")
        );
        assert_eq!(cluster_key("src/b", &collapsed), None);
        // Deterministic across calls (same input, same id).
        assert_eq!(
            cluster_key("src/a", &collapsed),
            cluster_key("src/a", &collapsed)
        );
    }

    #[test]
    fn default_layout_is_force() {
        assert_eq!(GraphUiState::default().layout, GraphLayout::Force);
        assert_eq!(GraphLayout::default(), GraphLayout::Force);
    }

    #[test]
    fn layout_pick_switches_only_to_the_other_layout() {
        use GraphLayout::{Force, Layered};
        // No click (both toggles mirror the current layout).
        assert_eq!(resolve_layout_pick(Force, true, false), None);
        assert_eq!(resolve_layout_pick(Layered, false, true), None);
        // Click the inactive one.
        assert_eq!(resolve_layout_pick(Force, true, true), Some(Layered));
        assert_eq!(resolve_layout_pick(Layered, true, true), Some(Force));
        // Click the active one again: no restage, no refit.
        assert_eq!(resolve_layout_pick(Force, false, false), None);
        assert_eq!(resolve_layout_pick(Layered, false, false), None);
    }

    #[test]
    fn empty_search_matches_and_is_case_insensitive() {
        assert!(matches_search("", &["anything"]));
        assert!(matches_search("   ", &["anything"]));
        assert!(matches_search("GRAPH", &["src/graph/render.rs"]));
        assert!(matches_search("render", &["Render"]));
        assert!(!matches_search("zzz", &["render", "src"]));
    }
}
