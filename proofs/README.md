# Almide v1 trust kernel (Coq/Rocq proof spine)

The flight-grade certification spine (docs/roadmap/active/v1-mir-architecture.md
§5, and the tier-1 stack). **We do not prove the compiler — we prove the
CHECKER.** The (large, untrusted) compiler emits an artifact + a certificate;
a tiny checker `K` verifies each build, and `K`'s soundness is proven here:

```
check(certificate, artifact) = accept  ⟹  P(artifact)
```

so trust shrinks from the ~100k-line compiler to a few-hundred-line checker
(the DO-330 qualification target). The seam between compiler and checker is the
certificate format, not a prover.

**Prover choice = Coq/Rocq (single TB), not Lean.** The daily proofs (RC
balance, stack balance) stay in the Lean Perceus belt (`crates/almide-perceus-belt/`),
but the flight-grade SPINE needs (a) a safety-certification precedent and (b) a
verified compiler IN THE SAME LOGIC to bring the checker to machine code — only
Coq/Rocq + CompCert/CertiCoq has both today (CompCert is the only verified
compiler with qualified use in safety-critical settings).

## Status

- **brick 1 (here): `OwnershipChecker.v` — KERNEL-VERIFIED.** The RC-balance /
  memory-safety checker `K` over the MIR ownership op sequence, and its soundness
  theorem `check_sound : check ops = true → no_double_free ops ∧ no_leak ops`.
  The irreducible single-object core. Verified three ways (Rocq 9.1.1):
  - kernel-checked (`coqc`), `Qed`, `0` admits;
  - **axiom-clean** — `Print Assumptions check_sound` = *Closed under the global
    context* (no extra axioms; the "Print Assumptions ⊆ standard" gate);
  - **independently re-checked** by `coqchk` (the De Bruijn criterion).

- **brick 4 start: `ALS.v` — ALS-as-normative + first honest translation-validation.**
  `exec` is named the NORMATIVE ownership semantics; `eager_copy_refines_safety`
  proves that an increment-only (eager) trace cannot double-free, and
  `dec_free_leaks` records that such a trace does NOT free (it emits no release).
  The wasm renderer has since moved to the RC regime (A1.1b: `Drop → call
  $rc_dec`), realizing cell-level leak-freedom (`balanced_cert_frees_in_memory`)
  with safety basis `balanced_cert_no_memory_fault`; the eager theorems remain the
  dual-oracle baseline. Kernel-checked, axiom-clean, coqchk-confirmed. (Full V =
  emitted wasm BYTES ⊒ ALS needs a wasm model in Coq — a later brick.)

- **brick 2: the proven checker RUNS on real bytes.** `Extract.v` extracts
  the kernel-proven `check` to OCaml; `driver.ml` + `build-checker.sh` link it
  into a `checker` binary that validates **certificate format v0** (a
  Metamath-simple token stream: `i`/`I` = an ownership +1, `d`/`D` = a −1,
  whitespace ignored). `./build-checker.sh` accepts a balanced certificate and
  rejects double-free / leak. This is the proof-carrying-code chain made
  operational — the proof stops being "about a model" and becomes "a running
  binary that carries its soundness proof and judges real output".

## Verify (the third-party `make verify`)

```
cd proofs
./check.sh           # coqc (+ axiom audit) then coqchk — the proof
./build-checker.sh   # extract → link → run the proven checker on certificates
```

## Known limitations (the honest base — config-management §7)

The trusted base is recorded so the rest may be called "proven":
- The **kernel** (Coq/Rocq) and **OCaml extraction** are trusted (CertiCoq +
  CompCert will close the extraction hole, brick 6).
- ~~The tokenizer in `driver.ml`~~ — **INTERNALIZED into Coq** (`parse` +
  `check_cert`, proven by `check_cert_sound`): the whole *bytes ⟶ accept/reject*
  pipeline is now kernel-checked. The trusted glue is reduced to file I/O only.
- The model is single-object RC; multi-object ✅ (`check_all`); the other
  properties (stack balance, name totality, …) are later bricks.

Rocq/Coq provides `coqc`/`coqchk`. Toolchain pin + axiom ledger is a later
brick (config management, §7 of the tier-1 stack).

## Roadmap (the tier-1 stack, in critical-path order)

1. ✅ first property (RC balance) checker + soundness — this file.
2. Certificate format v0 (a Metamath-simple witness language) — the seam.
3. Wire the compiler to emit the witness; CI gates "K accepts every build".
4. ALS core (Coq small-step semantics) + translation validator `V`
   (emitted wasm bytes ⊒ the verified model), per build.
5. One property at a time: stack balance / name totality / capability bound /
   type concretize / termination — each defined against ALS, K-soundness proven,
   witness emitted, CI-gated, added to the receipt.
6. CertiCoq + CompCert: extract `K`/`V` to machine code, closing the Thompson
   hole. Diversity: ≥2 independent checkers. Config management: toolchain pin +
   axiom ledger + known-limitations (the irreducible base: Coq kernel,
   CompCert/CertiCoq TB, hardware, and ALS validity).
