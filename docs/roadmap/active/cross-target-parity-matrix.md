<!-- description: Automated verification that Rust, TS, and WASM produce identical output -->
# Cross-Target Parity Matrix

**Priority:** High — Optimal timing while WASM support is in progress
**Prerequisites:** Cross-Target CI completed, WASM 167/194 pass (0 failed, 27 skipped), TS test runner added
**Goal:** Automated verification infrastructure to systematically detect and prevent behavioral differences across Rust/TS/WASM targets

> "All tests passing in CI is a necessary condition. The parity matrix is the sufficient condition."

---

## Why

Cross-Target CI checks "do the same tests pass on both." The parity matrix checks "do they return the same output for the same input." There are differences the former cannot detect:

- Floating-point rounding differences
- String encoding differences (UTF-8 boundaries)
- Integer overflow behavior
- Error message wording differences
- Collection ordering guarantees (Map iteration order)
- Division by zero / NaN propagation

As the WASM target stabilizes, we need a mechanism to eliminate "invisible differences" across the 3 targets.

---

## Design

### Parity Test Structure

```almd
// spec/parity/numeric_parity_test.almd
test "integer overflow behaves identically" {
    let x = 2147483647  // i32 max
    assert_eq(x + 1, 2147483648)  // i64 so no wrap
}

test "float precision consistent" {
    assert_eq(0.1 + 0.2 == 0.30000000000000004, true)
}
```

Parity tests have the same format as regular tests but are placed in `spec/parity/` and compared across all targets for output.

### Verification Layers

| Layer | What is Verified | Method |
|---|---|---|
| L1: Output match | stdout matches across all targets | Extension of existing CI |
| L2: Edge cases | Type boundaries, precision, encoding | Dedicated parity tests |
| L3: stdlib coverage | All stdlib functions behave identically across targets | Per-module matrix |
| L4: Error behavior | panic/throw/trap conditions match | Error case tests |

### CI Integration

```yaml
# .github/workflows/ci-parity.yml
# Auto-runs on develop push
# Executes each test with --target rust, --target ts, --target wasm
# Diffs stdout, fails on differences
```

---

## Phases

### Phase 1: Parity Test Infrastructure + Numeric/String

- [ ] Create `spec/parity/` directory
- [ ] Parity test runner (`almide test --parity`: run on all targets + diff)
- [ ] Numeric parity tests (integer boundaries, floating-point precision, NaN/Inf)
- [ ] String parity tests (UTF-8 boundaries, empty strings, emoji, combining characters)
- [ ] Add CI workflow

### Phase 2: stdlib Matrix

- [ ] Auto-generate 22 modules × 3 targets parity matrix
- [ ] Add parity tests for each stdlib function (focus on edge cases)
- [ ] Matrix report output (`almide test --parity --report`)

### Phase 3: Error Behavior Parity

- [ ] Unify error behavior for division by zero, out-of-bounds access, type mismatch
- [ ] Define acceptable tolerance for error message differences across targets
- [ ] Tests for matching trap/panic/throw trigger conditions

### Phase 4: Regression Prevention

- [ ] Make parity tests mandatory when adding new stdlib functions (CI gate)
- [ ] Auto-classification of parity violations (intentional difference vs bug)
- [ ] Target quality dashboard (linked with Tier 1/2/3)

---

## Success Criteria

- Parity tests exist for all stdlib functions
- 100% parity between Rust/TS
- Differences with WASM are documented, with only intentional differences remaining
- CI fails if a new stdlib addition lacks parity tests
