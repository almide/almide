# Trusted base ledger (config management — tier-1 layer 7)

The flight-grade discipline: **naming exactly what is trusted is what lets the
rest be called "proven."** A reviewer reads this page to know the boundary. It
is the honest counterpart to the receipt's `C-PROVEN` claim.

## Toolchain pin

| component | version | role |
|---|---|---|
| Rocq / Coq | **9.1.1** | kernel + `coqchk` independent re-check |
| OCaml | **5.4.1** | extraction target (until CertiCoq) |

Reproduce every claim: `make verify-trust` (or `proofs/check.sh` + `proofs/gate.sh`).

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
  double-free) but it **leaks** (no `no_leak`). Leak-freedom needs the real-RC
  renderer. So `C-SAFE` is claimable for the eager-copy artifact today; not yet
  leak-freedom.
- **V is not yet on real wasm BYTES.** The certificate certifies the MIR
  ownership ops; the wasm is a faithful render of the same MIR (§3 renderer
  contract). A per-build translation validator that the emitted wasm bytes
  refine the ALS is the main body of brick 4.
- **The certificate is emitted for representative MIR shapes**, not yet from a
  real `.almd` end-to-end compile (blocked on the full lowering, #29).
- **Extraction is trusted** (item 2 above) until CertiCoq/CompCert.
- **Single independent checker.** Diversity (≥2 independent checkers) is brick 6.

## Use-relativized completeness

Completeness is declared per use, not absolute. Today the proven property set is
complete for **memory-safety-of-the-ownership-fragment under the eager-copy
realization** (no double-free). It is NOT a claim of absolute-semantics coverage
(that diverges — CompCert-grade). The receipt names which use each artifact is
proven for.
