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

- **brick 1 (here): `OwnershipChecker.v`** — the RC-balance / memory-safety
  checker `K` over the MIR ownership op sequence, and its soundness theorem
  `check_sound : check ops = true → no_double_free ops ∧ no_leak ops`. The
  irreducible single-object core; `0 sorry`/`Qed`.

## Build

```
cd proofs
coq_makefile -f _CoqProject -o Makefile   # or: rocq makefile
make
```

(Rocq/Coq provides `coqc`/`rocq`. Toolchain version is pinned in the axiom
ledger — a later brick.)

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
