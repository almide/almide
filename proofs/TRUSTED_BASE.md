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
| `eager_copy_refines_safety` | ALS.v | Closed under the global context |

## Known limitations (what is NOT yet proven — recorded, not hidden)

The receipt's claims are scoped to exactly this:

- **One property** so far: memory safety / RC balance. Stack balance, name
  totality, capability bound, type-concretize, termination are later bricks.
- **The current wasm renderer is eager-copy**: proven memory-SAFE
  (`eager_copy_refines_safety` — it emits no `__rc_dec`, so it cannot
  double-free) but it **leaks** (it emits no release). Leak-freedom is now PROVEN
  at the memory-model level — `RuntimeModel.balanced_cert_frees_in_memory`: an
  accepted certificate leaves the runtime cell FREED (rc 0). That property is
  realized by a release-emitting (perceus / real-RC) renderer, NOT by the current
  eager-copy one. So `C-SAFE`'s no-double-free is claimable for the eager-copy
  artifact today; leak-freedom is proven of the MODEL and awaits the real-RC
  renderer to realize it on the artifact.
- **Byte-binding is partial.** The op→wasm-instruction TABLE is a formal Coq
  object (`Translation.v`) and the runtime heap is modeled as a memory state
  machine whose rc cell provably tracks the abstract refcount
  (`RuntimeModel.mrun_tracks_exec`); `validate_translation` re-checks per build
  that each op's pattern is emitted and the bytes are Dec-free. NOT yet done: the
  WasmCert-Coq ISA layer binding the memory machine to the actual wasm bytes
  (that `call $rc_dec` executes precisely the cell write) — the last mile of the
  bytes-refine-ALS chain.
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
