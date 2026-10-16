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
use repowalk_core::analyze;
use repowalk_core::complexity;
use repowalk_core::export;
use repowalk_core::metrics;
use repowalk_core::model::{EdgeKind, Index};
use repowalk_core::rules::{self, Finding, Severity};
use std::collections::BTreeMap;
use std::path::Path;

pub fn run(md: bool, json_out: bool, path: &Path) -> anyhow::Result<()> {
    // Markdown is the default; `--md` only makes that explicit.
    let _ = md;
    let art = artifacts::load(path)?;
    let findings = rules::check(&art.index, &art.analysis, &rules::load(path));
    // R-27: index entries whose language has no analyzer are named explicitly
    // (never silently shown as 0 symbols). Empty on a fully-analyzable repo.
    let unsupported = unsupported_languages(&art.index);

    if json_out {
        print!("{}", render_json(&art, &findings, &unsupported));
    } else {
        let markdown = export::markdown_report(&art.index, &art.analysis);
        print!("{}", with_unsupported_row(&markdown, &unsupported));
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

/// The languages among `index.files` that have no analyzer (R-27), with their
/// file counts, sorted by language. Derived from the same `Lang::from_walk`
/// dispatch the pipeline uses, so it names exactly what `learn` could not
/// extract; empty when every indexed file is analyzable.
fn unsupported_languages(index: &Index) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for f in &index.files {
        if analyze::Lang::from_walk(&f.lang).is_none() {
            *counts.entry(f.lang.as_str()).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .map(|(language, files)| (language.to_string(), files))
        .collect()
}

/// Insert an honest `not analyzed` row into the report's Overview table, e.g.
/// `| not analyzed | rb (1 file) |`. Returns `markdown` **byte-identical** when
/// nothing was skipped, so a fully-analyzable repo is unchanged.
fn with_unsupported_row(markdown: &str, unsupported: &[(String, usize)]) -> String {
    if unsupported.is_empty() {
        return markdown.to_string();
    }
    const LANGUAGE_MIX: &str = "| language mix |";
    let Some(start) = markdown.find(LANGUAGE_MIX) else {
        return markdown.to_string();
    };
    let line_end = markdown[start..]
        .find('\n')
        .map(|i| start + i + 1)
        .unwrap_or(markdown.len());
    let langs: Vec<String> = unsupported
        .iter()
        .map(|(lang, n)| format!("{lang} ({n} file{})", if *n == 1 { "" } else { "s" }))
        .collect();
    let mut out = String::with_capacity(markdown.len() + 48);
    out.push_str(&markdown[..line_end]);
    out.push_str(&format!("| not analyzed | {} |\n", langs.join(", ")));
    out.push_str(&markdown[line_end..]);
    out
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
pub fn render_json(
    art: &Artifacts,
    findings: &[Finding],
    unsupported: &[(String, usize)],
) -> String {
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

    // R-27: additive top-level field, present only when a language was skipped,
    // so a fully-analyzable repo's JSON (and every existing key) is unchanged.
    let unsupported_field = if unsupported.is_empty() {
        String::new()
    } else {
        let entries: Vec<String> = unsupported
            .iter()
            .map(|(lang, n)| format!("{{\"language\":{},\"files\":{n}}}", json::string(lang)))
            .collect();
        format!(",\"unsupported\":{}", json::array(&entries))
    };

    format!(
        "{{\"repo\":{},\"overview\":{},\"complexity\":{},\
         \"unused_variables\":{},\"types\":{},\"top_references\":{},\"guardrails\":{}{unsupported_field}}}\n",
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
            &[],
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

    fn art_with_langs(langs: &[&str]) -> Artifacts {
        let mut a = art(PathBuf::from("/tmp/demo"));
        a.index.files.clear();
        for (i, lang) in langs.iter().enumerate() {
            a.index.files.push(repowalk_core::model::FileEntry {
                path: format!("f{i}.{lang}"),
                lang: (*lang).into(),
                lines: 1,
                mtime: 0,
                is_entrypoint: false,
            });
        }
        a
    }

    #[test]
    fn unsupported_languages_groups_sorted_and_ignores_supported() {
        let a = art_with_langs(&["rust", "rb", "go", "rb", "typescript", "python"]);
        let list = unsupported_languages(&a.index);
        assert_eq!(
            list,
            vec![("go".to_string(), 1), ("rb".to_string(), 2)],
            "{list:?}"
        );
        assert!(unsupported_languages(&art_with_langs(&["rust", "typescript"]).index).is_empty());
    }

    #[test]
    fn overview_gets_a_not_analyzed_row_only_when_needed() {
        let markdown = "## Overview\n\n| metric | value |\n|---|---|\n| language mix | rust (1 file) |\n\n## Module map\n";
        assert_eq!(
            with_unsupported_row(markdown, &[]),
            markdown,
            "empty list must be byte-identical"
        );

        let out = with_unsupported_row(markdown, &[("go".into(), 1), ("rb".into(), 2)]);
        assert!(
            out.contains("| not analyzed | go (1 file), rb (2 files) |"),
            "{out}"
        );
        // Still directly under the `language mix` row and before the next section.
        let overview_row = out.find("| language mix |").unwrap();
        let not_analyzed = out.find("| not analyzed |").unwrap();
        assert!(overview_row < not_analyzed && not_analyzed < out.find("## Module map").unwrap());
    }

    #[test]
    fn json_unsupported_key_is_additive_and_omitted_when_all_analyzable() {
        let clean = render_json(&art_with_langs(&["rust", "python"]), &[], &[]);
        assert!(!clean.contains("\"unsupported\""), "{clean}");

        let out = render_json(
            &art_with_langs(&["rust", "rb"]),
            &[],
            &[("rb".to_string(), 1)],
        );
        assert!(
            out.contains("\"unsupported\":[{\"language\":\"rb\",\"files\":1}]"),
            "{out}"
        );
        // Every pre-existing key is still present.
        for key in [
            "\"repo\":",
            "\"overview\":",
            "\"complexity\":",
            "\"unused_variables\":",
            "\"types\":",
            "\"top_references\":",
            "\"guardrails\":",
        ] {
            assert!(out.contains(key), "missing {key}: {out}");
        }
    }
}
