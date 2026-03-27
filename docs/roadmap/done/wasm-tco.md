<!-- description: Tail call optimization for WASM target to prevent stack overflow -->
<!-- done: 2026-03-23 -->
# WASM Tail Call Optimization

## Status: Not implemented — stack overflow on deep recursion (100K+)

## Current State

Almide's TCO strategy is target-dependent:

| Target | TCO Strategy | Status |
|--------|-------------|--------|
| Rust | LLVM auto-transforms | Works |
| TS/JS | V8/JSC JIT optimizes | Works |
| **WASM** | **None** | **stack overflow** |

WASM codegen uses `call` instructions for all recursive calls. Neither the compiler nor the runtime has TCO, so deep recursion exhausts the stack.

```
// sum_to(100000, 0) → 100,000 frames → stack overflow
fn sum_to(n, acc) = if n <= 0 then acc else sum_to(n - 1, acc + n)
```

## Affected Tests

- `spec/lang/tco_test.almd` — "tco deep recursion" (sum_to 100K)
- All tests that indirectly use deep recursion

## Options

### A. Compiler IR pass: tail call → loop transformation (recommended)

An IR pass that converts self-recursive tail calls into loop + argument reassignment.

```
// Before (IR)
fn sum_to(n, acc) {
  if n <= 0 { return acc }
  return sum_to(n - 1, acc + n)   // tail position
}

// After (IR → loop rewrite)
fn sum_to(n, acc) {
  loop {
    if n <= 0 { return acc }
    let (n', acc') = (n - 1, acc + n)
    n = n'; acc = acc'
    continue
  }
}
```

**Advantages**:
- Works consistently across all targets (Rust/TS/JS/WASM)
- No runtime dependency
- Not affected by WASM proposal implementation status

**Detection rule**: `Call { target: Named(self_name) }` at function tail, where all arguments correspond to self's params

**Implementation location**: Add as a nanopass in `src/codegen/`. After mono, before codegen.

**Supported patterns**:
1. **Direct self-recursion** (Phase 1): `fn f(...) { ... f(...) }` — most common
2. **if/match branch tail position** (Phase 1): `if cond { base } else { f(...) }`
3. **Mutual recursion** (Phase 2): `fn f() { g() }; fn g() { f() }` — requires trampoline
4. **CPS transformation** (Phase 3): general tail calls — high difficulty

### B. Using WASM return_call Instructions

Use `return_call` / `return_call_indirect` instructions from the WASM Tail Call proposal.

**Advantages**: Handles all tail calls including mutual recursion
**Disadvantages**:
- wasmtime: requires `--wasm tail-call` flag (off by default)
- Browsers: experimental support in Chrome only, Firefox/Safari unsupported
- Need to verify wasm-encoder crate support
- Loss of portability

### C. Trampoline Pattern

Convert recursive calls to "return next call info" form and run in a driver loop.

```wasm
;; Each function returns "Continue(args)" or "Done(result)"
;; The driver processes Continue in a loop
```

**Advantages**: Handles mutual recursion
**Disadvantages**: Overhead on all calls (heap allocation), complex

## Recommended Implementation Plan

### Phase 1: Self-Recursive Tail Call → Loop (Top Priority)

1. **Tail position detector**: `is_tail_position(expr, fn_name) -> bool`
   - Call at the end of function body
   - Call at the end of each if/match branch
   - Call at the end of do block

2. **Loop rewrite pass**: `pass_tco.rs`
   - Target: functions that call themselves in tail position
   - Transform: wrap entire body in `loop { ... }`, replace tail calls with argument updates + `continue`
   - IR nodes: use existing `Loop` + `Continue` + `Assign`

3. **Tests**:
   - All tests in `tco_test.almd` pass on WASM
   - Existing tests on Rust target do not regress

### Phase 2: return_call Support (Optional)

Waiting for wasmtime default support. Once available, use wasm-encoder's `return_call` to also cover mutual recursion.

## Effort Estimate

- Phase 1 tail position detection: IR walk, 1 file ~150 lines
- Phase 1 loop transformation: IR rewrite, 1 file ~200 lines
- Phase 1 tests: verify with existing tests
- Total: ~350 lines of new code, 1-2 sessions

## Related Files

- `src/codegen/target.rs` — codegen pipeline definition
- `src/codegen/emit_wasm/calls.rs:128-137` — emit of `call` instructions
- `src/ir/mod.rs:218` — `Call` IR node
- `spec/lang/tco_test.almd` — TCO tests
