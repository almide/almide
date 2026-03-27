<!-- description: Fix correctness issues in generated code (auto-unwrap, range, guard) -->
<!-- done: 2026-03-15 -->
# Codegen Correctness Fixes

Fixes for issues affecting generated code correctness.

## P1 (all 7 items complete)

1. **Unify auto-`?` dual logic** ✅ — unified into `should_auto_unwrap_user/stdlib`
2. **Range type hardcoding** ✅ — retrieve element type from IR `expr.ty`
3. **Unbound variables in Box pattern destructuring** ✅ — added `box` / skip for non-Bind patterns
4. **Guard break/continue handling** ✅ — inspect IR node type for appropriate code generation
5. **Do-block + guard unreachable** ✅ — wrap with `loop { ... break; }`
6. **Auto-`?` for Module/Method calls** ✅ — insert `?` for non-Named CallTarget when in effect context + Result return
7. **Result wrapping in effect fn for-loop** ✅ — above fixes + `in_effect` propagated globally via `LowerCtx` field

## P2

1. **Borrowed subject for string patterns** ✅ — auto-insert `.as_str()` for String-type subjects
2. **Clone optimization for pattern destructuring** → merged into Clone Reduction Phase 4 (Member access already uses `is_copy` for determination)
