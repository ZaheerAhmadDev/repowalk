//! `repowalk report [--md] [--json] <path>` — the architecture report.
//! OWNER: PROD-3.
//!
//! Markdown is the default output (self-contained; overview, module map,
//! complexity, types, diagrams — all bought from `core::export::markdown_report`).
//! `--md` selects that explicitly; `--json` emits the *underlying data* the
//! Markdown report renders, for machine consumers.
//!
//! Guardrails (F15 / CORE-6): `.repowalk/rules.json` (falling back to
//! `<root>/rules.json`) is evaluated with `core::rules::check`; findings are
//! appended as a `## Guardrails` section and included in the JSON, and the
//! command **exits nonzero** when any finding is `Severity::Error`.
use crate::commands::artifacts::{self, Artifacts};
use crate::json;
use repowalk_core::complexity;
use repowalk_core::export;
use repowalk_core::metrics;
use repowalk_core::model::EdgeKind;
use repowalk_core::rules::{self, Finding, Severity};
use std::collections::BTreeMap;
use std::path::Path;

pub fn run(md: bool, json_out: bool, path: &Path) -> anyhow::Result<()> {
    // Markdown is the default; `--md` only makes that explicit.
    let _ = md;
    let art = artifacts::load(path)?;
    let findings = rules::check(&art.index, &art.analysis, &rules::load(path));

    if json_out {
        print!("{}", render_json(&art, &findings));
    } else {
        print!("{}", export::markdown_report(&art.index, &art.analysis));
        print!("{}", render_guardrails(&findings));
    }

    let errors = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    if errors > 0 {
        anyhow::bail!("{errors} Error-severity architecture finding(s) in rules.json");
    }
    Ok(())
}

/// `## Guardrails` section appended to the Markdown report.
pub fn render_guardrails(findings: &[Finding]) -> String {
    let mut out = String::from("\n## Guardrails\n\n");
    if findings.is_empty() {
        out.push_str("_No architecture-rule findings._\n");
        return out;
    }
    out.push_str("| severity | rule | file | line | message |\n|---|---|---|---|---|\n");
    for f in findings {
        out.push_str(&format!(
            "| {:?} | {} | `{}` | {} | {} |\n",
            f.severity, f.rule, f.file, f.line, f.message
        ));
    }
    out
}

/// The machine-readable form of the report: the same aggregates the Markdown
/// body renders, plus the rule findings, as one JSON object.
pub fn render_json(art: &Artifacts, findings: &[Finding]) -> String {
    let index = &art.index;
    let analysis = &art.analysis;
    let m = metrics::compute(index);
    let calls = index
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Call)
        .count();

    let languages: Vec<String> = m
        .langs
        .iter()
        .map(|(lang, n)| format!("{}:{}", json::string(lang), n))
        .collect();

    let overview = format!(
        "{{\"files\":{},\"lines\":{},\"symbols\":{},\"call_edges\":{},\
         \"modules\":{},\"types\":{},\"variables\":{},\"languages\":{{{}}}}}",
        index.files.len(),
        m.total_loc,
        index.symbols.len(),
        calls,
        analysis.modules.len(),
        analysis.types.len(),
        analysis.variables.len(),
        languages.join(",")
    );

    let hotspots: Vec<String> = complexity::hotspots(analysis, 0)
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
    let per_file: Vec<String> = complexity::per_file(analysis)
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
    let complexity_obj = format!(
        "{{\"hotspots\":{},\"per_file\":{}}}",
        json::array(&hotspots),
        json::array(&per_file)
    );

    let unused: Vec<String> = complexity::unused_variables(analysis)
        .iter()
        .map(|v| json::string(&v.id))
        .collect();

    let mut types = analysis.types.clone();
    types.sort_by(|a, b| a.file.cmp(&b.file).then_with(|| a.name.cmp(&b.name)));
    let types_json: Vec<String> = types
        .iter()
        .map(|t| {
            format!(
                "{{\"name\":{},\"kind\":{},\"file\":{},\"line\":{},\"fields\":{},\"methods\":{}}}",
                json::string(&t.name),
                json::string(&format!("{:?}", t.kind)),
                json::string(&t.file),
                t.line,
                t.fields.len(),
                t.methods.len()
            )
        })
        .collect();

    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for u in &analysis.uses {
        *counts.entry(u.target.as_str()).or_default() += 1;
    }
    let mut ranked: Vec<(&str, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let refs_json: Vec<String> = ranked
        .iter()
        .take(10)
        .map(|(target, n)| format!("{{\"target\":{},\"uses\":{}}}", json::string(target), n))
        .collect();

    let findings_json: Vec<String> = findings
        .iter()
        .map(|f| {
            format!(
                "{{\"rule\":{},\"severity\":{},\"file\":{},\"line\":{},\"message\":{}}}",
                json::string(&f.rule),
                json::string(&format!("{:?}", f.severity)),
                json::string(&f.file),
                f.line,
                json::string(&f.message)
            )
        })
        .collect();
    let errors = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    let warnings = findings.len() - errors;
    let guardrails = format!(
        "{{\"errors\":{errors},\"warnings\":{warnings},\"findings\":{}}}",
        json::array(&findings_json)
    );

    format!(
        "{{\"repo\":{},\"overview\":{},\"complexity\":{},\
         \"unused_variables\":{},\"types\":{},\"top_references\":{},\"guardrails\":{}}}\n",
        json::string(&index.repo_root),
        overview,
        complexity_obj,
        json::array(&unused),
        json::array(&types_json),
        json::array(&refs_json),
        guardrails
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use repowalk_core::analysis::{Analysis, FunctionCfg, TypeKind, TypeNode};
    use repowalk_core::model::Index;
    use std::path::PathBuf;

    fn art(root: PathBuf) -> Artifacts {
        let mut index = Index::new(root.to_string_lossy());
        index.files.push(repowalk_core::model::FileEntry {
            path: "src/a.rs".into(),
            lang: "rust".into(),
            lines: 10,
            mtime: 0,
            is_entrypoint: false,
        });
        let mut analysis = Analysis::new();
        analysis.cfgs.push(FunctionCfg {
            symbol: "src/a.rs:1".into(),
            nodes: Vec::new(),
            edges: Vec::new(),
            cyclomatic: 4,
            max_nesting: 2,
        });
        analysis.types.push(TypeNode {
            id: "src/a.rs:Person".into(),
            name: "Person".into(),
            kind: TypeKind::Struct,
            file: "src/a.rs".into(),
            line: 20,
            fields: Vec::new(),
            methods: Vec::new(),
            doc: String::new(),
            module_path: String::new(),
        });
        Artifacts {
            index,
            analysis,
            root,
        }
    }

    fn finding(sev: Severity) -> Finding {
        Finding {
            rule: "max_cyclomatic".into(),
            severity: sev,
            file: "src/a.rs".into(),
            line: 1,
            message: "too complex".into(),
        }
    }

    #[test]
    fn json_has_overview_complexity_and_guardrails() {
        let out = render_json(
            &art(PathBuf::from("/tmp/demo")),
            &[finding(Severity::Error)],
        );
        assert!(
            out.starts_with('{') && out.trim_end().ends_with('}'),
            "{out}"
        );
        assert!(out.contains("\"overview\":"), "{out}");
        assert!(out.contains("\"files\":1"), "{out}");
        assert!(out.contains("\"hotspots\":["), "{out}");
        assert!(out.contains("\"cyclomatic\":4"), "{out}");
        assert!(out.contains("\"per_file\":["), "{out}");
        assert!(out.contains("\"kind\":\"Struct\""), "{out}");
        assert!(
            out.contains("\"guardrails\":{\"errors\":1,\"warnings\":0"),
            "{out}"
        );
        assert!(out.contains("\"severity\":\"Error\""), "{out}");
        assert!(!out.contains('\u{1b}'), "no ANSI in JSON");
    }

    #[test]
    fn guardrails_section_lists_findings_or_says_none() {
        let none = render_guardrails(&[]);
        assert!(none.contains("## Guardrails"), "{none}");
        assert!(none.contains("No architecture-rule findings"), "{none}");

        let some = render_guardrails(&[finding(Severity::Error)]);
        assert!(some.contains("| severity | rule |"), "{some}");
        assert!(some.contains("| Error | max_cyclomatic |"), "{some}");
    }
}
