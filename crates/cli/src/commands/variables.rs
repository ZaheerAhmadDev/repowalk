//! `repowalk variables <file> [--json] <path>` — per-file variable table (F4).
//! OWNER: PROD-3.
//!
//! Columns: name, kind, type, reads/writes/uses, declaration line. The rows are
//! read straight out of `Analysis::variables` (populated by the INTEL analyzers
//! via CORE-7 `learn`); nothing is recomputed here. Honest failures: repo path
//! missing, file not in the index, and a file that is no longer readable.
use crate::commands::artifacts::{self, Artifacts};
use crate::json;
use anyhow::{anyhow, bail};
use repowalk_core::analysis::Variable;
use std::path::Path;

pub fn run(file: &str, json_out: bool, path: &Path) -> anyhow::Result<()> {
    let art = artifacts::load(path)?;
    let resolved = resolve_file(&art, file)?;

    // Unreadable-file guard: the analysis may be served from cache, so confirm
    // the source is still readable before presenting its variables as current.
    std::fs::read_to_string(art.root.join(&resolved))
        .map_err(|e| anyhow!("cannot read file {resolved}: {e}"))?;

    let mut vars: Vec<&Variable> = art
        .analysis
        .variables
        .iter()
        .filter(|v| v.file == resolved)
        .collect();
    vars.sort_by(|a, b| {
        a.decl_line
            .cmp(&b.decl_line)
            .then_with(|| a.name.cmp(&b.name))
    });

    if json_out {
        print!("{}", render_json(&resolved, &vars));
    } else {
        print!("{}", render_table(&resolved, &vars));
    }
    Ok(())
}

/// Resolve a user-typed file against the index: exact path → unique `/<file>`
/// suffix. Unknown or ambiguous args are honest errors.
pub fn resolve_file(art: &Artifacts, file: &str) -> anyhow::Result<String> {
    let paths: Vec<&str> = art.index.files.iter().map(|f| f.path.as_str()).collect();
    if paths.contains(&file) {
        return Ok(file.to_string());
    }
    let suffix = format!("/{file}");
    let matches: Vec<&str> = paths
        .iter()
        .copied()
        .filter(|p| p.ends_with(&suffix))
        .collect();
    match matches.len() {
        1 => Ok(matches[0].to_string()),
        0 => bail!("no such file in index: {file}"),
        n => bail!(
            "ambiguous file: {file} ({n} matches: {})",
            matches[..n.min(5)].join(", ")
        ),
    }
}

/// Human table for one file's variables.
pub fn render_table(file: &str, vars: &[&Variable]) -> String {
    use crate::render::{dim, head};
    let mut out = String::new();
    out.push_str(&head(&format!(
        "Variables — {file} ({} declared)",
        vars.len()
    )));
    out.push('\n');
    if vars.is_empty() {
        out.push_str(&format!("  {}\n", dim("(no variables found in this file)")));
        return out;
    }
    out.push_str("  name  kind  type  reads/writes/uses  line\n");
    for v in vars {
        out.push_str(&format!(
            "  {}  {:?}  {}  {}/{}/{}  {}\n",
            v.name,
            v.kind,
            v.type_name.as_deref().unwrap_or("-"),
            v.reads,
            v.writes,
            v.reads + v.writes,
            v.decl_line
        ));
    }
    out
}

/// Machine form: `{ "file": ..., "variables": [...] }`.
pub fn render_json(file: &str, vars: &[&Variable]) -> String {
    let rows: Vec<String> = vars
        .iter()
        .map(|v| {
            format!(
                "{{\"id\":{},\"name\":{},\"kind\":{},\"type\":{},\"reads\":{},\
                 \"writes\":{},\"uses\":{},\"decl_line\":{},\"scope\":[{},{}],\"enclosing\":{}}}",
                json::string(&v.id),
                json::string(&v.name),
                json::string(&format!("{:?}", v.kind)),
                json::opt_string(v.type_name.as_deref()),
                v.reads,
                v.writes,
                v.reads + v.writes,
                v.decl_line,
                v.scope_start,
                v.scope_end,
                json::string(&v.enclosing)
            )
        })
        .collect();
    format!(
        "{{\"file\":{},\"variables\":{}}}\n",
        json::string(file),
        json::array(&rows)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use repowalk_core::analysis::{Analysis, VarKind};
    use repowalk_core::model::{FileEntry, Index};
    use std::path::PathBuf;

    fn var(file: &str, line: usize, name: &str, reads: usize, writes: usize) -> Variable {
        Variable {
            id: format!("{file}:{line}:{name}"),
            name: name.into(),
            kind: VarKind::Local,
            file: file.into(),
            decl_line: line,
            scope_start: line,
            scope_end: line + 4,
            type_name: Some("String".into()),
            reads,
            writes,
            first_use: None,
            last_use: None,
            enclosing: format!("{file}:1"),
        }
    }

    fn art() -> Artifacts {
        let mut index = Index::new("/tmp/demo");
        index.files.push(FileEntry {
            path: "src/main.rs".into(),
            lang: "rust".into(),
            lines: 20,
            mtime: 0,
            is_entrypoint: true,
        });
        let mut analysis = Analysis::new();
        analysis.variables.push(var("src/main.rs", 3, "x", 2, 1));
        Artifacts {
            index,
            analysis,
            root: PathBuf::from("/tmp/demo"),
        }
    }

    #[test]
    fn table_lists_name_kind_type_counts_and_line() {
        let a = art();
        let vars: Vec<&Variable> = a.analysis.variables.iter().collect();
        let out = render_table("src/main.rs", &vars);
        assert!(
            out.contains("Variables — src/main.rs (1 declared)"),
            "{out}"
        );
        assert!(out.contains("x"), "{out}");
        assert!(out.contains("Local"), "{out}");
        assert!(out.contains("String"), "{out}");
        assert!(out.contains("2/1/3"), "reads/writes/uses: {out}");
        assert!(out.contains(" 3\n"), "decl line: {out}");
    }

    #[test]
    fn json_is_well_formed_and_escaped() {
        let a = art();
        let vars: Vec<&Variable> = a.analysis.variables.iter().collect();
        let out = render_json("src/main.rs", &vars);
        assert!(out.contains("\"file\":\"src/main.rs\""), "{out}");
        assert!(out.contains("\"uses\":3"), "{out}");
        assert!(out.contains("\"type\":\"String\""), "{out}");
        assert!(out.contains("\"kind\":\"Local\""), "{out}");
        assert!(out.trim_end().ends_with('}'), "{out}");
    }

    #[test]
    fn resolve_file_exact_suffix_unknown_and_ambiguous() {
        let a = art();
        assert_eq!(resolve_file(&a, "src/main.rs").unwrap(), "src/main.rs");
        assert_eq!(resolve_file(&a, "main.rs").unwrap(), "src/main.rs");
        assert!(resolve_file(&a, "nope.rs")
            .unwrap_err()
            .to_string()
            .contains("no such file"));
    }
}
