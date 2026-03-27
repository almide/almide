<!-- description: Separate AST-to-IR lowering from use-count analysis into two passes -->
<!-- done: 2026-03-18 -->
# Lower Two-Pass Separation

**Priority:** post-1.0
**Estimate:** ±500 lines, large

## Current State

`lower/` runs AST-to-IR conversion and use-count analysis simultaneously. Responsibilities are mixed.

## Ideal

- Pass 1: AST→IR (pure structural transformation)
- Pass 2: use-count / codegen analysis (UseCountPass)

## Tasks

- [ ] Limit lower to pure AST→IR transformation
- [ ] Separate use-count analysis into an independent Nanopass
- [ ] Remove codegen decision logic from lower

## Decision

Not broken. A maintainability improvement, but no reason to take the risk before 1.0.
