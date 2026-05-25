<!-- description: Dependency IR still emits old AlmdRec0 after field-name-based ID fix -->
# Cross-crate anonymous record: IR rewrite needed

> **Priority: high** — mc-bot-cli still fails after the AlmdRec naming fix.
> **Depends on**: AlmdRec field-name-based naming (done in 8c34ab6b)

## Problem

The fix to use `AlmdRec_{field1}_{field2}` naming works for the root crate, but dependency crate IR still contains `AlmdRec0` references. When the root crate defines `AlmdRec_data_state`, the dependency's `AlmdRec0 { data: ..., state: ... }` can't find the struct.

```
error[E0422]: cannot find struct `AlmdRec0` in this scope
  --> <generated.rs>:3422:147
```

The `AlmdRec0` comes from the aes crate (transitive dep of mc-bot-cli via mc-bot → mc-protocol → aes).

## Root Cause

When dependency crate IR is linked (`ir_link`), the anonymous record references in the dependency's function bodies are not rewritten from `AlmdRec{i}` to `AlmdRec_{fields}`. The struct definition is correctly renamed, but the construction sites still use the old name.

## Fix

During `ir_link`, when merging dependency IR, rewrite all `AlmdRec{i}` references in expressions to match the field-name-based naming. This includes:
- Struct construction sites (`AlmdRec0 { data: ..., state: ... }` → `AlmdRec_data_state { data: ..., state: ... }`)
- Type annotations
- Pattern matches

## Exit criteria

```bash
cd almd-mc/mc-bot-cli && almide test
# All 5 tests pass
```
