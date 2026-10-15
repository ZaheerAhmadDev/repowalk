// TypeScript golden fixture for F9 (symbols / variables / CFG / types).
//
// GOLDEN — hand-counted, source of truth for the TypeScript case of
// `crates/core/tests/analysis_golden.rs`.
//
// Deliberately UNUSED local: `unusedTag` inside `Dog.describe` (exactly one).
// Everything else is referenced, so the unused-variable count is exactly 1.
//
// CFG (hand count):
//   classify : cyclomatic=3 (one `for` + one `if` + 1), max_nesting=2, BACK edge
//   bark     : cyclomatic=2 (one `for` + 1), max_nesting=1, one BACK edge
//   speak    : cyclomatic=1, max_nesting=0, no back edge
//
// Types (hand count):
//   Animal : class, field `name: string`, method `speak`, ctor
//   Dog    : class, extends Animal; fields `legs` (inferred number) and
//            `barkCount: number`; methods `describe` + `bark`, ctor
//
// Variables (hand count, `uses = reads + writes`):
//   unusedTag : local, reads=0, writes=1, uses=1 (deliberately unused)
//   positives : local, reads=3, writes=3, uses=6 (decl; `+=`; `-=`; return)

export class Animal {
  name: string;

  constructor(name: string) {
    this.name = name;
  }

  speak(): string {
    return `${this.name} makes a sound`;
  }
}

export class Dog extends Animal {
  legs = 4;
  barkCount: number;

  constructor(name: string, barkCount: number) {
    super(name);
    this.barkCount = barkCount;
  }

  describe(): string {
    const unusedTag = "dead"; // deliberately unused local (reads=0, writes=1)
    return this.speak();
  }

  bark(times: number): string {
    let out = "woof";
    for (let i = 1; i < times; i++) {
      out += "woof";
    }
    return out;
  }
}

export function classify(samples: number[]): number {
  let positives = 0;
  for (let i = 0; i < samples.length; i++) {
    if (samples[i] > 0) {
      positives += 1;
    } else {
      positives -= 1;
    }
  }
  return positives;
}
