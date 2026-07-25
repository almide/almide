<!-- description: Anonymous record struct IDs collide across crates -->
<!-- done: 2026-05-25 -->
# Cross-crate anonymous record collision

> **Priority: high** — blocks mc-bot-cli (deep dependency chain) from compiling.

## Problem

When multiple crates in a dependency chain use anonymous records with the same field count but different field names, the codegen assigns them the same `AlmdRec` ID, causing field name mismatches.

## Reproduction

mc-bot-cli → mc-bot → mc-protocol → aes (4-level dependency chain)

aes module has `{ data: Bytes, state: Cfb8State }` (2 fields).
mc-protocol has `{ channel: String, payload: Bytes }` (2 fields).

Both get assigned `AlmdRec0<_, _>`. When aes code constructs `{ data: d, state: s }`, codegen emits `AlmdRec0 { data: ..., state: ... }` but the struct is defined as `AlmdRec0 { channel, payload }`.

```
error[E0560]: struct `AlmdRec0<_, _>` has no field named `data`
  = note: available fields are: `channel`, `payload`
```

## Root Cause

`AlmdRecN` naming is based on field count (or registration order), not field names. Cross-crate merging doesn't deduplicate or disambiguate records with the same arity but different field names.

## Fix

Anonymous record struct IDs must be keyed by **sorted field names**, not just arity. For example:

- `{ channel: String, payload: Bytes }` → `AlmdRec_channel_payload`
- `{ data: Bytes, state: Cfb8State }` → `AlmdRec_data_state`

Or use a hash of the field names to generate unique IDs.

## Exit criteria

```
cd almd-mc/mc-bot-cli && almide test
```

compiles and passes all 5 tests.
