//! Full index pipeline: walk → parse by lang → resolve → timestamp (+ store cache).
//! OWNER: CORE-7 (v2); R-27 language-dispatch hardening by CORE-8. This is what
//! CLI and GUI call.
//!
//! # Language dispatch (R-27)
//!
//! Every file's language is mapped through [`crate::analyze::Lang::from_walk`].
//! Rust/Python use the v1 symbol extractors ([`crate::parse_rust`],
//! [`crate::parse_py`]); TypeScript/JavaScript have no v1 parser, so their
//! symbols/call-sites are lifted from the v2 analyzer
//! ([`crate::analyze::analyze`]) into the frozen index shapes. A language with
//! no analyzer is **never** silently dropped: [`build_index_with_report`]
//! returns an explicit `(language, file_count)` list so a caller can say
//! "N files not analyzed (language)" instead of showing 0 symbols.
use crate::analysis::{Analysis, FunctionCfg, TypeKind, UseKind};
use crate::analyze::Lang;
use crate::model::{CallSite, FileEntry, Import, Index, Symbol, SymbolKind};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// A language with no analyzer: its files are indexed but nothing is extracted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedLanguage {
    pub language: String,
    pub files: usize,
}

/// A full index build plus an explicit account of what was *not* analyzed.
///
/// `unsupported` is sorted by language; it is empty when every file was
/// analyzed. This is the non-silent signal R-27 requires — the plain
/// [`build_index`] still returns only the frozen [`Index`] for existing callers.
#[derive(Debug, Clone)]
pub struct IndexReport {
    pub index: Index,
    pub unsupported: Vec<UnsupportedLanguage>,
}

impl IndexReport {
    /// `"N files not analyzed (lang1, lang2)"`, or `None` when everything was
    /// analyzed. A caller can surface this instead of silently showing
    /// zero symbols for a language it claims to support.
    pub fn unsupported_summary(&self) -> Option<String> {
        if self.unsupported.is_empty() {
            return None;
        }
        let total: usize = self.unsupported.iter().map(|u| u.files).sum();
        let langs: Vec<&str> = self
            .unsupported
            .iter()
            .map(|u| u.language.as_str())
            .collect();
        Some(format!("{total} files not analyzed ({})", langs.join(", ")))
    }
}

/// Build (or load from valid cache) the index for `root`.
/// `progress(files_done)` is called during walks/parses for UI overlays.
pub fn build_index(root: &Path, progress: &dyn Fn(usize)) -> anyhow::Result<Index> {
    Ok(build_index_with_report(root, progress)?.index)
}

/// Like [`build_index`] but also reports the languages that could not be
/// analyzed (R-27), so a caller never claims a language while dropping it.
pub fn build_index_with_report(
    root: &Path,
    progress: &dyn Fn(usize),
) -> anyhow::Result<IndexReport> {
    let files = crate::walk::walk(root)?;
    build_index_report_from_files(
        root,
        files,
        &|phase, done, _total| {
            if phase == "parse" {
                progress(done);
            }
        },
        None,
    )
}

/// Build the index from an already-walked file list, reporting
/// `progress(phase, done, total)` for the `parse` and `resolve` phases.
///
/// `cancel` is cooperative (model audit F-1): the per-file parse loop checks it
/// before each file and the cache write is skipped when it is set, so a
/// cancelled build returns the partial index and **persists nothing**. The
/// public [`build_index`]/[`build_index_with_report`] pass `None` and are
/// unaffected.
///
/// Split out of [`build_index`] so the v2 `learn` driver can reuse the walk it
/// already needed for its incremental stamps (F10) instead of walking twice,
/// while keeping all resolve/timestamp/cache logic in one place.
pub(crate) fn build_index_from_files(
    root: &Path,
    files: Vec<FileEntry>,
    progress: &dyn Fn(&'static str, usize, usize),
    cancel: Option<&AtomicBool>,
) -> anyhow::Result<Index> {
    Ok(build_index_report_from_files(root, files, progress, cancel)?.index)
}

/// Shared implementation behind [`build_index_from_files`] /
/// [`build_index_with_report`]: same phases and cache behaviour, plus the
/// unsupported-language list. See [`build_index_from_files`] for the `cancel`
/// contract.
fn build_index_report_from_files(
    root: &Path,
    files: Vec<FileEntry>,
    progress: &dyn Fn(&'static str, usize, usize),
    cancel: Option<&AtomicBool>,
) -> anyhow::Result<IndexReport> {
    let total = files.len();
    let unsupported = unsupported_languages(&files);
    if crate::store::cache_valid(root, &files) {
        progress("parse", total, total);
        progress("resolve", total, total);
        let index = crate::store::load(root).expect("cache_valid implies a loadable cache");
        return Ok(IndexReport { index, unsupported });
    }

    let cancelled = || cancel.is_some_and(|c| c.load(Ordering::Relaxed));
    let mut idx = Index::new(root.to_string_lossy());
    idx.files = files;
    progress("parse", 0, total);
    for (done, file) in idx.files.iter().enumerate() {
        if cancelled() {
            // A cancelled build must never persist: return the partial index
            // and leave the on-disk cache exactly as it was (F-1).
            return Ok(IndexReport {
                index: idx,
                unsupported,
            });
        }
        let text = std::fs::read_to_string(root.join(&file.path))?;
        let (symbols, sites, imports) = extract_symbols(&file.path, &file.lang, &text);
        idx.symbols.extend(symbols);
        idx.call_sites.extend(sites);
        idx.imports.extend(imports);
        progress("parse", done + 1, total);
    }
    if cancelled() {
        return Ok(IndexReport {
            index: idx,
            unsupported,
        });
    }

    let mut edges = crate::resolve::resolve(&idx.files, &idx.symbols, &idx.call_sites);
    edges.extend(crate::resolve::resolve_imports(&idx.files, &idx.imports));
    idx.edges = edges;
    idx.indexed_at = timestamp_now_utc();
    crate::store::save(root, &idx)?;
    progress("resolve", total, total);
    Ok(IndexReport {
        index: idx,
        unsupported,
    })
}

/// Route one file through [`Lang::from_walk`] (R-27). Unsupported languages
/// contribute nothing here — they are surfaced by [`unsupported_languages`].
fn extract_symbols(
    path: &str,
    lang: &str,
    source: &str,
) -> (Vec<Symbol>, Vec<CallSite>, Vec<Import>) {
    match Lang::from_walk(lang) {
        Some(Lang::Rust) => crate::parse_rust::extract(source, path),
        Some(Lang::Python) => crate::parse_py::extract(source, path),
        Some(lang @ (Lang::TypeScript | Lang::JavaScript)) => {
            extract_ts_via_analyze(path, lang, source)
        }
        None => (Vec::new(), Vec::new(), Vec::new()),
    }
}

/// Files whose language has no analyzer, grouped and sorted by language.
fn unsupported_languages(files: &[FileEntry]) -> Vec<UnsupportedLanguage> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for f in files {
        if Lang::from_walk(&f.lang).is_none() {
            *counts.entry(f.lang.clone()).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .map(|(language, files)| UnsupportedLanguage { language, files })
        .collect()
}

/// Lift the v2 TS/JS analyzer output into the frozen index shapes (R-27).
///
/// There is no v1 `parse_ts`; instead the analyzer's symbol-bearing output is
/// used: one `Function` symbol per function CFG, one type symbol per
/// [`crate::analysis::TypeNode`], and `CallSite`s from its `Call` use-sites.
/// The analyzer runs with an empty symbol slice (exactly as the golden
/// harness does), so unresolved callees keep their raw name and
/// [`crate::resolve`] still matches them by name.
fn extract_ts_via_analyze(
    path: &str,
    lang: Lang,
    source: &str,
) -> (Vec<Symbol>, Vec<CallSite>, Vec<Import>) {
    let mut analysis = Analysis::new();
    crate::analyze::analyze(path, lang, source, &[], &mut analysis);

    let mut symbols: Vec<Symbol> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for cfg in &analysis.cfgs {
        if !seen.insert(cfg.symbol.clone()) {
            continue;
        }
        let (start_line, end_line) = cfg_lines(cfg);
        let name = function_name(source, start_line);
        symbols.push(Symbol {
            id: cfg.symbol.clone(),
            name: if name.is_empty() {
                format!("fn@{start_line}")
            } else {
                name
            },
            kind: SymbolKind::Function,
            signature: source
                .lines()
                .nth(start_line.saturating_sub(1))
                .unwrap_or("")
                .trim()
                .to_string(),
            doc: String::new(),
            start_line,
            end_line,
            container: None,
            file: path.to_string(),
        });
    }

    for t in &analysis.types {
        if !seen.insert(t.id.clone()) {
            continue;
        }
        symbols.push(Symbol {
            id: t.id.clone(),
            name: t.name.clone(),
            kind: type_symbol_kind(t.kind),
            signature: String::new(),
            doc: t.doc.clone(),
            start_line: t.line,
            end_line: t.line,
            container: None,
            file: path.to_string(),
        });
    }
    symbols.sort_by(|a, b| a.id.cmp(&b.id));

    let mut site_set: BTreeSet<(String, String, usize, Option<String>)> = BTreeSet::new();
    for u in analysis.uses.iter().filter(|u| u.kind == UseKind::Call) {
        site_set.insert((
            u.file.clone(),
            u.target.clone(),
            u.line,
            crate::analyze::enclosing_symbol_at(&symbols, u.line).map(|s| s.id.clone()),
        ));
    }
    let sites: Vec<CallSite> = site_set
        .into_iter()
        .map(|(file, callee, line, enclosing)| CallSite {
            file,
            callee,
            line,
            enclosing,
        })
        .collect();

    let mut import_set: BTreeSet<(String, String)> = BTreeSet::new();
    for u in analysis.uses.iter().filter(|u| u.kind == UseKind::Import) {
        import_set.insert((u.file.clone(), u.target.clone()));
    }
    let imports: Vec<Import> = import_set
        .into_iter()
        .map(|(file, target)| Import { file, target })
        .collect();

    (symbols, sites, imports)
}

/// `(start_line, end_line)` for a function CFG from its `file:line` id + nodes.
fn cfg_lines(cfg: &FunctionCfg) -> (usize, usize) {
    let start = cfg
        .symbol
        .rsplit_once(':')
        .and_then(|(_, line)| line.parse::<usize>().ok())
        .unwrap_or(0);
    let end = cfg.nodes.iter().map(|n| n.end_line).max().unwrap_or(start);
    (start, end.max(start))
}

/// Best-effort function name from its declaration line (the analyzer's CFG
/// carries only a `file:line` id). Empty when nothing convincing is found.
fn function_name(source: &str, line: usize) -> String {
    let Some(text) = source.lines().nth(line.saturating_sub(1)) else {
        return String::new();
    };
    let head = text.trim().split(['{', ';']).next().unwrap_or("").trim();
    let before_paren = head.split('(').next().unwrap_or(head);
    let candidate = if before_paren.len() < head.len() {
        before_paren
    } else {
        head.split("=>").next().unwrap_or(head)
    };
    let last = candidate
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .rfind(|s| !s.is_empty())
        .unwrap_or("");
    if matches!(
        last,
        "" | "function"
            | "def"
            | "const"
            | "let"
            | "var"
            | "class"
            | "async"
            | "default"
            | "export"
    ) {
        return String::new();
    }
    last.to_string()
}

/// Map a v2 [`TypeKind`] onto the frozen [`SymbolKind`] (no `Interface` kind).
fn type_symbol_kind(kind: TypeKind) -> SymbolKind {
    match kind {
        TypeKind::Struct => SymbolKind::Struct,
        TypeKind::Enum => SymbolKind::Enum,
        TypeKind::Trait => SymbolKind::Trait,
        TypeKind::Class | TypeKind::Interface => SymbolKind::Class,
    }
}

/// RFC 3339 UTC timestamp, hand-rolled (no chrono dep — pinned stack only).
fn timestamp_now_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    let rem = secs % 86_400;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Howard Hinnant's civil-from-days: days since 1970-01-01 → (y, m, d).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EdgeKind, SymbolKind};
    use std::path::PathBuf;

    fn temp_root(tag: &str) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("rw-pipeline-test-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_sample(root: &Path) {
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/main.rs"),
            "//! entry\nfn main() {\n    helper();\n}\n\npub fn helper() {}\n",
        )
        .unwrap();
        std::fs::write(root.join("src/tool.py"), "def run():\n    pass\n").unwrap();
    }

    #[test]
    fn builds_index_with_files_symbols_and_edges() {
        let root = temp_root("build");
        write_sample(&root);

        let idx = build_index(&root, &|_| {}).unwrap();

        let paths: Vec<&str> = idx.files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"src/main.rs"));
        assert!(paths.contains(&"src/tool.py"));
        assert_eq!(idx.version, crate::model::SCHEMA_VERSION);
        assert_eq!(idx.repo_root, root.to_string_lossy());

        let names: Vec<&str> = idx.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"helper"));
        assert!(names.contains(&"run"));
        assert!(idx
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Function && s.file == "src/main.rs"));

        // helper is called from main: a resolved call edge to its symbol id must exist
        let helper = idx.symbols.iter().find(|s| s.name == "helper").unwrap();
        assert!(idx
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::Call && e.callee == helper.id));

        // cache was saved
        assert!(crate::store::load(&root).is_some());

        assert!(!idx.indexed_at.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn loads_cache_when_valid_instead_of_rebuilding() {
        let root = temp_root("cache");
        write_sample(&root);
        let idx = build_index(&root, &|_| {}).unwrap();
        assert!(!idx.symbols.is_empty());

        // Tamper with the cache: if the pipeline re-parsed, symbols would come back.
        let mut cached = crate::store::load(&root).unwrap();
        cached.symbols.clear();
        cached.edges.clear();
        crate::store::save(&root, &cached).unwrap();

        let again = build_index(&root, &|_| {}).unwrap();
        assert!(
            again.symbols.is_empty(),
            "must load the valid cache, not rebuild"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rebuilds_after_file_touch() {
        let root = temp_root("touch");
        write_sample(&root);
        let first = build_index(&root, &|_| {}).unwrap();
        assert!(!first.symbols.is_empty());

        // Touch one source file: mtime max changes → cache invalidated → re-parse.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

        let second = build_index(&root, &|_| {}).unwrap();
        let names: Vec<&str> = second.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(
            !names.contains(&"helper"),
            "stale symbol must be gone after re-index"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn progress_reports_monotonic_counts_ending_at_file_total() {
        let root = temp_root("progress");
        write_sample(&root);

        let seen = std::cell::RefCell::new(Vec::new());
        build_index(&root, &|n| seen.borrow_mut().push(n)).unwrap();
        let seen = seen.into_inner();

        assert!(!seen.is_empty(), "progress must be called at least once");
        assert!(
            seen.windows(2).all(|w| w[0] <= w[1]),
            "counts must be monotonic"
        );
        assert_eq!(*seen.last().unwrap(), 2, "final count == number of files");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_root_is_an_error() {
        let root = temp_root("missing-root").join("does-not-exist");
        assert!(build_index(&root, &|_| {}).is_err());
    }

    // ---- R-27: language dispatch is explicit, never a silent drop ----

    #[test]
    fn ts_only_dir_indexes_non_zero_symbols() {
        let root = temp_root("ts-symbols");
        std::fs::write(
            root.join("main.ts"),
            "export function greet(name: string): string {\n    return name;\n}\n\n\
             export class Greeter {\n    hello(): string {\n        return greet(\"world\");\n    }\n}\n",
        )
        .unwrap();

        let report = build_index_with_report(&root, &|_| {}).unwrap();
        let idx = &report.index;
        assert!(
            !idx.symbols.is_empty(),
            "a TS-only dir must extract symbols through build_index (R-27); \
             the old `_ =>` arm returned 0"
        );
        assert!(
            idx.symbols.iter().any(|s| s.name == "greet"),
            "the `greet` function must be a symbol: {:?}",
            idx.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        assert!(
            idx.symbols.iter().any(|s| s.name == "Greeter"),
            "the `Greeter` class must be a symbol"
        );
        assert!(
            report.unsupported.is_empty(),
            "typescript is a supported language: {:?}",
            report.unsupported
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unsupported_language_files_are_reported_explicitly() {
        let root = temp_root("unsupported");
        std::fs::write(root.join("main.go"), "package main\n\nfunc main() {}\n").unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
        )
        .unwrap();

        let report = build_index_with_report(&root, &|_| {}).unwrap();
        let go = report
            .unsupported
            .iter()
            .find(|u| u.language == "go")
            .expect("a `.go` file must be reported as not analyzed");
        assert_eq!(go.files, 1);
        assert_eq!(
            report.unsupported_summary().as_deref(),
            Some("1 files not analyzed (go)"),
            "caller must be able to say N files not analyzed (language)"
        );
        // The supported language is still fully extracted.
        assert!(report.index.symbols.iter().any(|s| s.name == "add"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unchanged_files_stay_a_cache_hit_with_report() {
        let root = temp_root("report-cache");
        write_sample(&root);
        let first = build_index_with_report(&root, &|_| {}).unwrap();
        assert!(!first.index.symbols.is_empty());
        // Second build must serve the cache and still report no unsupported files.
        let second = build_index_with_report(&root, &|_| {}).unwrap();
        assert_eq!(second.index.symbols.len(), first.index.symbols.len());
        assert!(second.unsupported.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
