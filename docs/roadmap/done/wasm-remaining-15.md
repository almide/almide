<!-- description: WASM spec skip burndown — fixable skips all resolved, 233/233 wasm-eligible (8 intentional skips) -->
<!-- done: 2026-06-05 -->
# WASM Remaining 15 Skips — Fix Roadmap

> **Status**: ✅ Done (2026-06-05) — fixable skips all resolved. Final state: 241/241 spec files
> native, 233/233 wasm-eligible (8 intentional `wasm:skip` — the "Unfixable" category below),
> 2807 test cases all passing. Numbers below are frozen at the 225/240 milestone for historical
> reference — do not cite "225/240" as a current regression. Related: closure-repr convergence
> (uniform `Rc<dyn Fn>`, PR #345) and [determinism-belt](../active/determinism-belt.md).

## Skip Categories

### A. Unfixable (7) — skip is correct

| Test | Reason |
|------|--------|
| extern_test | @extern not available in WASM |
| extern_c_test | @extern(c) not available in WASM |
| net_test | OS-only API (sockets) |
| zlib_test | OS-only API (native zlib) |
| random_test | random module resolver issue |
| process_exec_status_test | OS-only API |
| process_ext_test | OS-only API |

### B. TypeVar/Pipe Chain (5) — `pre_scan_closures` type registration

**Tests**: codegen_pipes, fold_lambda_param_inference, test_where, stream_fusion, generics_recursive

**Symptom**: `failed to compile: wasm[0]::function[N]` — type mismatch (expected i32, found f64/i64)

**Root Cause**: In pipe chains like `xs |> list.map(f) |> list.fold(init, g)`, the fold lambda `g`'s params are registered as `i32` (Unknown fallback) instead of `f64` (Float).

**Why**: `pre_scan_closures` in `closures.rs` registers lambda WASM types. For lifted lambdas, it reads `(vid, ty)` from the lambda's params. The `ty` is resolved, but **`closures.rs` also has a `resolve_lambda_param_ty` fallback** that reads VarTable. VarTable entries for fold lambda params inside pipe chains remain Unknown because:

1. `LambdaTypeResolvePass` runs top-down: resolves outer fold's lambda first, but `list.map` Call's `.ty` is still unresolved
2. `compute_stdlib_call_ret` returns `None` for `"map"` — doesn't compute `List[f.ret]`
3. `ConcretizeTypesPass` runs after, but pipe chain intermediate types may not propagate

**Fix Plan** (3 steps, each independently testable):

1. **`compute_stdlib_call_ret`: add `"map"` return type**
   - `"map"` → inspect lambda arg's `Fn.ret` or `Lambda.body.ty` → `List[ret]`
   - File: `pass_lambda_type_resolve.rs` line ~799

2. **`resolve_expr`: bottom-up for Call args**
   - Process args before `resolve_call_lambdas` so inner calls have resolved `.ty`
   - File: `pass_lambda_type_resolve.rs` line ~98

3. **`pre_scan_closures`: use lifted function param types**
   - When lambda is lifted to `program.functions`, the IrFunction's `params[].ty` is resolved
   - Match lambda by `lambda_id` → lifted function → use its param types
   - File: `closures.rs` line ~54

**Verification**: `echo 'test "t" { let sum = [(true, 1.0), (true, 2.0)] |> list.map((p) => p.1) |> list.fold(0.0, (a, b) => a + b); assert_eq(sum, 3.0) }' > /tmp/t.almd && almide test /tmp/t.almd --target wasm`

### C. Codegen Bugs (3) — specific feature gaps

| Test | Root Cause | Fix |
|------|-----------|-----|
| hash_protocol_test | User record Hash dispatch not implemented | Add hash codegen for Named types in `emit_hash_key` |
| opaque_type_test | `mod type` (opaque newtype) ConcretizeTypes Member resolution | Fix `pass_concretize_types.rs` to unwrap opaque types |
| mut_param_test | WASM pass-by-reference not implemented | Need cell-based indirection for mut params (like mutable captures) |

### D. No Test Blocks (remaining counted skips)

| Test | Note |
|------|------|
| coverage_assert_throws | WASM `unreachable` can't be caught — assert_throws is impossible |

## Insight from Grain/OCaml WASM Compilers

**Grain** solves the call_indirect type problem via **uniform representation**:
- All values are `Managed` (boxed, i32 pointer on heap with tag)
- call_indirect signature is always `(i32, i32, ...) -> i32`
- No type mismatch possible — everything is i32
- Perceus-compatible: all values have alloc headers, RC works uniformly

**OCaml (wasm_of_ocaml)** uses 31-bit tagged integers for uniform repr.
**Wasocaml** uses WASM-GC structs with structural subtyping on closures.

### Almide's Design Choice

Almide uses **unboxed values** (Int = i64, Float = f64) for performance.
This creates the call_indirect type signature problem:
- `list.fold(xs, 0.0, (a, b) => a + b)` needs `(i32, f64, f64) -> f64`
- But if lambda params are Unknown → registered as `(i32, i32, i32) -> i32`
- → WASM type mismatch trap

**Three possible solutions**:

1. **Selective boxing for closures** — box lambda params that can't be resolved.
   Cost: box/unbox at closure boundary. Benefit: no perf hit for non-closure code.
   Grain uses this universally; Almide can apply it selectively.

2. **Fix type inference chain** (current approach) — resolve all lambda params to
   concrete types before WASM emit. 3-step plan in section B above.
   Cost: complexity in pass ordering. Benefit: zero runtime overhead.

3. **Type-specialized function tables** — separate tables per closure signature.
   `(i32, f64, f64) -> f64` closures go in table_f64f64, etc.
   Cost: table proliferation. Benefit: no boxing, no inference needed.

**Recommendation**: Option 2 first (fix inference). If too fragile, fall back to
Option 1 (selective boxing) which is architecturally robust and Perceus-native.

## Priority Order

1. **B (TypeVar)** — 5 tests, single root cause, clear fix path
2. **C.hash_protocol** — 1 test, `emit_hash_key` extension
3. **C.opaque_type** — 1 test, ConcretizeTypes fix
4. **C.mut_param** — 1 test, architectural (cell indirection)

## Session Stats

| Metric | Before | After |
|--------|--------|-------|
| WASM tests | 193/240 | 225/240 |
| Magic numbers | 544 | 0 |
| String alloc bug class | present | structurally eliminated (`__string_alloc`) |
| Free list reuse | not implemented | implemented + zero-fill |
| Safety layers | 0 | 3 (validation, Verified gate, RC balance) |
