//! `repowalk graph [--format mermaid|dot|svg] [--scope call|cfg|types|modules|tree] [--symbol <id>] <path>`
//! OWNER: PROD-3.
//!
//! Renders one diagram to stdout by delegating to the PROD-2 emitters
//! (`core::export::{mermaid,dot,svg}`) — no rendering lives in the CLI.
//! `--scope cfg` requires `--symbol`; an unknown/ambiguous symbol exits nonzero
//! with a clear message. `--scope tree` draws the directory tree of the indexed
//! files (real names, LOC labels, entrypoint markers).
use crate::commands::artifacts::{self, Artifacts};
use anyhow::{anyhow, bail};
use repowalk_core::export::{self, Diagram};
use repowalk_core::model::{Index, Symbol};
use std::path::Path;

/// Diagram backend. `--format` (default `mermaid`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum GraphFormat {
    Mermaid,
    Dot,
    Svg,
}

/// Which diagram to draw. `--scope` (default `call`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum GraphScope {
    /// Symbol call graph over the whole index
    Call,
    /// Control-flow graph of one `--symbol`
    Cfg,
    /// Type / class diagram
    Types,
    /// Crate / module structure with import edges
    Modules,
    /// Directory tree of the really indexed files (real names + LOC)
    Tree,
}

pub fn run(
    format: GraphFormat,
    scope: GraphScope,
    symbol: Option<&str>,
    path: &Path,
) -> anyhow::Result<()> {
    let art = artifacts::load(path)?;
    let diagram = diagram_for_scope(scope, symbol, &art.index)?;
    print!("{}", render(format, &art, &diagram));
    Ok(())
}

/// Map a requested scope (and `--symbol`) to a diagram. Kept separate from
/// `run` so scope policy (`--symbol` rejection, symbol resolution, tree →
/// `Diagram::Directory`) is testable without a repository on disk.
pub fn diagram_for_scope(
    scope: GraphScope,
    symbol: Option<&str>,
    index: &Index,
) -> anyhow::Result<Diagram> {
    match scope {
        GraphScope::Call => {
            reject_symbol(scope, symbol)?;
            Ok(Diagram::CallGraph)
        }
        GraphScope::Types => {
            reject_symbol(scope, symbol)?;
            Ok(Diagram::Types)
        }
        GraphScope::Modules => {
            reject_symbol(scope, symbol)?;
            Ok(Diagram::Modules)
        }
        GraphScope::Tree => {
            reject_symbol(scope, symbol)?;
            // The plain tree: dashed aggregated import edges stay off unless a
            // caller (GUI export) asks for them explicitly.
            Ok(Diagram::Directory { imports: false })
        }
        GraphScope::Cfg => {
            let arg = symbol.ok_or_else(|| anyhow!("--scope cfg requires --symbol <id>"))?;
            let sym = resolve_symbol(index, arg)?;
            Ok(Diagram::Cfg {
                symbol: sym.id.clone(),
            })
        }
    }
}

/// `--symbol` only means something for `--scope cfg`; refuse to silently ignore
/// it elsewhere.
fn reject_symbol(scope: GraphScope, symbol: Option<&str>) -> anyhow::Result<()> {
    if let Some(s) = symbol {
        bail!("--symbol {s:?} is only valid with --scope cfg (this scope is {scope:?})");
    }
    Ok(())
}

/// Render `diagram` with the requested backend, newline-terminated.
pub fn render(format: GraphFormat, art: &Artifacts, diagram: &Diagram) -> String {
    let mut out = match format {
        GraphFormat::Mermaid => export::mermaid(&art.index, &art.analysis, diagram),
        GraphFormat::Dot => export::dot(&art.index, &art.analysis, diagram),
        GraphFormat::Svg => export::svg(&art.index, &art.analysis, diagram),
    };
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Resolve a user-typed `--symbol`: exact id → unique name → unique name
/// suffix. Ambiguous or unknown args are honest errors.
pub fn resolve_symbol<'a>(index: &'a Index, arg: &str) -> anyhow::Result<&'a Symbol> {
    if let Some(sym) = index.symbols.iter().find(|s| s.id == arg) {
        return Ok(sym);
    }
    let named: Vec<&Symbol> = index.symbols.iter().filter(|s| s.name == arg).collect();
    match named.len() {
        1 => return Ok(named[0]),
        0 => {}
        n => bail!(
            "ambiguous symbol: {arg} ({n} matches: {})",
            id_list(&named, 5)
        ),
    }
    let suffixed: Vec<&Symbol> = index
        .symbols
        .iter()
        .filter(|s| s.name.ends_with(arg))
        .collect();
    match suffixed.len() {
        1 => Ok(suffixed[0]),
        0 => bail!("unknown symbol: {arg}"),
        n => bail!(
            "ambiguous symbol: {arg} ({n} matches: {})",
            id_list(&suffixed, 5)
        ),
    }
}

/// Up to `cap` symbol ids joined for an ambiguity message.
fn id_list(symbols: &[&Symbol], cap: usize) -> String {
    let mut ids: Vec<&str> = symbols.iter().map(|s| s.id.as_str()).collect();
    ids.sort();
    let shown = ids.len().min(cap);
    let mut s = ids[..shown].join(", ");
    if ids.len() > shown {
        s.push_str(&format!(", +{} more", ids.len() - shown));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use repowalk_core::analysis::Analysis;
    use repowalk_core::model::SymbolKind;
    use std::path::PathBuf;

    fn symbol(id: &str, name: &str) -> Symbol {
        Symbol {
            id: id.into(),
            name: name.into(),
            kind: SymbolKind::Function,
            signature: format!("fn {name}()"),
            doc: String::new(),
            start_line: 1,
            end_line: 2,
            container: None,
            file: "src/a.rs".into(),
        }
    }

    fn art() -> Artifacts {
        let mut index = Index::new("/tmp/demo");
        index.symbols.push(symbol("src/a.rs:1", "main"));
        index.symbols.push(symbol("src/a.rs:5", "helper"));
        Artifacts {
            index,
            analysis: Analysis::new(),
            root: PathBuf::from("/tmp/demo"),
        }
    }

    #[test]
    fn resolve_tiers_id_name_and_suffix() {
        let a = art();
        assert_eq!(
            resolve_symbol(&a.index, "src/a.rs:5").unwrap().name,
            "helper"
        );
        assert_eq!(resolve_symbol(&a.index, "main").unwrap().id, "src/a.rs:1");
        assert_eq!(resolve_symbol(&a.index, "elper").unwrap().id, "src/a.rs:5");
    }

    #[test]
    fn unknown_and_ambiguous_are_honest_errors() {
        let a = art();
        let err = resolve_symbol(&a.index, "nope").unwrap_err();
        assert!(err.to_string().contains("unknown symbol: nope"), "{err}");

        let mut index = Index::new("/tmp/demo");
        index.symbols.push(symbol("x.rs:1", "dup"));
        index.symbols.push(symbol("y.rs:1", "dup"));
        let err = resolve_symbol(&index, "dup").unwrap_err();
        assert!(err.to_string().contains("ambiguous symbol: dup"), "{err}");
    }

    #[test]
    fn renders_all_three_backends() {
        let a = art();
        let mermaid = render(GraphFormat::Mermaid, &a, &Diagram::CallGraph);
        assert!(mermaid.starts_with("flowchart"), "{mermaid}");
        let dot = render(GraphFormat::Dot, &a, &Diagram::CallGraph);
        assert!(dot.starts_with("digraph"), "{dot}");
        let svg = render(GraphFormat::Svg, &a, &Diagram::CallGraph);
        assert!(svg.contains("<svg"), "{svg}");
        assert!(svg.ends_with('\n'));
    }

    #[test]
    fn tree_scope_rejects_symbol_and_maps_to_the_directory_diagram() {
        // --symbol must be refused for tree, exactly like types/modules/call.
        let index = Index::new("/tmp/demo");
        let err = diagram_for_scope(GraphScope::Tree, Some("main"), &index).unwrap_err();
        assert!(
            err.to_string().contains("--symbol"),
            "symbol rejected for tree: {err}"
        );

        let diagram = diagram_for_scope(GraphScope::Tree, None, &index).unwrap();
        assert_eq!(
            diagram,
            Diagram::Directory { imports: false },
            "tree maps to Directory with import edges off"
        );
    }
}
