# Proven vs trusted — the boundary map

The first question an auditor asks about a compiler that ships proofs is *which
part is proven*. This page answers it, and points at the evidence for each
answer.

The full ledger — toolchain pins, the irreducible trust roots, the tamper drill,
the per-bug-class regression pins — is [`proofs/TRUSTED_BASE.md`](../../proofs/TRUSTED_BASE.md).
This page is the map; that page is the territory.

## The one-sentence version

An **accepted certificate proves the function is memory-safe** — no double-free,
no leak, no dangling name, no undeclared capability. It proves **nothing about
whether the lowering picked the right semantics**: a certified-sound function
can still print the wrong string.

## Where the boundary runs

| Stage | Status | What backs it |
|---|---|---|
| Lexer, parser | trusted | differential fuzz, the spec corpus |
| Type checker kernel | **proven** | `proofs/OwnershipChecker.v`, `TypeConcretization.v`, `NameTotality.v` |
| Coown patterns | **proven** | `proofs/CoownCompose.v`, `CoownLoop.v`, `CowSafety.v` |
| AST → IR lowering | trusted | the checker's `TypeMap` is the source of truth; 974-file emit baselines |
| IR → MIR lowering | **trusted** | ← *this is the gap F3 (#777) is about* |
| MIR ownership witness | **proven to be re-checkable** | `proofs/gate.sh`: the untrusted producer emits a witness, the kernel-proven checker re-verifies it |
| MIR → wasm bytes | trusted | `proofs/check-wasm-bytes.sh`, `WasmEncode.v` for the `rc_inc`/`rc_dec` byte trees |
| wasmtime | unqualified tool | out of scope by construction |

## Why the IR → MIR row is the one that matters

Every one of the five output-breaking bugs found in the 2026-07-03 audit lived
in that row, and all five were *accepted by the certificate* — the MIR was
RC-balanced and name-resolvable, it simply computed something else.

They share a shape, quoted from the trusted-base ledger:

> a REGISTRY or CONVENTION (tracking sets, layout tables, calling convention,
> the scalar deferred-Const fallback) drifted between producer and consumer
> inside the trusted zone.

A certificate cannot catch that, because drift produces *valid* MIR. What
catches it is a post-pass over the lowering's own output, checking invariants
the lowering is supposed to maintain — the same shape as
`assert_names_resolvable` and the `ConcretizeTypes` gate. That gate exists now
(`crates/almide-mir/src/mir_wellformed.rs`, #777 item 2): every function the
lowering emits is checked for def-before-use over its op stream, the
defines/reads split is asserted to partition `op_values` on every real op (so
the three occurrence functions cannot drift), and a violation surfaces as a
named wall rather than rendering. It runs inside `lower_function_all`, so every
CI leg that lowers — Test WASM, Cross-Target, Trust Spine — exercises it on
every build.

The per-class regression pins for all five are in
[`proofs/TRUSTED_BASE.md`](../../proofs/TRUSTED_BASE.md#the-five-2026-07-03-trusted-zone-bug-classes-and-their-regression-pins).

## What each gate actually claims

| Gate | Claim | NOT a claim |
|---|---|---|
| `proofs/gate.sh` | the witnessed MIR is RC-safe, name-total, capability-bounded | that the wasm bytes match the witness |
| `proofs/corpus-wall.sh` | `lower_function` is total over the corpus: every function is `Ok` or an explicit `Unsupported` | that an `Ok` function has correct output |
| `proofs/output-parity.sh` | native and wasm agree, for the baseline set | anything outside that set |
| `scripts/check-contracts.sh` | every observable cross-target promise has executable evidence | that the promise is the right one |
| the cross-target fuzz | no divergence found in N programs | no divergence exists |

Reproduce all of it: `make verify-trust`.
