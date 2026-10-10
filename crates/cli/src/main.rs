//! repowalk CLI — thin renderer over repowalk-core (plan §3.3).
//! OWNER: RSK-11 (main dispatch + render.rs helpers).
use clap::{Parser, Subcommand};
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
}

mod commands;
mod render;

fn main() -> anyhow::Result<()> {
    match Cli::parse().cmd {
        Cmd::Map { path } => commands::map::run(&path),
        Cmd::Tour { path } => commands::tour::run(&path),
        Cmd::Explain { symbol, path } => commands::explain::run(&symbol, &path),
        Cmd::Find { query, path } => commands::find::run(&query, &path),
    }
}
