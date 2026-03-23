# WASM Remaining 3 Failures — Root Cause Analysis & Fix Plan

## Current Status
- **151 passed, 3 failed, 28 skipped** (of 182 files)
- Session progress: 105 → 151 (+46)

## The 3 Failures

### 1. `exercises/config-merger/config_merger.almd` — COMPILE ERROR
**Symptom**: `values remaining on stack at end of block` in func 73 (fold closure)

**Root cause**: Lambda matching by `param_ids` (VarId) fails when monomorphization duplicates functions. `parse_config` calls `str_to_int` which contains fold lambdas. After mono, both the original and the copy share the same VarIds. `emit_lambda_closure` does `find()` on `param_ids`, always returning the **first** match — which may be the wrong lambda.

**Evidence**: func 73's WAT shows `call 71` (parse_config body) followed by drops — this is str_to_int's lambda body being emitted where parse_config's fold lambda body should be.

**Fix**: The lambda identification system needs to be index-based, not VarId-based.

**Options**:
1. **IR-level fix**: Assign a unique `lambda_id: u32` to each `IrExprKind::Lambda` during lowering. Store this in the IR node. During pre_scan, record the lambda_id → LambdaInfo mapping. During emission, look up by lambda_id. This is the **correct** fix — O(1) lookup, no ambiguity.
2. **Mono fix**: During monomorphization, renumber VarIds in copied functions to avoid collisions. This fixes the param_ids uniqueness assumption.
3. **Workaround**: Encode more context into the key (e.g., parent function name + lambda position) to disambiguate. Fragile.

**Recommended**: Option 1. Add `lambda_id: Option<u32>` to `IrExprKind::Lambda`. Set it during lowering (global counter). Use it for lookup in codegen.

### 2. `exercises/grade-report/grade_report.almd` — RUNTIME TRAP
**Symptom**: `indirect call type mismatch` at func 69 (5/6 tests pass)

**Root cause**: Same as config-merger — lambda matching returns wrong lambda. func 69 is `(i32) → i32` (1-param closure = str_to_int itself), but fold's call_indirect expects `(i32, i32, i64) → i32` (3-param closure). The table_idx points to the wrong function.

**Fix**: Same as config-merger — lambda_id-based lookup.

### 3. `exercises/data-table/data_table.almd` — RUNTIME TRAP
**Symptom**: `out of bounds memory access` at func 77 (0/4 tests pass)

**Root cause**: Generic function `pluck_ids[T: { id: Int, .. }]` is monomorphized for `T = Row`. The lambda `(x) => x.id` inside `list.map` gets the correct closure type `(i32, i32) → i64`. However, the closure's `x` parameter receives a Row ptr (i32) and does `i64.load` at offset 0 to get `id`. The Row ptr is valid, but the test uses `let TABLE = [...]` as a **top-level let** (global). Top-level let initialization via `compile_init_globals` may not correctly handle List[Row] with heap-allocated Row records — the global stores a list ptr, but the Rows may not be properly allocated in the init globals context (different heap state).

**Fix**: Investigate `compile_init_globals` to ensure Record construction within List literals works correctly for top-level lets. Alternatively, test with local `let TABLE` to confirm the issue is top-level-specific.

## Architectural Observations

### Lambda Identification Problem
The current system identifies lambdas by `param_ids` (Vec<u32> of VarIds). This breaks when:
- Monomorphization copies functions (VarIds are not renumbered)
- Multiple lambdas with same param types exist in the same scope

The fix is to assign each lambda a unique ID at IR construction time, preserving it through mono and codegen.

### Impact of Fix
- config-merger: Would fix the compile error (closure body mismatch)
- grade-report: Would fix the indirect call type mismatch (correct lambda → correct type)
- data-table: Unrelated (top-level let / generic mono issue)

### Estimated Effort
- Lambda ID (option 1): ~2 hours. Touch `ir/mod.rs`, `lower/expressions.rs`, `codegen/emit_wasm/closures.rs`, `codegen/emit_wasm/calls_lambda.rs`.
- data-table top-level let: ~1 hour. Investigate `compile_init_globals` for Record-in-List allocation.
