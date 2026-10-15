//! Reference index (F7): every use site of a symbol or variable.
//!
//! OWNER: TEXT-03 (card TEXT-3) for the core lookups + the consuming panel;
//! INTEL populates [`UseSite`]. The four original lookups are the agreed
//! starting contract — keep the names/signatures. [`references_ex`],
//! [`RefSet`] and [`references_by_file`] are additive.
//!
//! ## Resolution honesty (risk R-20)
//!
//! `UseSite::target` is a **canonical symbol id** (`file:line`, `file:name`,
//! `file:line:name`) for a *resolved* use, but a **raw name** for a use the
//! analyzer could not resolve (docs/architecture.md, "Analyzer output
//! semantics"). The plain [`references`] family returns the union and so
//! cannot tell a confirmed hit from a name-only guess. Consumers that must not
//! pretend — the refs panel is one — use [`references_ex`], which splits the
//! two into [`RefSet::resolved`] and [`RefSet::by_name`].
//!
//! Which ids and names can be linked comes from the [`Analysis`] alone: only
//! [`crate::analysis::Variable`] (`id` ↔ `name`) and [`crate::analysis::TypeNode`]
//! (`id` ↔ `name`) carry a declared name next to an id here, so name-only
//! matches can be attached to a declaration for those. Function/method symbol
//! ids carry no name in this artifact — query those by id, and treat a
//! name-only function match as unresolved.
//!
//! ## Determinism
//!
//! Every lookup is a pure read of [`Analysis`]; the same artifact always yields
//! the same list regardless of the order the extractors appended rows. Sites
//! are ordered by `(file, line, col, kind)`; counts rank by count (desc) then
//! target id (asc). **Duplicates are kept**: two distinct use sites on the same
//! file/line are two references, so nothing is silently deduped away.
use crate::analysis::{Analysis, UseKind, UseSite};
use std::collections::{BTreeMap, BTreeSet};

/// Uses of one target, split by how the analyzer recorded them.
///
/// `resolved` and `by_name` are disjoint and each in file/line order. See the
/// module docs for why this split exists (risk R-20).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefSet {
    /// Uses whose `target` is a canonical id — the analyzer resolved them.
    pub resolved: Vec<UseSite>,
    /// Uses whose `target` is a raw name — the analyzer did not resolve them.
    pub by_name: Vec<UseSite>,
}

impl RefSet {
    /// Number of sites across both halves.
    pub fn len(&self) -> usize {
        self.resolved.len() + self.by_name.len()
    }

    /// True when neither half has a site.
    pub fn is_empty(&self) -> bool {
        self.resolved.is_empty() && self.by_name.is_empty()
    }

    /// The union, in file/line order. Resolved and name-only sites are
    /// interleaved by position (the two halves are not labelled here); use the
    /// fields when the distinction matters.
    pub fn all(&self) -> Vec<UseSite> {
        let mut out = Vec::with_capacity(self.len());
        out.extend(self.resolved.iter().cloned());
        out.extend(self.by_name.iter().cloned());
        sort_sites(&mut out);
        out
    }
}

/// Stable rank for a use kind, used only as a last sort key.
fn use_rank(kind: UseKind) -> u8 {
    match kind {
        UseKind::Call => 0,
        UseKind::TypeRef => 1,
        UseKind::FieldAccess => 2,
        UseKind::VarRead => 3,
        UseKind::VarWrite => 4,
        UseKind::Import => 5,
    }
}

/// Order sites by `(file, line, col, kind)`; ties keep their input order
/// (stable sort), so true duplicates are preserved rather than merged.
fn sort_sites(sites: &mut [UseSite]) {
    sites.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.col.cmp(&b.col))
            .then_with(|| use_rank(a.kind).cmp(&use_rank(b.kind)))
    });
}

/// Canonical ids reachable from this [`Analysis`], mapped to their declared
/// name when one exists (`None` for ids this artifact has no name for).
fn declared_ids(analysis: &Analysis) -> BTreeMap<String, Option<String>> {
    let mut ids: BTreeMap<String, Option<String>> = BTreeMap::new();
    for v in &analysis.variables {
        ids.insert(v.id.clone(), Some(v.name.clone()));
    }
    for t in &analysis.types {
        ids.insert(t.id.clone(), Some(t.name.clone()));
    }
    // Ids with no name in the artifact: keep them so a use can still be
    // classified as resolved, but never invent a name to match on.
    for c in &analysis.cfgs {
        ids.entry(c.symbol.clone()).or_insert(None);
    }
    for v in &analysis.variables {
        if !v.enclosing.is_empty() {
            ids.entry(v.enclosing.clone()).or_insert(None);
        }
    }
    for e in &analysis.type_edges {
        ids.entry(e.from.clone()).or_insert(None);
        ids.entry(e.to.clone()).or_insert(None);
    }
    ids
}

/// Classify every use of `target` as resolved (id) or name-only (raw name).
fn select(analysis: &Analysis, target: &str) -> RefSet {
    let ids = declared_ids(analysis);

    // `target` is itself an id → also accept its name-only uses.
    let target_name: Option<&str> = ids.get(target).and_then(|n| n.as_deref());
    // `target` is a raw name → also accept ids declared with that name.
    let mut named_ids: BTreeSet<&str> = BTreeSet::new();
    if !ids.contains_key(target) {
        for (id, name) in &ids {
            if name.as_deref() == Some(target) {
                named_ids.insert(id.as_str());
            }
        }
    }

    let mut set = RefSet::default();
    for u in &analysis.uses {
        let relevant = u.target == target
            || target_name == Some(u.target.as_str())
            || named_ids.contains(u.target.as_str());
        if !relevant {
            continue;
        }
        if ids.contains_key(u.target.as_str()) {
            set.resolved.push(u.clone());
        } else {
            set.by_name.push(u.clone());
        }
    }
    sort_sites(&mut set.resolved);
    sort_sites(&mut set.by_name);
    set
}

/// All use sites of `target`, in file/line order.
///
/// `target` may be a canonical id or a raw name; this returns the **union** of
/// resolved and name-only hits (see [`references_ex`] to keep them apart).
pub fn references(analysis: &Analysis, target: &str) -> Vec<UseSite> {
    select(analysis, target).all()
}

/// Resolved-vs-name-only split of [`references`] (risk R-20). Prefer this in
/// the refs panel: it can label name-only hits instead of pretending they are
/// confirmed.
pub fn references_ex(analysis: &Analysis, target: &str) -> RefSet {
    select(analysis, target)
}

/// Use sites of `target` narrowed to one kind (still the union of resolved and
/// name-only hits, in file/line order).
pub fn references_of_kind(analysis: &Analysis, target: &str, kind: UseKind) -> Vec<UseSite> {
    references(analysis, target)
        .into_iter()
        .filter(|u| u.kind == kind)
        .collect()
}

/// Reference counts per `UseSite::target`, highest first; ties broken by target
/// id (asc), so the order never leaks input order.
pub fn reference_counts(analysis: &Analysis) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for u in &analysis.uses {
        *counts.entry(u.target.as_str()).or_default() += 1;
    }
    let mut out: Vec<(String, usize)> = counts
        .into_iter()
        .map(|(target, n)| (target.to_string(), n))
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

/// Distinct files a target is used in, sorted.
pub fn referencing_files(analysis: &Analysis, target: &str) -> Vec<String> {
    let mut files: BTreeSet<String> = BTreeSet::new();
    for u in references(analysis, target) {
        files.insert(u.file);
    }
    files.into_iter().collect()
}

/// The union of [`references`] grouped by file, files sorted, sites in
/// file/line order within each group — the shape the refs panel renders.
pub fn references_by_file(analysis: &Analysis, target: &str) -> Vec<(String, Vec<UseSite>)> {
    let mut by_file: BTreeMap<String, Vec<UseSite>> = BTreeMap::new();
    for u in references(analysis, target) {
        by_file.entry(u.file.clone()).or_default().push(u);
    }
    by_file.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{TypeKind, TypeNode, VarKind, Variable};

    fn variable(id: &str, name: &str) -> Variable {
        let file = id.rsplit_once(':').map(|(f, _)| f).unwrap_or(id);
        let decl_line = id
            .rsplit_once(':')
            .and_then(|(_, l)| l.parse::<usize>().ok())
            .unwrap_or(1);
        Variable {
            id: id.into(),
            name: name.into(),
            kind: VarKind::Local,
            file: file.into(),
            decl_line,
            scope_start: decl_line,
            scope_end: decl_line + 20,
            type_name: None,
            reads: 0,
            writes: 0,
            first_use: None,
            last_use: None,
            enclosing: String::new(),
        }
    }

    fn type_node(id: &str, name: &str) -> TypeNode {
        let file = id.rsplit_once(':').map(|(f, _)| f).unwrap_or(id);
        TypeNode {
            id: id.into(),
            name: name.into(),
            kind: TypeKind::Struct,
            file: file.into(),
            line: 1,
            fields: Vec::new(),
            methods: Vec::new(),
            doc: String::new(),
            module_path: String::new(),
        }
    }

    fn site(target: &str, file: &str, line: usize, col: usize, kind: UseKind) -> UseSite {
        UseSite {
            target: target.into(),
            file: file.into(),
            line,
            col,
            kind,
            snippet: format!("line {line}"),
        }
    }

    #[test]
    fn separates_resolved_from_name_only_for_a_variable_name() {
        let mut a = Analysis::new();
        a.variables.push(variable("src/a.rs:3:q", "q"));
        a.uses = vec![
            site("src/a.rs:3:q", "src/a.rs", 5, 9, UseKind::VarRead),
            site("src/a.rs:3:q", "src/a.rs", 6, 3, UseKind::VarWrite),
            // unresolved analyzer fallback: the raw name, not the id
            site("q", "src/a.rs", 7, 1, UseKind::VarRead),
            site("unrelated", "src/a.rs", 8, 1, UseKind::Call),
        ];

        let set = references_ex(&a, "q");
        assert_eq!(set.resolved.len(), 2);
        assert_eq!(set.by_name.len(), 1);
        assert!(set.resolved.iter().all(|u| u.target == "src/a.rs:3:q"));
        assert_eq!(set.by_name[0].target, "q");
        assert_eq!(set.by_name[0].line, 7);

        // Querying by the id yields the same split (name-only uses attached to
        // the declaration's name).
        let by_id = references_ex(&a, "src/a.rs:3:q");
        assert_eq!(by_id, set);

        // The union is not labelled but has every site.
        assert_eq!(references(&a, "q").len(), 3);
        assert_eq!(
            references_of_kind(&a, "q", UseKind::VarRead)
                .iter()
                .map(|u| u.line)
                .collect::<Vec<_>>(),
            vec![5, 7]
        );
    }

    #[test]
    fn separates_resolved_from_name_only_for_a_type_name() {
        let mut a = Analysis::new();
        a.types.push(type_node("src/a.ts:Point", "Point"));
        a.uses = vec![
            site("src/a.ts:Point", "src/b.ts", 2, 1, UseKind::TypeRef),
            site("Point", "src/b.ts", 4, 1, UseKind::TypeRef),
        ];

        let set = references_ex(&a, "Point");
        assert_eq!(set.resolved.len(), 1);
        assert_eq!(set.by_name.len(), 1);
        assert_eq!(set.resolved[0].file, "src/b.ts");
        assert_eq!(set.by_name[0].target, "Point");
    }

    #[test]
    fn variable_id_resolves_to_its_var_read_and_var_write_sites() {
        let mut a = Analysis::new();
        a.variables.push(variable("src/a.rs:10:i", "i"));
        a.uses = vec![
            site("src/a.rs:10:i", "src/a.rs", 11, 5, UseKind::VarRead),
            site("src/a.rs:10:i", "src/b.rs", 2, 1, UseKind::VarWrite),
            site("other", "src/a.rs", 12, 1, UseKind::Call),
        ];

        let hits = references(&a, "src/a.rs:10:i");
        assert_eq!(hits.len(), 2);
        assert_eq!(
            hits.iter().map(|u| u.kind).collect::<Vec<_>>(),
            vec![UseKind::VarRead, UseKind::VarWrite]
        );
        // file/line order: a.rs before b.rs
        assert_eq!(hits[0].file, "src/a.rs");
        assert_eq!(hits[1].file, "src/b.rs");

        assert_eq!(
            references_of_kind(&a, "src/a.rs:10:i", UseKind::VarRead).len(),
            1
        );
        assert_eq!(
            referencing_files(&a, "src/a.rs:10:i"),
            vec!["src/a.rs".to_string(), "src/b.rs".to_string()]
        );
    }

    #[test]
    fn counts_rank_by_count_then_target_id() {
        let mut a = Analysis::new();
        a.uses = vec![
            site("c", "f.rs", 1, 1, UseKind::Call),
            site("b", "f.rs", 2, 1, UseKind::Call),
            site("a", "f.rs", 3, 1, UseKind::Call),
            site("b", "f.rs", 4, 1, UseKind::Call),
            site("a", "f.rs", 5, 1, UseKind::Call),
        ];
        assert_eq!(
            reference_counts(&a),
            vec![
                ("a".to_string(), 2),
                ("b".to_string(), 2),
                ("c".to_string(), 1),
            ]
        );

        // Deterministic under input order: shuffle and re-check.
        a.uses.reverse();
        assert_eq!(
            reference_counts(&a),
            vec![
                ("a".to_string(), 2),
                ("b".to_string(), 2),
                ("c".to_string(), 1),
            ]
        );
    }

    #[test]
    fn duplicates_are_not_deduped() {
        let mut a = Analysis::new();
        a.uses = vec![
            site("t", "f.rs", 1, 5, UseKind::Call),
            site("t", "f.rs", 1, 5, UseKind::Call),
            site("t", "f.rs", 1, 5, UseKind::TypeRef),
        ];
        // Same file/line/col/kind twice → both kept; a differing kind adds a
        // third. Nothing is silently merged.
        assert_eq!(references(&a, "t").len(), 3);
        assert_eq!(reference_counts(&a), vec![("t".to_string(), 3)]);
    }

    #[test]
    fn groups_by_file_for_the_panel() {
        let mut a = Analysis::new();
        a.uses = vec![
            site("t", "src/b.rs", 2, 1, UseKind::Call),
            site("t", "src/a.rs", 9, 1, UseKind::Call),
            site("t", "src/a.rs", 1, 1, UseKind::Call),
        ];
        let grouped = references_by_file(&a, "t");
        assert_eq!(
            grouped.iter().map(|(f, _)| f.as_str()).collect::<Vec<_>>(),
            vec!["src/a.rs", "src/b.rs"]
        );
        assert_eq!(
            grouped[0].1.iter().map(|u| u.line).collect::<Vec<_>>(),
            vec![1, 9]
        );
    }

    #[test]
    fn empty_analysis_yields_empty_results() {
        let a = Analysis::new();
        assert!(references(&a, "anything").is_empty());
        assert!(references_of_kind(&a, "x", UseKind::Call).is_empty());
        assert!(references_ex(&a, "x").is_empty());
        assert!(reference_counts(&a).is_empty());
        assert!(referencing_files(&a, "x").is_empty());
        assert!(references_by_file(&a, "x").is_empty());
    }
}
