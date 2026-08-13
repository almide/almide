# DO-330 Gap Analysis — the consolidated Stage-5 document

The single page the staged plan calls for: existing gates mapped to tool-
qualification objectives, the three formalized artifacts, and the enumerated
remaining gaps. This is an INDEX over instrument-backed sources — every claim
here is gated elsewhere; every `source:` line below must resolve
(`scripts/check-tor-refs.sh` scans this file too). Ferrocene (Rust's
ISO 26262/DO-178C qualification) is the prior-example model throughout.

## 1. The qualification argument (mapping)

The DO-178C Table-A objective mapping and the DO-330 TQL argument live in
the G-F5 design: the PCC asymmetry demotes the UNTRUSTED compiler to a
TQL-5/Criteria-3 tool (its output independently re-verified every build)
and leaves the small kernel-proven checker as the qualification object —
qualified by formal proof (DO-330 + DO-333), not by test.
source: docs/roadmap/active/flight-qualification.md

Claim strength at a glance (measured, drift-gated):
source: proofs/STAGE-STATUS.md

## 2. The three formalized artifacts (goal-directed trio — all instrument-backed)

| artifact | home | enforcement |
|---|---|---|
| Tool Operational Requirements | proofs/TOR.md (TOR-1..9) | check-tor-refs.sh: every requirement keeps its enforcing instrument |
| Verification-tool verification | proofs/gate-verification.toml | check-gate-verification.sh: every gate classified by how it can FAIL; UNVERIFIED shrink-only |
| Known-problems formalization | proofs/releases/ (per-release seals) | release-seal.sh: `known_problems` REQUIRED, derived fields re-measured against the tag forever |

source: proofs/TOR.md
source: proofs/gate-verification.toml
source: scripts/release-seal.sh

## 3. Remaining gaps (enumerated, owned)

| gap | state | owner / ref |
|---|---|---|
| MC/DC & branch coverage | line coverage only (ratcheted) | flight-evidence-gaps F2 remainder |
| C-WCET (counted-loop keystone あ) | PENDING in the receipt design | flight-reference-app Slice 2 |
| C-FAITHFUL + Ferrocene leg (keystone い) | PENDING; no qualified native toolchain in the chain | flight-reference-app Slice 3, needs Ferrocene access |
| wasmtime qualification pedigree | unqualified COTS, procedural TOR-9 | #865 |
| ALS syntax-element authoring | shrink-only ratchet; live count in the ledger's `unwritten_ceiling` and proofs/STAGE-STATUS.md | proofs/als-element-coverage.toml (freeze precondition) |
| Durability evidence | streak meter live, day 0 | research/benchmark/fuzz-green/ (90-day milestone) |
| Process independence | ratchet-separation gate exists; human/organizational review split is org-level work | flight-evidence-gaps F5 close note |
| PSAC / certification planning | no plan document yet | flight-qualification §G-F6 (dossier template) |
| External assessment itself | a market/authority fact, not a ledger fact | out of repo scope by definition |

source: docs/roadmap/active/flight-evidence-gaps.md
source: proofs/als-element-coverage.toml

## 4. What this page is not

Not a claim of qualification, adoption, or completeness — it is the honest
inventory an assessor starts from, regenerable numbers included by pointer
rather than by copy so it cannot silently drift.
