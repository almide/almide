# Function Reference Passing [WON'T DO]

> Verbose form (`fn(x) => f(x)`) is always correct and LLM-friendly. Not worth the complexity.

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

## Why low priority

The verbose `fn(p) => emit_pat(p)` form is **always correct** regardless of context. LLMs default to it because it's safe. Introducing a shorter form creates a new decision point: "can I pass this function directly, or does it need a wrapper?" (e.g., tuple destructuring cases can't use the short form). This decision itself becomes an error source.

Almide's mission is LLM accuracy, not brevity. The verbose form works — leave it alone.

## Scope (if pursued)

- Ensure named functions (including module-qualified `tmrule.emit_pat`) are first-class values passable to higher-order functions
- Verify UFCS methods work the same way
- May overlap with eta-reduction / partial application design
