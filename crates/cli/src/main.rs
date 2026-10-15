//! repowalk CLI — thin renderer over repowalk-core (plan §3.3).
//! OWNER: RSK-11 (main dispatch + render.rs helpers).
use clap::{Parser, Subcommand};
use commands::complexity::ComplexityBy;
use commands::graph::{GraphFormat, GraphScope};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "repowalk", version, about = "Offline codebase understanding")]
pub struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Orientation in one screen: metrics, tree, health
    Map { path: PathBuf },
    /// Numbered reading order with why-lines
    Tour { path: PathBuf },
    /// Explain a symbol: signature, docs, callers, callees
    Explain { symbol: String, path: PathBuf },
    /// Ranked fuzzy hits: path:line  name  score
    Find { query: String, path: PathBuf },
    /// Call flow: A→B trace with hop confidence, or wave report from a source
    Flow {
        /// First endpoint (trace mode); with --from, the repo path
        a: Option<String>,
        /// Second endpoint (trace mode)
        b: Option<String>,
        /// Wave mode: source symbol (replaces A and B)
        #[arg(long)]
        from: Option<String>,
        /// Wave-mode depth cap
        #[arg(long, default_value_t = 6)]
        depth: usize,
        /// Markdown export (no ANSI colors)
        #[arg(long)]
        md: bool,
        /// Repository path to index (trace mode)
        path: Option<PathBuf>,
    },
    /// Architecture report (Markdown by default; --json for the underlying data)
    Report {
        /// Markdown output (the default; explicit for scripts)
        #[arg(long)]
        md: bool,
        /// Emit the underlying data as JSON instead of Markdown
        #[arg(long)]
        json: bool,
        /// Repository path to index
        path: PathBuf,
    },
    /// Render one diagram to stdout (Mermaid/DOT/SVG)
    Graph {
        /// Diagram backend
        #[arg(long, value_enum, default_value = "mermaid")]
        format: GraphFormat,
        /// Which diagram to draw
        #[arg(long, value_enum, default_value = "call")]
        scope: GraphScope,
        /// Symbol id/name (required by --scope cfg; rejected elsewhere)
        #[arg(long)]
        symbol: Option<String>,
        /// Repository path to index
        path: PathBuf,
    },
    /// Per-file variable table: name, kind, type, reads/writes/uses, decl line
    Variables {
        /// File to inspect (repo-relative path, or a unique suffix)
        file: String,
        /// Emit JSON instead of a table
        #[arg(long)]
        json: bool,
        /// Repository path to index
        path: PathBuf,
    },
    /// Sorted complexity: hotspots per function, or a per-file roll-up
    Complexity {
        /// Rank by function or by file
        #[arg(long, value_enum, default_value = "function")]
        by: ComplexityBy,
        /// Cap the result (0 = all); applies to both views
        #[arg(long, default_value_t = 20)]
        top: usize,
        /// Emit JSON instead of a table
        #[arg(long)]
        json: bool,
        /// Repository path to index
        path: PathBuf,
    },
    /// All use sites of a symbol or variable id: file:line + snippet
    Refs {
        /// Symbol id/name or variable id/name
        target: String,
        /// Emit JSON instead of a table
        #[arg(long)]
        json: bool,
        /// Repository path to index
        path: PathBuf,
    },
}

mod commands;
mod json;
mod render;

fn main() -> anyhow::Result<()> {
    match Cli::parse().cmd {
        Cmd::Map { path } => commands::map::run(&path),
        Cmd::Tour { path } => commands::tour::run(&path),
        Cmd::Explain { symbol, path } => commands::explain::run(&symbol, &path),
        Cmd::Find { query, path } => commands::find::run(&query, &path),
        Cmd::Flow {
            a,
            b,
            from,
            depth,
            md,
            path,
        } => commands::flow::run(a, b, from, depth, md, path),
        Cmd::Report { md, json, path } => commands::report::run(md, json, &path),
        Cmd::Graph {
            format,
            scope,
            symbol,
            path,
        } => commands::graph::run(format, scope, symbol.as_deref(), &path),
        Cmd::Variables { file, json, path } => commands::variables::run(&file, json, &path),
        Cmd::Complexity {
            by,
            top,
            json,
            path,
        } => commands::complexity::run(by, top, json, &path),
        Cmd::Refs { target, json, path } => commands::refs::run(&target, json, &path),
    }
}
