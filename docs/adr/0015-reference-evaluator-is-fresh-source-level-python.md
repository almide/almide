# ADR-0015: The reference evaluator is a fresh, source-level, judge-owned evaluator behind a black-box protocol — seeded by λ_almd, ratcheted by an abstain ledger (amended: Rust)

- **Status**: Accepted (ratified 2026-08-21 as Python with the aviation-quality clauses; **amended the same day to Rust** — see Amendment; the file name keeps its original slug, ADR ids and paths are permanent)
- **Date**: 2026-08-20
- **Context**: QUALIFICATION.md limitation 2 — the cross legs judge
  AGREEMENT, not truth; both targets being wrong identically is invisible.
  [#10](https://github.com/almide/als/issues/10) names the instrument that
  closes it: a reference evaluator owned by this repository and a `ref` leg
  in the runner whose verdict is *legs == ref*. What exists today, measured:
  - The corpus the leg must judge: `spec/wasm_cross` 592 fixtures,
    `spec/programs`, `spec/wasm_cross_pkg`; they call **478 distinct stdlib
    functions** (`module.fn`, top modules int/list/string/bytes/value/float/
    regex/map/json/fs/set/fan).
  - The only mechanized semantics is the λ_almd kernel (C-280, the
    edit-locality belt, Lean 4.29, 1,583 lines, no Mathlib): **12 expression
    forms** (int/string literals, var, let, single-arg call, ok/err, match on
    Result, `!`, `??`, print, seq), a fuel-indexed evaluator proven sound
    against the relation, and a generated corpus of **48 programs whose
    traces are evaluator-pinned** (`proofs/kernel-conformance/` in the
    implementation repository) plus the surface fixture
    `spec/wasm_cross/kernel_conformance.almd`.
  - The implementation's third oracle, `crates/almide-interp`, is an IR
    interpreter (10.8k lines) sharing parser, checker and lowering with the
    compiler — a front-half defect moves all three legs identically.
  - The judge's toolchain is Python 3 + bash; `gates` has no compiler,
    no cargo, no Lean. `tree-sitter-almide` is a hand-maintained mirror of
    the compiler's parser (1,050-line grammar.js, Rust bindings only, drift
    gated weekly on the implementation side).
  - ADR-0014 already commits the shape of the output: pinned expectations
    are derived by an instrument; the Roc-form traces for `spec/wasm_cross`
    appear only as the reference evaluator's output.
- **Decision**:
  1. **Fresh, from the normative text.** The evaluator is written from the
     ALS chapters (`docs/specs/als/`, 110 sections), the language grammar
     (the EBNF the implementation publishes as `docs/GRAMMAR.md`, carried
     here as the judge's parser specification) and the stdlib
     specifications — never by porting `almide-interp`'s evaluation rules.
     A port would copy the implementation's semantics and keep the co-drift
     it is meant to catch; a fresh reading finds the spec's gaps, exactly as
     the doctest burn-down did.
  2. **Source-level, own front end, abstract values.** It parses `.almd`
     source with its own recursive-descent parser (no compiler crate, no
     tree-sitter build: a third grammar would add its own drift and a native
     build step to a repository whose gates are Python). Values are abstract
     (Int as unbounded-then-wrapped per ALS, String, List, Map, Set, Tuple,
     Record, Variant, Option, Result, Fn) — no byte heap: the judge models
     *what* a program observes, not *how* the implementation lays it out.
  3. **Rust (stable, pinned), a standalone crate, behind a black-box
     protocol.** The evaluator is the crate `ref/` in this repository:
     stable Rust only (`rust-toolchain.toml` pins the channel; no nightly
     feature, so a qualified toolchain — Ferrocene, [#18](https://github.com/almide/als/issues/18)
     — can rebuild it unchanged), no dependency on any `almide-*` crate
     (gated), dependencies kept to zero or a vetted few. The runner never
     links it: it calls a REF PROTOCOL — `<ref> run <file.almd> --json`
     emits `{"exit": n, "stdout": s, "stderr": s}` or
     `{"abstain": {"class": c, "reason": r}}` — so any evaluator in any
     language (the compiled Lean evaluator on its fragment, a future port)
     is pluggable, and the judge's seam is the protocol, not the
     implementation. (Amendment — the first ratification said Python; see
     below for why it changed before a line was written.)
  4. **Seeded by λ_almd, 100% from day one.** The 48 kernel programs and
     `kernel_conformance.almd` are the first oracle: the evaluator must
     reproduce their evaluator-pinned traces byte-for-byte before it judges
     anything else, and that agreement is a floor of 1.0 that CI holds
     forever (the "λ_almd-agreement fraction" of #10, with the fragment gate
     as its seed; it can only grow as the kernel grows).
  5. **Honest by abstaining.** Anything the evaluator does not implement is
     an ABSTAIN with a class (`stdlib:<module.fn>`, `syntax:<form>`,
     `runtime:<capability>`), recorded per fixture in
     `proofs/ref-abstain.toml`, shrink-only and classified — an abstain is
     never a pass and never a fail. Silent fallback is forbidden.
  6. **The leg's verdict is *legs == ref*.** For a fixture the evaluator
     evaluates, native and wasm must each equal the reference observables
     (exit, stdout, stderr, trimmed as in `cross`); agreement between the
     two targets is no longer sufficient. For an abstained fixture the
     existing agreement verdict stands and the abstain is counted. Each
     contract gains a reference-backed attribute (all its fixtures
     evaluated), printed by the README generator as a grow-only fraction.
- **Rationale**: The value of a reference is its independence, and
  independence here is achievable in exactly one dimension at a time:
  independent FRONT END (a second parser from the grammar), independent
  SEMANTICS (a second reading of the ALS text), independent STDLIB (a second
  implementation from the stdlib spec). Each of the rejected alternatives
  gives one of those up for convenience. Python is the cheapest host for a
  tree-walker over small programs and keeps the judge installable with
  `python3` alone; the protocol makes the language choice reversible at the
  cost of a port, not a redesign. Starting from the kernel corpus means the
  first 49 programs the evaluator must get right are the ones whose
  expected traces are *proven* rather than agreed — the only place in either
  repository where truth, not agreement, is available today.
- **Alternatives**:
  (a) *Extend the λ_almd belt in Lean until it covers the surface* —
  rejected as the executable leg: surface-wide mechanization is research-
  grade (#10); the belt stays the mechanized seed and the agreement ratchet
  relates the two. (b) *Port `almide-interp`'s rules behind a fresh parser* —
  rejected: copies the semantics under test. (c) *tree-sitter-almide as the
  front end* — rejected: a third hand-maintained grammar with its own drift,
  a native build in a Python-gated repository, and a parser that accepts
  what the compiler rejects. (d) *Python first* — the original ratification; superseded the same
  day by the Amendment below: at zero switching cost the option with the
  higher ceiling wins. (e) *Judge only a hand-picked subset* —
  rejected: the abstain ledger judges everything and says what it skipped.
- **Aviation-quality clauses** (ratified with the decision; each is a
  GATE, not a guideline — restated for Rust by the Amendment):
  1. *Determinism.* The evaluator depends on no host ordering: every
     iteration over a Map/Set follows the ALS-specified order, never the
     host's. `HashMap`/`HashSet` (randomized iteration per process) are
     forbidden types (`clippy.toml` `disallowed-types`); ordered
     structures only. The gate runs the corpus twice and requires
     byte-identical traces and verdicts.
  2. *Totality-or-abstain.* Syntactic totality is by construction — every
     AST node kind is a variant and every `match` over the AST is
     exhaustive (the compiler rejects a forgotten form); an unimplemented
     form is an explicit `Abstain` arm with a class, never a wildcard.
     Stdlib totality is measured: every `module.fn` the judged corpora call
     is implemented or in `proofs/ref-abstain.toml`; an unhandled call at
     run time is an evaluator FAILURE (protocol error), never a silent
     pass — the gate enumerates both tables and fails on any gap.
  3. *Mutation testing of the evaluator itself.* `cargo-mutants` over the
     crate, judged by the kernel + cross corpora; the kill rate is a
     shrink-only-in-the-wrong-direction ratchet with survivors listed (the
     edit-locality survey ranks mutation first among the 12 laws).
  4. *Pinned, qualifiable toolchain — Ferrocene-tracking.* `rust-toolchain.toml`
     pins the stable upstream version that the CURRENT Ferrocene release is
     built from (at ratification: Ferrocene 26.05.0 == Rust 1.95.0; the
     subscription itself is #18's resource item), so the switch from stock
     rustc to `criticalup`-installed Ferrocene is a CI-step change with no
     code or version change; no nightly feature; the toolchain version is
     recorded in the conformance statement beside the candidate binary and
     platform. The pin moves only together with a Ferrocene release.
  5. *No host delegation for specified semantics (the host-diversity
     clause).* The native target runs on Rust `std`; a reference that
     wrote `string.split` as `str::split` would share host semantics with
     the implementation and agreement would measure the shared host, not
     the ALS text. Therefore the std operations whose behaviour the ALS
     specifies itself (string splitting/trimming/case, float formatting
     and parsing, integer parsing, sorting/ordering, collection iteration
     order, hashing) are **forbidden methods** (`clippy.toml`
     `disallowed-methods`) — the evaluator writes them over char/byte
     primitives from the ALS text. The wasm target (self-hosted Almide)
     remains a genuinely different host, so the three-way vote keeps one
     unrelated leg.
  6. *Independence from the implementation.* `cargo tree` of the crate
     names no `almide-*` crate, no path/git dependency into the
     implementation repository (gated).
- **Amendment (2026-08-21, before any evaluator code existed)**: the
  first ratification chose Python on the grounds that DO-330 does not
  require the compiler of a TQL-4/5 verification tool to be qualified (it
  does not — the claim rests on the tool's own verification, clauses 1–4)
  and that a Python host adds N-version diversity. Re-judged against the
  claim ladder ([#9](https://github.com/almide/als/issues/9),
  [#18](https://github.com/almide/als/issues/18)): the credibility ceiling
  differs — a judge whose reference evaluator is a stable-Rust crate
  rebuildable under a qualified toolchain is a stronger statement to a
  future reviewer than a CPython script, Rust supplies syntactic totality
  by construction (the one clause Python could only measure), and the
  host-diversity loss is recoverable by a structural gate (clause 5), not
  by discipline. Python's advantages were authoring speed and a zero
  toolchain — real, but reversible only by a rewrite. With zero code
  written the switching cost was zero; the option with the higher ceiling
  was taken. What did NOT change: fresh from the normative text, own front
  end, abstract values, the black-box protocol, the λ_almd seed at 1.0,
  the abstain ledger, the verdict rule. `gates` gains a cached cargo step
  for the crate (its header no longer says "no compiler").
- **Falsifiers**: (1) the evaluator's wall-clock over the corpus exceeds the
  cross leg's own — the design is wrong, not the language; (2) the
  abstain ledger stops shrinking for two releases — then the stdlib-spec
  gap, not the evaluator, is the blocker and goes to #10 as a chapter debt;
  (3) a kernel-agreement regression — the evaluator is wrong by definition
  (the kernel is proven), never the other way.
- **Consequences**: `ref/` (a standalone crate: lexer, parser, evaluator,
  stdlib, and the protocol CLI `ref run <file> --json`); `scripts/check-ref-*.sh`
  gates (kernel agreement 1.0, independence, lints, totality, abstain
  ledger, mutation ratchet); `scripts/conformance.py --legs ref` with a `--ref`
  command; `selftest-conformance.py` scenarios for every ref verdict class
  (ref pass, native ≠ ref, wasm ≠ ref, both ≠ ref while agreeing — the
  co-drift class — abstain counted, malformed protocol output is red); a
  row in `proofs/gate-verification.toml`; `proofs/ref-abstain.toml`
  shrink-only; the kernel corpus carried here under `proofs/kernel-
  conformance/` with the generator commit recorded (regenerated never
  edited, ADR-0014). The Roc-form `.expected` traces for `spec/wasm_cross`
  become possible only after the abstain set is empty for a fixture — ADR-
  0014's condition, now with a named instrument.
