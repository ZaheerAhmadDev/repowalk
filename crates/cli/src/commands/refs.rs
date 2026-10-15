//! `repowalk refs <symbol|variable-id> [--json] <path>` — all use sites (F7).
//! OWNER: PROD-3.
//!
//! Lookups are delegated to `core::refs` (TEXT-3): the CLI resolves the user's
//! argument to a canonical target id and renders the returned [`RefSet`],
//! keeping **resolved** hits apart from **name-only** (unresolved) ones so the
//! command never presents a guess as a confirmed reference (risk R-20). While
//! `core::refs` is still an interface stub this command says so explicitly
//! (nonzero) instead of showing a misleading "0 references".
use crate::commands::artifacts::{self, Artifacts};
use crate::json;
use anyhow::bail;
use repowalk_core::analysis::UseSite;
use repowalk_core::analysis::Variable;
use repowalk_core::model::Symbol;
use repowalk_core::refs::{self, RefSet};
use std::path::Path;

pub fn run(target: &str, json_out: bool, path: &Path) -> anyhow::Result<()> {
    let art = artifacts::load(path)?;
    let target_id = resolve_target(&art, target)?;
    let set = refs::references_ex(&art.analysis, &target_id);

    // Stub guard: `core::refs` (TEXT-3) landed in parallel. If the analysis
    // carries use sites but both the lookup and the module-wide counts come
    // back empty, the module is still a stub — report that honestly rather than
    // pretending the target simply has no references.
    if set.is_empty()
        && !art.analysis.uses.is_empty()
        && refs::reference_counts(&art.analysis).is_empty()
    {
        bail!(
            "reference index unavailable: core::refs is still a stub (TEXT-3 pending); \
             {} use site(s) exist in the analysis for {}",
            art.analysis.uses.len(),
            art.root.display()
        );
    }

    if json_out {
        print!("{}", render_json(&target_id, &set));
    } else {
        print!("{}", render_table(&target_id, &set));
    }
    Ok(())
}

/// Resolve a user-typed target to a canonical id: exact symbol id → exact
/// variable id → unique symbol name → unique variable name → unique name
/// suffix (symbols then variables). Unknown and ambiguous args are honest.
pub fn resolve_target(art: &Artifacts, arg: &str) -> anyhow::Result<String> {
    if let Some(s) = art.index.symbols.iter().find(|s| s.id == arg) {
        return Ok(s.id.clone());
    }
    if let Some(v) = art.analysis.variables.iter().find(|v| v.id == arg) {
        return Ok(v.id.clone());
    }

    let symbols: Vec<&Symbol> = art.index.symbols.iter().filter(|s| s.name == arg).collect();
    match symbols.len() {
        1 => return Ok(symbols[0].id.clone()),
        0 => {}
        n => bail!("ambiguous symbol name: {arg} ({n} symbols)"),
    }
    let vars: Vec<&Variable> = art
        .analysis
        .variables
        .iter()
        .filter(|v| v.name == arg)
        .collect();
    match vars.len() {
        1 => return Ok(vars[0].id.clone()),
        0 => {}
        n => bail!("ambiguous variable name: {arg} ({n} variables)"),
    }

    let symbol_suffix: Vec<&Symbol> = art
        .index
        .symbols
        .iter()
        .filter(|s| s.name.ends_with(arg))
        .collect();
    match symbol_suffix.len() {
        1 => return Ok(symbol_suffix[0].id.clone()),
        0 => {}
        n => bail!("ambiguous symbol name: {arg} ({n} symbols)"),
    }
    let var_suffix: Vec<&Variable> = art
        .analysis
        .variables
        .iter()
        .filter(|v| v.name.ends_with(arg))
        .collect();
    match var_suffix.len() {
        1 => return Ok(var_suffix[0].id.clone()),
        0 => {}
        n => bail!("ambiguous variable name: {arg} ({n} variables)"),
    }

    bail!("unknown symbol or variable: {arg}")
}

fn site_line(s: &UseSite) -> String {
    format!(
        "    {}:{}:{}  {:?}  {}\n",
        s.file, s.line, s.col, s.kind, s.snippet
    )
}

/// Human table: resolved hits first, then name-only hits, each labelled.
pub fn render_table(target: &str, set: &RefSet) -> String {
    use crate::render::{dim, head};
    let mut out = String::new();
    out.push_str(&head(&format!(
        "References — {target} ({} resolved, {} name-only)",
        set.resolved.len(),
        set.by_name.len()
    )));
    out.push('\n');
    if set.is_empty() {
        out.push_str(&format!("  {}\n", dim("(no use sites found)")));
        return out;
    }
    if !set.resolved.is_empty() {
        out.push_str(&format!("  {}\n", dim("resolved")));
        for s in &set.resolved {
            out.push_str(&site_line(s));
        }
    }
    if !set.by_name.is_empty() {
        out.push_str(&format!("  {}\n", dim("name-only (unresolved)")));
        for s in &set.by_name {
            out.push_str(&site_line(s));
        }
    }
    out
}

fn site_json(s: &UseSite) -> String {
    format!(
        "{{\"target\":{},\"file\":{},\"line\":{},\"col\":{},\"kind\":{},\"snippet\":{}}}",
        json::string(&s.target),
        json::string(&s.file),
        s.line,
        s.col,
        json::string(&format!("{:?}", s.kind)),
        json::string(&s.snippet)
    )
}

/// Machine form: `{ "target": ..., "count": N, "resolved": [...], "by_name": [...] }`.
pub fn render_json(target: &str, set: &RefSet) -> String {
    let resolved: Vec<String> = set.resolved.iter().map(site_json).collect();
    let by_name: Vec<String> = set.by_name.iter().map(site_json).collect();
    format!(
        "{{\"target\":{},\"count\":{},\"resolved\":{},\"by_name\":{}}}\n",
        json::string(target),
        set.len(),
        json::array(&resolved),
        json::array(&by_name)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use repowalk_core::analysis::{Analysis, UseKind, VarKind};
    use repowalk_core::model::{Index, SymbolKind};
    use std::path::PathBuf;

    fn sym(id: &str, name: &str) -> Symbol {
        Symbol {
            id: id.into(),
            name: name.into(),
            kind: SymbolKind::Function,
            signature: format!("fn {name}()"),
            doc: String::new(),
            start_line: 1,
            end_line: 2,
            container: None,
            file: "src/util.rs".into(),
        }
    }

    fn art() -> Artifacts {
        let mut index = Index::new("/tmp/demo");
        index.symbols.push(sym("src/util.rs:1", "greet"));
        let mut analysis = Analysis::new();
        analysis.variables.push(Variable {
            id: "src/util.rs:5:name".into(),
            name: "name".into(),
            kind: VarKind::Param,
            file: "src/util.rs".into(),
            decl_line: 5,
            scope_start: 5,
            scope_end: 7,
            type_name: Some("&str".into()),
            reads: 1,
            writes: 0,
            first_use: Some(6),
            last_use: Some(6),
            enclosing: "src/util.rs:1".into(),
        });
        analysis.uses.push(UseSite {
            target: "src/util.rs:5:name".into(),
            file: "src/util.rs".into(),
            line: 6,
            col: 20,
            kind: UseKind::VarRead,
            snippet: "format!(\"{name}\")".into(),
        });
        analysis.uses.push(UseSite {
            target: "name".into(),
            file: "src/util.rs".into(),
            line: 9,
            col: 4,
            kind: UseKind::VarRead,
            snippet: "let n = name;".into(),
        });
        Artifacts {
            index,
            analysis,
            root: PathBuf::from("/tmp/demo"),
        }
    }

    #[test]
    fn resolve_target_finds_symbols_variables_names_and_suffixes() {
        let a = art();
        assert_eq!(
            resolve_target(&a, "src/util.rs:1").unwrap(),
            "src/util.rs:1"
        );
        assert_eq!(
            resolve_target(&a, "src/util.rs:5:name").unwrap(),
            "src/util.rs:5:name"
        );
        assert_eq!(resolve_target(&a, "greet").unwrap(), "src/util.rs:1");
        assert_eq!(resolve_target(&a, "name").unwrap(), "src/util.rs:5:name");
        assert!(resolve_target(&a, "nope")
            .unwrap_err()
            .to_string()
            .contains("unknown"));
    }

    #[test]
    fn table_labels_resolved_and_name_only() {
        let a = art();
        let set = refs::references_ex(&a.analysis, "src/util.rs:5:name");
        assert_eq!(set.resolved.len(), 1);
        assert_eq!(set.by_name.len(), 1);

        let table = render_table("src/util.rs:5:name", &set);
        assert!(table.contains("1 resolved, 1 name-only"), "{table}");
        assert!(table.contains("resolved"), "{table}");
        assert!(table.contains("name-only (unresolved)"), "{table}");
        assert!(table.contains("src/util.rs:6:20"), "{table}");
        assert!(table.contains("format!"), "{table}");
    }

    #[test]
    fn json_splits_resolved_and_by_name() {
        let a = art();
        let set = refs::references_ex(&a.analysis, "src/util.rs:5:name");
        let json = render_json("src/util.rs:5:name", &set);
        assert!(json.contains("\"count\":2"), "{json}");
        assert!(json.contains("\"resolved\":[{"), "{json}");
        assert!(json.contains("\"by_name\":[{"), "{json}");
        assert!(json.contains("\"kind\":\"VarRead\""), "{json}");
        assert!(json.trim_end().ends_with('}'), "{json}");
    }

    #[test]
    fn empty_sites_render_honestly() {
        let set = RefSet::default();
        let table = render_table("x", &set);
        assert!(table.contains("no use sites found"), "{table}");
        assert!(render_json("x", &set).contains("\"count\":0"));
    }
}
