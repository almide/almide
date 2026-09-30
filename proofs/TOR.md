# Tool Operational Requirements (TOR) — the Almide verification chain

The DO-330-shaped operational contract: **what an operator must do — and must
not assume — for the chain's claims to apply to their use of it**. This is the
third of the goal-directed Stage-5 artifact trio (tool-verification ledger =
`proofs/gate-verification.toml`; known-problems formalization = the
`known_problems` field every release seal must carry). Companion boundary
documents: `proofs/TRUSTED_BASE.md` (what is trusted vs proven) and
`docs/contracts/proven-vs-trusted.md` (what each gate does and does not claim).
The DO-330-shaped package INDEX over all of these is
`proofs/TOOL-QUALIFICATION.md` (#574).

Each requirement names the instrument that ENFORCES it (`enforced-by:`) or is
marked `procedural` (relies on the operator; a future instrument is named where
one is planned). `scripts/check-tor-refs.sh` verifies every `enforced-by:`
path resolves — a TOR row pointing at a deleted instrument fails CI.

## 1. Tool identification

- Tool: the Almide compiler + its proof-carrying verification chain
  (`make verify-trust` = `proofs/check.sh` + `proofs/gate.sh` +
  `proofs/structural-wall.sh` + `proofs/corpus-wall.sh` (the native MIR
  certificate) + the MIR and almide-verify tests; `make receipt` =
  `proofs/receipt.sh`).
- Artifact identity (#2760): the receipt's C-SAFE row names
  `proofs/structural-artifacts.sha256` and that file's own digest — one row
  per certified fixture, the SHA-256 of the bytes `almide build --target
  wasm` ships for it. Each certificate bundle pins its row; `almide-verify`
  recomputes it from the file. A claim is about those bytes, not a build.
- Version identity: a release is the git TAG; its evidence state is the seal
  in `proofs/releases/<tag>.toml` (re-measured against the tag by CI forever).
- Claim scope: the receipt's claim table (C-PROVEN / C-SAFE / C-REPRO /
  C-WCET / C-FAITHFUL) — nothing outside it is claimed.

## 2. Operational requirements

**TOR-1 — Evidence binds to a stamped tree.** Every verdict is valid only for
the exact tree + toolchain fingerprint it was produced on. The gates refuse to
run when the PATH binary does not match the workspace build; re-run
`make install` and re-verify. Never transplant a verdict across trees.
enforced-by: proofs/lib/stamp.sh

**TOR-2 — Re-derive on your own machine.** CI is OUTSIDE the trust base. An
operator claiming the chain's assurances must run `make verify-trust &&
make receipt` locally; the receipt is the claim record. (CI is a convenience
mirror, not evidence.)
enforced-by: proofs/receipt.sh

**TOR-3 — A wall is a refusal, not an error to work around.** Code outside the
verified subset gets an explicit wall and NO claims: a program the structural
leg cannot lower is an E082 refusal, and a frame the witness does not cover
DECLINES with a counted reason, so its program is outside the certified set
and carries no C-SAFE claim. Rewriting code until the wall disappears moves it
INTO the claimed subset; suppressing, patching around, or ignoring a wall
voids every claim for that program.
enforced-by: proofs/structural-wall.sh

**TOR-4 — Claims mean exactly their receipt row.** C-SAFE is memory/name/
capability soundness of the STRUCTURAL build's wasm, for the certified
fixtures the row counts — it does NOT imply functional correctness (a PID
controller can be memory-safe and control the wrong thing), and a program
outside the certified set carries no C-SAFE claim at all. C-SAFE and C-PROVEN
are receipt claims, not rows of docs/contracts/contracts.toml. Requirements-level
verification (MC/DC, requirements tracing) is the OPERATOR's domain process,
outside this tool's scope.
enforced-by: proofs/receipt.sh

**TOR-5 — Review the release's known problems before relying on it.** Every
release seal carries a `known_problems` entry (checker-enforced, never blank).
Operation must review it and apply its dispositions; a problem disclosed there
is not covered by any claim.
enforced-by: scripts/release-seal.sh

**TOR-6 — Pin the toolchain the seal names.** The seal records the Rust and
wasmtime versions the release was verified with (the Coq pin lives in
TRUSTED_BASE). Substituting versions invalidates the evidence — re-verify
under TOR-1/TOR-2 if you must deviate.
enforced-by: scripts/release-seal.sh

**TOR-7 — Weigh gate verdicts by the gate-verification ledger.** Gates whose
fail direction has never been demonstrated are classified UNVERIFIED there
(shrink-only ceiling; burn-down #1244). An UNVERIFIED gate's green is weaker
evidence than a NEGATIVE_TESTED/KERNEL_PROVEN gate's green — the ledger says
which is which.
enforced-by: scripts/check-gate-verification.sh

**TOR-8 — The executable spec adjudicates disagreement.** Where the reference
interpreter votes (the 3-way corpus), interp behavior is the specification's
reading; a target that disagrees is wrong even if the two targets agree with
each other. Abstentions are classified and shrink-only — an abstaining area
has spec text and contracts, but not the executable third vote.
enforced-by: scripts/check-abstain-classes.sh

**TOR-9 — wasm execution environments are unqualified.** wasmtime is pinned
but has no qualification pedigree (no DO-330 path exists for it today). A
Critical-profile deployment must treat the wasm runtime as an unqualified
COTS component in its own safety case. procedural — tracked as #865.

**TOR-10 — Check the artifact against its bundle, not the build that made
it.** A certificate bundle (version 2) names the SHA-256 of the file its
witnesses describe. Before relying on a shipped `.wasm`, run
`almide-verify bundle <bundle>` next to it: a mismatch — one byte changed
after the build — is a REJECT, whatever the witnesses say. A bundle without
an `artifact` record (version 1) certifies witnesses, not a file.
enforced-by: crates/almide-verify/src/bundle.rs

## 3. Known limitations (operational view)

| limitation | operational consequence |
|---|---|
| MC/DC not measured (line coverage only, ratcheted) | structural-coverage credit above statement level cannot be claimed; plan it in the domain process |
| C-WCET pending (counted-loop keystone) | no allocation/time bound claims — do not derive timing budgets from the receipt |
| C-FAITHFUL partial on the wasm leg (the four runtime RC routines are proven bytes-to-semantics; every other emitted body, and the link from a program's instructions to its ownership witness, is not) | treat the structural build's behaviour beyond the RC runtime as differentially tested (native / wasm / interp byte identity), not proven |
| C-FAITHFUL pending on the native leg (production Rust renderer + Ferrocene) | the native leg's object-code trust runs through rustc, not a qualified toolchain |
| fuzz endurance streak is young | the durability metric (90-day milestone) is accruing; consult the current streak before citing it |

## 4. Traceability

TOR rows ↔ instruments are kept honest by `scripts/check-tor-refs.sh` (CI).
The evidence trail for any release: seal → tag → `make verify-trust` +
`make receipt` on that tag's tree → this TOR for how to operate it.
