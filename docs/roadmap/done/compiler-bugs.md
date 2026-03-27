<!-- description: Codegen bugs and runtime gaps found while writing 400+ test blocks -->
<!-- done: 2026-03-20 -->
# Compiler Bugs and Gaps — Status

Discovered while writing 400+ new test blocks across 32 test files (v0.8.4).

## Fixed (12 items)

All codegen bugs and missing runtime functions have been resolved.

### Codegen Bugs — All Fixed
- [x] **Record spread** — `Config { ...base, debug: true }` now parses and generates correct Rust
- [x] **result.is_ok / is_err UFCS** — Runtime changed to take by-value (was `&Result`)
- [x] **math.pow sig/runtime mismatch** — Runtime fixed to `(i64, i64) -> i64`
- [x] **value.as_* TOML names** — Fixed `value_as_int` → `almide_rt_value_as_int` prefix
- [x] **value.as_* return types** — TOML sigs fixed from `Option` to `Result` to match runtime
- [x] **error.message/context runtime** — Runtime fixed to take `Result<T, String>` instead of `&str`

### Missing Runtime Functions — All Implemented
- [x] `result.flat_map`
- [x] `result.unwrap_or_else`
- [x] `result.to_option`
- [x] `result.to_err_option`
- [x] `float.is_nan`
- [x] `float.is_infinite`
- [x] `json.null`

### Dead Keywords — Removed
- [x] `async`, `await`, `try`, `deriving`, `unsafe` removed from lexer (42 → 37 keywords)
- [x] grammar/tokens.toml, almide-grammar, vscode-almide, tree-sitter-almide all synced

## Language Design Decisions (not bugs)

These are intentional gaps that require design decisions, not bug fixes.

### newtype constructor
- `type UserId = newtype Int` works as a type alias
- `UserId(42)` constructor syntax is not implemented
- **Decision needed**: Should newtype be a true wrapper type (with constructor/unwrap) or remain a type alias?
- **Current behavior**: Type alias only — `let id: UserId = 42` works

### trait/impl block methods
- `trait T { fn f... }` + `impl T for X { fn f... }` is parsed
- But impl methods are NOT registered in the type environment
- **Current alternative**: Convention methods `fn X.f(...)` work and are the recommended pattern
- **Decision needed**: Full trait/impl integration or keep convention methods as the primary mechanism?

### pub on type declarations
- `pub type Point = { ... }` is rejected — `pub` only applies to `fn`
- Types are module-public by default
- **Decision needed**: Add `pub`/`local` visibility to type declarations, or document that types are always public?
