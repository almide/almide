<!-- description: Fix cases where same .almd produces different results on Rust vs TS -->
<!-- done: 2026-03-18 -->
# Cross-Target Semantics

Fix cases where the same `.almd` produces different results on Rust vs TS. Guarantee Almide's "same code works on both" premise.

## P0: Map deep comparison broken in TS

**Problem:** `__deep_eq` uses `Object.keys()`, so `Map` objects appear empty.

```typescript
// emit_ts_runtime.rs — __deep_eq
const ka = Object.keys(a), kb = Object.keys(b);  // Map には効かない
```

`Object.keys()` returns `[]` for `Map`, so all Map comparisons become `true` (both appear empty).

**Fix:**
- [x] Add `Map` detection to `__deep_eq`: `if (a instanceof Map && b instanceof Map)`
- [x] Size comparison → per-entry recursive comparison
- [ ] Test: verify cases with Map in `assert_eq` on both Rust/TS

## P0: Map entries() order is non-deterministic in Rust

**Problem:** Rust's `HashMap` has non-deterministic iteration order. `map.entries()` is returned unsorted.

```rust
// collection_runtime.txt
fn almide_rt_map_entries<K, V>(map: &HashMap<K, V>) -> Vec<(K, V)> {
    map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()  // ソートなし
}
```

TS `Map` guarantees insertion order. The same program returns different order on each Rust execution.

**Fix:**
- [x] Add key sorting to `almide_rt_map_entries` (same pattern as `map.keys()`)
- [ ] Make `for (k, v) in map` iteration order sorted as well
- [ ] Test: verify `map.entries()` results match between Rust/TS

## P1: Integer overflow behavior difference

**Problem:** Rust uses i64 wrapping; TS uses BigInt (infinite precision) or Number (53-bit float).

```almide
let x = 9223372036854775807  // i64::MAX
let y = x + 1               // Rust: wraps to -9223372036854775808, TS: 9223372036854775808n
```

**Options:**
- (A) Document it ("overflow behavior is target-dependent")
- (B) Emulate i64 range wrapping in TS (`BigInt.asIntN(64, value)`)
- (C) Warn at compile time when literals exceed i64 range

**Fix:**
- [x] Decision: (B) Emulate i64 wrapping in TS
- [x] Insert `BigInt.asIntN(64, result)` after BigInt operations in TS codegen (`__bigop`, `__div`)
- [ ] Test: cross-target tests at overflow boundaries

## P1: Float stringification precision difference

**Problem:** Rust's `Display` trait and JS's `.toString()` produce different string representations for Float.

```almide
let f = 0.1 + 0.2
println("{f}")  // Rust: "0.30000000000000004", TS: "0.30000000000000004" (通常一致するが保証なし)
```

Differences appear at extremely large/small values.

**Fix:**
- [x] Policy: Recommend explicit format function `float.format(f, 6)`; document implicit stringification as "approximate" (see note below)
- [ ] Test: verify basic float value string interpolation matches cross-target

**Note:** Implicit Float stringification (`"{f}"` etc.) depends on Rust's `Display` and JS's `.toString()`, so differences may occur at extreme values (very large/small, subnormals). Use `float.format(f, precision)` when precise formatting is needed. Normal values (`0.1 + 0.2` etc.) match on both targets per IEEE 754 compliance.

## P2: Map assert_eq display is empty in TS

**Problem:** `assert_eq` uses `JSON.stringify`, but Map becomes `"{}"`.

**Fix:**
- [ ] Add custom stringify for Map (enumerate entries)
- [ ] Display actual values in `__deep_eq` error messages

## P2: Result error value lost inside TS test blocks

**Problem:** When re-wrapping `__Err` in try-catch inside test blocks, the original error structure is lost.

```typescript
catch (__e) { x = new __Err(__e.message); }  // original value is lost
```

**修正:**
- [ ] Preserve `__e.__almd_value` during re-wrap
- [ ] Test: verify nested Result match works correctly in TS tests

## P2: Map keys() sort broken for non-string keys in TS

**Problem:** `[...m.keys()].sort()` uses string comparison. Sort is undefined for object keys.

**Fix:**
- [ ] Add custom comparator (type-appropriate comparison)
- [ ] Or: guarantee Map keys are always primitive types at compile time (verified in checker, but also defend in codegen)

## Test strategy

Cross-target semantic guarantees require a CI pipeline that **runs the same tests on both Rust and TS and compares results**.

- [ ] Result comparison script for `almide test --target rust` vs `almide test --target ts`
- [ ] Test failure on any difference
- [ ] Minimum: run all spec/stdlib/ and spec/lang/ tests on both targets
