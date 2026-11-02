# Mission-Critical Stage Status — measured, regenerated

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](STAGE-STATUS.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](STAGE-STATUS.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](STAGE-STATUS.zh-CN.md)

> Non-normative historical reader snapshot of the [canonical source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/STAGE-STATUS.md), pinned to 35297a612fb29e3f78f6587aba253aa91d51f466. The original remains authoritative; this copy is not automatically synchronized and is not generator or proof input. No commands, proofs, measurements or qualification assessments were independently run. Source references, generated markers and dated figures are retained as historical material, not certification, qualification, authority acceptance or publication evidence.

The five-stage plan (Stage 1 extinction of accept-and-wrong → Stage 2 comprehensive translation
validation → Stage 3 semantics freeze → Stage 4 durability measurement → Stage 5 DO-330
gap analysis) is the standing adoption roadmap. This page is its SINGLE
checkable status artifact: every number between the markers is measured from
the committed ledgers into `proofs/ledger-counts.toml` — stamped with its date
by `scripts/gen-ledger-counts.sh` (a release step, or the answer to the nightly
`scripts/check-ledger-counts.sh` reporting drift), rendered by
`scripts/gen-claims.sh`, and a hand edit fails CI (`check-contracts.sh` runs
`--check`) — an auditor reads THIS, not prose claims scattered across sessions. Stage semantics and evidence pointers:
`docs/roadmap/active/flight-evidence-gaps.md` (the audit findings ledger,
re-measured 2026-08-12) and `proofs/TOR.md` (the operational contract).

<!-- stages:generated:start — derived from the proofs/ ledgers by scripts/gen-claims.sh; DO NOT EDIT between the markers -->
<!-- counts:generated:start (as of 2026-10-21) — stamped totals from proofs/ledger-counts.toml; refreshed only by scripts/gen-ledger-counts.sh, never by a fixture/contract PR; DO NOT EDIT between the markers -->
> **Stage 1 (accept-and-wrong extinction): audits COMPLETE and gated** —
> scalar-read 64 arms / 0 UNGUARDED;
> platform-libm 4 sites classified. New entries cannot land unclassified.
>
> **Stage 2 (translation validation): 940/957 fixtures cast a real 3-way vote (98%)** —
> the abstain remainder is classified and shrink-only (the interp-heap arc, #1226).
>
> **Stage 3 (semantics freeze): 388/388 contracts spec-keyed; syntax-element coverage
> 73/73 sectioned (0 UNWRITTEN, shrink-only — the freeze precondition is 0).**
>
> **Stage 4 (durability): fuzz true-green streak = 0 day(s)** (dated meter;
> the correctness-only night verdict shipped 2026-08-12 — 90 days is the milestone).
>
> **Stage 5 (auditability): 19 release seal(s); 126 verification gates classified
> (0 UNVERIFIED under a shrink-only ceiling); TOR with 10 enforced rows;
> gap analysis consolidated in proofs/DO330-GAP.md (reference-gated).**
<!-- counts:generated:end -->
<!-- stages:generated:end -->

What the numbers do NOT claim: external adoption (a market fact, not a
ledger fact), domain-level MC/DC qualification credit, or full artifact
refinement. Line/condition coverage and safety-set decision vectors are
separate evidence instruments. Consult the receipt for C-WCET / C-FAITHFUL
scope and remaining assumptions. The dated syntax-element mapping above
satisfies that mapping precondition, not semantic completeness or an
authority's acceptance of a frozen specification.
