# ADR-0017: The bounded profile (née "flight profile") is a normative subset of the language, specified here before any checker enforces it

- **Status**: Accepted — adjudicated 2026-08-21 ([#29](https://github.com/almide/als/issues/29))
  with ONE amendment: **the profile is named by the guarantee it gives, not
  by an industry.** It is the *bounded profile* — statically bounded time
  and memory, a closed call graph, capability-bounded effects — its marker
  is `@bounded`, its chapter is `bounded.md` with prefix `B` (`ALS-B<n>`).
  `@flight` does not appear in the language; "flight" remains the
  motivating context in QUALIFICATION.md and the implementation's roadmap.
  ⚖ 3 and ⚖ 4 ratified as written; ⚖ 5 ratified with the rename. The file
  slug keeps its original words (ids and paths are permanent, ADR-0015
  precedent); the text below is the amended decision, with the original
  name quoted where the implementation's documents still use it.
- **Date**: 2026-08-21 (proposed and accepted the same day)
- **Context**: Aviation-grade code is written in a SUBSET of a language —
  MISRA C, Ada Ravenscar, SPARK — and the subset is the thing a certification
  program actually qualifies against. The implementation has designed
  Almide's (flight-subset-spec.md, G-F1 of the flight ladder): counted
  loops only, no allocation inside a loop, bounded static allocation, no
  recursion (call-graph acyclicity), no closures / higher-order / `any P`
  dispatch, capability-bounded effects, `break`/`continue` out, Float
  provisionally out, enforced per function by an attribute (spelled
  `@flight` in that design; `@bounded` here — see Status) whose membership
  the per-build certificate PROVES rather than a reviewer checking it. Today that design lives only on the implementation side,
  with no `ALS-<id>` section, no contract, no fixture — which means it is
  not yet a requirement of the language and cannot be cited by one.
  CONTRIBUTING.md's order says a behaviour lands here first; this profile
  is the largest such behaviour on the roadmap, and the one whose
  provenance will be read most carefully (QUALIFICATION.md limitation 7).
  One more fact shaped the amendment: the subset's rules are not about
  aircraft — counted loops, bounded allocation, no recursion, closed call
  graphs serve embedded, real-time, sandboxed and game code equally, and a
  general-purpose language whose source carries an industry's name in an
  attribute has let a customer leak into its grammar.
- **Decision**: The bounded profile is a **normative subset** specified in
  this repository — a new ALS chapter `bounded.md` owning the prefix `B`
  (registered in STANDARD.md) — and enforcement follows specification:
  1. **Subset, not dialect.** Every `@bounded` program is a valid Almide
     program with identical observable behaviour with or without the
     attribute (SPARK ⊂ Ada). A fixture family asserts exactly that: each
     in-profile program in `spec/wasm_cross` runs byte-identically with the
     attribute present and absent.
  2. **Membership is a checker verdict, and the verdict is a requirement.**
     Each OUT and RESTRICTED rule of the subset becomes an `ALS-B<n>`
     section stating the observable: a `@bounded` function that does X is
     REJECTED with a pinned diagnostic code, and `tests/diagnostics/<case>/`
     carries the `broken.almd` / `fixed.almd` pair for it (the rejection
     surface is part of the language — README, "tests/diagnostics").
  3. ⚖ **The IN/OUT table of flight-subset-spec.md §2 is adopted as the
     initial normative content**, including its resolved open questions:
     `Dup`-in-loop IN, `break`/`continue` OUT (exact bound, MISRA
     single-exit), recursion OUT by call-graph acyclicity with unknown
     callees rejected conservatively, nested counted loops IN with the
     product bound. Each row becomes one section; rows marked 未 (enforcement
     not yet built) are still specified now — that is the point of
     requirements-first — and the implementation's pin advance is what
     makes them green.
  4. ⚖ **Float is OUT of the bounded profile until ADR-0016's family is
     normative AND the cert seat has a float operation set** (the MIR has no
     `FloatOp` today). The section says so in the indicative ("a `@bounded`
     function that performs Float arithmetic is rejected with E…") and is
     revised — not silently relaxed — when both preconditions hold.
  5. ⚖ **The attribute is per function and named for the guarantee**:
     `@bounded` on `fn` / `effect fn`, with a module-level spelling as
     sugar; a `@bounded` function may call only `@bounded` functions and
     first-order members of pure stdlib modules (closed call graph — the
     precondition of the acyclicity and capability proofs). A compile flag
     is NOT the mechanism (it would drag imported unbounded code into the
     verdict). The word `flight` is not a token of the language.
  6. **The honest residuals of flight-subset-spec.md §6** (bounded
     recursion, fixed-point/deterministic Float, static-size arrays,
     byte-level memory bounds, `≤ B` early exit, readable layouts,
     functional-correctness traceability) are recorded in the chapter as
     a declared-limitations list, mirroring QUALIFICATION.md's form — what
     the subset does NOT yet express, so it cannot be mistaken for a claim.
- **Rationale**: (1) DO-178C credit is claimed for this subset only
  (flight-qualification.md §1: "DAL-A for the flight subset"); a subset that
  is not a requirement cannot carry credit. (2) The certificate-proves-
  membership design is the project's differentiator, but a proof needs a
  statement to prove — the statement is the `ALS-B` section. (3) Landing the
  sections before the walls is the only way the profile's provenance
  reads `requirements-first`; landing them after would make the project's
  most scrutinized requirements `retroactive`. (4) Diagnostics-corpus
  enforcement makes every OUT rule executable evidence on day one, with no
  checker yet — the judge can fail an implementation that accepts a
  recursive `@bounded` function before any implementation has the gate.
  (5) The name: a profile is a promise about the code, and the promise is
  boundedness; naming it after one buyer would be wrong for every other
  buyer and would look wrong in a general-purpose grammar.
- **Alternatives**: (a) keep the subset as an implementation design and
  specify it once the checker enforces it — rejected: reverses the two-PR
  order for exactly the requirements an assessor will read first. (b) a
  compile flag (`--flight` / `--bounded`) instead of an attribute —
  rejected in flight-subset-spec.md §4 (imported stdlib dragged in; the
  enforcement atom is the function). (c) a separate dialect with its own
  grammar — rejected: subset-not-dialect is the SPARK/MISRA lesson and is
  what keeps one judge for both. (d) keep the design's spelling `@flight` —
  rejected at adjudication: an industry is not a property; `@bounded`
  names what is proved.
- **Consequences**: A new chapter `bounded.md`, a new prefix row `B` in
  STANDARD.md, ~15–20 `ALS-B` sections, one contract each, a diagnostics
  case per OUT rule and
  a cross-target fixture family for the subset-not-dialect law. The
  implementation's G-F1 work becomes "make the judge green" rather than
  "design and enforce". The `unvalidated` ceiling in
  `proofs/als-validation.toml` rises by the number of new sections unless
  each lands with its review row — the PR must choose, visibly.
- **Falsifier**: A `@bounded` program whose behaviour changes with the
  attribute present vs absent refutes rule 1 and the chapter is wrong, not
  the program. A certification assessor (#15) preferring a whole-module or
  whole-program unit of enforcement overturns rule 5. A Float operation set
  landing in the cert seat without ADR-0016's family being normative is a
  process violation, not a reason to relax rule 4.
- **References**: almide/almide `docs/roadmap/active/flight-subset-spec.md`
  (§2 table, §3 resolved questions, §4 enforcement, §6 residuals);
  `flight-profile.md` §3.5, §7.2 (G-F1); `flight-qualification.md` §1;
  MISRA C:2012 Rules 17.2, 21.3, 15.x; Ada Ravenscar profile (ISO/IEC
  8652 D.13); SPARK subset rationale; ADR-0016; this repository's
  CONTRIBUTING.md (the order of change), QUALIFICATION.md limitation 7,
  issues #11 and #9.
