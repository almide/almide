<!-- description: Postfix !, ??, ? and ?. operators for explicit Result/Option unwrapping -->
<!-- done: 2026-03-27 -->
# Unwrap Operators: `!` `??` `?` `?.`

## Status

**Implemented.** All operators work on all three targets (Rust, TypeScript, WASM). 18 tests passing.

### What's done

- [x] Lexer: `!`, `?`, `??`, `?.` tokens
- [x] Parser: postfix operators in `parse_postfix()`
- [x] AST: `Unwrap`, `UnwrapOr`, `ToOption`, `OptionalChain` nodes
- [x] Type checker: Option/Result-aware inference for all operators
- [x] IR: dedicated nodes + `OptionalChain` for target-specific rendering
- [x] Lowering: AST → IR mapping
- [x] Walker: template-based rendering with type dispatch
- [x] Rust codegen: `?`, `.ok_or()?`, `match`, `.as_ref().map()`
- [x] TS codegen: passthrough, IIFE null-check, `?.` + `?? null`
- [x] WASM codegen: Option/Result layout-aware (ptr==0 vs tag-based)
- [x] All nanopass passes updated (20+ files)
- [x] `From` convention removed from docs
- [x] Tests: 18 tests covering all operators on both types
- [x] Docs site updated

### What remains

- [ ] Remove `pass_result_propagation.rs` auto-`?` insertion (coexists for now)
- [ ] Migration lint: warn on bare Result calls in `effect fn` without operator
- [ ] Update all existing spec/test files to use explicit `!` instead of auto-`?`
- [ ] Bump to v0.9.4 release

## Operators

| Operator | On success | On failure | Context |
|----------|-----------|------------|---------|
| `expr!` | unwrap | propagate err | `effect fn` only |
| `expr ?? val` | unwrap | use fallback | anywhere |
| `expr?` | unwrap → some | `none` | anywhere |
| `expr?.field` | access field | `none` | anywhere |

## Error type design

**Decision: `Result[T, String]` as default, `Result[T, E]` for opt-in typed errors.**

The error type parameter `E` is the #1 source of LLM errors in Rust. Removing that choice for the common case maximizes modification survival rate.

## Breaking changes

1. **`From` convention removed** — Use `result.map_err(...)!` for explicit error type conversion.
2. **auto-`?` still present** — Will be removed in a future release. Explicit `!` is preferred.
3. **`!` as prefix** — Still gives error "use `not`". Postfix `expr!` is a new token; no conflict.
