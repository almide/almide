# PORTLOG — provenance and gate verdicts for every ported unit

Discipline (ARCHITECTURE.md §4): every port names its source commit, its exact
file set, its gate, and the gate's verdict. Deviations are enumerated, never
wildcarded, and may only shrink. This log is append-only.

---

## Unit 0 — spec corpus + contract ledger + checkers (LANDED 2026-08-19)

- **Source:** `almide@a877d2138` (develop), extracted via `git archive`
  (guarantees no working-tree contamination).
- **Ported verbatim:** `spec/` (1,098 `.almd` test files, incl. `spec/wasm_cross/`
  fixtures), `docs/contracts/` (contracts.toml + per-contract docs + generators
  + conformance.md + README.md), `docs/specs/als/` (9 normative sections),
  `docs/TRUST-SPINE.md`, `scripts/check-contracts.sh`, `scripts/gen-claims.sh`,
  `scripts/lib/contract-classes.txt`.
- **Boundary adaptations (new files, incumbent code untouched):**
  - `README.md` stub carrying the claims markers. The derived claims block is
    EMPTY until unit 5 (gen-claims counts `proofs/*.v`).
  - `scripts/check-contracts-port-gate.sh` — runs the incumbent gate verbatim
    and admits only registered forward references.
  - `scripts/lib/port-deviations.txt` — the deviation register: 96 enumerated
    missing evidence paths, each mapped to its resolving unit
    (unit 3: 1, unit 5: 10, unit 6: 25, unit 7: 30, unit 9: 30).
    Shrink-only; ceiling pinned at 96 in the port gate.

### Gate verdict

`bash scripts/check-contracts-port-gate.sh` → **GREEN**.

- Incumbent gate findings: 126, **all** registered forward references,
  **0 unexplained**. Every non-deviation check passed on the ported set:
  - ledger schema, id contiguity C-001..C-280, flagged-for-revision ratchet = 0
  - fixture ↔ contract links bidirectional and symmetric
  - generated `conformance.md` and `docs/contracts/README.md` fresh
  - ALS spec-keying: 97 referenced sections, all resolve; every normative
    section cited by ≥1 contract
  - evidence histogram: fixture 649, by-construction 16, fuzz 6, lean 2
- Known open deviation beyond the register: **D-GENCLAIMS** (gen-claims needs
  `proofs/*.v`; failure signature pinned in the port gate; resolves at unit 5).

### Gate mutation test (the gate can actually fail)

- de-registering one path → exit 1 (unexplained finding) ✓
- materializing one registered path → exit 1 (stale deviation) ✓
- restored → exit 0 ✓
