<!-- description: Three-tier import visibility for stdlib modules -->
<!-- done: 2026-03-24 -->
# Stdlib Import Control

**Priority:** 1.0
**Prerequisites:** None

## Problem

Currently, all stdlib modules (22 modules) can be used without `import`. Writing `math.abs(-5)` works without `import math`. This violates the specification.

## Ideal Design

Referencing Swift's Foundation / UIKit model, three tiers of visibility:

### Tier 1: Implicit import (no import needed)

Modules directly tied to core language types. Usable via both UFCS and `module.func()`.

Candidates:
- `string` — String type operations
- `int` — Int type conversions
- `float` — Float type conversions
- `list` — List type operations
- `map` — Map type operations
- `set` — Set type operations
- `option` — Option type operations
- `result` — Result type operations
- `bool` — Bool type operations (if exists)

**Rationale:** These are method sets for core types. If `"hello".len()` (UFCS) works without import, `string.len("hello")` should work equally.

### Tier 2: Explicit import required

General-purpose utilities. Many programs do not use them.

Candidates:
- `math` — Math functions
- `json` — JSON operations
- `regex` — Regular expressions
- `random` — Random numbers
- `datetime` — Date and time
- `env` — Environment variables
- `fs` — Filesystem
- `http` — HTTP client/server
- `path` — Path operations
- `process` — Processes
- `log` — Logging
- `time` — Time
- `codec` — Encoding/decoding

### Tier 3: Built-in (usable without module name)

Language keyword level:
- `println`, `eprintln` — top-level functions
- `assert`, `assert_eq` — for testing
- `ok`, `err`, `some`, `none` — constructors
- `fan` — concurrency

## Design Decisions Needed

1. **Is the Tier 1 list correct?** Is string/int/float/list/map/set/option/result sufficient, or too many?
2. **Relationship with UFCS:** `x.abs()` resolves to `math.abs` from the type. Should it work without import?
   - Option A: UFCS always works (type-based resolution is independent of import)
   - Option B: UFCS also requires import (`(-5).abs()` does not work without `import math`)
   - **Recommendation: Option A** — UFCS is syntactic sugar for method calls, import controls module namespace
3. **Impact on existing tests:** Many tests use stdlib without `import`. Setting Tier 1 correctly should avoid breaking most of them

## Implementation Approach

1. Add `imported_stdlib: HashSet<String>` to `TypeEnv`
2. Collect module names from `program.imports` in `check_program`
3. Implicitly add Tier 1 modules to `imported_stdlib`
4. Check `imported_stdlib` in `resolve_module_call` (infer.rs) and `static_dispatch.rs`
5. stdlib fallback via UFCS in `calls.rs` does not check import (type-based resolution)

## Implementation Progress

| Phase | Content | Status |
|---|---|---|
| Phase 1 | Finalize tier classification (adopted Swift model) | ✅ Complete |
| Phase 2 | imported_stdlib + Tier 1 implicit registration | ✅ Complete |
| Phase 3 | import gate in resolve_module_call + static_dispatch | ✅ Complete |
| Phase 4 | All existing tests pass (import added to 19 files) | ✅ Complete |
| Phase 5 | Error message improvement ("did you mean: import math?") | Not started |
