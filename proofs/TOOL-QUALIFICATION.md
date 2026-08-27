# Tool-qualification data package — the index (#574, DO-330 shape)

The package a qualification engineer takes to an audit. It is an INDEX,
not a new artifact: every section points at a standing, CI-gated document
or ledger — the point of building those first is that qualification is
assembly, not reorganization. Gaps are listed at the bottom by name; the
package asserts no certification status (that determination belongs to an
applicant and an authority).

## 1. Tool identification and environment

- Tool, version identity, claim scope: `proofs/TOR.md` §1 — a release IS
  its git tag; its evidence state is the seal (`proofs/releases/<tag>.toml`),
  re-measured against the tag by CI forever.
- Compiler-below-the-tool: the Ferrocene lane
  (`scripts/check-ferrocene-lane.sh`, weekly workflow) proves generated
  Rust builds under the qualified toolchain's upstream pin (Ferrocene
  26.05.0 == rustc 1.95.0); the responsibility split — Almide = the code
  generator, Ferrocene = the qualified compiler — is stated in the lane's
  headers (#573, closed on the full-corpus green run).
- Release integrity: every asset Sigstore-attested
  (`gh attestation verify <asset> -R almide/almide`), checksummed, and the
  dependency tree audited against RustSec weekly (`SECURITY.md`, #1534).

## 2. Tool Operational Requirements

`proofs/TOR.md` — each requirement names the instrument that ENFORCES it,
and `scripts/check-tor-refs.sh` fails CI when a row points at a deleted
instrument. Evidence is operator-re-derivable by construction (TOR-2: CI
is outside the trust base; `make verify-trust && make receipt` locally).

## 3. Tool verification data

- The gate ledger: `proofs/gate-verification.toml` — 66 verdict-bearing
  gates, EVERY one classified by how it can fail (KERNEL_PROVEN /
  MUTATION_TESTED / NEGATIVE_TESTED / EXERCISED; UNVERIFIED ceiling 0,
  shrink-only). A gate without demonstrated failure evidence cannot ship.
- Structural coverage: `proofs/coverage.sh` ratchet + per-file safety
  floors (`proofs/coverage-safety-baseline.txt`); MC/DC-grade evidence
  for the safety set: `proofs/mcdc-ledger.toml` — zero unresolved
  multi-condition decisions, independence-pair vectors mechanically
  verified by operator-swap mutants (`proofs/mcdc-mutation.sh`, nightly)
  (#566, closed).
- The proof spine: `make verify-trust` (Rocq kernel + coqchk + axiom
  audit + the PCC gate + corpus wall), receipt via `proofs/receipt.sh`
  with the tree+toolchain fingerprint honesty rail.

## 4. Qualification test cases, derived from the ALS

The ALS (almide/als) is the normative spec — 128 sections, every one
carrying validation rows; the almide-side derivation:

- The contract ledger: `docs/contracts/contracts.toml` — 320 cross-target
  contracts, each citing its ALS section (`spec = "ALS-…"`), every
  normative section cited by ≥1 contract, every fixture bidirectionally
  linked (`scripts/check-contracts.sh`).
- The executable corpora: `spec/` (the run/check/ast parity manifests —
  ratified hashes over 630+ fixtures), the wall corpus, the differential
  fuzz legs, and the 3-way oracle (native / wasm / reference interpreter,
  with the abstain ledger shrink-only).
- The independent reference evaluator (als `ref/`, Ferrocene-pinned by
  the same convention) — the N-version leg.

## 5. Formal credit

`proofs/FORMAL-CREDIT.md` (#575) — the seven Rocq proof families as
objective-shaped claims (FC-1..FC-7), each with assumptions and coverage
boundary; the model/implementation split per row; the Lean belts
explicitly excluded from credit.

## 6. Known limitations

- Per-release: the seal's `known_problems` field (mandatory).
- Standing: the trusted rows of `docs/contracts/proven-vs-trusted.md`
  (the IR→MIR row above all), the design walls (a wall is a refusal, not
  an error — TOR-3; the target-availability doctrine #1423), and the
  contract ledger's flagged-for-revision list (currently zero; the count
  may only shrink).

## 7. Configuration management

Git tags as identity; seals re-measured against tags forever
(`scripts/release-seal.sh check` in CI); the release-blocker gate (#1482)
refusing a final tag over open I-severity issues; Sigstore attestation on
assets and the dossier; the dependency-audit lane on every lockfile
change.

## 8. The assembled bundle

`scripts/gen-dossier.sh <tag>` (#571) emits the per-release bundle —
receipt + seal + formal credit + stage block + gate summary + contract
summary with KNOWN PROBLEMS + MC/DC state + a sha256 input manifest —
attested and attached to the release by the trust-spine tag run.

## Gaps, by name (the honest bottom line)

| Gap | Issue | State |
|---|---|---|
| WCET-analyzable codegen for the Critical profile | #569 | story + gated reference kernel (`docs/project/WCET-STORY.md`); target calibration external |
| Line-level source traceability in generated Rust | #572 | closed — `--trace-map` + review guide |
| Lean/Rocq proof coverage of the RUNTIME (allocator, free-list, RC ops as shipped) | #576 | model proven (FC-3); implementation gap open |
| Qualified/minimal wasm execution environment | #865 | not started |
| Critical-profile subset (`almide check --profile critical`) + static memory mode | #567, #568 | not started |
| Flight reference app through `make verify` | #776 | not started |
| First dossier-carrying release (fires #571/#1534 closure) | #571, #1534 | machinery live; next tag |

A qualification engineer reading this package starts at §2 (what the
operator must do), verifies §3's ledgers re-derive (`make verify-trust`),
and samples §4's corpora; the gaps table is the scope boundary of what
may be claimed today.
