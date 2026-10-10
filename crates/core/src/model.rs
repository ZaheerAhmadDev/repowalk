//! Shared data contract (plan §6) — FROZEN: changing a type here requires
//! coordinating with every consumer (gui, cli, tests) first.
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    Mod,
    Class,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    Call,
    Import,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Confidence {
    Exact,
    NameOnly,
    Ambiguous,
    Unresolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub lang: String,
    pub lines: usize,
    pub mtime: u64,
    pub is_entrypoint: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    /// Stable id: `path:start_line` (e.g. `src/main.rs:12`).
    pub id: String,
    pub name: String,
    pub kind: SymbolKind,
    pub signature: String,
    pub doc: String,
    pub start_line: usize,
    pub end_line: usize,
    pub container: Option<String>,
    pub file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallSite {
    pub file: String,
    pub callee: String,
    pub line: usize,
    pub enclosing: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub caller: String,
    pub callee: String,
    pub line: usize,
    pub kind: EdgeKind,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    pub file: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub version: u32,
    pub repo_root: String,
    pub indexed_at: String,
    pub files: Vec<FileEntry>,
    pub symbols: Vec<Symbol>,
    pub call_sites: Vec<CallSite>,
    pub edges: Vec<Edge>,
    pub imports: Vec<Import>,
}

impl Index {
    pub fn new(repo_root: impl Into<String>) -> Self {
        Index {
            version: SCHEMA_VERSION,
            repo_root: repo_root.into(),
            indexed_at: String::new(),
            files: Vec::new(),
            symbols: Vec::new(),
            call_sites: Vec::new(),
            edges: Vec::new(),
            imports: Vec::new(),
        }
    }
}
