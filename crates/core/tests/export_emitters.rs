//! Export emitter tests (PROD-2). Hand-built `Index` + `Analysis` only — no
//! fixture parsing — so in-flight analyzer work cannot break these, and the
//! emitters' purity/determinism can be asserted independently of the pipeline.
use repowalk_core::analysis::{
    Analysis, CfgEdge, CfgEdgeKind, CfgKind, CfgNode, Field, FunctionCfg, ModuleEdge,
    ModuleEdgeKind, ModuleKind, ModuleNode, TypeEdge, TypeEdgeKind, TypeKind, TypeNode, UseKind,
    UseSite, VarKind, Variable,
};
use repowalk_core::export::{dot, markdown_report, mermaid, svg, Diagram};
use repowalk_core::model::{
    Confidence, Edge, EdgeKind, FileEntry, Import, Index, Symbol, SymbolKind,
};

fn sym(id: &str, name: &str, kind: SymbolKind, file: &str, line: usize) -> Symbol {
    Symbol {
        id: id.into(),
        name: name.into(),
        kind,
        signature: String::new(),
        doc: String::new(),
        start_line: line,
        end_line: line + 10,
        container: None,
        file: file.into(),
    }
}

fn call(caller: &str, callee: &str, line: usize, confidence: Confidence) -> Edge {
    Edge {
        caller: caller.into(),
        callee: callee.into(),
        line,
        kind: EdgeKind::Call,
        confidence,
    }
}

fn fixture_index() -> Index {
    let mut idx = Index::new("/repo");
    idx.files = vec![
        FileEntry {
            path: "src/main.rs".into(),
            lang: "rust".into(),
            lines: 20,
            mtime: 0,
            is_entrypoint: true,
        },
        FileEntry {
            path: "src/lib.rs".into(),
            lang: "rust".into(),
            lines: 40,
            mtime: 0,
            is_entrypoint: false,
        },
        FileEntry {
            path: "src/util.py".into(),
            lang: "python".into(),
            lines: 15,
            mtime: 0,
            is_entrypoint: false,
        },
    ];
    idx.symbols = vec![
        sym(
            "src/main.rs:1",
            "main",
            SymbolKind::Function,
            "src/main.rs",
            1,
        ),
        sym(
            "src/main.rs:8",
            "helper",
            SymbolKind::Function,
            "src/main.rs",
            8,
        ),
        sym("src/lib.rs:1", "Point", SymbolKind::Struct, "src/lib.rs", 1),
        sym(
            "src/lib.rs:20",
            "Shape",
            SymbolKind::Trait,
            "src/lib.rs",
            20,
        ),
    ];
    idx.edges = vec![
        call("src/main.rs:1", "src/main.rs:8", 3, Confidence::Exact),
        call("src/main.rs:8", "src/lib.rs:1", 9, Confidence::NameOnly),
        // Duplicate pair with weaker confidence: dedupe keeps the Exact one.
        call("src/main.rs:1", "src/main.rs:8", 3, Confidence::Ambiguous),
        Edge {
            caller: "src/main.rs:1".into(),
            callee: "src/lib.rs:1".into(),
            line: 4,
            kind: EdgeKind::Import,
            confidence: Confidence::Exact,
        },
    ];
    idx.imports = vec![Import {
        file: "src/main.rs".into(),
        target: "src/lib.rs".into(),
    }];
    idx
}

fn fixture_analysis() -> Analysis {
    let mut a = Analysis::new();
    a.variables = vec![
        Variable {
            id: "src/main.rs:2:x".into(),
            name: "x".into(),
            kind: VarKind::Local,
            file: "src/main.rs".into(),
            decl_line: 2,
            scope_start: 2,
            scope_end: 6,
            type_name: Some("i32".into()),
            reads: 3,
            writes: 1,
            first_use: Some(3),
            last_use: Some(6),
            enclosing: "src/main.rs:1".into(),
        },
        Variable {
            id: "src/main.rs:4:unused".into(),
            name: "unused".into(),
            kind: VarKind::Local,
            file: "src/main.rs".into(),
            decl_line: 4,
            scope_start: 4,
            scope_end: 6,
            type_name: None,
            reads: 0,
            writes: 1,
            first_use: None,
            last_use: None,
            enclosing: "src/main.rs:1".into(),
        },
    ];
    a.cfgs = vec![FunctionCfg {
        symbol: "src/main.rs:1".into(),
        nodes: vec![
            CfgNode {
                id: 0,
                kind: CfgKind::Entry,
                start_line: 1,
                end_line: 1,
                label: "entry".into(),
            },
            CfgNode {
                id: 1,
                kind: CfgKind::Branch,
                start_line: 3,
                end_line: 3,
                label: "x > 0".into(),
            },
            CfgNode {
                id: 2,
                kind: CfgKind::Call,
                start_line: 4,
                end_line: 4,
                label: "helper()".into(),
            },
            CfgNode {
                id: 3,
                kind: CfgKind::Exit,
                start_line: 6,
                end_line: 6,
                label: "exit".into(),
            },
        ],
        edges: vec![
            CfgEdge {
                from: 0,
                to: 1,
                kind: CfgEdgeKind::Normal,
                line: 1,
            },
            CfgEdge {
                from: 1,
                to: 2,
                kind: CfgEdgeKind::True,
                line: 3,
            },
            CfgEdge {
                from: 1,
                to: 3,
                kind: CfgEdgeKind::False,
                line: 3,
            },
            CfgEdge {
                from: 2,
                to: 1,
                kind: CfgEdgeKind::Back,
                line: 4,
            },
        ],
        cyclomatic: 3,
        max_nesting: 1,
    }];
    a.types = vec![
        TypeNode {
            id: "src/lib.rs:Point".into(),
            name: "Point".into(),
            kind: TypeKind::Struct,
            file: "src/lib.rs".into(),
            line: 1,
            fields: vec![
                Field {
                    name: "x".into(),
                    type_name: "f64".into(),
                    visibility: "pub".into(),
                    line: 2,
                },
                Field {
                    name: "y".into(),
                    type_name: "f64".into(),
                    visibility: "pub".into(),
                    line: 3,
                },
            ],
            methods: vec!["new".into(), "new".into()],
            doc: String::new(),
            module_path: "crate::lib".into(),
        },
        TypeNode {
            id: "src/lib.rs:Shape".into(),
            name: "Shape".into(),
            kind: TypeKind::Trait,
            file: "src/lib.rs".into(),
            line: 20,
            fields: Vec::new(),
            methods: vec!["area".into()],
            doc: String::new(),
            module_path: "crate::lib".into(),
        },
        TypeNode {
            id: "src/lib.rs:Vec2".into(),
            name: "Vec2".into(),
            kind: TypeKind::Struct,
            file: "src/lib.rs".into(),
            line: 30,
            fields: vec![Field {
                name: "p".into(),
                type_name: "Point".into(),
                visibility: "private".into(),
                line: 31,
            }],
            methods: Vec::new(),
            doc: String::new(),
            module_path: "crate::lib".into(),
        },
    ];
    a.type_edges = vec![
        TypeEdge {
            from: "src/lib.rs:Point".into(),
            to: "src/lib.rs:Shape".into(),
            kind: TypeEdgeKind::Impl,
            line: 25,
        },
        TypeEdge {
            from: "src/lib.rs:Vec2".into(),
            to: "src/lib.rs:Point".into(),
            kind: TypeEdgeKind::Field,
            line: 31,
        },
        // Dangling endpoint: must be dropped.
        TypeEdge {
            from: "src/lib.rs:Point".into(),
            to: "src/lib.rs:Ghost".into(),
            kind: TypeEdgeKind::Inherit,
            line: 99,
        },
    ];
    a.modules = vec![
        ModuleNode {
            id: "src".into(),
            kind: ModuleKind::Dir,
            loc: 60,
            symbols: 3,
        },
        ModuleNode {
            id: "src/lib.rs".into(),
            kind: ModuleKind::File,
            loc: 40,
            symbols: 2,
        },
        ModuleNode {
            id: "src/main.rs".into(),
            kind: ModuleKind::File,
            loc: 20,
            symbols: 2,
        },
    ];
    a.module_edges = vec![ModuleEdge {
        from: "src/main.rs".into(),
        to: "src/lib.rs".into(),
        kind: ModuleEdgeKind::Import,
        count: 2,
    }];
    a.uses = vec![
        UseSite {
            target: "src/lib.rs:1".into(),
            file: "src/main.rs".into(),
            line: 4,
            col: 3,
            kind: UseKind::Call,
            snippet: "Point::new()".into(),
        },
        UseSite {
            target: "src/lib.rs:1".into(),
            file: "src/main.rs".into(),
            line: 9,
            col: 1,
            kind: UseKind::TypeRef,
            snippet: "Point".into(),
        },
        UseSite {
            target: "src/main.rs:8".into(),
            file: "src/main.rs".into(),
            line: 3,
            col: 5,
            kind: UseKind::Call,
            snippet: "helper()".into(),
        },
    ];
    a
}

fn all_diagrams() -> Vec<Diagram> {
    vec![
        Diagram::CallGraph,
        Diagram::Cfg {
            symbol: "src/main.rs:1".into(),
        },
        Diagram::Types,
        Diagram::Modules,
    ]
}

#[test]
fn mermaid_call_graph_uses_real_names_and_is_balanced() {
    let (idx, a) = (fixture_index(), fixture_analysis());
    let out = mermaid(&idx, &a, &Diagram::CallGraph);

    assert!(out.starts_with("flowchart LR"), "{out}");
    assert!(out.contains("main"), "real symbol name: {out}");
    assert!(out.contains("helper"), "real symbol name: {out}");
    assert!(out.contains("Point"), "real symbol name: {out}");
    // Confidence styling: NameOnly edge is dashed, Exact is solid.
    assert!(out.contains("-.->"), "weak edge dashed: {out}");
    assert!(out.contains("-->"), "exact edge solid: {out}");
    // One node declaration per graph node (3 nodes), no per-edge node lines.
    let node_lines = out.lines().filter(|l| l.contains("(\"")).count();
    assert_eq!(node_lines, 3, "{out}");
}

#[test]
fn mermaid_cfg_shapes_labels_and_placeholder() {
    let (idx, a) = (fixture_index(), fixture_analysis());
    let out = mermaid(
        &idx,
        &a,
        &Diagram::Cfg {
            symbol: "src/main.rs:1".into(),
        },
    );
    assert!(out.starts_with("flowchart TD"), "{out}");
    for needle in ["x > 0", "true", "false", "back"] {
        assert!(out.contains(needle), "missing {needle:?} in {out}");
    }
    assert!(out.contains('{'), "diamond for Branch: {out}");

    let missing = mermaid(
        &idx,
        &a,
        &Diagram::Cfg {
            symbol: "src/nope.rs:1".into(),
        },
    );
    assert!(missing.starts_with("flowchart TD"), "{missing}");
    assert!(missing.contains("no CFG"), "placeholder: {missing}");
    assert!(!missing.trim().is_empty());
}

#[test]
fn mermaid_types_is_a_class_diagram_with_edges() {
    let (idx, a) = (fixture_index(), fixture_analysis());
    let out = mermaid(&idx, &a, &Diagram::Types);
    assert!(out.starts_with("classDiagram"), "{out}");
    assert!(out.contains("class "), "{out}");
    assert!(out.contains("Point"), "{out}");
    assert!(out.contains("Shape"), "{out}");
    assert!(out.contains("+x: f64"), "field emitted: {out}");
    assert!(out.contains("+new()"), "method emitted: {out}");
    assert!(out.contains("..|>"), "impl realization: {out}");
    assert!(out.contains("*--"), "field composition: {out}");
    assert!(!out.contains("Ghost"), "dangling edge dropped: {out}");
}

#[test]
fn mermaid_modules_shows_import_counts() {
    let (idx, a) = (fixture_index(), fixture_analysis());
    let out = mermaid(&idx, &a, &Diagram::Modules);
    assert!(out.starts_with("flowchart LR"), "{out}");
    assert!(out.contains("src/main.rs"), "{out}");
    assert!(out.contains("2 imports"), "import count label: {out}");
}

#[test]
fn dot_is_a_valid_digraph() {
    let (idx, a) = (fixture_index(), fixture_analysis());
    for diagram in all_diagrams() {
        let out = dot(&idx, &a, &diagram);
        assert!(out.starts_with("digraph repowalk {"), "{diagram:?}: {out}");
        assert!(out.trim_end().ends_with('}'), "{diagram:?}: {out}");
        assert!(out.contains("rankdir="), "{diagram:?}: {out}");
        assert!(out.contains("->"), "{diagram:?}: {out}");
        let opens = out.matches('{').count();
        let closes = out.matches('}').count();
        assert_eq!(opens, closes, "{diagram:?}: unbalanced braces: {out}");
    }
    let call = dot(&idx, &a, &Diagram::CallGraph);
    assert!(call.contains("style=dashed"), "weak edge dashed: {call}");
    assert!(call.contains("\"main\""), "quoted real name: {call}");
}

#[test]
fn svg_is_standalone_and_balanced() {
    let (idx, a) = (fixture_index(), fixture_analysis());
    for diagram in all_diagrams() {
        let out = svg(&idx, &a, &diagram);
        assert!(out.starts_with("<svg"), "{diagram:?}: {out}");
        assert!(out.trim_end().ends_with("</svg>"), "{diagram:?}");
        assert!(out.contains("</style>"), "{diagram:?}");
        assert!(out.contains("<text"), "{diagram:?}");
        assert_eq!(
            out.matches("<text").count(),
            out.matches("</text>").count(),
            "{diagram:?}: <text> balance"
        );
        assert_eq!(
            out.matches("<g ").count(),
            out.matches("</g>").count(),
            "{diagram:?}: <g> balance"
        );
        assert_eq!(
            out.matches("<marker ").count(),
            out.matches("</marker>").count(),
            "{diagram:?}: marker balance"
        );
        assert_eq!(
            out.matches("<defs>").count(),
            out.matches("</defs>").count(),
            "{diagram:?}: defs balance"
        );
    }
    let call = svg(&idx, &a, &Diagram::CallGraph);
    assert!(call.contains("marker-end=\"url(#arrow-normal)\""), "{call}");
    assert!(call.contains("#0F1115"), "dark palette: {call}");
}

#[test]
fn svg_escapes_dangerous_label_characters() {
    let mut idx = fixture_index();
    // A symbol name with XML metacharacters must be escaped in the <text> node.
    idx.symbols.push(sym(
        "src/main.rs:50",
        "a<b>&\"c'",
        SymbolKind::Function,
        "src/main.rs",
        50,
    ));
    idx.edges.push(call(
        "src/main.rs:1",
        "src/main.rs:50",
        50,
        Confidence::Exact,
    ));
    let a = fixture_analysis();
    let out = svg(&idx, &a, &Diagram::CallGraph);
    assert!(out.contains("a&lt;b&gt;&amp;&quot;c&apos;"), "{out}");
    assert!(!out.contains("a<b>"), "raw metacharacters leaked: {out}");
}

#[test]
fn report_has_every_section_and_four_mermaid_blocks() {
    let (idx, a) = (fixture_index(), fixture_analysis());
    let report = markdown_report(&idx, &a);

    for heading in [
        "# repowalk architecture report",
        "## Overview",
        "## Module map",
        "## Complexity",
        "## Unused variables",
        "## Types",
        "## Top referenced symbols",
        "## Diagrams",
        "### Call graph",
        "### Control-flow graph",
        "### Types",
        "### Modules",
    ] {
        assert!(report.contains(heading), "missing heading {heading:?}");
    }
    assert_eq!(
        report.matches("```mermaid").count(),
        4,
        "four embedded diagrams"
    );
    assert!(report.contains("1 variable with `reads == 0`"), "{report}");
    assert!(!report.contains("/repo"), "no absolute path: {report}");
}

#[test]
fn emitters_are_byte_stable() {
    let (idx, a) = (fixture_index(), fixture_analysis());
    for diagram in all_diagrams() {
        assert_eq!(
            mermaid(&idx, &a, &diagram),
            mermaid(&idx, &a, &diagram),
            "mermaid {diagram:?}"
        );
        assert_eq!(dot(&idx, &a, &diagram), dot(&idx, &a, &diagram), "dot");
        assert_eq!(svg(&idx, &a, &diagram), svg(&idx, &a, &diagram), "svg");
    }
    assert_eq!(markdown_report(&idx, &a), markdown_report(&idx, &a));
}

#[test]
fn empty_inputs_still_emit_valid_output() {
    let idx = Index::new("/repo");
    let a = Analysis::new();
    assert!(mermaid(&idx, &a, &Diagram::CallGraph).starts_with("flowchart LR"));
    assert!(mermaid(&idx, &a, &Diagram::Types).starts_with("classDiagram"));
    assert!(mermaid(&idx, &a, &Diagram::Modules).starts_with("flowchart LR"));
    assert!(dot(&idx, &a, &Diagram::CallGraph).trim_end().ends_with('}'));
    assert!(svg(&idx, &a, &Diagram::Types)
        .trim_end()
        .ends_with("</svg>"));
    let report = markdown_report(&idx, &a);
    assert!(report.contains("## Overview"), "{report}");
}
