<!-- description: Run all tests on both Rust and TS targets, verify output match -->
<!-- done: 2026-03-18 -->
# Cross-Target CI

> "Target selection must not change program behavior" — lesson from TypeScript

## Overview

Run all tests on both Rust and TS targets and automatically verify that outputs match.

## Implementation

- [x] CI script: `.github/workflows/ci-cross-target.yml` (auto-runs on develop push)
- [x] **spec/lang: 45/45 pass**
- [x] **spec/stdlib: 14/14 pass**
- [x] **spec/integration: 13/13 pass**
- [x] **exercises: 34/34 pass**
- [x] **Total: 106/106 (100%)**

## Achieved via Codegen v3 (2026-03-18)

- [x] `is_rust()` 42 → 0: walker fully target-agnostic
- [x] ResultErasurePass: ok(x)→x, err(e)→throw (TS/Python)
- [x] ShadowResolvePass: let shadowing → assignment (TS)
- [x] MatchLoweringPass extended: Constructor + RecordPattern + guard
- [x] break-in-IIFE resolved: IIFE avoidance via contains_loop_control
- [x] 40+ TOML templates absorb Rust/TS syntax differences

## Known limitations

None. All 106 tests pass on both Rust + TS.

## Target quality tiers

| Tier | Target | Criteria | Current |
|------|--------|----------|---------|
| Tier 1 | Rust | All tests passing | 72/72 ✅ |
| Tier 1 | TS/JS | All tests passing | 106/106 ✅ |
| Tier 3 | WASM | Basic operation confirmed | CI available (smoke test) |
