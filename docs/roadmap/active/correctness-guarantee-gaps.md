# Correctness Guarantee Gaps

> "well-typed source -> correct binary" chain: layers that lack mechanical/mathematical proof

Status: **Active** — analysis complete, fixes tracked per-layer

## Layers WITH guarantees

| Layer | Guarantee | Basis |
|-------|-----------|-------|
| Perceus RC | heap alloc exactly-once freed | Lean 4: 23 theorems |
| StackBalance | void context never leaks stack values | structural invariant (tail=None => no Ret) |
| MonoVerify | no TypeVar survives in live code | live VarId collection + exhaustive check |
| ConcretizeTypes | IR expr.ty and VarTable agree | postcondition check |

## Layers WITHOUT guarantees

### 1. WASM emitter (`emit_wasm/*.rs`) — largest gap

Hand-written WASM instruction emission. Hundreds of `wasm!()` macro calls whose stack effects are verified by human review only.

- `rt_string.rs`, `rt_list.rs` etc: manual instruction sequences
- `expressions.rs`: BinOp, Match, Record — branch stack effects are eyeballed
- Layout offsets: `LayoutRegistry` centralizes them, but correct usage is manual

**Fix path**: `wasm-engine-redesign` WasmIR (typed instruction builder that statically checks stack effects). Designed but not yet implemented.

### 2. ANF pass — heap alloc visibility unproven

ANF must lift every heap intermediate to a VDecl so Perceus can track it. `needs_lift()` covers known cases but there is no proof it covers ALL cases. A miss = heap leak.

### 3. Closure conversion — env layout correctness

`ClosureConversionPass` packs capture variables into an env struct and reads them via `EnvLoad` with computed offsets. Offset correctness is verified only by tests.

### 4. Type inference -> IR lowering fidelity

Types inferred in `almide-frontend` must be faithfully reflected in IR `expr.ty`. `ConcretizeTypes` postcondition checks exist but coverage of all patterns is unproven.

### 5. Perceus Inc/Dec insertion — Lean-to-Rust fidelity

The Lean 4 theorems prove the algorithm correct, but the Rust implementation was hand-translated, not mechanically extracted. Conformance is verified by manual comparison with the Lean spec.

## Risk ranking

1. **WASM emitter** — highest risk, broadest surface, no static checking
2. **ANF `needs_lift()`** — silent failure mode (leak, not crash)
3. **Closure env offsets** — wrong offset = memory corruption
4. **Type lowering fidelity** — mitigated by postcondition checks
5. **Perceus Lean conformance** — mitigated by extensive test suite

## Resolution strategy

| Gap | Approach | Tracked in |
|-----|----------|------------|
| WASM emitter | WasmIR typed builder with stack-effect types | `wasm-engine-redesign` |
| ANF coverage | Property-based test: round-trip IR and assert all heap values have VDecl | — |
| Closure offsets | Fuzzer + offset consistency assertion in debug builds | — |
| Type lowering | Expand ConcretizeTypes postcondition to cover all Expr variants | — |
| Perceus conformance | Differential testing against Lean reference impl | — |
