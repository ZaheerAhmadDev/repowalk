//! `repowalk complexity [--by function|file] [--top N] [--json] <path>` (F6).
//! OWNER: PROD-3.
//!
//! All ranking and roll-up is bought from `core::complexity` (`hotspots` /
//! `per_file`) so the CLI and the GUI/report stay in lockstep; `--top 0` means
//! "all" (the aggregate convention). `--by file` truncates the ordered
//! per-file roll-up to `--top`.
use crate::commands::artifacts;
use crate::json;
use repowalk_core::complexity::{self, FileComplexity, Hotspot};
use std::path::Path;

/// Which complexity view to print. `--by` (default `function`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ComplexityBy {
    Function,
    File,
}

pub fn run(by: ComplexityBy, top: usize, json_out: bool, path: &Path) -> anyhow::Result<()> {
    let art = artifacts::load(path)?;
    match by {
        ComplexityBy::Function => {
            let rows = complexity::hotspots(&art.analysis, top);
            if json_out {
                print!("{}", json_functions(&rows));
            } else {
                print!("{}", table_functions(&rows));
            }
        }
        ComplexityBy::File => {
            let mut rows = complexity::per_file(&art.analysis);
            if top > 0 {
                rows.truncate(top);
            }
            if json_out {
                print!("{}", json_files(&rows));
            } else {
                print!("{}", table_files(&rows));
            }
        }
    }
    Ok(())
}

fn table_functions(rows: &[Hotspot]) -> String {
    use crate::render::{dim, head};
    let mut out = String::new();
    out.push_str(&head(&format!("Complexity — {} functions", rows.len())));
    out.push('\n');
    if rows.is_empty() {
        out.push_str(&format!(
            "  {}\n",
            dim("(no control-flow data — re-index or check analyzers)")
        ));
        return out;
    }
    out.push_str("  symbol  cyclomatic  nesting\n");
    for h in rows {
        out.push_str(&format!(
            "  {}  {}  {}\n",
            h.symbol, h.cyclomatic, h.max_nesting
        ));
    }
    out
}

fn table_files(rows: &[FileComplexity]) -> String {
    use crate::render::{dim, head};
    let mut out = String::new();
    out.push_str(&head(&format!("Complexity by file — {} files", rows.len())));
    out.push('\n');
    if rows.is_empty() {
        out.push_str(&format!(
            "  {}\n",
            dim("(no control-flow data — re-index or check analyzers)")
        ));
        return out;
    }
    out.push_str("  file  total  max  functions\n");
    for f in rows {
        out.push_str(&format!(
            "  {}  {}  {}  {}\n",
            f.file, f.total, f.max, f.functions
        ));
    }
    out
}

fn json_functions(rows: &[Hotspot]) -> String {
    let items: Vec<String> = rows
        .iter()
        .map(|h| {
            format!(
                "{{\"symbol\":{},\"file\":{},\"line\":{},\"cyclomatic\":{},\"max_nesting\":{}}}",
                json::string(&h.symbol),
                json::string(&h.file),
                h.line,
                h.cyclomatic,
                h.max_nesting
            )
        })
        .collect();
    format!("{{\"by\":\"function\",\"rows\":{}}}\n", json::array(&items))
}

fn json_files(rows: &[FileComplexity]) -> String {
    let items: Vec<String> = rows
        .iter()
        .map(|f| {
            format!(
                "{{\"file\":{},\"total\":{},\"max\":{},\"functions\":{}}}",
                json::string(&f.file),
                f.total,
                f.max,
                f.functions
            )
        })
        .collect();
    format!("{{\"by\":\"file\",\"rows\":{}}}\n", json::array(&items))
}

#[cfg(test)]
mod tests {
    use super::*;
    use repowalk_core::analysis::{Analysis, FunctionCfg};

    fn cfg(symbol: &str, cyclomatic: u32, nesting: u32) -> FunctionCfg {
        FunctionCfg {
            symbol: symbol.into(),
            nodes: Vec::new(),
            edges: Vec::new(),
            cyclomatic,
            max_nesting: nesting,
        }
    }

    fn analysis() -> Analysis {
        let mut a = Analysis::new();
        a.cfgs = vec![
            cfg("src/a.rs:1", 3, 0),
            cfg("src/b.rs:1", 9, 4),
            cfg("src/a.rs:9", 5, 1),
        ];
        a
    }

    #[test]
    fn function_table_ranks_and_json_matches() {
        let rows = complexity::hotspots(&analysis(), 0);
        let table = table_functions(&rows);
        assert!(table.contains("src/b.rs:1"), "{table}");
        assert!(
            table.find("src/b.rs:1").unwrap() < table.find("src/a.rs:1").unwrap(),
            "{table}"
        );
        let json = json_functions(&rows);
        assert!(json.contains("\"by\":\"function\""), "{json}");
        assert!(json.contains("\"cyclomatic\":9"), "{json}");
    }

    #[test]
    fn file_table_rolls_up_and_honours_top() {
        let mut rows = complexity::per_file(&analysis());
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].file, "src/b.rs");
        assert_eq!(rows[0].total, 9);
        assert_eq!(rows[1].file, "src/a.rs");
        assert_eq!(rows[1].total, 8);
        rows.truncate(1);
        let json = json_files(&rows);
        assert!(json.contains("\"by\":\"file\""), "{json}");
        assert!(json.contains("\"total\":9"), "{json}");
    }

    #[test]
    fn empty_analysis_is_honest_not_a_panic() {
        let table = table_functions(&[]);
        assert!(table.contains("no control-flow data"), "{table}");
        assert!(json_functions(&[]).contains("\"rows\":[]"));
    }
}
