//! Half of the sample import cycle.

use crate::b;

/// Calls into b, closing the a → b → a cycle.
pub fn a_fn() {
    b::b_fn();
}
