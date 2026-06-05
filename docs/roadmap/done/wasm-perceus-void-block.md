<!-- description: StackBalancePass — void-context blocks never leak stack values (wasmtime 45+ strict validation) -->
<!-- done: 2026-06-05 -->
# Perceus Void Block Stack Balance — CI Blocker

> **Status**: ✅ Done (v0.23.11, 2026-06-05) — StackBalancePass landed (`pass_stack_balance.rs`), CI + Cross-Target CI green
> **Tests**: `wasm_list_nested_map_filter`, `wasm_cross_target_spec`
> **Error**: `values remaining on stack at end of block` (wasmtime 45+)

## Problem

After ANF lifts heap sub-expressions into Blocks (`wrap_with_lets`), void-context
blocks may have non-Unit tail expressions. Perceus converts these tails to
`FnBody::Ret`, then excludes "returned" variables from `RcDec`. This causes:

1. **Stack imbalance**: the tail pushes a value in a context that expects none
2. **RC leak**: variables "returned" by the tail are not Dec'd (ownership transferred
   to a caller that doesn't exist)

The root cause: Perceus cannot distinguish void-context blocks from value-context
blocks — it treats every tail as a return value.

## Solution: StackBalancePass (ANF → **StackBalance** → Perceus)

A NanoPass that runs **after ANF, before Perceus**. Propagates expected type
top-down from function signatures:

- Void function body → demote tail to `Expr` statement
- `Expr` statement value → demote nested block tails
- Bind/Assign value → preserve tail (value context)

After the pass, void-context blocks have no tails. Perceus sees `Nop` terminus
and correctly Dec's all heap vars.

```
ANF → StackBalance → Perceus → PerceusOpt → PerceusVerify → TailCallMark
```

### Why this placement

| Placement | Problem |
|-----------|---------|
| Inside Perceus (Option A) | Couples RC logic to void-context awareness. Perceus should only care about reference counts. |
| After Perceus (Option B) | Moving tail to Expr stmt changes RC semantics — vars that Perceus excluded from Dec now leak. |
| Emit level (Option C) | Trusts `.ty` annotations that Perceus/ANF may not update. Reactive, not preventive. |
| **Before Perceus** | Perceus sees correct block structure. No type trust. No RC interference. |

### Design principles

- **Context-driven**: expected type comes from function signature, not from `.ty` annotations
- **By construction**: void blocks structurally cannot have tails after the pass
- **Single responsibility**: StackBalance handles stack, Perceus handles RC
- **Defense in depth**: emit-level drops (functions.rs:166, expressions.rs:205) remain as safety nets

## Implementation

`crates/almide-codegen/src/pass_stack_balance.rs`

Pipeline registration: `crates/almide-codegen/src/target.rs` (after `AnfPass`, before `PerceusPass`)

## Previous attempts

### Emit-level drop (partial — still in place as safety net)

`functions.rs:166`: drops if void function body produces value.
`expressions.rs:205`: drops if block type is Unit but tail type is non-Unit.

These catch some cases but fail when:
- Perceus updates `.ty` to match the tail (condition never triggers)
- Nested blocks have mismatched annotations

## References

- Grain `MDrop` IR node: uniform representation avoids the problem entirely
- Perceus `block_to_fnbody`: `crates/almide-codegen/src/pass_perceus.rs:49`
- ANF `wrap_with_lets`: `crates/almide-codegen/src/pass_anf.rs:58`
