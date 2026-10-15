//! Complexity + hotspot aggregates over the v2 analysis (F6).
//!
//! OWNER: CORE-03/04 (card CORE-4). The names below are the agreed starting
//! contract for health-v2 / CLI / report consumers; extend them, but do not
//! rename or change the meaning of an existing one without messaging PMO.
//!
//! Every function is a pure read of [`Analysis`] and is deterministic: the same
//! artifact always yields the same vector, regardless of the order the
//! extractors appended rows in. Ties are broken by an explicit, stable key
//! (symbol id / file path / declaration line) so ordering never leaks input
//! order into the UI.
use crate::analysis::{Analysis, FunctionCfg, Variable};
use std::collections::BTreeMap;

/// One ranked control-flow hotspot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotspot {
    /// Symbol id (`file:line`).
    pub symbol: String,
    pub file: String,
    pub line: usize,
    pub cyclomatic: u32,
    pub max_nesting: u32,
}

/// Per-file complexity roll-up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileComplexity {
    pub file: String,
    pub total: u32,
    pub max: u32,
    pub functions: usize,
}

/// Split a canonical symbol id (`src/main.rs:12`) into `(file, line)`.
/// Returns `(id, 0)` when the id has no trailing line number.
pub fn parse_symbol_id(id: &str) -> (String, usize) {
    match id.rsplit_once(':') {
        Some((file, line)) if !file.is_empty() => match line.parse::<usize>() {
            Ok(n) => (file.to_string(), n),
            Err(_) => (id.to_string(), 0),
        },
        _ => (id.to_string(), 0),
    }
}

/// Project one CFG onto a [`Hotspot`], splitting its symbol id.
fn hotspot_of(cfg: &FunctionCfg) -> Hotspot {
    let (file, line) = parse_symbol_id(&cfg.symbol);
    Hotspot {
        symbol: cfg.symbol.clone(),
        file,
        line,
        cyclomatic: cfg.cyclomatic,
        max_nesting: cfg.max_nesting,
    }
}

/// Most complex functions first, capped at `top` (0 = all).
///
/// Ranked by cyclomatic complexity (desc), ties by symbol id (asc), so the
/// list is identical for a given analysis no matter how the CFGs were ordered.
pub fn hotspots(analysis: &Analysis, top: usize) -> Vec<Hotspot> {
    let mut out: Vec<Hotspot> = analysis.cfgs.iter().map(hotspot_of).collect();
    out.sort_by(|a, b| {
        b.cyclomatic
            .cmp(&a.cyclomatic)
            .then_with(|| a.symbol.cmp(&b.symbol))
    });
    if top > 0 {
        out.truncate(top);
    }
    out
}

/// Functions with the deepest nesting first, capped at `top` (0 = all).
///
/// Same projection as [`hotspots`] but ranked by `max_nesting` (desc), ties by
/// symbol id (asc). Cheap to compute from the already-walked CFGs.
pub fn nested(analysis: &Analysis, top: usize) -> Vec<Hotspot> {
    let mut out: Vec<Hotspot> = analysis.cfgs.iter().map(hotspot_of).collect();
    out.sort_by(|a, b| {
        b.max_nesting
            .cmp(&a.max_nesting)
            .then_with(|| a.symbol.cmp(&b.symbol))
    });
    if top > 0 {
        out.truncate(top);
    }
    out
}

/// Per-file roll-up, sorted by total complexity (desc), then file path.
///
/// `total` sums the file's function complexities, `max` is its single most
/// complex function, `functions` counts the CFGs attributed to the file.
pub fn per_file(analysis: &Analysis) -> Vec<FileComplexity> {
    let mut by_file: BTreeMap<String, FileComplexity> = BTreeMap::new();
    for cfg in &analysis.cfgs {
        let (file, _) = parse_symbol_id(&cfg.symbol);
        let entry = by_file.entry(file.clone()).or_insert(FileComplexity {
            file,
            total: 0,
            max: 0,
            functions: 0,
        });
        entry.total = entry.total.saturating_add(cfg.cyclomatic);
        entry.max = entry.max.max(cfg.cyclomatic);
        entry.functions += 1;
    }

    let mut out: Vec<FileComplexity> = by_file.into_values().collect();
    out.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.file.cmp(&b.file)));
    out
}

/// Variables with `reads == 0` — dead-code candidates (F4/F6 surface).
///
/// Sorted by file, then declaration line, then name, so the panel order is
/// stable. An empty analysis yields an empty list.
pub fn unused_variables(analysis: &Analysis) -> Vec<&Variable> {
    let mut out: Vec<&Variable> = analysis.variables.iter().filter(|v| v.reads == 0).collect();
    out.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.decl_line.cmp(&b.decl_line))
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{FunctionCfg, VarKind};

    fn cfg(symbol: &str, cyclomatic: u32, max_nesting: u32) -> FunctionCfg {
        FunctionCfg {
            symbol: symbol.into(),
            nodes: Vec::new(),
            edges: Vec::new(),
            cyclomatic,
            max_nesting,
        }
    }

    fn var(file: &str, decl_line: usize, name: &str, reads: usize) -> Variable {
        Variable {
            id: format!("{file}:{decl_line}:{name}"),
            name: name.into(),
            kind: VarKind::Local,
            file: file.into(),
            decl_line,
            scope_start: decl_line,
            scope_end: decl_line + 1,
            type_name: None,
            reads,
            writes: 0,
            first_use: None,
            last_use: None,
            enclosing: String::new(),
        }
    }

    #[test]
    fn parse_symbol_id_splits_file_and_line() {
        assert_eq!(
            parse_symbol_id("src/main.rs:12"),
            ("src/main.rs".into(), 12)
        );
        assert_eq!(parse_symbol_id("main"), ("main".into(), 0));
        assert_eq!(parse_symbol_id(""), ("".into(), 0));
    }

    #[test]
    fn hotspots_rank_by_cyclomatic_then_symbol_id() {
        let mut a = Analysis::new();
        a.cfgs = vec![
            cfg("src/a.rs:10", 5, 2),
            cfg("src/b.rs:1", 9, 1),
            cfg("src/a.rs:1", 5, 4),
        ];

        let all = hotspots(&a, 0);
        assert_eq!(
            all.iter().map(|h| h.symbol.as_str()).collect::<Vec<_>>(),
            vec!["src/b.rs:1", "src/a.rs:1", "src/a.rs:10"]
        );
        assert_eq!(all[0].file, "src/b.rs");
        assert_eq!(all[0].line, 1);
        assert_eq!(all[0].cyclomatic, 9);

        // `top` caps the list; 0 means "all".
        let top = hotspots(&a, 2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].symbol, "src/b.rs:1");
        assert_eq!(hotspots(&a, 0).len(), 3);
    }

    #[test]
    fn nested_ranks_by_max_nesting_then_symbol_id() {
        let mut a = Analysis::new();
        a.cfgs = vec![
            cfg("f.rs:1", 1, 3),
            cfg("f.rs:5", 9, 1),
            cfg("f.rs:9", 1, 3),
        ];

        let n = nested(&a, 0);
        assert_eq!(
            n.iter().map(|h| h.symbol.as_str()).collect::<Vec<_>>(),
            vec!["f.rs:1", "f.rs:9", "f.rs:5"]
        );
        assert_eq!(nested(&a, 1).len(), 1);
    }

    #[test]
    fn per_file_rolls_up_and_orders_by_total_then_path() {
        let mut a = Analysis::new();
        a.cfgs = vec![
            cfg("src/a.rs:1", 3, 0),
            cfg("src/a.rs:9", 4, 0),
            cfg("src/b.rs:1", 6, 0),
            cfg("src/c.rs:1", 6, 0),
        ];

        let pf = per_file(&a);
        assert_eq!(
            pf,
            vec![
                FileComplexity {
                    file: "src/a.rs".into(),
                    total: 7,
                    max: 4,
                    functions: 2
                },
                FileComplexity {
                    file: "src/b.rs".into(),
                    total: 6,
                    max: 6,
                    functions: 1
                },
                FileComplexity {
                    file: "src/c.rs".into(),
                    total: 6,
                    max: 6,
                    functions: 1
                },
            ]
        );
    }

    #[test]
    fn unused_variables_filters_and_sorts() {
        let mut a = Analysis::new();
        a.variables = vec![
            var("src/b.rs", 5, "z", 0),
            var("src/b.rs", 2, "y", 3),
            var("src/a.rs", 9, "x", 0),
            var("src/a.rs", 1, "w", 0),
        ];

        let unused = unused_variables(&a);
        assert_eq!(
            unused.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(),
            vec!["src/a.rs:1:w", "src/a.rs:9:x", "src/b.rs:5:z"]
        );
    }

    #[test]
    fn empty_analysis_yields_empty_aggregates() {
        let a = Analysis::new();
        assert!(hotspots(&a, 0).is_empty());
        assert!(nested(&a, 3).is_empty());
        assert!(per_file(&a).is_empty());
        assert!(unused_variables(&a).is_empty());
    }
}
