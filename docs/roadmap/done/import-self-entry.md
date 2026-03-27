<!-- description: Allow main.almd to access pub functions from same-package mod.almd -->
<!-- done: 2026-03-13 -->
# `import self` — Package Entry Point Access

## Problem

`main.almd` cannot access pub functions from the same package's `mod.almd` (library entry point).

```
src/
  mod.almd    ← External: resolved via import almide_grammar
  main.almd   ← CLI: wants to use pub fn from mod.almd
```

- `import self.mod` → `mod` is a keyword, so parse error
- Separating data into another file (`grammar.almd`) makes the external API deep: `almide_grammar.grammar.keyword_groups()`
- No re-export, so flattening via `mod.almd` is also impossible

## Design

**Make `mod.almd` accessible via `import self`.**

```almide
// main.almd
import self               // → loads src/mod.almd
import self as grammar     // → alias also available

grammar.keyword_groups()   // access pub fn from mod.almd
```

### Semantics

- `import self` = imports the package's `src/mod.almd`
- Existing `import self.xxx` (submodule) remains unchanged
- Error if `mod.almd` doesn't exist: `"package has no mod.almd entry point"`
- Without alias, package name is the prefix (`almide_grammar.keyword_groups()`)

### Why not alternatives

| Alternative | Problem |
|-------------|---------|
| `main.almd` implicitly references `mod.almd` | Implicit behavior goes against Almide's design philosophy |
| re-export (`pub import`) | High introduction cost for a new concept |
| Remove `mod` as a keyword | Large impact on existing code |

## Implementation

`src/resolve.rs` の `resolve_imports_with_deps` 内:

```rust
if is_self_import {
    if path.len() < 2 {
        // NEW: import self → load mod.almd
        let mod_name = alias.as_deref()
            .unwrap_or_else(|| pkg_name.as_deref().unwrap_or("self"));
        // ... load src/mod.almd
    }
    // existing: import self.xxx
}
```

Only one change location in `resolve.rs`. No parser changes needed (`import self` is already a valid token sequence).

## Motivation

This became a blocker when separating `mod.almd` (data definitions) and `main.almd` (CLI generator) in the `almide-grammar` package. The library + CLI pattern will increase in the future, so this should be resolved early.
