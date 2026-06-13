# Trusted base ledger (config management — tier-1 layer 7)

The flight-grade discipline: **naming exactly what is trusted is what lets the
rest be called "proven."** A reviewer reads this page to know the boundary. It
is the honest counterpart to the receipt's `C-PROVEN` claim.

## Toolchain pin

| component | version | role |
|---|---|---|
| Rocq / Coq | **9.1.1** | kernel + `coqchk` independent re-check (canonical, local source build) |
| OCaml | **5.4.1** | extraction target (until CertiCoq) |

Reproduce every claim: `make verify-trust` (or `proofs/check.sh` + `proofs/gate.sh`).

**CI cross-version note (honest).** The `Trust Spine` GitHub Actions workflow
re-derives the whole spine on opam's latest Rocq 9.x — currently **9.2** (opam
has no 9.1.1; the canonical pin above is a local source build). The proofs are
kernel-checked and **axiom-clean on BOTH 9.1.1 and 9.2** — a cross-version
re-derivation, not a single-version artifact (a strength, not a gap). Rocq 9.2
ships only the `rocq` driver, so CI provides `coqc`/`coqchk` as thin shims over
`rocq compile` / `rocq check` (the latter IS the Rocq Proof Checker — the
independent De Bruijn re-check is genuine).

## The irreducible base (cannot be discharged by proof — "消えない底")

These four are trusted by necessity; everything else is proven against them:

1. **The Coq/Rocq kernel.** Decades of adversarial scrutiny; `coqchk` re-checks
   every `.vo` independently (the De Bruijn criterion). New logics have zero
   accumulated scrutiny — hence we borrow, not invent.
2. **OCaml extraction + the OCaml compiler.** The proven `check_cert` is
   extracted to OCaml and compiled by `ocamlopt`. This is the Thompson hole;
   **CertiCoq + CompCert close it** (extract to CompCert Clight → machine code,
   all in-logic) — brick 6, not yet done.
3. **Hardware.** The CPU executes the machine code faithfully.
4. **ALS validity** — that the formal semantics captures the INTENDED meaning.
   This is the one item checked empirically (interp + dojo + use), never proved.

## Axiom ledger (the "Print Assumptions ⊆ standard" gate)

Every theorem rests on **nothing but the kernel** — `Print Assumptions` reports
*Closed under the global context* for all of them (no `Admitted`, no extra
axioms). Verified by `proofs/check.sh`:

| theorem | file | assumptions |
|---|---|---|
| `check_sound` | OwnershipChecker.v | Closed under the global context |
| `check_all_sound` | OwnershipChecker.v | Closed under the global context |
| `check_cert_sound` | OwnershipChecker.v | Closed under the global context |
| `check_reuse_sound` | OwnershipChecker.v | Closed under the global context |
| `eager_copy_refines_safety` | ALS.v | Closed under the global context |
| `mrun_tracks_exec` | RuntimeModel.v | Closed under the global context |
| `alloc_not_live` | FreeList.v | Closed under the global context |
| `rc_dec_prog_realizes_rt_dec` | WasmRcDec.v | Closed under the global context |
| `make_unique_yields_unique` | CowSafety.v | Closed under the global context |

## Known limitations (what is NOT yet proven — recorded, not hidden)

The receipt's claims are scoped to exactly this:

- **The flight-grade property SET is complete on the value-semantics subset**:
  RC balance (memory safety), name totality, capability bound (incl. transitive),
  type-concretization, memory-model leak-freedom, reuse soundness (a `Reuse` acts
  only on a UNIQUELY-owned object — no aliased in-place reuse), free-list
  reuse-safety (a valid allocation never returns a currently-LIVE block — no
  reuse-after-free, `FreeList.alloc_not_live`), copy-on-write alias-safety
  (`MakeUnique` yields a uniquely-owned block — no aliased in-place mutation,
  `CowSafety.make_unique_yields_unique`), byte-binding table + the `$rc_dec` /
  `$rc_inc` instruction-trees realizing `rt_dec` / `rt_inc` (`WasmRcDec`),
  operand-stack balance, and termination of the loop-free fragment — all
  kernel-checked and axiom-clean (28 theorems). What remains is DEPTH (the byte-binding ISA layer; and
  the RENDERER realizing the free-list/`rc_inc` — its safety MODEL is now proven,
  so that slice REFINES a proof rather than adding trusted runtime) and BREADTH
  (lowering beyond the subset: control flow, closures, stdlib) — not new properties
  on the subset.
- **The wasm renderer is in the RC regime (A1.1b): it emits a release per drop.**
  A `Drop` now renders as `call $rc_dec`, decrementing the refcount cell (laid at
  heap offset 0 by the A1.1a relayout = `RuntimeModel.RC_OFFSET`) to 0 — so the
  binary actually FREES at the cell level. The safety basis moved accordingly:
  no longer `eager_copy_refines_safety` (the artifact is no longer Dec-free) but
  `RuntimeModel.balanced_cert_no_memory_fault` — an accepted (balanced)
  certificate has no double-free in the memory machine — together with
  `balanced_cert_frees_in_memory` — its cell ends FREED (rc 0). Both are already
  kernel-proven and axiom-clean (this slice is pure proof-REUSE: no `.v` changed).
  The per-build `validate_translation_perceus` V binds each witness drop to a
  `call $rc_dec` byte (one release per drop, no fewer), so the proof transfers to
  the REAL bytes; and the `$rc_dec` runtime SENTINEL traps a double-free at run
  (`unreachable` on an already-0 cell — verified firing on wasmtime). So `C-SAFE`'s
  no-double-free AND cell-level leak-freedom are now claimable for the EMITTED
  artifact, not just the model. PHYSICAL reclamation is now REALIZED (A1.2-render):
  `$rc_dec` at rc→0 returns the block to a free-list and `$alloc` reuses an
  exact-size head, REFINING the proven `FreeList.v` model (`alloc_not_live`: a valid
  allocation never returns a currently-LIVE block — no reuse-after-free). The
  double-free sentinel is PRESERVED — the free-list link lives in the dead LEN field,
  NOT the rc cell, so a re-release of a freed block still traps (verified: the
  double-free trap test and a reuse test, `p1==p2` on alloc/free/realloc, both pass
  on wasmtime; the value-semantics output is byte-unchanged). HONEST scope of what is
  SHARING is now REALIZED too (A1.3-render): `Dup` shares via `rc_inc` (no copy) and
  `MakeUnique` is a copy-on-write (clone-on-shared — `rc_dec`-FIRST so the alias keeps
  the original alive and no temp is needed, then `list_copy`), refining
  `CowSafety.make_unique_yields_unique`. The rc cell now ACTUALLY tracks the abstract
  refcount (1→2→1→0), exercising the proven rc machine (`WasmRcDec`/`RuntimeModel`),
  with byte-unchanged value-semantics output. So A1's renderer is now FULLY real-RC —
  share / cow / free-list / double-free sentinel — every piece REFINING a proof, zero
  trusted runtime added. NOT yet done (perf/encoding, not safety): the free-list is
  exact-size HEAD-match only (a mismatched-size freed block is not yet reused — missed
  reuse, NEVER unsafe; size-classes / walking is a later slice); and the raw-BYTE
  encoding of the instruction trees (A2's deferred heavy half).
- **Byte-binding is partial.** The op→wasm-instruction TABLE is a formal Coq
  object (`Translation.v`) and the runtime heap is modeled as a memory state
  machine whose rc cell provably tracks the abstract refcount
  (`RuntimeModel.mrun_tracks_exec`); `validate_translation` re-checks per build
  that each op's pattern is emitted (a drop's is `call $rc_dec`) and
  `validate_translation_perceus` that one release is emitted per drop. The model's
  `RC_OFFSET = 0` now COINCIDES with the renderer's physical rc-cell offset (the
  A1.1a relayout) and `call $rc_dec` writes that cell. **A2 first slice DONE
  (instruction-tree level), `WasmRcDec.rc_dec_prog_realizes_rt_dec`**: the EXACT
  `$rc_dec` instruction tree the renderer emits (modeled as data, with a small
  operational semantics for the load/add/sub/store/trap fragment) provably computes
  `RuntimeModel.rt_dec` — same trap (cell 0), same decrement. So the abstract
  release the leak/no-double-free proofs use is what the emitted INSTRUCTIONS
  compute, not a token. NOT yet done: the raw-BYTE encoding (instruction-tree ↔
  bytes — the assembler / a full WasmCert-Coq ISA); the SEMANTICS of the release
  is now proven at the instruction level, the remaining gap is purely the byte
  encoding of those instructions.
- **One real `.almd` now flows end-to-end** (`proofs/fixtures/return_list.almd`
  → the actual frontend → MIR → proven checker, for ownership + names — weekly
  indicator ① 0→1). The lowering covers only the value-semantics subset (heap
  literals, alias, index-assign copy-on-write, scalar/heap-move-out return — NO
  calls or control flow yet, #29), so the broader reject cases and the
  capability witness are still REPRESENTATIVE MIR shapes (emit_cert.rs).
- **Extraction is trusted** (item 2 above) until CertiCoq/CompCert.
- **Single independent checker.** Diversity (≥2 independent checkers) is brick 6.

## Use-relativized completeness

Completeness is declared per use, not absolute. Today the proven property set is
complete for **memory-safety-of-the-ownership-fragment under the eager-copy
realization** (no double-free). It is NOT a claim of absolute-semantics coverage
(that diverges — CompCert-grade). The receipt names which use each artifact is
proven for.
