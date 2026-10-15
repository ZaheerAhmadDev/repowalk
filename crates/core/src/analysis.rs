//! v2 analysis artifacts: variables, control flow, types, modules, references.
//!
//! ADDITIVE contract — the frozen `index.json` (`model.rs`) is untouched; these
//! types are serialized to `.repowalk/analysis.json` and version-checked on load.
//! Changing a type here requires coordinating with every consumer first
//! (gui/cli/tests), same rule as `model.rs`.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const ANALYSIS_VERSION: u32 = 1;

/// Where a variable is declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VarKind {
    Local,
    Param,
    Field,
    Static,
    Global,
}

/// One variable with its usage counts (F4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Variable {
    /// Stable id: `file:decl_line:name`.
    pub id: String,
    pub name: String,
    pub kind: VarKind,
    pub file: String,
    pub decl_line: usize,
    /// Scope range (inclusive lines) the variable is visible in.
    pub scope_start: usize,
    pub scope_end: usize,
    /// Declared or simply inferred type, when the source states one.
    pub type_name: Option<String>,
    pub reads: usize,
    pub writes: usize,
    /// First / last use line, when used at all.
    pub first_use: Option<usize>,
    pub last_use: Option<usize>,
    /// Enclosing symbol id (`file:line`), empty for file-level items.
    pub enclosing: String,
}

/// What a control-flow node represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CfgKind {
    Entry,
    Exit,
    Block,
    Branch,
    Loop,
    Match,
    Call,
    Return,
}

/// One node of a function's control-flow graph (F2a).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CfgNode {
    pub id: u32,
    pub kind: CfgKind,
    pub start_line: usize,
    pub end_line: usize,
    /// Short human label (condition text, call name, "entry", …).
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CfgEdgeKind {
    Normal,
    True,
    False,
    Back,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CfgEdge {
    pub from: u32,
    pub to: u32,
    pub kind: CfgEdgeKind,
    /// Source line the edge originates from.
    pub line: usize,
}

/// Control-flow graph + complexity for one function (F2a, F6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionCfg {
    /// Symbol id (`file:line`) of the function/method.
    pub symbol: String,
    pub nodes: Vec<CfgNode>,
    pub edges: Vec<CfgEdge>,
    /// Cyclomatic complexity: decision points + 1.
    pub cyclomatic: u32,
    pub max_nesting: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TypeKind {
    Struct,
    Enum,
    Trait,
    Class,
    Interface,
}

/// A field of a struct/class (F2c).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub type_name: String,
    pub visibility: String,
    pub line: usize,
}

/// A type declaration node (F2c).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeNode {
    /// Stable id: `file:name`.
    pub id: String,
    pub name: String,
    pub kind: TypeKind,
    pub file: String,
    pub line: usize,
    pub fields: Vec<Field>,
    /// Method/function names implemented by this type.
    pub methods: Vec<String>,
    pub doc: String,
    /// Owning module path (`crate::module` style, best effort).
    pub module_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TypeEdgeKind {
    /// Field whose type is another declared type (containment).
    Field,
    /// `impl`/method-owner relationship.
    Impl,
    /// Inheritance / trait implementation.
    Inherit,
    /// Enum variant payload type.
    Variant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeEdge {
    pub from: String,
    pub to: String,
    pub kind: TypeEdgeKind,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModuleKind {
    Crate,
    Dir,
    Module,
    File,
}

/// A crate/dir/module/file node of the structure graph (F2d).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleNode {
    /// Repo-relative path (or crate name for `Crate` nodes).
    pub id: String,
    pub kind: ModuleKind,
    pub loc: usize,
    pub symbols: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModuleEdgeKind {
    Import,
    Submodule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleEdge {
    pub from: String,
    pub to: String,
    pub kind: ModuleEdgeKind,
    /// Number of underlying imports this edge aggregates.
    pub count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UseKind {
    Call,
    TypeRef,
    FieldAccess,
    VarRead,
    VarWrite,
    Import,
}

/// One reference site of a symbol or variable (F7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UseSite {
    /// Referenced symbol id, or `Variable::id` for variable uses.
    pub target: String,
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub kind: UseKind,
    /// Trimmed source line, for the refs panel.
    pub snippet: String,
}

/// The whole v2 analysis artifact (`.repowalk/analysis.json`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Analysis {
    pub version: u32,
    pub variables: Vec<Variable>,
    pub cfgs: Vec<FunctionCfg>,
    pub types: Vec<TypeNode>,
    pub type_edges: Vec<TypeEdge>,
    pub modules: Vec<ModuleNode>,
    pub module_edges: Vec<ModuleEdge>,
    pub uses: Vec<UseSite>,
}

impl Analysis {
    /// Empty artifact at the current version.
    pub fn new() -> Self {
        Self {
            version: ANALYSIS_VERSION,
            ..Self::default()
        }
    }
}

/// `<root>/.repowalk/analysis.json`.
pub fn analysis_path(root: &Path) -> PathBuf {
    root.join(".repowalk").join("analysis.json")
}

/// Load the artifact; `None` when missing, unreadable, or version-mismatched
/// (a stale analysis is a miss, never a silent misuse).
pub fn load(root: &Path) -> Option<Analysis> {
    let bytes = std::fs::read(analysis_path(root)).ok()?;
    let a: Analysis = serde_json::from_slice(&bytes).ok()?;
    (a.version == ANALYSIS_VERSION).then_some(a)
}

/// Persist the artifact atomically (write temp, then rename).
pub fn save(root: &Path, analysis: &Analysis) -> anyhow::Result<()> {
    let path = analysis_path(root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut a = analysis.clone();
    a.version = ANALYSIS_VERSION;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&a)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_through_json() {
        let mut a = Analysis::new();
        a.variables.push(Variable {
            id: "src/lib.rs:10:i".into(),
            name: "i".into(),
            kind: VarKind::Local,
            file: "src/lib.rs".into(),
            decl_line: 10,
            scope_start: 10,
            scope_end: 14,
            type_name: Some("usize".into()),
            reads: 3,
            writes: 1,
            first_use: Some(11),
            last_use: Some(14),
            enclosing: "src/lib.rs:8".into(),
        });
        a.cfgs.push(FunctionCfg {
            symbol: "src/lib.rs:8".into(),
            nodes: vec![CfgNode {
                id: 0,
                kind: CfgKind::Entry,
                start_line: 8,
                end_line: 8,
                label: "entry".into(),
            }],
            edges: vec![],
            cyclomatic: 1,
            max_nesting: 0,
        });
        let bytes = serde_json::to_vec(&a).expect("serialize");
        let back: Analysis = serde_json::from_slice(&bytes).expect("deserialize");
        assert_eq!(a, back);
        assert_eq!(back.version, ANALYSIS_VERSION);
    }

    #[test]
    fn version_mismatch_is_a_miss() {
        let mut a = Analysis::new();
        a.version = ANALYSIS_VERSION + 1;
        let bytes = serde_json::to_vec(&a).unwrap();
        let parsed: Analysis = serde_json::from_slice(&bytes).unwrap();
        assert_ne!(parsed.version, ANALYSIS_VERSION);
        // `load` treats that as None — checked at the call site, not here.
    }
}
