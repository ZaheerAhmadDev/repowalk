//! Shared ANSI helpers for command renderers (colored crate).
//! OWNER: RSK-11.
use colored::Colorize;

pub fn head(s: &str) -> String {
    format!("{}", s.bold().cyan())
}

pub fn dim(s: &str) -> String {
    format!("{}", s.dimmed())
}

pub fn warn(s: &str) -> String {
    format!("{}", s.yellow())
}
