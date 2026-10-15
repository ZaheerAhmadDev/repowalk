"""Variable-usage golden for the Python analyzer (F4).

THIS DOCSTRING IS THE GOLDEN SOURCE OF TRUTH for the Python case of
`crates/core/tests/analysis_golden.rs`. The counts below are hand-counted
against the exact source in this file.

Counting convention (mirrors `fixtures/vars/rust_counts.rs`):
  * a binding (`x = ...`) counts as exactly one WRITE;
  * every later appearance that only reads the value counts as one READ;
  * a compound assignment (`x += v`) counts as one READ and one WRITE;
  * `uses = reads + writes`.

Hand counts (variable: kind, reads, writes, uses):
  x         : param, reads=3, writes=0, uses=3   (x + 1; y = x; x + y)
  y         : local, reads=2, writes=2, uses=4   (`y = x + 1` write; `y > 3`
              read; `y = x` write; `x + y` read)
  limit     : param, reads=1, writes=0, uses=1   (range(limit))
  total     : local, reads=2, writes=2, uses=4   (`total = 0` write; `total += i`
              is one read + one write; `return total` read)
  out       : local, reads=1, writes=1, uses=2   (`out = clamp_like(2)` write;
              value read when passed to print)
  total_sum : local, reads=1, writes=1, uses=2   (write; read when passed to print)

`i` (the `for` loop binding) is intentionally NOT part of the golden: its
per-iteration read semantics differ between analyzers, so we do not pin it.

Functions / CFG (hand count):
  clamp_like : cyclomatic=2 (one `if` + 1), max_nesting=1, no back edge
  count_up   : cyclomatic=2 (one `for` + 1), max_nesting=1, one BACK edge
  main       : cyclomatic=1, max_nesting=0, no back edge
"""


def clamp_like(x):
    y = x + 1
    if y > 3:
        y = x
    return x + y


def count_up(limit):
    total = 0
    for i in range(limit):
        total += i
    return total


def main():
    out = clamp_like(2)
    total_sum = count_up(4)
    print(out, total_sum)
