# ADR-0016: Numeric determinism is one normative family, written once — not a property the targets happen to share

- **Status**: Accepted — adjudicated 2026-08-21 ([#28](https://github.com/almide/als/issues/28)):
  all five ⚖ decisions ratified as written. The ⚖ marks are kept so the
  reader can see which sentences were decisions and which were
  consolidation.
- **Date**: 2026-08-21 (proposed and accepted the same day)
- **Context**: Floating-point is where "byte-identical on every target" is
  easiest to lose and hardest to see: a NaN's sign bit differs between x86
  and aarch64, engines may or may not propagate payloads, a host libm rounds
  `sin` differently from a vendored one, a compiler may contract `a*b+c`
  into one FMA rounding on one target and not the other, and a flush-to-zero
  mode can be inherited from a process. The ALS already pins several of
  these, section by section — `float.parse` is correctly rounded
  (ALS-T2, C-024), `float.to_string` is the shortest round-tripping decimal
  (ALS-T13), `float.to_fixed` is half-even on the exact binary value
  (ALS-T9), NaN is canonical at every OBSERVATION boundary (C-210 under
  ALS-T2: `float.to_bits` answers `0x7FF8000000000000`), sign and `min/max`
  NaN rules (ALS-T15), `totalOrder` for sorting with `-0 < +0` (ALS-C9),
  `NaN != NaN` for equality (ALS-M10, collections.md), non-finite constants
  display as `inf`/`-inf`/`NaN` (ALS-R4), `-0.0` displays with its sign
  (ALS-E3), and the implementation shares ONE vendored libm across targets
  ("host libm dependence is non-conforming", text-and-numbers.md). What does
  NOT exist is the family as a whole: a reader cannot find the numeric
  determinism doctrine in one place, three rules have never been written
  down, and the aviation seat (ADR / greenfield ARCHITECTURE §6.6(b):
  "numeric determinism pinned in the spec — canonical NaN at every boundary,
  one vendored libm/softfloat policy across interp and backend, ±0 and
  fmin/fmax tie rules written once") names this as a unit-6 obligation the
  implementation must not have to invent. DO-178C programs treat floating
  point as a requirements item, not a platform detail (F7 in the
  implementation's flight-evidence-gaps ledger was exactly such a case:
  `float.parse` 1-ulp boundary drift, closed by an exact algorithm). This is
  also the first normative batch that can be written REQUIREMENTS-FIRST
  under the two-repo order — its contracts will be `requirements-first` in
  `proofs/contract-provenance.toml` by construction.
- **Decision**: Numeric determinism is ONE normative family in the ALS —
  a named group of sections under the `T` prefix, each with its contract and
  fixtures, landing here before any implementation changes — consisting of
  the rules already normative (listed above, re-cited from the family's
  head section, not moved) plus the following, which are written for the
  first time:
  1. ⚖ **Rounding mode and contraction.** Every Float operation rounds to
     nearest, ties to even; no other rounding mode is observable. No
     contraction: `a * b + c` is two roundings on every target (wasm has no
     fused multiply-add; a native backend must not emit one unless the
     source names `math.fma`, which — if it exists — is its own section).
  2. ⚖ **Subnormals are preserved.** No flush-to-zero, no
     denormals-are-zero, on any target; `5e-324` parses, prints and
     computes as the smallest subnormal (ALS-T2 already pins the parse).
  3. ⚖ **One libm, stated accuracy.** Every transcendental and
     power-family function (`math.sin`, `cos`, `tan`, `exp`, `log`, `pow`,
     `sqrt`, `tanh`, `atan`, …) is computed by ONE vendored implementation
     shared by every target — never the host's. `sqrt` is correctly rounded
     (IEEE mandates it); the others carry a stated accuracy bound (≤ 1 ulp,
     measured and pinned per function by the fixture family, NOT "correctly
     rounded", which is not claimed). Byte-identity across targets is the
     law; closeness to the true value is the stated bound.
  4. ⚖ **Signed zero.** Arithmetic preserves the sign of zero per
     IEEE 754; `0.0 == -0.0` is true; `-0.0` displays as `-0.0` (ALS-E3);
     `float.min(-0.0, 0.0)` is `-0.0` and `float.max` is `0.0` — the
     IEEE 754-2019 `minimum`/`maximum` zero ordering, consistent with
     `totalOrder` (ALS-C9) and with ALS-T15's NaN-ignoring rule (one of
     min/max's two arguments NaN → the other; both NaN → NaN).
  5. ⚖ **Float → Int conversion names its rounding and its range rule.**
     Each conversion function states truncation/rounding and whether an
     out-of-range or NaN input saturates, errors, or aborts — in the
     indicative, with a fixture at every edge (`±inf`, NaN, `±2^63`, `-0.0`).
  6. **Canonical NaN at every observation boundary** — C-210 as written,
     re-cited by the family head so the list of boundaries (`to_bits`,
     `to_string`, JSON/Value encoding, hashing where Float is a key) is
     enumerated in one place and each boundary has a fixture.
  The family head section states the doctrine in one sentence — *a Float
  computation's observable result is a function of the program alone* —
  and names every member section.
- **Rationale**: (1) The rules exist in the implementation as behaviour and
  in scattered sections as text; an aviation reviewer asks for the family,
  not the members. (2) Writing them here first is the only way the
  provenance ledger will ever show a `requirements-first` batch of any
  size; C-300/C-301 are the whole population today. (3) Each ⚖ rule is the
  kind that silently diverges between a wasm engine and a native backend —
  contraction and FTZ in particular are compiler/process flags, not
  language semantics, and a spec that does not forbid them has no grounds
  to call a divergence a bug. (4) "One libm, stated accuracy" is honest
  where "correctly rounded transcendentals" would be a claim nobody can
  back: CR-libm-class implementations exist but the implementation ships a
  transcribed vendored libm; the fixture family pins what IS.
- **Alternatives**: (a) leave numerics as per-function sections and let
  agreement decide — rejected: agreement cannot see a divergence that both
  targets share (QUALIFICATION.md limitation 2), and it cannot see a rule
  that was never written. (b) require correctly-rounded transcendentals —
  rejected for now as unbacked; the falsifier below reopens it. (c) make
  the cert profile fixed-point only and leave Float unpinned — rejected:
  Float is in the language for everyone, not only the bounded profile;
  ADR-0017 takes Float's admissibility in that profile as a separate question
  that DEPENDS on this family existing.
- **Consequences**: A new `## ALS-T<n>` family head plus ~5 member sections
  in `text-and-numbers.md`, ~6 contracts, and fixtures in `spec/wasm_cross`
  for every edge named above — all landing before the implementation pins
  the commit. The implementation then either already conforms (most rules)
  or gains a gate (no-contraction, no-FTZ are build-flag audits; the libm
  accuracy bounds are a measured table). The cost is the fixture work and
  one honest measurement per transcendental.
- **Falsifier**: A transcendental whose measured error exceeds the pinned
  bound on any target reopens rule 3 (tighten the bound or the libm). A
  documented need for a second rounding mode or for `fma` semantics reopens
  rule 1 (it becomes a named function, never an ambient mode). A
  `requirements-first` count in the provenance ledger that does not move
  after this family lands would mean the two-repo order is not being
  followed, and that is a process finding, not a numeric one.
- **References**: IEEE 754-2019 §5.3.1 (minimum/maximum), §4.3 (rounding),
  §7.2; WebAssembly core spec, numerics (`fmin`/`fmax` NaN and zero rules;
  no FMA in the MVP numeric instruction set); ALS-T2/T9/T13/T15, ALS-C9,
  ALS-E3, ALS-M10, ALS-R4; C-024, C-210; greenfield `ARCHITECTURE.md`
  §6.6(b); almide/almide `proofs/libm-determinism-audit.toml`;
  `docs/roadmap/active/flight-evidence-gaps.md` F7.
