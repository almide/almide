<!-- description: Replace regex-based runtime scanner with syn crate for robust parsing -->
# build.rs Runtime Scanner Hardening

**Priority:** post-1.0
**Estimate:** ±200 lines, medium. Trade-off with build time.

## Current State

Parses function signatures from `runtime/rs/src/*.rs` using regex. Fragile but working.

## Ideal

Parse accurately using the `syn` crate.

## Tasks

- [ ] Introduce syn crate (build-dependencies)
- [ ] Switch function signature extraction to AST-based
- [ ] Measure build time impact

## Verdict

Can wait until it breaks. syn is heavy (build time +5-10s).
