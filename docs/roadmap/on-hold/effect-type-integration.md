<!-- description: Embed EffectSet into FnType for type-level effect tracking -->
# Effect Type Integration — Embed EffectSet in FnType

**Priority:** 2.x
**Prerequisites:** Effect System Phase 1-2 complete, HKT Foundation Phase 1-4 complete
**Syntax constraint:** Never leaks into user syntax. `effect fn` is the only marker.

## Overview

Currently, Effect inference results are stored in `IrProgram.effect_map` (HashMap).
Promote this to the type level, embedding EffectSet as part of `FnType`.

## Current State

```rust
// External to IrProgram
pub effect_map: EffectMap,  // HashMap<String, FunctionEffects>
```

## Goal

```rust
// As part of FnType
struct FnType {
    params: Vec<Ty>,
    ret: Ty,
    effects: EffectSet,  // {IO, Net, ...} — automatically inferred by compiler
}
```

## Benefits

- Type checker can use effects as part of types for constraint checking
- Type-level guarantee that "this function only uses {Net}"
- Enables effect-polymorphic abstractions when integrated with the trait system (internally)
- Accuracy of almide check --effects is linked to type inference

## Impact on Syntax

**None.** Users continue writing only `fn` / `effect fn`.
Effect granularity (IO/Net/Env, etc.) is internal type system information.
Syntax like `effect fn[IO]` is **not introduced**.
