//! Markdown architecture report. OWNER: PROD-2.
//!
//! Pure and deterministic: no I/O, no clock, no absolute paths (the repo root is
//! reduced to its final path component). Complexity aggregates are *bought* from
//! [`crate::complexity`]; when that module is still an interface stub the report
//! degrades to a clearly-labelled table derived from the raw `Analysis::cfgs`,
//! never silently dropping the section.
use super::{mermaid, Diagram};
use crate::analysis::{Analysis, FunctionCfg};
use crate::complexity;
use crate::metrics;
use crate::model::{EdgeKind, Index, Symbol, SymbolKind};
use std::collections::{BTreeMap, BTreeSet};

const HOTSPOT_LIMIT: usize = 20;
const USES_LIMIT: usize = 10;

/// Build the self-contained Markdown report.
pub fn markdown(index: &Index, analysis: &Analysis) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# repowalk architecture report — {}\n\n",
        repo_name(&index.repo_root)
    ));

    overview(&mut out, index, analysis);
    module_map(&mut out, analysis);
    complexity_section(&mut out, analysis);
    unused_variables(&mut out, analysis);
    type_inventory(&mut out, analysis);
    top_references(&mut out, index, analysis);
    diagrams(&mut out, index, analysis);

    out
}

/// Final path component of the repo root, so no absolute path leaks into a diff.
fn repo_name(root: &str) -> String {
    root.rsplit(['/', '\\'])
        .find(|s| !s.is_empty())
        .unwrap_or("repository")
        .to_string()
}

fn overview(out: &mut String, index: &Index, analysis: &Analysis) {
    let m = metrics::compute(index);
    let calls = index
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Call)
        .count();
    let langs = if m.langs.is_empty() {
        "none".to_string()
    } else {
        m.langs
            .iter()
            .map(|(lang, n)| format!("{lang} ({n} file{})", if *n == 1 { "" } else { "s" }))
            .collect::<Vec<_>>()
            .join(", ")
    };

    out.push_str("## Overview\n\n");
    out.push_str("| metric | value |\n|---|---|\n");
    out.push_str(&format!("| files | {} |\n", index.files.len()));
    out.push_str(&format!("| lines of code | {} |\n", m.total_loc));
    out.push_str(&format!("| symbols | {} |\n", index.symbols.len()));
    out.push_str(&format!("| call edges | {calls} |\n"));
    out.push_str(&format!("| modules | {} |\n", analysis.modules.len()));
    out.push_str(&format!("| types | {} |\n", analysis.types.len()));
    out.push_str(&format!("| variables | {} |\n", analysis.variables.len()));
    out.push_str(&format!("| language mix | {langs} |\n"));
    out.push('\n');
}

fn module_map(out: &mut String, analysis: &Analysis) {
    out.push_str("## Module map\n\n");
    if analysis.modules.is_empty() {
        out.push_str("_Module graph unavailable (analyzer pending)._\n\n");
        return;
    }

    let mut modules = analysis.modules.clone();
    modules.sort_by(|a, b| a.id.cmp(&b.id));
    out.push_str("| module | kind | loc | symbols |\n|---|---|---|---|\n");
    for m in &modules {
        out.push_str(&format!(
            "| `{}` | {:?} | {} | {} |\n",
            m.id, m.kind, m.loc, m.symbols
        ));
    }
    out.push('\n');

    let mut edges = analysis.module_edges.clone();
    edges.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.from.cmp(&b.from)));
    if !edges.is_empty() {
        out.push_str("Top import edges:\n\n");
        for e in edges.iter().take(USES_LIMIT) {
            out.push_str(&format!(
                "- `{}` → `{}` ({}, count {})\n",
                e.from,
                e.to,
                format!("{:?}", e.kind).to_lowercase(),
                e.count
            ));
        }
        out.push('\n');
    }
}

/// True when `complexity` looks like a still-empty stub: it produced nothing
/// even though raw CFGs exist (any real implementation would non-empty per-file).
fn complexity_is_stub(analysis: &Analysis) -> bool {
    !analysis.cfgs.is_empty() && complexity::per_file(analysis).is_empty()
}

fn complexity_section(out: &mut String, analysis: &Analysis) {
    out.push_str("## Complexity\n\n");

    if analysis.cfgs.is_empty() {
        out.push_str("_No control-flow data available._\n\n");
        return;
    }

    let (hotspots, per_file) = if complexity_is_stub(analysis) {
        out.push_str(
            "_Complexity aggregates unavailable (dependency stub); \
             the tables below are derived from the raw CFGs._\n\n",
        );
        (fallback_hotspots(analysis), fallback_per_file(analysis))
    } else {
        (
            complexity::hotspots(analysis, HOTSPOT_LIMIT)
                .iter()
                .map(|h| {
                    (
                        h.symbol.clone(),
                        h.file.clone(),
                        h.line,
                        h.cyclomatic,
                        h.max_nesting,
                    )
                })
                .collect(),
            complexity::per_file(analysis)
                .iter()
                .map(|f| (f.file.clone(), f.total, f.max, f.functions))
                .collect(),
        )
    };

    out.push_str("### Hotspots\n\n");
    out.push_str("| symbol | file | line | cyclomatic | nesting |\n|---|---|---|---|---|\n");
    for (symbol, file, line, cyclomatic, nesting) in hotspots.iter().take(HOTSPOT_LIMIT) {
        out.push_str(&format!(
            "| `{}` | `{}` | {} | {} | {} |\n",
            symbol, file, line, cyclomatic, nesting
        ));
    }
    out.push('\n');

    out.push_str("### Per file\n\n");
    out.push_str("| file | total | max | functions |\n|---|---|---|---|\n");
    for (file, total, max, functions) in &per_file {
        out.push_str(&format!("| `{file}` | {total} | {max} | {functions} |\n"));
    }
    out.push('\n');
}

/// `(symbol, file, line, cyclomatic, nesting)`, most complex first.
fn fallback_hotspots(analysis: &Analysis) -> Vec<(String, String, usize, u32, u32)> {
    let mut rows: Vec<(String, String, usize, u32, u32)> = analysis
        .cfgs
        .iter()
        .map(|c| {
            let (file, line) = complexity::parse_symbol_id(&c.symbol);
            (c.symbol.clone(), file, line, c.cyclomatic, c.max_nesting)
        })
        .collect();
    rows.sort_by(|a, b| {
        b.3.cmp(&a.3)
            .then_with(|| b.4.cmp(&a.4))
            .then_with(|| a.0.cmp(&b.0))
    });
    rows
}

/// `(file, total, max, functions)` sorted by total desc then file.
fn fallback_per_file(analysis: &Analysis) -> Vec<(String, u32, u32, usize)> {
    let mut agg: BTreeMap<String, (u32, u32, usize)> = BTreeMap::new();
    for c in &analysis.cfgs {
        let (file, _) = complexity::parse_symbol_id(&c.symbol);
        let entry = agg.entry(file).or_insert((0, 0, 0));
        entry.0 = entry.0.saturating_add(c.cyclomatic);
        entry.1 = entry.1.max(c.cyclomatic);
        entry.2 += 1;
    }
    let mut rows: Vec<(String, u32, u32, usize)> = agg
        .into_iter()
        .map(|(file, (total, max, functions))| (file, total, max, functions))
        .collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    rows
}

fn unused_variables(out: &mut String, analysis: &Analysis) {
    // Buy from CORE-4; if that is still a stub (returns nothing while variables
    // exist) fall back to the documented predicate (`reads == 0`), which is
    // exactly what the aggregate is specified to return.
    let count = {
        let from_helper = complexity::unused_variables(analysis);
        if from_helper.is_empty() && !analysis.variables.is_empty() {
            analysis.variables.iter().filter(|v| v.reads == 0).count()
        } else {
            from_helper.len()
        }
    };
    out.push_str("## Unused variables\n\n");
    out.push_str(&format!(
        "{count} variable{} with `reads == 0` (dead-code candidates).\n\n",
        if count == 1 { "" } else { "s" }
    ));
}

fn type_inventory(out: &mut String, analysis: &Analysis) {
    out.push_str("## Types\n\n");
    if analysis.types.is_empty() {
        out.push_str("_No type declarations available._\n\n");
        return;
    }
    let mut types = analysis.types.clone();
    types.sort_by(|a, b| a.file.cmp(&b.file).then_with(|| a.name.cmp(&b.name)));
    out.push_str("| type | kind | file | line | fields | methods |\n|---|---|---|---|---|---|\n");
    for t in &types {
        let methods = t.methods.iter().collect::<BTreeSet<_>>().len();
        out.push_str(&format!(
            "| `{}` | {:?} | `{}` | {} | {} | {} |\n",
            t.name,
            t.kind,
            t.file,
            t.line,
            t.fields.len(),
            methods
        ));
    }
    out.push('\n');
}

fn top_references(out: &mut String, index: &Index, analysis: &Analysis) {
    out.push_str("## Top referenced symbols\n\n");
    if analysis.uses.is_empty() {
        out.push_str("_No reference data available._\n\n");
        return;
    }

    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for u in &analysis.uses {
        *counts.entry(u.target.as_str()).or_default() += 1;
    }
    let mut ranked: Vec<(&str, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

    out.push_str("| symbol | uses |\n|---|---|\n");
    for (target, n) in ranked.iter().take(USES_LIMIT) {
        out.push_str(&format!(
            "| `{}` | {} |\n",
            super::util::symbol_name(index, target),
            n
        ));
    }
    out.push('\n');
}

fn pick_cfg_symbol(index: &Index, analysis: &Analysis) -> String {
    if let Some(h) = complexity::hotspots(analysis, 1).into_iter().next() {
        if !h.symbol.is_empty() {
            return h.symbol;
        }
    }
    let mut cfgs: Vec<&FunctionCfg> = analysis.cfgs.iter().collect();
    cfgs.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    if let Some(c) = cfgs.first() {
        return c.symbol.clone();
    }
    let mut syms: Vec<&Symbol> = index
        .symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
        .collect();
    syms.sort_by(|a, b| a.id.cmp(&b.id));
    syms.first().map(|s| s.id.clone()).unwrap_or_default()
}

fn diagrams(out: &mut String, index: &Index, analysis: &Analysis) {
    out.push_str("## Diagrams\n\n");

    out.push_str("### Call graph\n\n```mermaid\n");
    out.push_str(&mermaid(index, analysis, &Diagram::CallGraph));
    out.push_str("```\n\n");

    let cfg_symbol = pick_cfg_symbol(index, analysis);
    out.push_str(&format!(
        "### Control-flow graph — `{}`\n\n```mermaid\n",
        if cfg_symbol.is_empty() {
            "(none)".to_string()
        } else {
            cfg_symbol.clone()
        }
    ));
    out.push_str(&mermaid(
        index,
        analysis,
        &Diagram::Cfg { symbol: cfg_symbol },
    ));
    out.push_str("```\n\n");

    out.push_str("### Types\n\n```mermaid\n");
    out.push_str(&mermaid(index, analysis, &Diagram::Types));
    out.push_str("```\n\n");

    out.push_str("### Modules\n\n```mermaid\n");
    out.push_str(&mermaid(index, analysis, &Diagram::Modules));
    out.push_str("```\n");
}
