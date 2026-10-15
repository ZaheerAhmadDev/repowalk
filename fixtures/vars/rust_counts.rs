//! Variable-usage golden for the Rust analyzer (F4).
//!
//! THIS COMMENT BLOCK IS THE GOLDEN SOURCE OF TRUTH for the Rust case of
//! `crates/core/tests/analysis_golden.rs`. The counts below are hand-counted
//! against the exact source in this file.
//!
//! Counting convention (mirrored by `fixtures/vars/python_counts.py`):
//!   * a declaration / binding counts as exactly one WRITE;
//!   * every later appearance that only reads the value counts as one READ;
//!   * a plain reassignment (`x = v`) counts as one WRITE;
//!   * a compound assignment (`x += v`) counts as one READ and one WRITE;
//!   * `uses = reads + writes`.
//!
//! Hand counts (variable: kind, reads, writes, uses):
//!   x        : param, reads=3, writes=0, uses=3  (x + 1; y = x; x + y)
//!   y        : local, reads=2, writes=2, uses=4  (decl write; `y > 3` read;
//!              `y = x` write; `x + y` read)
//!   limit    : param, reads=1, writes=0, uses=1  (0..limit)
//!   total    : local, reads=2, writes=2, uses=4  (decl write; `total += i` is
//!              one read + one write; `total` returned ⇒ read)
//!   out      : local, reads=1, writes=1, uses=2  (decl write; `out + sum` read)
//!   sum      : local, reads=1, writes=1, uses=2  (decl write; `out + sum` read)
//!   combined : local, reads=1, writes=1, uses=2  (decl write; `_ = combined` read)
//!
//! `i` (the `for` loop binding) is intentionally NOT part of the golden: its
//! per-iteration read semantics differ between analyzers, so we do not pin it.
//!
//! Functions / CFG (hand count):
//!   clamp_like : cyclomatic=2 (one `if` + 1), max_nesting=1, no back edge
//!   count_up   : cyclomatic=2 (one `for` + 1), max_nesting=1, one BACK edge
//!   main       : cyclomatic=1, max_nesting=0, no back edge

fn clamp_like(x: i32) -> i32 {
    let mut y = x + 1;
    if y > 3 {
        y = x;
    }
    x + y
}

fn count_up(limit: i32) -> i32 {
    let mut total = 0;
    for i in 0..limit {
        total += i;
    }
    total
}

fn main() {
    let out = clamp_like(2);
    let sum = count_up(4);
    let combined = out + sum;
    let _ = combined;
}
