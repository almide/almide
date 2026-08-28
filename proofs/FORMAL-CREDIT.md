# Formal-credit claims table (#575, DO-333 shape)

Formal-methods supplements let a PROOF discharge a verification objective
that would otherwise be discharged by testing. To be creditable, each proof
artifact must be stated as an objective-shaped claim with its assumptions
and its coverage boundary — that statement, per proof family, is this
table. It asserts NO certification status: whether any claim earns credit
under a given standard is an applicant's and an authority's determination.
What this table does is make each claim precise enough to evaluate.

Ground truth for the boundary column:
[docs/contracts/proven-vs-trusted.md](../docs/contracts/README.md) (the
map) and [proofs/TRUSTED_BASE.md](./TRUSTED_BASE.md) (the territory —
toolchain pins, irreducible trust roots, tamper drill). The mechanized
proofs are Rocq (`proofs/*.v` — 153 Theorem/Lemma/Corollary statements
counted 2026-08-28; the release-audited figure is asserted by
`proofs/check.sh`, never hand-written). The Lean "belt" crates are DESIGN
MIRRORS, listed separately at the bottom — they are not proofs of the
implementation and are never claimed as such.

## The claims

| # | Objective-shaped claim | Proof artifacts | Assumptions (trusted base) | Coverage boundary |
|---|---|---|---|---|
| FC-1 | An ACCEPTED ownership certificate implies the certified function performs no double-free, no leak, and no dangling-name use | `OwnershipChecker.v` (17), `OwnershipFilter.v` (3), `OwnershipLoop.v` (4), `Subset.v` (5), `Termination.v` (2), `StackBalance.v` (1) | The Rocq kernel (coqchk re-check, cross-version 9.1.1/9.2); the extraction glue `driver.ml` (TRUSTED — negative-tested both directions, see gate-verification `proofs/build-checker.sh`); the certificate FAITHFULLY DESCRIBES the MIR (the producer is untrusted; the checker re-verifies every build via `proofs/gate.sh`) | Proves memory-safety of what the certificate describes. Proves NOTHING about semantic correctness of the lowering — the IR→MIR row is trusted, guarded by `mir_wellformed.rs` + differential legs, not by proof |
| FC-2 | Copy-on-write aliasing preserves value semantics: a snapshot taken before a write observes the pre-write value | `CowSafety.v` (3), `CoownCompose.v` (3), `CoownLoop.v` (5) | Same kernel/extraction roots; the runtime implements the modeled COW discipline (bridged by C-033's fixture family and the differential fuzz) | The MODEL's COW rule is proven; the implementation's conformance to the model is evidence-class fixture+differential, not proof |
| FC-3 | The modeled free-list allocator never hands out an aliased or freed block, and RC transitions preserve the heap invariant | `FreeList.v` (29), `FreeListRc.v` (27), `RuntimeModel.v` (5), `StructuralRuntime.v` (18), `StructuralAlloc.v` (3) | Kernel/extraction; `StructuralRuntime.v` proves the STRUCTURAL leg's emitted `$inc`/`$dec_flat`/`$free` instruction trees realize `rt_inc`/`rt_dec` and the size-class list push, with the class-math keystones (ceil-class coverage, file/take agreement — the under-serve corruption shape is unrepresentable); the tree↔bytes binding is the Rust-side hash pin (`runtime_alloc.rs::runtime_trees_match_the_proof_transcription`) | Tree-level realization for inc/dec/free on the structural leg; `StructuralAlloc.v` adds `$alloc`'s take/bump paths (the pop closes the reuse cycle against `dec_unique_files_take_class`; the grow body stays one abstract step under the no-grow hypothesis); the grow-body transcription and the byte-decode half are #576's remaining slices; the structural `$dec_flat` carries no zero-sentinel — the double-free defense is the upstream Perceus certificate, carried as the theorem's `1 <= rc` precondition |
| FC-4 | The emitted `rc_inc`/`rc_dec` wasm byte trees decode and execute to exactly the modeled RC transition | `WasmIsa.v` (13), `WasmExec.v` (9), `WasmDecode.v` (6), `WasmEncode.v` (1), `WasmRcDec.v` (2) | Kernel/extraction; `proofs/check-wasm-bytes.sh` binds the CLAIMED byte trees to the EMITTED bytes per build | Covers the RC instruction sequences, not the whole emitted module; the rest of emission is evidence-class Σ-probe/differential |
| FC-5 | A function's reachable capability set is bounded by its declared effects: no undeclared capability is reachable through the checked call graph | `CapabilityBound.v` (2), `CapabilityReach.v` (3), `CallModes.v` (6) | Kernel/extraction; the checker's E006/E007 seams enforce the modeled rule at the source boundary (those seams are in the MC/DC safety set, #566) | Proven over the modeled call graph; dynamic dispatch beyond the model (computed calls) is walled, not proven |
| FC-6 | Name resolution is total over the accepted subset, and type concretization never leaves an abstract type in emitted positions | `NameTotality.v` (2), `TypeConcretization.v` (1) | Kernel/extraction; `assert_names_resolvable` + the `ConcretizeTypes` gate run the corresponding implementation checks every build | Same model/implementation split: the theorem is about the model; the in-build gates carry the implementation side |
| FC-7 | The certificate language's translation preserves the ALS-specified observable semantics for the modeled constructs | `Translation.v` (2), `ALS.v` (2) | Kernel/extraction; the ALS ledger's contract rows carry the per-construct fixtures | Narrow by design — the modeled constructs only; the 320-contract ledger is the breadth instrument |

Per-build binding, common to every row: `proofs/gate.sh` (the extracted,
kernel-proven checker re-verifies each build's certificates — toolchain
stamp FATAL on mismatch), `proofs/corpus-wall.sh` (checker-phase ACCEPTs
are kernel-proven; a live REJECT caught the via_if miscompile — the fail
direction has fired on a real bug), and `translation_validation.rs` (the
per-build certificate-to-bytes chain, #570).

## The Lean belts (design mirrors, not credit claims)

`crates/almide-perceus-belt`, `almide-race-belt`, `almide-edit-belt` are
executable Lean mirrors of design disciplines (the RC insertion rules, the
race determinism argument, the edit-locality theorem). They pin the DESIGN
— the Rust implementations cite them — but no claim in the table above
rests on them, because nothing binds them to the shipped code mechanically.

## Dossier status

This table is the #575 deliverable in standing form. Its inclusion in a
per-release dossier is #571's assembly work; until then the release seal
(`proofs/releases/*.toml`) plus this file constitute the audit trail.
