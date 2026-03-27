<!-- description: WASM compile error elimination (type mismatches, lambda issues) -->
<!-- done: 2026-03-23 -->
# WASM Compile Error Elimination Roadmap

## Status: 10 failures (109/182 passed)

## Error Classification and Fixes

### Category A: i32/i64 type mismatch (6 files)

**Symptom**: `expected i32, found i64` or `expected i64, found i32`
**Affected files**: typed-api-client, almide-grep, codegen_variant_record_test, hash_protocol_test, set_extra_test, (grade_report — moved to another category)

**Root cause**: When building call_indirect types for lambda/closures, param/ret ValTypes are derived from the lambda's Fn type (`args[N].ty`), but in cases where type inference is incomplete, Unknown/TypeVar remains, and `ty_to_valtype`'s catch-all returns i32. When the actual value is i64 (Int) or f64 (Float), this causes a mismatch.

**Already fixed**:
- list.map: derive out_elem_ty from call-site return type
- list.fold: derive acc_ty and elem_ty from concrete argument types
- list.filter: derive elem_ty from list type, fix ret to i32 (Bool)

**Remaining fixes**:
1. **Unify all closure functions to concrete type derivation** — remaining functions in calls_list_closure.rs, calls_list_closure2.rs
2. **Create common helper `emit_closure_call_indirect`**:
   ```rust
   fn emit_closure_call_indirect(&mut self, param_types: &[&Ty], ret_ty: &Ty)
   ```
   Consolidate each function's call_indirect construction to one line. Manage Unknown fallback logic in one place.
3. **Apply same fix to map closure functions** (calls_map_closure.rs)
4. **Remaining open record generics**: additional fixes for incomplete VarTable updates in mono

### Category B: nothing on stack (2 files)

**Symptom**: `expected a type but nothing on stack`
**Affected files**: data_types_test, map_higher_order_test

**Root cause**: Code paths fail to produce a value in blocks that should return one. Candidates:
- match arm returns Unit while the outer context expects a value
- do-block's tail expression is missing
- Value disappears during Result unwrap in effect fn

**Fixes**:
1. Extract minimal reproduction case from data_types_test, map_higher_order_test
2. Identify the problem function from WASM validator offset
3. Fix the relevant emit_expr path (stack consistency for Block/If/Match)

### Category C: local index out of bounds (2 files)

**Symptom**: `unknown local N: local index out of bounds`
**Affected files**: list_completion_test (local 14), grade_report (local 22)

**Root cause**: `count_scratch_depth` undercounts the maximum scratch local usage within a function. Actual local indices used during emit exceed the allocated count.

**Fixes**:
1. Identify missing patterns in `count_scratch_depth` (statements.rs)
2. Dump the function's IR to confirm the actual scratch count needed
3. Increase depth for the affected patterns
4. Especially nested closure call + multiple scratch usage combinations

### Category D: values remaining on stack (1 file)

**Symptom**: `values remaining on stack at end of block`
**Affected files**: config_merger

**Root cause**: Extra values remain on the stack at block end. For example, one branch of Block/If/Match returns a value while the other doesn't.

**Fixes**:
1. Extract minimal reproduction case from config_merger
2. Identify the problem block from WASM validator offset
3. Fix the stack balance in the relevant emit path

## Implementation Order

1. **Category A helper extraction** (highest impact: 6 files) — create `emit_closure_call_indirect` helper, apply to all closure functions
2. **Category C scratch depth** (2 files) — fix missing patterns in count_scratch_depth
3. **Category B nothing on stack** (2 files) — minimal reproduction → fix
4. **Category D values remaining** (1 file) — minimal reproduction → fix

## Related Files

- `src/codegen/emit_wasm/calls_list_closure.rs` — closure list functions (find, any, all, etc.)
- `src/codegen/emit_wasm/calls_list_closure2.rs` — closure list functions (take_while, fold, map, etc.)
- `src/codegen/emit_wasm/calls_map_closure.rs` — closure map functions
- `src/codegen/emit_wasm/statements.rs` — count_scratch_depth
- `src/mono.rs` — VarTable update (open record generic)
