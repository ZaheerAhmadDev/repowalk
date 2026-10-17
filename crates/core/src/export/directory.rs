//! Directory-tree emitter (B5-01). OWNER: B5-01.
//!
//! Pure and deterministic: `(index, analysis, options)` in, one
//! [`super::util::Graph`] out, reusing the shared escaping/id helpers so the
//! Mermaid/DOT/SVG backends render it with the same conventions as the sibling
//! diagrams. Built from `Index::files` **only**: the repository root, then the
//! directories that actually contain indexed files (their ancestor chain),
//! then the files as leaves — no node is ever invented for a directory the
//! index does not reach.
use super::util::{self, Arrow, GEdge, GNode, Graph, Shape, Style};
use crate::analysis::Analysis;
use crate::model::Index;
use std::collections::{BTreeMap, BTreeSet};

/// Emitter knobs. The default is the plain containment tree: labels carry
/// `(N LOC)`, dashed import edges are off unless the caller asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectoryOptions {
    /// Dashed `Import` edges aggregated between directories (default OFF).
    pub import_edges: bool,
    /// Append `(N LOC)` to file labels, from `FileEntry::lines`.
    pub loc_labels: bool,
}

impl Default for DirectoryOptions {
    fn default() -> Self {
        DirectoryOptions {
            import_edges: false,
            loc_labels: true,
        }
    }
}

/// Build the directory tree of `index`.
///
/// Node shapes: root and directories [`Shape::Box`], plain files
/// [`Shape::Rounded`], entrypoint files [`Shape::Oval`] (a distinct stadium in
/// Mermaid and ellipse in DOT/SVG; their label also carries a `★` marker for
/// text backends). Containment edges are plain [`Arrow::Open`] lines; optional
/// import edges are dashed arrows with an `N import(s)` label.
pub fn build(index: &Index, _analysis: &Analysis, options: &DirectoryOptions) -> Graph {
    // repo-relative, forward-slash normalized, sorted: one entry per unique path.
    let mut files: BTreeMap<String, (String, bool, usize)> = BTreeMap::new();
    let mut dirs: BTreeSet<String> = BTreeSet::new();
    let mut file_dir: BTreeMap<String, String> = BTreeMap::new();
    for f in &index.files {
        let path = f.path.replace('\\', "/");
        let comps: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
        if comps.is_empty() {
            continue;
        }
        let key = comps.join("/");
        files.insert(
            key.clone(),
            (comps[comps.len() - 1].to_string(), f.is_entrypoint, f.lines),
        );
        let mut dir = String::new();
        for c in &comps[..comps.len() - 1] {
            if !dir.is_empty() {
                dir.push('/');
            }
            dir.push_str(c);
            dirs.insert(dir.clone());
        }
        file_dir.insert(key, dirs_ancestor(&comps));
    }

    if files.is_empty() {
        return Graph {
            nodes: vec![GNode {
                id: util::sanitize("d", "none"),
                label: "(no indexed files)".to_string(),
                shape: Shape::Box,
            }],
            edges: Vec::new(),
            caption: Some("no indexed files".to_string()),
        };
    }

    // Nodes: root first, then real directories sorted by path.
    let mut nodes: Vec<GNode> = vec![GNode {
        id: util::sanitize("d", ""),
        label: root_name(&index.repo_root),
        shape: Shape::Box,
    }];
    for dir in &dirs {
        let name = dir.rsplit('/').next().unwrap_or(dir.as_str());
        nodes.push(GNode {
            id: util::sanitize("d", dir),
            label: name.to_string(),
            shape: Shape::Box,
        });
    }
    // Then real files sorted by path.
    for (path, (name, is_entrypoint, lines)) in &files {
        let loc = *lines;
        let label = if *is_entrypoint {
            format!("{name} ★")
        } else {
            name.clone()
        };
        let label = if options.loc_labels {
            format!("{label} ({loc} LOC)")
        } else {
            label
        };
        nodes.push(GNode {
            id: util::sanitize("f", path),
            label,
            shape: if *is_entrypoint {
                Shape::Oval
            } else {
                Shape::Rounded
            },
        });
    }

    // Structure edges: plain containment lines from a parent to its child.
    let mut edges: Vec<GEdge> = Vec::new();
    let root_id = util::sanitize("d", "");
    for dir in &dirs {
        let (parent, _) = split_dir(dir);
        edges.push(GEdge {
            from: parent_id(&parent, &root_id),
            to: util::sanitize("d", dir),
            label: String::new(),
            arrow: Arrow::Open,
            style: Style::Solid,
        });
    }
    for path in files.keys() {
        let dir = &file_dir[path];
        edges.push(GEdge {
            from: parent_id(dir, &root_id),
            to: util::sanitize("f", path),
            label: String::new(),
            arrow: Arrow::Open,
            style: Style::Solid,
        });
    }

    // Optional: dashed Import edges aggregated between the directories of two
    // *indexed* files. Anything else (module paths, external crates) is not a
    // node here and is skipped rather than invented.
    if options.import_edges {
        let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
        for imp in &index.imports {
            let from = imp.file.replace('\\', "/");
            let to = imp.target.replace('\\', "/");
            let (Some(from_dir), Some(to_dir)) = (file_dir.get(&from), file_dir.get(&to)) else {
                continue;
            };
            if from_dir == to_dir {
                continue;
            }
            *counts
                .entry((from_dir.clone(), to_dir.clone()))
                .or_default() += 1;
        }
        for ((from, to), n) in &counts {
            edges.push(GEdge {
                from: parent_id(from, &root_id),
                to: parent_id(to, &root_id),
                label: format!("{n} import{}", plural(*n)),
                arrow: Arrow::Normal,
                style: Style::Dashed,
            });
        }
    }

    Graph {
        nodes,
        edges,
        caption: None,
    }
}

/// Directory key of a path's parent ("": repo root).
fn dirs_ancestor(comps: &[&str]) -> String {
    if comps.len() > 1 {
        comps[..comps.len() - 1].join("/")
    } else {
        String::new()
    }
}

/// Directory key of a path's parent ("": repo root).
fn split_dir(dir: &str) -> (String, &str) {
    match dir.rsplit_once('/') {
        Some((parent, name)) => (parent.to_string(), name),
        None => (String::new(), dir),
    }
}

/// Parent node id for a directory key: the root node when the key is empty.
fn parent_id(dir: &str, root_id: &str) -> String {
    if dir.is_empty() {
        root_id.to_string()
    } else {
        util::sanitize("d", dir)
    }
}

/// Repository name for the root label: the basename of the indexed root.
fn root_name(repo_root: &str) -> String {
    std::path::Path::new(repo_root)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| repo_root.to_string())
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FileEntry;

    fn file(path: &str, lines: usize, is_entrypoint: bool) -> FileEntry {
        FileEntry {
            path: path.into(),
            lang: "rust".into(),
            lines,
            mtime: 0,
            is_entrypoint,
        }
    }

    /// `fixtures/sample`-shaped index: the flat root set — `a.rs`, `b.rs`,
    /// `main.rs` (entrypoint), `model.rs`, `tool.py`, `util.rs`.
    fn sample_index() -> Index {
        let mut idx = Index::new("/tmp/sample");
        idx.files = vec![
            file("a.rs", 10, false),
            file("b.rs", 20, false),
            file("main.rs", 30, true),
            file("model.rs", 40, false),
            file("tool.py", 50, false),
            file("util.rs", 60, false),
        ];
        idx
    }

    /// Same names plus a nested directory so real directory names show up.
    fn tree_index() -> Index {
        let mut idx = Index::new("/tmp/sample");
        idx.files = vec![
            file("a.rs", 10, false),
            file("main.rs", 30, true),
            file("src/model.rs", 40, false),
            file("src/deep/util.rs", 60, false),
            file("lib/tool.py", 50, false),
        ];
        idx.imports = vec![
            crate::model::Import {
                file: "a.rs".into(),
                target: "src/deep/util.rs".into(),
            },
            crate::model::Import {
                file: "a.rs".into(),
                target: "crate::other".to_string(), // not an indexed path: never mapped
            },
            crate::model::Import {
                file: "main.rs".into(),
                target: "a.rs".into(), // same directory: filtered out
            },
        ];
        idx
    }

    fn analysis() -> Analysis {
        Analysis::new()
    }

    #[test]
    fn flat_sample_root_and_every_real_file_name() {
        let idx = sample_index();
        let a = analysis();
        let out = crate::export::mermaid(
            &idx,
            &a,
            &crate::export::Diagram::Directory { imports: false },
        );
        assert!(out.starts_with("flowchart TD"), "{out}");
        assert!(out.contains("[\"sample\"]"), "root label: {out}");
        for name in ["a.rs", "b.rs", "main.rs", "model.rs", "tool.py", "util.rs"] {
            assert!(out.contains(name), "real file name {name}: {out}");
        }
        // LOC labels come from FileEntry::lines.
        assert!(out.contains("main.rs ★ (30 LOC)"), "{out}");
        // Entry point: distinct text marker (and stadium shape).
        assert!(out.contains("main.rs ★ (30 LOC)"), "{out}");
        assert!(out.contains("([\"main.rs ★ (30 LOC)\"])"), "{out}");
        // No directories exist in this fixture — nothing may be invented
        // (one root node + six file leaves, no other node lines).
        let node_lines = out
            .lines()
            .filter(|l| l.contains("[\"") || l.contains("(\""))
            .count();
        assert_eq!(node_lines, 7, "root + 6 files: {out}");
        assert!(!out.contains("src"), "{out}");
        // Plain containment lines, no arrows.
        assert!(out.contains("---"), "{out}");
        assert!(!out.contains("-->"), "{out}");
    }

    #[test]
    fn dot_marks_entrypoint_shape_and_quotes_real_names() {
        let idx = sample_index();
        let a = analysis();
        let out = crate::export::dot(
            &idx,
            &a,
            &crate::export::Diagram::Directory { imports: false },
        );
        assert!(out.starts_with("digraph repowalk {"), "{out}");
        assert!(out.contains("rankdir=TB;"), "{out}");
        assert!(out.contains("label=\"sample\""), "{out}");
        for name in ["a.rs", "b.rs", "main.rs", "model.rs", "tool.py", "util.rs"] {
            assert!(out.contains(name), "real file name {name}: {out}");
        }
        assert!(out.contains("label=\"main.rs ★ (30 LOC)\""), "{out}");
        // The entrypoint file is the only ellipse; everything else is a box.
        let lines: Vec<&str> = out.lines().collect();
        let ellipse_count = lines.iter().filter(|l| l.contains("ellipse")).count();
        assert_eq!(ellipse_count, 1, "{out}");
        assert!(out.contains("arrowhead=none"), "plain containment: {out}");
    }

    #[test]
    fn nested_dirs_use_real_directory_names_and_no_invented_nodes() {
        let idx = tree_index();
        let a = analysis();
        let out = crate::export::mermaid(
            &idx,
            &a,
            &crate::export::Diagram::Directory { imports: false },
        );
        assert!(out.contains("[\"src\"]"), "real dir name: {out}");
        assert!(out.contains("[\"deep\"]"), "real nested dir name: {out}");
        assert!(out.contains("[\"lib\"]"), "real dir name: {out}");
        assert!(out.contains("[\"sample\"]"), "{out}");
        // One node per real thing only: root + src + deep + lib + 5 files.
        let node_lines = out
            .lines()
            .filter(|l| l.contains("[\"") || l.contains("(\""))
            .count();
        assert_eq!(node_lines, 9, "nothing invented: {out}");
        assert!(!out.contains("other"), "{out}");
        // deep is a child of src, lib a child of the root.
        assert!(out.contains("---"), "{out}");
    }

    #[test]
    fn import_edges_are_optional_off_by_default() {
        let idx = tree_index();
        let a = analysis();
        let off = crate::export::mermaid(
            &idx,
            &a,
            &crate::export::Diagram::Directory { imports: false },
        );
        assert!(!off.contains("-.->"), "no import edges by default: {off}");
        let on = crate::export::dot(
            &idx,
            &a,
            &crate::export::Diagram::Directory { imports: true },
        );
        assert!(on.contains("style=dashed"), "dashed import edge: {on}");
        assert!(on.contains("label=\"1 import\""), "aggregate count: {on}");
        // The unmappable `crate::other` target contributed nothing.
        assert!(!on.contains("other"), "no invented nodes: {on}");
    }

    #[test]
    fn byte_identical_on_repeat_call() {
        let idx = tree_index();
        let a = analysis();
        let d1 = crate::export::Diagram::Directory { imports: false };
        let d2 = crate::export::Diagram::Directory { imports: true };
        assert_eq!(
            crate::export::mermaid(&idx, &a, &d1),
            crate::export::mermaid(&idx, &a, &d1),
            "mermaid"
        );
        assert_eq!(
            crate::export::dot(&idx, &a, &d2),
            crate::export::dot(&idx, &a, &d2),
            "dot (import edges on)"
        );
        assert_eq!(
            crate::export::svg(&idx, &a, &d1),
            crate::export::svg(&idx, &a, &d1),
            "svg"
        );
    }

    #[test]
    fn empty_index_degrades_to_placeholder() {
        let idx = Index::new("/tmp/empty");
        let a = analysis();
        let out = crate::export::mermaid(
            &idx,
            &a,
            &crate::export::Diagram::Directory { imports: false },
        );
        assert!(out.contains("(no indexed files)"), "{out}");
    }
}
