<!-- description: Rust-side unit/integration test coverage targets (600+ cases) -->
<!-- done: 2026-03-12 -->
# Rust Compiler Test Coverage

Rust-side unit/integration tests (`cargo test`). Separate from `.almd` language tests.

Current: 9 test files, 470 test cases. All passing.

## Goal

Reach **600+ test cases** across all compiler modules. Every module with >100 LOC should have dedicated tests.

## Coverage Targets

| Module | LOC | Tests | Target | Status |
|--------|-----|-------|--------|--------|
| **lexer** | 974 | 58 | 60 | tokens, numbers, strings, keywords, newlines, comments, interpolation, edge cases |
| **parser/** | 2410 | 67 | 80 | decls, stmts, exprs, patterns, operators, imports, spans |
| **check/** | 2276 | 76 | 80 | type errors, operators, stdlib calls, match, records, lambdas |
| **types** | 328 | 56 | 56 | done: display, compatible, unify, substitute, TypeEnv |
| **ir** | 261 | 41 | 41 | done: all node types, VarTable, patterns |
| **lower** | 924 | 65 | 80 | all constructs, member/tuple/index access, pipes, module calls |
| **diagnostic** | 263 | 23 | 25 | creation, display, source rendering, secondary spans, clone |
| **fmt** | 781 | 42 | 40 | done: idempotency, records, lists, expressions, declarations |
| **emit_ts/** | 1390 | 42 | 50 | fns, types, match, ok/err, let/var, operators, TS annotations |
| **emit_rust/** | 2664 | 0 | 60 | blocked: binary-only module, needs lib.rs export |
| **stdlib** | 161 | 0 | 15 | UFCS resolution, module registry |
| **resolve** | 496 | 0 | 20 | import resolution, circular detection |
| **project** | 345 | 0 | 20 | TOML parsing, dep resolution, cache |
| **emit_common** | 36 | 0 | 5 | shared codegen utilities |
| **ast** | 305 | 0 | 10 | Span helpers, ResolvedType |
| **emit_ts_runtime** | 880 | 0 | 10 | runtime string presence |
| **borrow** | 458 | 0 | 30 | analyze_program, param inference |

## Blockers

### emit_rust is binary-only
`emit_rust/` is `mod emit_rust` in `main.rs`, not exported from `lib.rs`. Internal files reference `almide::ir` (library crate). Moving to lib.rs requires changing `almide::` → `crate::` references. Until resolved, emit_rust can only be tested indirectly via end-to-end `.almd` compilation.

## Priority Areas

### P0 — High-value gaps
1. **emit_rust → lib.rs migration** — unblock 60 tests for the largest untested module (2664 LOC)
2. **checker error paths** — verify every type error produces the correct message and hint
3. **borrow inference** — unit test `analyze_program` with crafted IR programs

### P1 — Moderate gaps
4. **parser edge cases** — deeply nested exprs, all pattern forms, operator precedence chains
5. **lower desugaring** — UFCS method calls, do-block auto-unwrap, destructuring bind
6. **fmt idempotency** — `format(format(x)) == format(x)` for all syntax

### P2 — Low priority
7. **project.rs** — TOML parsing, dep fetching (needs filesystem mocking)
8. **resolve.rs** — import chains, circular import detection
9. **stdlib.rs** — UFCS candidate resolution

## Metrics

Track with `cargo test 2>&1 | grep "^running" | grep -v "0 tests" | awk '{sum+=$2} END {print sum}'`

| Date | Test Files | Test Cases | Notes |
|------|-----------|------------|-------|
| 2026-03-12 | 9 | 317 | Initial: lexer, types, checker, diagnostic, ir, lower, parser, fmt, emit_ts |
| 2026-03-12 | 9 | 470 | Round 2: expanded all 9 test files (+153 tests) |
