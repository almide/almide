# Function Reference Passing [ON HOLD]

Make passing named functions as arguments seamless, eliminating unnecessary closure wrappers.

## Motivation

Self-tooling code is full of redundant wrappers:

```almide
// Current — verbose
list.map(pats, fn(p) => emit_pat(p))

// Desired — direct function reference
list.map(pats, emit_pat)
// or with UFCS
pats.map(emit_pat)
```

LLMs default to the verbose form because they're unsure whether direct reference works. If the language guarantees it consistently, LLM output becomes shorter and more idiomatic.

## Scope

- Ensure named functions (including module-qualified `tmrule.emit_pat`) are first-class values passable to higher-order functions
- Verify UFCS methods work the same way
- May overlap with eta-reduction / partial application design
