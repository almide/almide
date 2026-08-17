# Mission-Critical Stage Status — measured, regenerated

The five-stage plan (Stage 1 accept-and-wrong の族滅 → Stage 2 translation
validation 全数化 → Stage 3 意味論凍結 → Stage 4 持続性計測 → Stage 5 DO-330
ギャップ分析) is the standing adoption roadmap. This page is its SINGLE
checkable status artifact: every number between the markers is measured from
the committed ledgers by `scripts/gen-claims.sh` and drift fails CI
(`check-contracts.sh` runs `--check`) — an auditor reads THIS, not prose
claims scattered across sessions. Stage semantics and evidence pointers:
`docs/roadmap/active/flight-evidence-gaps.md` (the audit findings ledger,
re-measured 2026-08-12) and `proofs/TOR.md` (the operational contract).

<!-- stages:generated:start — derived from the proofs/ ledgers by scripts/gen-claims.sh; DO NOT EDIT between the markers -->
> **Stage 1 (accept-and-wrong extinction): audits COMPLETE and gated** —
> scalar-read 61 arms / 0 UNGUARDED; WAT prelude 63 fns classified;
> platform-libm 5 sites classified. New entries cannot land unclassified.
>
> **Stage 2 (translation validation): 325/440 fixtures cast a real 3-way vote (73%)** —
> the abstain remainder is classified and shrink-only (the interp-heap arc, #1226).
>
> **Stage 3 (semantics freeze): 289/288 contracts spec-keyed; syntax-element coverage
> 72/72 sectioned (0 UNWRITTEN, shrink-only — the freeze precondition is 0).**
>
> **Stage 4 (durability): fuzz true-green streak = 0 day(s)** (dated meter;
> the correctness-only night verdict shipped 2026-08-12 — 90 days is the milestone).
>
> **Stage 5 (auditability): 1 release seal(s); 49 verification gates classified
> (2 UNVERIFIED under a shrink-only ceiling); TOR with 9 enforced rows;
> gap analysis consolidated in proofs/DO330-GAP.md (reference-gated).**
<!-- stages:generated:end -->

What the numbers do NOT claim: external adoption (a market fact, not a
ledger fact), MC/DC coverage (line coverage only, ratcheted), C-WCET /
C-FAITHFUL (keystone-gated, honestly PENDING in the receipt design), or ALS
authoring completeness (the section→contract direction is gated; the
syntax-element→section direction is Stage 3's remaining half).
