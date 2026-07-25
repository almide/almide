# v1: The Trust Spine — Proof-Carrying Compilation

> **Shipped.** v1 is the default and — since the v0 wasm emitter was retired (#782, 2026-07-20) — the *only* wasm compilation path. This was a ground-up redesign of the compiler's *trust model*, not a feature on top of v0.

The [Perceus proof](../crates/almide-perceus-belt/) proves one compiler pass, once. v1 generalizes that principle to the **whole pipeline** — but instead of proving every pass, it proves a tiny *checker* and makes the compiler re-verify itself on every build.

The retired v0 compiler took the shortest path: `AST → IR → codegen`. It was fast, and correct *as far as the tests could tell*. v1 asks a harder question: **not "do the tests pass?" but "can a machine prove the output is correct?"**

## The idea

You don't make a compiler trustworthy by making it perfect — a correct 100k-line compiler is a proof obligation no one can discharge. Instead:

> **Don't prove the compiler. Prove a tiny checker — and have the compiler emit a certificate on every build that the checker re-verifies.**

This rests on an asymmetry the whole field stands on: **building is hard, checking is cheap.** Solving a sudoku is work; verifying a solved one is a glance. So the compiler is *allowed* to have bugs — if it emits a wrong artifact, the attached certificate won't check out and the checker rejects it. The only thing that must be proven correct is the checker, and the only theorem is:

> *If the checker accepts, the artifact has the property* — and this theorem never mentions the compiler's internals.

That single move collapses the **trusted base from ~100,000 lines to a few hundred.** The big compiler becomes *untrusted* — free to be as large and buggy as it likes, because nothing trusts it.

## The pipeline (proof-carrying code)

```mermaid
flowchart TB
    ALS["ALS — normative semantics<br/>(Rocq; the single source of truth for meaning)"]

    subgraph U["UNTRUSTED — any size, bugs allowed"]
        P[".almd → check → lower → MIR → emit"]
    end

    A(["wasm bytes a + certificate bundle c"])

    subgraph T["TRUSTED — a few hundred lines, machine-proven sound in Rocq"]
        K["K property checker<br/>K(c, a) accepts ⟹ a satisfies property P"]
        V["V translation checker<br/>V(a, ALS) accepts ⟹ a refines ALS(s)"]
    end

    ALS -->|refine| P
    P --> A
    A --> K
    A --> V
    ALS -->|refine| V
```

- **K (property checker)** verifies the certificate: memory safety, name totality, capability upper bound, stack balance, termination behavior.
- **V (translation checker)** verifies — *on every build* — that the emitted wasm actually refines the language semantics. This is the answer to the reviewer's killer question: *"You proved a model — but does the thing that actually runs match it?"*
- **ALS** (Almide Language Specification) is the normative semantics, in Rocq (formerly Coq). The compiler and both backends don't define meaning; they *refine* ALS. So byte-for-byte agreement between targets isn't an afterthought — it falls out of the design.

The **trusted base is a single Rocq kernel** (plus CompCert/CertiCoq, the hardware, and the assumption that ALS says what we intend). Everything else is either proven against it or untrusted. There is no third category.

## Receipts — verify it yourself

Each build folds its certificates into claims, each with a published refutation procedure:

| Receipt | Claim |
|---|---|
| **C-SAFE** | Capability-bounded, no undefined behavior — checkable from the artifact alone |
| **C-REPRO** | Same source → byte-identical output on any host |
| **C-FAITHFUL** | Observable behavior refines the language semantics |
| **C-PROVEN** | Kernel-checked universal properties (RC balance, stack balance, …) |

Run `make verify-trust` and you re-derive every claim **on your own machine.** CI is a courtesy pre-run, deliberately *outside* the trusted base — you never have to trust our infrastructure to trust the artifact.

## Why it's slower — on purpose

v0 was fast because it stopped at "the tests pass." v1 is slower because every unit of work runs the full verification gauntlet: the property checker (the *corpus-wall*) re-verifies ownership / name / capability certificates for every function; the 3-way oracle (native / wasm / `almide-interp`) and the `spec/wasm_cross` fixture gate hold observable behavior byte-identical across targets; and where needed the Rocq kernel plus an independent `coqchk` re-check confirm the proofs introduce no stray axioms (`Print Assumptions ⊆ standard`). A single change can trigger minutes of checking.

That cost isn't inefficiency. It's the price of replacing **"it should be correct" (trust the tests) with "a machine has verified that it is" (trust the proof).** v0 was quick but hopeful; v1 ships only what the checker has accepted.

## Where it stands

Shipped end-to-end. v1 carried the full byte-gate corpus before v0 was deleted (57.5k lines of unverified emitter removed in #782), walls are honest hard errors, and the verified output now *outperforms* the rustc-compiled Rust backend on the benchmark suite while being several times smaller — see [BENCHMARKS.md](./BENCHMARKS.md) and [WASM-OUTPUT.md](./WASM-OUTPUT.md). Remaining hardening work is tracked in [`roadmap/active/v1-proof-architecture.md`](./roadmap/active/v1-proof-architecture.md) and [`v1-system-map.md`](./roadmap/active/v1-system-map.md).
