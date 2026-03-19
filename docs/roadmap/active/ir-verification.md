# IR Verification Pass [ACTIVE]

Debug-only integrity checks on the typed IR. Runs after optimization, before monomorphization. Catches internal compiler errors before they reach codegen or rustc.

## Implemented (Phase 1)

### Structural integrity
- [x] **VarId bounds** — every `Var { id }`, `Bind { var }`, parameter, and pattern binding references a valid VarTable entry
- [x] **Parameter VarId uniqueness** — no two parameters in a function share the same VarId
- [x] **Loop context** — `Break`/`Continue` only inside `ForIn`, `While`, or `DoBlock` (guard-loop)
- [x] **Pattern variable validation** — all VarIds in match patterns are within VarTable bounds

### Type consistency
- [x] **BinOp type dispatch** — `AddInt` requires Int operands, `AddFloat` requires Float, etc. (all 20 variants)
- [x] **UnOp type dispatch** — `NegInt` requires Int, `NegFloat` requires Float, `Not` requires Bool
- [x] **Unknown/TypeVar tolerance** — unresolved types are skipped (error recovery / generics)

### Type declaration integrity
- [x] **Duplicate record fields** — no two fields in a record type share the same name
- [x] **Duplicate variant cases** — no two cases in a variant type share the same name

### Module coverage
- [x] All checks apply to both main program and imported user modules

## Planned (Phase 2)

| Check | Purpose |
|-------|---------|
| **Use-count cross-check** | Independent variable reference count vs VarTable.use_count — catches bugs in loop bumping / lambda capture logic |
| **CallTarget validity** | Named function calls reference functions that exist in the program or stdlib |
| **Exhaustive IrExprKind walk** | Ensure new IR node variants added in the future don't bypass verification |

## Design

- **Debug-only**: `#[cfg(debug_assertions)]` — zero cost in release builds
- **Pipeline position**: Lower → optimize → **verify** → mono → codegen
- **Error type**: `IrVerifyError` with message, function name, and span — printed as `internal compiler error:`
- **Pattern**: Same walk-based post-pass as `unknown.rs` and `use_count.rs`

## Affected files

| File | Role |
|------|------|
| `src/ir/verify.rs` | Verification logic and tests (16 tests) |
| `src/ir/mod.rs` | Module registration |
| `src/main.rs` | Pipeline insertion point |
