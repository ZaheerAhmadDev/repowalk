//! Headless probe of the real shell: drives the actual eframe app through
//! kittest (accesskit) so clicks/typing are exercised the way a user's mouse
//! and keyboard exercise them. These tests exist because the GUI can not be
//! visually verified from the agent side — they are the regression net for
//! "the button does nothing" / "the search box swallows my typing" bugs.
//!
//! Lives as a child module of `app` so it can read/write private app state.
use super::*;

use egui_kittest::{kittest::Queryable, Harness};

/// Build a harness around a real [`App`], sized like the desktop window.
fn harness() -> Harness<'static, App> {
    Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .build_eframe(|cc| App::new(cc))
}

/// A minimal but real index so panels that need one actually render.
fn sample_index() -> repowalk_core::model::Index {
    use repowalk_core::model::{FileEntry, Index, Symbol, SymbolKind};
    let mut idx = Index::new("/tmp/sample");
    idx.files.push(FileEntry {
        path: "src/main.rs".into(),
        lang: "rust".into(),
        lines: 42,
        mtime: 0,
        is_entrypoint: true,
    });
    idx.symbols.push(Symbol {
        id: "src/main.rs:10".into(),
        name: "run".into(),
        kind: SymbolKind::Function,
        signature: "fn run()".into(),
        doc: String::new(),
        start_line: 10,
        end_line: 12,
        container: None,
        file: "src/main.rs".into(),
    });
    idx
}

/// Print the whole accesskit tree — used to see what the shell exposes.
fn dump_tree(harness: &Harness<'static, App>) -> String {
    fn walk(node: &egui_kittest::kittest::AccessKitNode<'_>, depth: usize, out: &mut String) {
        let label = node.value().unwrap_or_default();
        out.push_str(&format!(
            "{}{:?} {:?}\n",
            "  ".repeat(depth),
            node.role(),
            label
        ));
        for child in node.children() {
            walk(&child, depth + 1, out);
        }
    }
    let mut out = String::new();
    walk(&harness.kittest_state().root(), 0, &mut out);
    out
}

#[test]
fn shell_exposes_toolbar_and_central_content() {
    let mut harness = harness();
    harness.run_steps(3);
    let tree = dump_tree(&harness);
    println!("=== ACCESSKIT TREE ===\n{tree}");

    // The toolbar must expose the four mode segments as buttons.
    assert!(
        harness.query_by_label("Graph").is_some(),
        "no 'Graph' toolbar button in tree:\n{tree}"
    );
    // The central pane must actually paint something in Text mode.
    assert!(
        tree.contains("Open a repository"),
        "central view painted nothing (Text mode, no repo):\n{tree}"
    );
}

#[test]
fn clicking_mode_segments_switches_mode() {
    let mut harness = harness();
    harness.run_steps(3);

    harness.get_by_label("Graph").click();
    harness.run_steps(3);
    assert_eq!(
        harness.state().mode,
        Mode::Graph,
        "Graph button did not switch mode"
    );

    harness.get_by_label("Flow").click();
    harness.run_steps(3);
    assert_eq!(
        harness.state().mode,
        Mode::Flow,
        "Flow button did not switch mode"
    );

    harness.get_by_label("Diagrams").click();
    harness.run_steps(3);
    assert_eq!(
        harness.state().mode,
        Mode::Diagrams,
        "Diagrams button did not switch mode"
    );

    harness.get_by_label("Text").click();
    harness.run_steps(3);
    assert_eq!(
        harness.state().mode,
        Mode::Text,
        "Text button did not switch mode"
    );
}

#[test]
fn search_box_accepts_typed_text() {
    let mut harness = harness();
    // Give the app an index + the search panel so the box exists.
    harness.state_mut().index = Some(std::sync::Arc::new(sample_index()));
    harness.state_mut().sidebar_tab = crate::sidebar::Tab::Search;
    harness.run_steps(3);

    let tree = dump_tree(&harness);
    let input = harness
        .query_by_role(accesskit::Role::TextInput)
        .unwrap_or_else(|| panic!("search box (TextInput) missing from tree:\n{tree}"));
    input.click();
    input.type_text("run");
    harness.run_steps(3);

    assert_eq!(
        harness.state().query,
        "run",
        "typing into the search box did not reach app.query"
    );
}

/// The user's actual state: a real repo learned. Everything must paint and
/// respond here — this is the state the "nothing shows up" report came from.
#[test]
fn shell_with_indexed_repo_paints_and_responds() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/sample")
        .canonicalize()
        .expect("fixtures/sample must exist");
    let (idx, analysis) =
        repowalk_core::learn::learn(&repo, &|_: repowalk_core::learn::LearnProgress| {})
            .expect("sample repo must learn");

    let mut harness = harness();
    harness.state_mut().index = Some(std::sync::Arc::new(idx));
    harness.state_mut().analysis = Some(std::sync::Arc::new(analysis));
    harness.run_steps(3);

    let tree = dump_tree(&harness);
    println!("=== TREE (indexed repo) ===\n{tree}");

    // Mode buttons still switch with a repo loaded.
    harness.get_by_label("Graph").click();
    harness.run_steps(3);
    assert_eq!(
        harness.state().mode,
        Mode::Graph,
        "Graph button dead with a repo loaded"
    );
    harness.get_by_label("Text").click();
    harness.run_steps(3);
    assert_eq!(
        harness.state().mode,
        Mode::Text,
        "Text button dead with a repo loaded"
    );

    // Search typing still works with a repo loaded.
    harness.state_mut().sidebar_tab = crate::sidebar::Tab::Search;
    harness.run_steps(3);
    let input = harness
        .query_by_role(accesskit::Role::TextInput)
        .unwrap_or_else(|| panic!("search box missing with repo loaded:\n{tree}"));
    input.click();
    input.type_text("run");
    harness.run_steps(3);
    assert_eq!(
        harness.state().query,
        "run",
        "search box swallowed typing with a repo loaded"
    );
}

/// Small-window resilience: the shell must keep its toolbar buttons clickable
/// and its central view painted at every size the window manager may hand us.
#[test]
fn shell_survives_small_windows() {
    for (w, h) in [
        (1280.0, 800.0),
        (1024.0, 700.0),
        (860.0, 600.0),
        (760.0, 480.0),
        (640.0, 420.0),
        (420.0, 320.0),
    ] {
        let mut harness = Harness::builder()
            .with_size(egui::vec2(w, h))
            .build_eframe(|cc| App::new(cc));
        harness.run_steps(3);
        let tree = dump_tree(&harness);

        assert!(
            harness.query_by_label("Graph").is_some(),
            "at {w}x{h}: the Graph toolbar button vanished:\n{tree}"
        );
        assert!(
            tree.contains("Open a repository"),
            "at {w}x{h}: central view painted nothing:\n{tree}"
        );

        harness.get_by_label("Graph").click();
        harness.run_steps(3);
        assert_eq!(
            harness.state().mode,
            Mode::Graph,
            "at {w}x{h}: Graph button did not switch mode"
        );

        harness.get_by_label("Text").click();
        harness.run_steps(3);
        assert_eq!(
            harness.state().mode,
            Mode::Text,
            "at {w}x{h}: Text button did not switch mode"
        );
    }
}

/// The Flow tab must explain its two endpoints *where the empty canvas is*,
/// not point at a sidebar section the reader has to hunt for.
#[test]
fn flow_view_explains_its_endpoints_in_place() {
    let mut harness = harness();
    harness.state_mut().index = Some(std::sync::Arc::new(sample_index()));
    harness.state_mut().mode = Mode::Flow;
    harness.run_steps(3);
    let tree = dump_tree(&harness);

    assert!(
        tree.contains("Trace a call path"),
        "flow trace card missing:\n{tree}"
    );
    assert!(tree.contains("Start (A)"), "A slot missing:\n{tree}");
    assert!(tree.contains("End (B)"), "B slot missing:\n{tree}");
    assert!(
        tree.contains("not set"),
        "empty endpoint state missing:\n{tree}"
    );
    assert_eq!(
        harness.query_all_by_label("choose…").count(),
        2,
        "expected a choose button for each endpoint:\n{tree}"
    );

    // Arming A must immediately offer functions (not an empty \"no matches\"
    // list) and with the sample index the one function is `run`.
    {
        let choose_a = harness.query_all_by_label("choose…").next().unwrap();
        choose_a.click();
    }
    harness.run_steps(3);
    let armed = dump_tree(&harness);
    assert!(
        armed.contains("run"),
        "armed picker should suggest the sample function:\n{armed}"
    );
}

/// Arming a picker with an empty query must offer functions immediately; an
/// empty result list used to read as if the repository had no functions.
#[test]
fn empty_picker_query_offers_functions() {
    let idx = sample_index();
    let hits = crate::flow::picker_hits(&idx, "", 6);
    assert_eq!(hits.len(), 1, "expected the sample function: {hits:?}");
    assert_eq!(hits[0].0, "src/main.rs:10");
}

/// Picking A hands straight over to B, then forms the shared trace pair.
#[test]
fn flow_endpoint_picking_advances_from_a_to_b_to_the_pair() {
    let mut harness = harness();
    let app = harness.state_mut();
    app.flow.pick = TracePick::PickA;
    crate::flow::assign_endpoint(app, "src/main.rs:10".into());
    assert_eq!(app.flow.trace_a, "src/main.rs:10");
    assert_eq!(
        app.flow.pick,
        TracePick::PickB,
        "after A the picker should wait for B"
    );

    crate::flow::assign_endpoint(app, "src/main.rs:10".into());
    assert_eq!(app.flow.trace_b, "src/main.rs:10");
    assert_eq!(app.flow.pick, TracePick::None);
    assert_eq!(
        app.trace,
        Some(("src/main.rs:10".into(), "src/main.rs:10".into()))
    );
}

/// Clicking through files replaces the clean preview tab instead of piling
/// up redundant tabs; unsaved work is never displaced.
#[test]
fn opening_files_reuses_a_clean_tab_but_keeps_a_dirty_one() {
    let mut harness = harness();
    let app = harness.state_mut();
    app.select(Selection::File("src/main.rs".into()));
    app.select(Selection::File("src/lib.rs".into()));
    assert_eq!(app.tabs.len(), 1, "clean preview tabs should be reused");
    assert_eq!(app.tabs[0].title(), "lib.rs");

    app.dirty_edits.insert("src/lib.rs".into());
    app.select(Selection::File("src/main.rs".into()));
    assert_eq!(app.tabs.len(), 2, "a dirty tab must survive the next open");
    assert!(app
        .tabs
        .iter()
        .any(|t| t.file.as_deref() == Some("src/lib.rs")));
}
