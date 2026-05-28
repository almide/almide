# Correctness Guarantee Gaps

> "well-typed source -> correct binary" chain: layers that lack mechanical/mathematical proof

Status: **Active** — analysis complete, milestone steps defined per-layer

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

**Current state**: WasmBuilder + WasmIR + LayoutRegistry infrastructure exists in `emit_wasm/engine/`. Partial migration done (list_layout.rs uses WasmBuilder). Most of `expressions.rs` and `rt_*.rs` still use raw `wasm!()`.

### 2. ANF pass — heap alloc visibility unproven

ANF must lift every heap intermediate to a VDecl so Perceus can track it. `needs_lift()` covers known cases but there is no proof it covers ALL cases. A miss = heap leak.

**Current state**: `needs_lift()` covers Call, RuntimeCall, BinOp, If, Match, Block. **Known gaps**: Fan, List/MapLiteral/Record/Tuple literals, StringInterp, IterChain, ForIn, While, UnwrapOr, OptionalChain, Member, IndexAccess, ClosureCreate are NOT matched. No dedicated ANF test exists.

### 3. Closure conversion — env layout correctness

`ClosureConversionPass` packs capture variables into an env struct and reads them via `EnvLoad` with computed offsets. Offset correctness is verified only by tests.

**Current state**: Offset formula is `index * 8` with captures sorted by VarId. Emission uses type-aware loads (I32/I64/F64). Zero `debug_assert` on offset bounds or env size consistency. Test coverage: `closure_nested_capture_test.almd` + `monkey06_closures_test.almd`.

### 4. Type inference -> IR lowering fidelity

Types inferred in `almide-frontend` must be faithfully reflected in IR `expr.ty`. `ConcretizeTypes` postcondition checks exist but coverage of all patterns is unproven.

**Current state**: `resolve_node_ty` handles 17 variants explicitly. 24+ variants return None (MapLiteral, Record, SpreadRecord, Range, MapAccess, StringInterp, Try, UnwrapOr, ToOption, OptionalChain, Clone, Deref, Borrow, BoxNew, RcWrap, ClosureCreate, FnRef, ForIn, While, Fan, etc.). Postcondition audit (`audit_remaining_unresolved`) catches unresolved types but does not guarantee all variants were visited. Design is intentionally best-effort.

### 5. Perceus Inc/Dec insertion — Lean-to-Rust fidelity

The Lean 4 theorems prove the algorithm correct, but the Rust implementation was hand-translated, not mechanically extracted. Conformance is verified by manual comparison with the Lean spec.

**Current state**: Strongest of all gaps. `perceus_verified.rs` mirrors Lean proofs in Rust. proptest validates is_freed/has_dec. PerceusVerifyPass runs on every WASM build. `perceus_monkey_test.almd` has adversarial cases. Lean proofs: 23 theorems, 0 sorry.

## Risk ranking

1. **WASM emitter** — highest risk, broadest surface, no static checking
2. **ANF `needs_lift()`** — silent failure mode (leak, not crash)
3. **Closure env offsets** — wrong offset = memory corruption
4. **Type lowering fidelity** — mitigated by postcondition checks
5. **Perceus Lean conformance** — mitigated by extensive test suite

---

## Milestone Steps

### Gap 1: WASM emitter -> WasmIR migration

Tracked in: [wasm-engine-redesign.md](wasm-engine-redesign.md)

| Step | Description | Deliverable | Depends on |
|------|-------------|-------------|------------|
| 1a | Audit remaining raw `wasm!()` call sites | List of files/functions not yet using WasmBuilder | — |
| 1b | Migrate `rt_string.rs` to WasmBuilder | Zero raw `wasm!()` in rt_string | — |
| 1c | Migrate `rt_list.rs` to WasmBuilder | Zero raw `wasm!()` in rt_list | — |
| 1d | Migrate `expressions.rs` (BinOp, Match, Record) | Zero raw `wasm!()` in expressions | — |
| 1e | Add stack-effect type annotations to WasmIR Op enum | Each Op declares `(pops, pushes)` | 1b-1d |
| 1f | Implement stack-effect verifier on WasmIR | Reject instruction sequences where net effect != expected | 1e |
| 1g | Delete old raw emission paths | `wasm!()` macro removed or dead | 1f |

**Gate**: after 1f, every WASM function's instruction stream is statically verified for stack balance before encoding.

### Gap 2: ANF `needs_lift()` completeness

| Step | Description | Deliverable | Depends on |
|------|-------------|-------------|------------|
| 2a | Add missing IrExprKind variants to `needs_lift()` | Fan, List, MapLiteral, Record, Tuple, StringInterp, IterChain, ForIn, While, UnwrapOr, OptionalChain, Member, IndexAccess, ClosureCreate | — |
| 2b | Add postcondition assert: after ANF, walk all Call/RuntimeCall/BinOp args and assert each heap-typed arg is `IrExprKind::Var` | Debug-mode panic on violation | 2a |
| 2c | Add ANF-specific spec test: nested heap expressions in every position | `spec/lang/anf_lift_test.almd` | 2a |

**Gate**: after 2b, a missed case triggers a debug-mode panic instead of a silent leak.

### Gap 3: Closure env offset verification

| Step | Description | Deliverable | Depends on |
|------|-------------|-------------|------------|
| 3a | Add `debug_assert!(index < captures.len())` in EnvLoad emission | Panic on out-of-bounds index | — |
| 3b | Add ClosureVerifyPass (postcondition on ClosureConversionPass): for each lifted fn, assert all EnvLoad indices < param env_size / 8 | Registered as postcondition | — |
| 3c | Add closure capture fuzzer: random capture patterns (0-20 vars, mixed types, nested) | proptest in `tests/closure_env_test.rs` | 3b |

**Gate**: after 3b, offset mismatch is caught at compile time (debug builds).

### Gap 4: ConcretizeTypes postcondition coverage

| Step | Description | Deliverable | Depends on |
|------|-------------|-------------|------------|
| 4a | Add trivial `resolve_node_ty` cases: StringInterp->String, Clone->expr.ty, Deref->inner, Range->List[Int] | Reduce None-returning variants from 24 to ~15 | — |
| 4b | Add per-variant visit counter to postcondition audit | CI log shows which variants were never visited (coverage blind spots) | — |
| 4c | Convert audit to hard error for non-whitelisted Unknown types | Unknown in non-whitelisted variant = compile error, not silent fallthrough | 4a, 4b |

**Gate**: after 4c, new IR variants that produce Unknown without being whitelisted fail the build.

### Gap 5: Perceus Lean->Rust conformance

| Step | Description | Deliverable | Depends on |
|------|-------------|-------------|------------|
| 5a | Differential test: serialize IR to JSON, run both Lean `perceusTransform` and Rust `perceus_fnbody`, compare Inc/Dec positions | `tests/perceus_differential_test.rs` | — |
| 5b | Expand proptest coverage: closure captures, mutable reassignment, nested match | Additional proptest strategies in `perceus_verified.rs` | — |

**Gate**: 5a provides mechanical conformance evidence. Full extraction (Lean->Rust codegen) is a long-term aspiration, not a near-term step.

---

## Priority order

Quick wins first (small effort, high leverage):

1. **2a + 2b** — ANF needs_lift() fix + assert. Days, not weeks. Closes silent leak risk.
2. **3a + 3b** — Closure offset asserts. Trivial to add.
3. **4a** — ConcretizeTypes easy cases. Straightforward.
4. **5b** — Expand Perceus proptest. Low effort, incremental.
5. **1b-1g** — WASM emitter migration. Largest effort, tracked separately.
