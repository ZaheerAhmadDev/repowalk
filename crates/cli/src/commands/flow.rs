//! `repowalk flow <A> <B> <path>` / `repowalk flow --from <symbol> <path>` —
//! A→B call trace with hop confidences, or wave layers + reachability footer.
//! OWNER: TL-PROD (PROD-1).
use anyhow::anyhow;
use repowalk_core::flow;
use repowalk_core::metrics;
use repowalk_core::model::{Confidence, EdgeKind, Index, Symbol};
use repowalk_core::pipeline;
use std::path::PathBuf;

/// Max member names printed per wave layer before "+K more".
const MAX_LAYER_NAMES: usize = 8;

enum Mode {
    Trace { a: String, b: String },
    Waves { source: String },
}

pub fn run(
    a: Option<String>,
    b: Option<String>,
    from: Option<String>,
    depth: usize,
    md: bool,
    path: Option<PathBuf>,
) -> anyhow::Result<()> {
    // clap fills positionals left-to-right, so with --from the single trailing
    // positional lands in `a` — re-seat it as the repo path.
    let (repo, mode) = match from {
        Some(source) => {
            if b.is_some() {
                anyhow::bail!("usage: repowalk flow --from <symbol> <path> [--depth N] [--md]");
            }
            let repo = path.or_else(|| a.map(PathBuf::from)).ok_or_else(|| {
                anyhow!("usage: repowalk flow --from <symbol> <path> [--depth N] [--md]")
            })?;
            (repo, Mode::Waves { source })
        }
        None => match (a, b, path) {
            (Some(a), Some(b), Some(repo)) => (repo, Mode::Trace { a, b }),
            _ => anyhow::bail!("usage: repowalk flow <A> <B> <path> [--md]"),
        },
    };
    let idx = pipeline::build_index(&repo, &|_done| {})?;
    let out = match mode {
        Mode::Trace { a, b } => trace_report(&idx, &a, &b, md)?,
        Mode::Waves { source } => wave_report(&idx, &source, depth, md)?,
    };
    print!("{out}");
    Ok(())
}

/// Resolve a user-typed endpoint: exact id → unique symbol name → unique
/// fuzzy suffix of the name. `None` when no tier hits or a tier is ambiguous.
fn resolve_endpoint<'a>(idx: &'a Index, arg: &str) -> Option<&'a Symbol> {
    if let Some(sym) = idx.symbols.iter().find(|s| s.id == arg) {
        return Some(sym);
    }
    let by_name: Vec<&Symbol> = idx.symbols.iter().filter(|s| s.name == arg).collect();
    if by_name.len() == 1 {
        return Some(by_name[0]);
    }
    let by_suffix: Vec<&Symbol> = idx
        .symbols
        .iter()
        .filter(|s| s.name.ends_with(arg))
        .collect();
    if by_suffix.len() == 1 {
        return Some(by_suffix[0]);
    }
    None
}

/// Confidence of the first resolved call edge `from → to` (index order;
/// deterministic), `None` when the pair has no call edge.
fn hop_confidence(idx: &Index, from: &str, to: &str) -> Option<Confidence> {
    idx.edges
        .iter()
        .find(|e| e.kind == EdgeKind::Call && e.caller == from && e.callee == to)
        .map(|e| e.confidence)
}

/// Resolved A→B trace as a rendered step list. Honest errors: unknown
/// endpoint ("unknown symbol: X"), unreachable B ("no path from A to B").
pub fn trace_report(idx: &Index, a: &str, b: &str, md: bool) -> anyhow::Result<String> {
    let from = resolve_endpoint(idx, a).ok_or_else(|| anyhow!("unknown symbol: {a}"))?;
    let to = resolve_endpoint(idx, b).ok_or_else(|| anyhow!("unknown symbol: {b}"))?;
    let graph = normalized(idx);
    let Some(path) = flow::trace(&graph, &from.id, &to.id) else {
        anyhow::bail!("no path from {} to {}", from.name, to.name);
    };
    Ok(render_trace(&graph, &path, md))
}

/// Wave layers from `source` plus a reachability footer. Honest error on an
/// unresolvable source symbol.
pub fn wave_report(idx: &Index, source: &str, depth: usize, md: bool) -> anyhow::Result<String> {
    let sym = resolve_endpoint(idx, source).ok_or_else(|| anyhow!("unknown symbol: {source}"))?;
    let graph = normalized(idx);
    Ok(render_waves(&graph, &sym.id, depth, md))
}

/// Call-graph working copy for traversal: the pipeline keys Call-edge
/// `caller` by the enclosing *name* while `callee` is a symbol id, which
/// breaks multi-hop BFS at the name/id boundary. Rewrite resolvable callers
/// (unique symbol with that name whose range contains the call line) to
/// their symbol id; ambiguous or file-level callers stay untouched — honest
/// partial resolution beats a wrong path.
fn normalized(idx: &Index) -> Index {
    let mut graph = idx.clone();
    for e in graph.edges.iter_mut().filter(|e| e.kind == EdgeKind::Call) {
        let mut cands = idx
            .symbols
            .iter()
            .filter(|s| s.name == e.caller && s.start_line <= e.line && e.line <= s.end_line);
        if let (Some(hit), None) = (cands.next(), cands.next()) {
            e.caller = hit.id.clone();
        }
    }
    graph
}

/// Numbered step list for a resolved trace path (endpoint-inclusive ids):
/// step index, symbol name, signature, file:line, hop confidence per edge.
pub fn render_trace(idx: &Index, path: &[String], md: bool) -> String {
    let label = |id: &str| {
        idx.symbols
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| id.to_string())
    };
    let title = format!(
        "Flow: {} → {}",
        label(&path[0]),
        label(&path[path.len() - 1])
    );
    let mut out = String::new();
    if md {
        out.push_str(&format!("# {title}\n"));
    } else {
        out.push_str(&crate::render::head(&title));
        out.push('\n');
    }
    for (i, id) in path.iter().enumerate() {
        let sym = idx.symbols.iter().find(|s| s.id == *id);
        let name = sym.map(|s| s.name.as_str()).unwrap_or(id.as_str());
        let sig = sym.map(|s| s.signature.as_str()).unwrap_or("");
        let loc = sym
            .map(|s| format!("{}:{}", s.file, s.start_line))
            .unwrap_or_else(|| id.clone());
        let hop = (i + 1 < path.len())
            .then(|| hop_confidence(idx, id, &path[i + 1]))
            .flatten();
        if md {
            let mut line = format!("{}. **{name}**", i + 1);
            if !sig.is_empty() {
                line.push_str(&format!(" — `{sig}`"));
            }
            line.push_str(&format!(" — `{loc}`"));
            out.push_str(&line);
            out.push('\n');
            if let Some(conf) = hop {
                out.push_str(&format!("   - hop: **{conf:?}**\n"));
            }
        } else {
            out.push_str(&format!("{}. {name}\n", i + 1));
            if !sig.is_empty() {
                out.push_str(&format!("   {sig}\n"));
            }
            out.push_str(&format!("   {loc}\n"));
            if let Some(conf) = hop {
                out.push_str(&format!(
                    "   hop → {}\n",
                    crate::render::dim(&format!("{conf:?}"))
                ));
            }
        }
    }
    out
}

/// Wave layers from `source_id`: one line per layer with member count and
/// names (capped), plus the `metrics::reachability` footer.
pub fn render_waves(idx: &Index, source_id: &str, depth: usize, md: bool) -> String {
    let name = idx
        .symbols
        .iter()
        .find(|s| s.id == source_id)
        .map(|s| s.name.clone())
        .unwrap_or_else(|| source_id.to_string());
    let layers = flow::waves(idx, source_id, depth);
    let stats = metrics::reachability(idx, source_id, depth);
    let footer = format!(
        "reachable {} of {} symbols, max depth {}",
        stats.reachable, stats.symbols, stats.max_depth
    );
    let mut out = String::new();
    let title = format!("Waves from {name} — depth {depth}");
    if md {
        out.push_str(&format!("# {title}\n"));
    } else {
        out.push_str(&crate::render::head(&title));
        out.push('\n');
    }
    if layers.is_empty() {
        if md {
            out.push_str(&format!("\n*no call edges from {name}*\n\n{footer}\n"));
        } else {
            out.push_str(&format!(
                "  {}\n",
                crate::render::dim(&format!("no call edges from {name}"))
            ));
            out.push_str(&format!("{}\n", crate::render::dim(&footer)));
        }
        return out;
    }
    if md {
        out.push_str("\n| Layer | Count | Members |\n|------:|------:|---------|\n");
        for (k, layer) in layers.iter().enumerate() {
            out.push_str(&format!(
                "| {k} | {} | {} |\n",
                layer.len(),
                layer_names(idx, layer)
            ));
        }
        out.push_str(&format!("\n{footer}\n"));
    } else {
        for (k, layer) in layers.iter().enumerate() {
            out.push_str(&format!(
                "{k} ({}): {}\n",
                layer.len(),
                layer_names(idx, layer)
            ));
        }
        out.push_str(&format!("{}\n", crate::render::dim(&footer)));
    }
    out
}

/// Display names for one layer (id → name), capped at `MAX_LAYER_NAMES`
/// with a "+K more" suffix.
fn layer_names(idx: &Index, layer: &[String]) -> String {
    let names: Vec<&str> = layer
        .iter()
        .map(|id| {
            idx.symbols
                .iter()
                .find(|s| s.id == *id)
                .map(|s| s.name.as_str())
                .unwrap_or(id.as_str())
        })
        .collect();
    let shown = names.len().min(MAX_LAYER_NAMES);
    let mut line = names[..shown].join(", ");
    if names.len() > shown {
        line.push_str(&format!(" +{} more", names.len() - shown));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn temp_root(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("rw-cli-flow-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// main → helper → greet chain in one file (same-module edges → Exact).
    fn sample_index(root: &Path) -> Index {
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/main.rs"),
            "fn main() { helper(); }\nfn helper() { greet(); }\npub fn greet() {}\n",
        )
        .unwrap();
        pipeline::build_index(root, &|_| {}).unwrap()
    }

    /// main + 10 callees: layer 1 has 10 members (exercises the name cap).
    fn wide_index(root: &Path) -> Index {
        std::fs::create_dir_all(root.join("src")).unwrap();
        let mut src = String::from("fn main() {\n");
        for i in 0..10 {
            src.push_str(&format!("    c{i}();\n"));
        }
        src.push_str("}\n");
        for i in 0..10 {
            src.push_str(&format!("fn c{i}() {{}}\n"));
        }
        std::fs::write(root.join("src/main.rs"), src).unwrap();
        pipeline::build_index(root, &|_| {}).unwrap()
    }

    #[test]
    fn resolve_endpoint_tiers_id_name_suffix() {
        let root = temp_root("resolve");
        let idx = sample_index(&root);

        let id = idx
            .symbols
            .iter()
            .find(|s| s.name == "helper")
            .unwrap()
            .id
            .clone();
        assert_eq!(resolve_endpoint(&idx, &id).unwrap().name, "helper");
        assert_eq!(resolve_endpoint(&idx, "helper").unwrap().name, "helper");
        // unique fuzzy suffix of a name
        assert_eq!(resolve_endpoint(&idx, "lper").unwrap().name, "helper");
        assert!(resolve_endpoint(&idx, "definitely_missing").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn trace_report_renders_steps_signature_loc_and_hops() {
        let root = temp_root("trace");
        let idx = sample_index(&root);
        let out = trace_report(&idx, "main", "greet", false).unwrap();

        assert!(out.contains("Flow: main → greet"), "title: {out}");
        assert!(out.contains("1. main"), "step 1: {out}");
        assert!(out.contains("2. helper"), "step 2: {out}");
        assert!(out.contains("3. greet"), "step 3: {out}");
        assert!(out.contains("fn helper()"), "signature: {out}");
        assert!(out.contains("src/main.rs:"), "file:line: {out}");
        assert!(out.contains("hop → Exact"), "hop confidence: {out}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn trace_report_no_path_is_honest() {
        let root = temp_root("nopath");
        let idx = sample_index(&root);
        let err = trace_report(&idx, "greet", "main", false).unwrap_err();
        assert!(
            err.to_string().contains("no path from greet to main"),
            "{err}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn trace_report_unknown_symbol_is_honest() {
        let root = temp_root("unknown");
        let idx = sample_index(&root);
        let err = trace_report(&idx, "nope", "main", false).unwrap_err();
        assert!(err.to_string().contains("unknown symbol: nope"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn wave_report_layers_counts_cap_and_footer() {
        let root = temp_root("waves");
        let idx = wide_index(&root);
        let out = wave_report(&idx, "main", 6, false).unwrap();

        assert!(out.contains("Waves from main — depth 6"), "title: {out}");
        assert!(out.contains("0 (1): main"), "layer 0: {out}");
        assert!(out.contains("1 (10): "), "layer 1 count: {out}");
        assert!(out.contains("+2 more"), "capped names: {out}");
        assert!(
            out.contains("reachable 11 of 11 symbols, max depth 1"),
            "footer: {out}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn wave_report_unknown_symbol_is_honest() {
        let root = temp_root("wave-unknown");
        let idx = sample_index(&root);
        let err = wave_report(&idx, "nope", 6, false).unwrap_err();
        assert!(err.to_string().contains("unknown symbol: nope"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn md_mode_is_markdown_without_ansi() {
        let root = temp_root("md");
        let idx = sample_index(&root);

        let trace_md = trace_report(&idx, "main", "greet", true).unwrap();
        assert!(
            trace_md.starts_with("# Flow: main → greet"),
            "heading: {trace_md}"
        );
        assert!(trace_md.contains("1. **main**"), "numbered: {trace_md}");
        assert!(trace_md.contains("hop: **Exact**"), "hop: {trace_md}");
        assert!(!trace_md.contains('\u{1b}'), "no ANSI: {trace_md}");

        let wave_md = wave_report(&idx, "main", 6, true).unwrap();
        assert!(
            wave_md.starts_with("# Waves from main"),
            "heading: {wave_md}"
        );
        assert!(
            wave_md.contains("| Layer | Count | Members |"),
            "table: {wave_md}"
        );
        assert!(!wave_md.contains('\u{1b}'), "no ANSI: {wave_md}");
        let _ = std::fs::remove_dir_all(&root);
    }
}
