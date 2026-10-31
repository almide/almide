# Almide Perceus: Type-Guided Automatic Memory Management

> Last updated: 2026-10-31
>
> No ownership annotations or explicit `free` in user code. Ownership inference
> and reference-counting machinery are implementation mechanisms, not a proof
> that every accepted program or emitted artifact is memory-safe.

## Overview

Almide uses Perceus-style ownership analysis to decide where heap values are
introduced, shared and consumed. Execution differs by target: native output uses
Rust ownership/runtime operations, while the structural Wasm emitter manages its
heap and reference counts. The current account of proved properties, trusted
components and open gaps is [Memory Safety](../design/MEMORY-SAFETY.md).

## Reference-counting model

These are conceptual rules, not a literal instruction sequence promised for each
source expression. Lowering, borrowing, static values and optimizations affect
which operations are needed.

1. **Share:** a new owned reference must be accounted for, typically by an
   increment when ownership cannot simply be transferred or borrowed.
2. **Release:** consuming or discarding an owned reference must be accounted for.
   A decrement can release the allocation when its count reaches zero.
3. **Compound drop:** releasing an owning container must also account for the
   heap values it owns. Layout and representation determine how children are
   visited; a closure environment is part of this obligation.

A static string, an owned heap buffer and a borrowed reference are not
interchangeable cases. A value being used once does not mean that its final
release can be omitted.

## Optimizations and limits

Ownership and uniqueness information permit reuse or in-place updates for
supported operations. Fusion can avoid intermediate containers. These are
conditional optimizations, not a guarantee that an arbitrary `map` / `filter` /
`fold` pipeline allocates nothing. Callback behavior, representation, aliasing
and the selected lowering all matter.

Single-use values and owned temporaries can avoid redundant sharing operations.
They still require correct consumption or cleanup. Neither single use nor a
resolved type alone proves that all RC operations can be removed.

## Verification boundary

The implementation has several kinds of evidence, with different scopes:

- [`perceus_verified.rs`](../../crates/almide-codegen/src/perceus_verified.rs)
  mirrors Lean definitions and names model-local insertion/counting properties.
  Those properties do not establish correctness of every lowering and runtime
  operation in a complete emitted program.
- The Rocq/Coq certificate and memory-safety models are described in
  [Memory Safety](../design/MEMORY-SAFETY.md) and
  [`proofs/Certified.v`](../../proofs/Certified.v). Their stated hypotheses and
  represented traces are part of the claims, not details to omit.
- [`almide verify`](../../src/cli/verify.rs) produces structural-Wasm witnesses
  and invokes the independent `almide-verify` binary. Artifact byte binding
  connects a bundle to a particular file; it does not turn the current checks
  into an unrestricted whole-artifact memory-safety theorem.

In particular, **type-checker acceptance does not currently imply a proved
no-leak/no-use-after-free/no-double-free theorem for every emitted artifact**.
The open lowering, trace and native-runtime obligations remain in the linked
memory-safety gap table. Do not infer them from the type-directed RC model.

## References

- Reinking et al., "Perceus: Garbage Free Reference Counting with Reuse" (ICFP 2021)
- Ullrich & de Moura, "Counting Immutable Beans" (IFL 2019)
- Tofte & Talpin, "Region-Based Memory Management" (POPL 1997)
