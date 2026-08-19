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

---

## Unit 1 — `almide-diag` (LANDED 2026-08-19)

- **Source:** `almide@a877d2138`, extracted via `git show <sha>:<path>`.
  All three source files verified byte-identical between that SHA and the
  incumbent working tree before goldens were generated from it.
- **Ported verbatim:**
  - `crates/almide-diag/src/diagnostic.rs` ← `crates/almide-base/src/diagnostic.rs`
    (525 lines: Diagnostic, Applicability matrix #1312, fix engine
    `apply_try_to`, levenshtein/suggest, 11 unit tests)
  - `crates/almide-diag/src/span.rs` ← `crates/almide-base/src/span.rs`
  - `crates/almide-diag/src/render.rs` ← `src/diagnostic_render.rs`
    (display / display_with_source / manual JSON)
  - `docs/diagnostics/` — all 60 EXXX reference pages
- **Boundary adaptations (recorded, nothing else changed):**
  - `render.rs` line 7: `use almide_base::diagnostic::…` →
    `use crate::diagnostic::…` (verified as the ONLY diff vs the SHA)
  - New scaffolding: workspace `Cargo.toml` (edition 2024, resolver 3,
    **`rust-version = "1.89"` pinned** — the incumbent stated it in prose only,
    audit finding), `[workspace.lints] unsafe_code = "forbid"`, crate manifest,
    thin `lib.rs`, `.gitignore`, CI workflow `.github/workflows/greenfield.yml`.
- **Known quirk carried over on purpose:** `render::to_json` escapes
  backslashes in `suggestions[].replacement` but NOT in `message`/`try`
  (invalid JSON when a message contains `\`). Reproduced in the golden;
  fixing it is a later contract-visible change, never a silent porting edit.

### Gate verdict

- `cargo test --workspace` → **12/12 green** (11 ported unit tests +
  1 golden parity test).
- **Golden parity (the A/B gate):** one shared battery body
  (`tests/golden/battery_body.rs`) is compiled against BOTH the incumbent
  (almide-base + diagnostic_render.rs via `include!`) and the ported crate.
  The committed `tests/golden/diag-golden.txt` (384 lines) is the incumbent
  build's byte-exact output covering: all display forms, JSON incl. escaping
  quirks, the #1312 applicability matrix, guessed-span refusal, secondary-span
  gutter/ellipsis rendering, unicode caret columns, 8 `apply_try_to` edges,
  suggest/levenshtein, and all label suffixes. Greenfield replays the same
  body → byte-for-byte equal.
- **Parity-gate mutation test:** corrupted one golden byte → test fails ✓;
  regenerated from incumbent → green ✓.
- Contract port gate: unchanged (126 registered forward refs, 0 unexplained).
- clippy `-D warnings` + MSRV (`cargo +1.89 check`) run in CI (local sandbox
  has no rustup). First run flagged 6 style lints in the verbatim modules
  (collapsible_if ×3, empty_line_after_doc_comments ×2,
  manual_ignore_case_cmp); resolved with **scoped `#[allow]` on the two
  ported module declarations only** (`src/lib.rs`) — bodies untouched,
  scaffolding fully linted. **CI verdict: GREEN** (run 32211750565:
  port gate + tests + clippy + MSRV, 53s).
