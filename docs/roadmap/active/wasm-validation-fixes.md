# WASM Validation Error Fixes [ACTIVE]

## Status: 20 validation errors across 20 test files

WASM binary validation fails before execution. These block ALL tests in the affected file.
Fixing these is the highest-leverage work for increasing WASM test pass rate.

## Root Cause A: Scratch Local Undercount (4 files)

**Error**: `unknown local N: local index out of bounds`

**Files**: control_flow_test, for_test, scope_test, variable_test

**Root Cause**: `count_scratch_depth` in `statements.rs` doesn't account for all scratch-consuming patterns. When for..in list body contains constructs that use scratch locals (closures, list construction, Try, etc.), the total depth is underestimated, leading to out-of-bounds local access.

**Patterns not yet counted or undercounted**:
- Closure call inside for..in list body (`call_indirect` uses scratch for env/table_idx lookup)
- Nested list construction inside for body (`[x * x]` uses 1 scratch, for..in uses 2, total should be additive)
- `BindDestructure` inside for..in

**Fix approach**:
1. Audit every `emit_*` method that uses `match_i32_base + match_depth` or `match_i64_base + match_depth`
2. Ensure corresponding `count_scratch_depth` returns the correct depth
3. Key rule: when `match_depth += N` before emitting children, `count_scratch_depth` must return `N + child_depth`

**Complexity**: Medium. Mechanical but requires careful tracing of every scratch usage site.

---

## Root Cause B: Codec/Auto-Generated Function Type Mismatch (10 files)

**Error**: `type mismatch: expected i32, found i64` or `expected i32, found f64`

**Files**: codec_convenience, codec_list, codec_nested, codec_p0, codec_test, codec_weather, auto_derive, edge_cases, protocol_stress, value_utils

**Root Cause**: The Almide compiler auto-generates codec (encode/decode) functions via derive macros. These functions construct `Value` (a JSON-like variant type) from record fields. The generated code does:

```
Value.Int(field_value)   // field_value is i64, but Value.Int payload stored in i32 slot
Value.Float(field_value) // field_value is f64, but stored in i32 slot
```

The WASM codegen emits `i64.store` or `f64.store` into a record field that should hold an `i32` (Value pointer), because the auto-generated IR treats the inner value type directly rather than wrapping it in a heap-allocated Value.

**Fix approach**:
1. Identify how codec functions are lowered to IR — check `src/lower/` for derive/codec handling
2. Ensure Value construction in IR uses `Record { name: "Int", fields: [("value", LitInt)] }` which allocates a heap struct and returns an i32 pointer
3. Alternatively, implement Value-aware codegen in WASM: detect when a function constructs a Value variant and emit the correct allocation + store sequence

**Complexity**: High. Requires understanding the codec/derive pipeline end-to-end.

**Alternative**: Skip codec functions in WASM for now (mark as unsupported), which would clear 7 of 10 files.

---

## Root Cause C: Effect Fn / Closure Return Type Mismatch (6 files)

**Error**: `type mismatch: expected i64, found i32` or `expected f64, found i32`

**Files**: function_test, generics_test, default_fields_test, protocol_advanced, protocol_extreme, type_system_test

**Root Cause**: Multiple sub-causes all resulting in an i32 (pointer) value on the WASM stack where an i64/f64 primitive is expected:

### C1: Effect fn call in non-effect context (function_test, type_system_test)
- Type checker auto-unwraps effect fn return: `add(1,2)` typed as `Int` not `Result[Int,String]`
- But WASM `add` returns `i32` (Result pointer)
- `ResultPropagation` doesn't insert `Try` in non-effect, non-fan contexts
- **Fix**: Extend `ResultPropagation` to insert Try around effect fn calls in ALL contexts, or have WASM codegen detect the mismatch and auto-unwrap at call site

### C2: Closure call_indirect return type inference (scope_test → now local OOB, but related)
- Lambda `(v: Int) => v * multiplier` registered with type `(i32, i64) -> i64`
- But `call_indirect` may reference a type index that doesn't exist or has wrong signature
- **Fix**: Ensure all closure calling convention types are pre-registered in the type section

### C3: RecordPattern field binding scope collision (default_fields_test)
- `Rect { width, height, .. }` pattern binds fields by searching VarTable by name
- Multiple test functions share VarTable, so wrong-scope VarId can be found
- Current fix: search from end of VarTable (most recent scope first)
- **Fix**: Use a more robust VarId resolution: pass function-scoped VarId range, or use the lowerer's explicit VarId from pattern AST

### C4: Generic function monomorphization type mismatch (generics_test, protocol_advanced, protocol_extreme)
- Monomorphized functions may have residual TypeVar in body expressions
- Substitution misses nested type references
- **Fix**: Ensure `substitute_expr_types` in mono.rs handles all expression kinds

**Complexity**: High for C1 (architectural), Medium for C2-C4.

---

## Priority Order

| Priority | Root Cause | Files | Impact | Effort |
|----------|-----------|-------|--------|--------|
| **1** | A: Scratch undercount | 4 | Unblocks 4 files directly | Medium |
| **2** | C1: Effect fn auto-unwrap | 2-3 | Core architectural fix | High |
| **3** | C3: RecordPattern VarId | 1-2 | Correctness fix | Medium |
| **4** | C4: Mono substitution | 2-3 | Fixes generics | Medium |
| **5** | B: Codec Value type | 7-10 | Codec/derive support | High (or skip) |
| **6** | C2: Closure type registration | 1-2 | Edge case | Low |

## Relationship to Other WASM Work

After validation errors are fixed, remaining WASM test failures are runtime traps from:
- Unimplemented stdlib (string.split/join, map.get, float.to_string proper decimal)
- Deep equality for nested containers (List[List[T]], variants with string payloads)
- Protocol/convention method dispatch
- TCO (tail call optimization)

These are independent of validation fixes and can be worked in parallel.
