//! The other half of the sample import cycle.

use crate::a;

/// Calls back into a, closing the cycle.
pub fn b_fn() {
    a::a_fn();
}
