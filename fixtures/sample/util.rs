//! Sample helpers with doc comments.

/// Greets one person by name.
pub fn greet(name: &str) -> String {
    format!("hello, {name}")
}

/// Runs the helper pipeline (called from main).
pub fn helper() {
    let _ = greet("world");
}
