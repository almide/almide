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

### Evolution E1 — survey-driven adoptions (2026-08-19)

Post-port, diff-visible evolution of the landed unit, driven by the
9-compiler diagnostics survey (`../almide-references/RESEARCH-diagnostics.md`,
all citations SHA-pinned). Incumbent-parity is PRESERVED: the shared battery
still matches the incumbent golden byte-for-byte; every new capability is
`None`/unused on any diagnostic the incumbent could produce.

| steal | from | landed as |
|---|---|---|
| Multi-part atomic fixes | rustc `Substitution.parts` | `src/multifix.rs`: `with_machine_fix_parts` / `machine_multi_fix()` single read path / atomic `apply_multi_to` (reverse-order), overlap + guessed-span refusal; JSON `suggestions[].parts` entry (emitted only when present). 11 unit tests |
| Cross-language misspelling catalogue | Roc `common_misspellings.zig` | `src/misspellings.rs`: 74 curated entries (token/keyword/type/function), each grounded in CHEATSHEET/llms.txt normative text, `machine_fix` flag under #1312 discipline. Wiring obligations: tokenizer (unit 2), resolver (unit 4). 4 invariant tests incl. "valid Almide spellings never match" |
| Versioned code lifecycle | Lean 4 `ErrorExplanation.Metadata` | `src/codes.rs`: `CodeInfo { since_dialect, removed_dialect, … }` keyed to dialect epochs; 59 legacy rows frozen at `None` behind a shrink-only ratchet (`LEGACY_NONE_ROWS`) — every new code must state its lifecycle |
| Recoverability signal | MoonBit code band 3800–3999 | `CodeInfo.recoverable` field (explicit, not a numeric band); same legacy ratchet; parser-recovery consumer arrives at unit 2 |
| Battery exhaustiveness | Roc comptime-enumerated parity suite | `tests/golden_parity.rs::battery_witnesses_every_incumbent_field_and_variant`: full no-`..` destructure (new field ⇒ compile error ⇒ forced battery review) + runtime witnesses for every field/variant in populated AND absent form |
| Registry↔docs bidirectional sync | (incumbent contract-ledger doctrine) | `tests/codes_docs_sync.rs`: every code has a page, every page a row, titles byte-identical |
| Incremental diagnostic stability | Zig `test/incremental/` (unique in the field) | **Obligation recorded** in ARCHITECTURE.md §5 unit 4 gate — implementable only once the query core exists |

---

## Unit 2 — `almide-syntax` + `almide-base` facade (LANDED 2026-08-19)

- **Source:** `almide@a877d2138` (`git archive` / `git show`).
- **Ported verbatim:** `crates/almide-syntax` in full (8,054 lines: ast/lexer/
  parser/parse_cache — **zero edits, zero import adaptations**) and
  `crates/almide-base/src/{intern,profile}.rs`.
- **Boundary adaptations (recorded):**
  - `almide-base` is a **facade**: `intern`/`profile` verbatim + `span`/
    `diagnostic` re-exported from `almide-diag`, reproducing the incumbent's
    exact import surface so ported crates compile unchanged.
  - `almide-base` opts out of the workspace `unsafe_code = "forbid"` for the
    ONE sound foundation unsafe: `intern.rs:31` returns `&'static str` from a
    never-deallocating `ThreadedRodeo`. Scoped in that crate's Cargo.toml
    with rationale; every other crate stays forbidden.
  - `[dev-dependencies] sha2` appended to the ported syntax Cargo.toml (gate
    only). (An identical append accidentally hit the INCUMBENT's Cargo.toml
    mid-session — reverted immediately by deleting the appended lines;
    incumbent verified clean via `git status`.)
- **Sym JSON stability verified:** `Sym` serializes by resolved string
  (`intern.rs:94-98` manual serde impls), so AST JSON is process-independent.

### Gate verdict — AST parity vs clean-SHA oracle

- **Oracle:** `almide` built `--release` from a **clean detached worktree at
  a877d2138** (not the dirty working tree). `--emit-ast` never runs the
  checker (`src/cli/emit.rs:131`: `run_check` excludes `emit_ast`), and
  `parse_file` (src/compile_driver.rs:12-27) is exactly
  tokenize → `Parser::new(tokens).with_file(f)` → `parse()` — so the gate
  isolates precisely the ported surface.
- **Manifest:** `scripts/gen-ast-manifest.sh` hashed the oracle's
  `--emit-ast` output ("pretty JSON + one \n") for **1,095 of 1,098**
  `spec/**/*.almd`; 3 exclusions, each with its recorded oracle reason
  (unresolvable multi-package imports: `dmod_d` ×2, `extlib`). Determinism
  spot-check: first 25 files re-run, identical. No silent gaps: the gate
  asserts every corpus file is in exactly one of manifest/exclusions.
- **Verdict:** `spec_corpus_ast_matches_oracle_hashes` → **GREEN, 1,095/1,095
  byte-identical**, 2.9s.
- **Gate mutation test:** corrupted one manifest hash → red ✓; deleted one
  manifest line → coverage xor red ✓; restored → green ✓.
- One real gate bug found and fixed during bring-up: the first manifest
  loader keyed rows by hash, and two byte-identical spec files legitimately
  share an AST hash — rows are keyed by path now, with a duplicate-path
  assertion.
- **Deferred from E1 into later units (unchanged):** misspelling-catalogue
  and recoverable-code wiring need the diagnostic-emitting driver, which
  arrives with sema (unit 4) — the bare parser port stays verbatim.
- **CI verdict: GREEN** (run 32214450860). Two clippy rounds on the way:
  2 lints in the base facade's verbatim modules (scoped `#[allow]` on the
  module decls) and 9 style lints across the 8,054-line syntax crate
  (crate-scoped `[lints.clippy]` allows in its Cargo.toml — zero source
  edits). The oracle build worktree was removed after golden generation;
  regeneration is one `scripts/gen-ast-manifest.sh` run against a fresh
  a877d2138 build.

---

## Unit 4 STAGE 1 — checker stack + per-file check query (2026-08-19)

- **Source:** `almide@a877d2138` (`git archive` / `git show`).
- **Ported verbatim, ZERO source edits:** `crates/almide-types` (incl.
  build.rs stdlib embedding), `crates/almide-lang`, `crates/almide-ir`,
  `crates/almide-frontend` (25,327 lines), `stdlib/` (309 `.almd` sources as
  build inputs; data only — self-host runtime gating stays unit 9),
  `src/resolve.rs` + `src/project.rs`.
- **Boundary adaptations (recorded):**
  - New root facade crate `almide` reproducing the incumbent's src/lib.rs
    re-export map, trimmed to ported crates; `diagnostic_render` resolves to
    almide-diag's render (unit 1). resolve/project verbatim inside it.
  - Cargo deps `toml`/`semver` added to the facade (project.rs needs them).
  - `infer_module_capturing` copied verbatim into `almide-spine/src/s3.rs`
    with attribution (its home, compile_driver.rs, sits beside unported
    codegen); exit/stderr sites of the check driver become returned values.
  - Purity contract: `check_file_json` only accepts stdlib-only-import files
    (resolve must not read the FS inside a query); harnesses prefilter.
- **Gate (g) verdict: PASS — 1539x** (see docs/spikes/S3-real-checker-query.md):
  batch 6,807 ms vs warm 4.42 ms over 1,062 files / 53,943 lines, 100 real
  diagnostics, max 1 check/edit, 0 on no-edit.
- **Register shrink, first firing:** porting `stdlib/` satisfied 29 forward
  references; the stale-deviation check flagged all 29; register 96 → **67**,
  ceiling lowered. 0 unexplained.
- **Process incidents (recorded):** the incumbent repo was touched twice by
  cwd slips — its workspace `Cargo.toml` members line was overwritten and
  restored byte-identically (verified via `git status` clean), and one
  python edit aborted harmlessly on a missing path. Rule adopted: every
  greenfield shell command starts with an explicit `cd` into the worktree.
- **Still owed before unit 4 LANDS:** diagnostics parity vs oracle
  (`almide check --json` A/B manifest), per-decl granularity (S2a shape
  fused into the real checker), Zig-style incremental diagnostic scenarios,
  E1 wiring (misspellings + recoverable codes).
