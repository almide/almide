# WASM Remaining 15 Skips — Fix Roadmap

> Current: 225/240 pass, 0 fail, 15 skip

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
