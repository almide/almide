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
- **Diagnostics parity (2026-08-19): GREEN — 1,062/1,062 byte-identical.**
  `check_file_json` was made fully `cmd_check_json`-faithful first (two gaps
  found by reading the oracle before hashing it: the fatal-parse path prints
  the ACCUMULATED parser errors and discards the `Err` value; and after a
  clean check the oracle lowers to IR and appends unused-variable warnings —
  `lower_program` + `collect_unused_var_warnings`). Manifest:
  `scripts/gen-check-manifest.sh` hashed oracle `almide check <f> --json`
  stdout for 1,095/1,098 files (3 oracle exclusions, same unresolvable
  multi-package imports as the AST manifest); gate:
  `crates/almide-spine/tests/check_parity.rs` — every corpus file in exactly
  one of manifest/exclusions, 33 purity-skipped (non-stdlib imports, printed
  not silent), **1,062 compared, 0 divergent**, >900 floor asserted.
  Gate mutation test: one hash flipped → red ✓, restored → green ✓.
### Stage 2 (2026-08-19) — structure-new changes, parity-adjudicated

First stage-2 structural edits to ported code (all diff-visible, all judged
by the 1,062-file oracle manifest staying byte-identical):

- `#[derive(Clone)]` on `Checker` (check/mod.rs), `TypeEnv` (type_env.rs),
  `Constraint` (check/types.rs) — the whole cascade.
- New split functions in almide-frontend/canonicalize/mod.rs:
  `canonicalize_modules_env` (module half) + `canonicalize_entry_onto`
  (entry half); the verbatim `canonicalize_program` is UNTOUCHED and remains
  the v1/v2 path. Validity domain (stdlib-only entries) documented at the
  definitions. These live in a clippy-frozen crate; reviewed by hand.
- almide-spine s3: `check_file_json_v2` (drops the #862 stdlib loop —
  measured 63.3% of per-file cost, observably inert for stdlib-only entries)
  and `check_file_json_v3` (per-import-set env template: module-canonicalize
  + from_env + refresh computed once, `Checker` cloned per file).
- Numbers: warm keystroke check 4.42 → **0.62 ms**; batch 6,807 → **924 ms**;
  (g) 1499x; invalidation exactness unchanged. Full story:
  docs/spikes/S4-stage2-tax-removal.md.

- **Incremental diagnostic scenarios (2026-08-19): GREEN.** Adopted from
  Zig's `test/incremental/` (unique in the 9-compiler field). The property:
  for EVERY step of every edit script, incremental diagnostics equal the
  from-scratch answer — with memoization witnessed (≤1 check per edit, an
  untouched neighbor file at 0 re-checks throughout), so the equivalence is
  never vacuous. Six scenarios: error toggle (template-clone state-leak
  detector), span-only shift (line numbers proven to MOVE), decl add/remove,
  parse-error recovery, unused-warning toggle, import-set switch (template
  cache key change). One language fact learned and encoded: `main` must
  return Unit (E044), so scenario programs use plain fns.
  `crates/almide-spine/tests/incremental_diag_test.rs`, serialized in one
  test (the executions counter and template cache are process-global).
- **Still owed before unit 4 LANDS:** per-decl granularity for the largest
  files, E1 wiring (misspellings + recoverable codes — behavioral, will go
  through the intentional-change protocol).

---

## Unit 3 — the executable spec: `almide-interp` + pipeline (LANDED 2026-08-19)

- **Ported verbatim (zero source edits):** `crates/almide-optimize`,
  `crates/almide-driver` (the crate that PINS the canonical
  lower→optimize→mono→ir_link order — it exists because a real ordering
  divergence once shipped), `crates/almide-interp` (9.6k lines incl. its own
  104-test suite), and `runtime/rs/src` (37 files — the interpreter
  `include!`s the vendored libm from there).
- **The new engine EXECUTES.** `almide-spine/src/s5.rs::run_file` assembles
  the interpreter's canonical cut (per the crate's own eval_test) through the
  full resolve/canonicalize path: parse → check → `lower_program` →
  `almide_driver::link_ir` → `Interpreter::run_main`. The ported interp test
  suite passes 104/104.

### Gate verdict — run parity vs the oracle over the CONTRACT corpus

Oracle: clean-a877d2138 `almide run --target wasm` over all 591
`spec/wasm_cross` + `spec/wasm_fail` fixtures (the wasm leg is legitimate as
reference because wasm_cross fixtures are cross-target byte-identical by the
incumbent's own CI definition; also ~10x faster to harvest than per-fixture
rustc builds). `scripts/gen-run-manifest.sh`, parallel harvest, sha256(stdout)
+ exit code; 1 oracle exclusion (`guard_else_exit_code`, oracle exit 3).

`tests/run_parity.rs` → **GREEN: 451/451 comparable fixtures identical
(stdout hash + exit code), 0 divergent.** Two distinguished skip classes,
both the incumbent oracle's own doctrine, both ceilinged shrink-only:
- 138 `Unsupported` (bridge coverage — the reasons printed are the
  incumbent's own recorded debts verbatim: `prim.handle` slices #1226,
  `prim.alloc_*` families, `args.*`, `fd_write`);
- 1 `FuelExhausted` (`range_bind_huge`) — "NOT a hang or panic" by design.

One boundary adaptation in s5: `Unsupported`'s reason string is surfaced via
the returned stderr so harnesses can report skip classes precisely.

### Register shrink, second firing

interp + runtime paths now exist: 17 more stale deviations flagged and
removed; register 67 → **50**, ceiling lowered. 0 unexplained.

### Lint policy change (2026-08-19): two tiers, structural

The growing per-crate `[lints.clippy]` allow tables were heading toward
"checked in appearance only" — and a blanket allow list on a ported crate
would also blunt the linter for NEW code added there later. Replaced by a
structural policy, enforced in CI shape rather than allow lists:

- **Greenfield-authored tier** (`almide-spine`, `almide-diag`,
  `almide-base`): `clippy -D warnings`, zero tolerance. (The two
  verbatim-ported leaf modules inside diag/base keep their narrow
  module-scoped `#[allow]`s.)
- **Verbatim-ported tier** (`almide-syntax`, `almide-types`, `almide-ir`,
  `almide-frontend`, `almide`): the incumbent's lint state, frozen — built
  and tested, not clippy-gated. All allow tables removed. When a ported
  module is rebuilt as new structure (unit 4 stage 2+), it moves into the
  strict tier with that rebuild.

---

## Unit 6 — `almide-wasm` stage 0+1: the canonical backend's first light (2026-08-19)

The first fully NEW-CONSTRUCTION unit (§3 bans porting the incumbent's
WAT-text/TOML-template/dual-renderer emission).

### Stage 0 — layout DSL (§6.6 obligation, precondition)

- New crate `almide-layout`: THE single source for block layout
  (`[rc@0][len@4][cap@8][payload@12]`, NULL_ADDR) with a contiguity test, a
  generated doc table, and a **pinned digest** — any layout change must
  re-pin deliberately (an intentional-change event), ending comments-as-spec.
- The interpreter's arena now derives its constants from it (structure-new
  edit to the ported crate; **run-parity stayed 468/0 — byte-neutrality
  proven by the net**).

### Stage 1 — emission skeleton + first light + burn-up ratchet

- `almide-wasm`: typed IR → core wasm via wasm-encoder, structural only.
  String literals are laid out as REAL layout blocks from byte one. Every
  module passes the wasmparser wall before instantiation (tests/harness).
- **First light**: source → IR → emit → validate → wasmtime → output
  byte-identical to the interpreter (the definition), Unicode included.
- **Burn-up gate** (`tests/backend_parity.rs`): sweeps the full 590-fixture
  run manifest. Two lines held at once: any fixture the backend CLAIMS is
  executed and must match the oracle hash+exit (divergence = failure, never
  a skip); everything else lands in a precise reason histogram. Supported
  count is a **grow-only floor** (pinned 1). First sweep verdict:
  1 supported / 590; the treasure map for the next slices:
  `stmt:Bind ×368 → println-arg:Call ×123 → StringInterp ×24 → Match ×22`.
- The gate caught its first over-claim immediately: eager top-level lets
  were silently skipped (`top_let_div_eager` diverged) → refused explicitly
  until that slice lands.
- Known wrinkle recorded: the 9 unit-6 deviation-register rows cite
  incumbent implementation paths (`crates/almide-mir/...`) that greenfield
  will never create; retiring them at unit-6 completion requires an
  intentional-change edit to the contract STATEMENTS, not a port.

---

## Unit 6 — stage 2: the scalar-program slice (Bind → calls) (2026-08-19)

User directive: "Bind スライスいこう". The ×368 `stmt:Bind` wall — and the
walls it revealed behind itself — fell in one slice, entirely NEW
CONSTRUCTION (no incumbent renderer consulted):

- **Value model**: Int/Int64→i64, Bool→i32, String→i32 = block BASE address
  (payload/len always derived through `almide-layout`, never a bare
  payload pointer). let/var/assign → wasm locals (VarIds pre-resolve
  shadowing).
- **Semantics kept honest at birth** (not retrofitted): `and`/`or`
  SHORT-CIRCUIT via `if` blocks (a strict bitop would evaluate — and
  possibly trap — the dead operand); the emitted-wasm itoa works in the
  NEGATIVE domain so `i64::MIN` never overflows; value-`if` arm types are
  inferred before emission (wasm block types are up-front).
- **User functions**: scalar-signature fns become real wasm functions
  (params = leading locals, direct calls, recursion free). A body that
  doesn't lower yet gets an `unreachable` stub; emission REFUSES the
  program iff such a stub is reachable from `main` (call-graph BFS) — a
  stub can never fire.
- **Top-level lets**: lowered as `main`'s eager prelude — observably
  identical while `main` is the only entry and cross-function global reads
  are refused (`var:unmapped`).
- **Runtime helpers emitted as wasm** (never templates): block-print
  (println/eprintln imports), append_copy/append_i64/append_bool (line
  buffer for `${}` interpolation), itoa, **bump allocator** (`$alloc`:
  layout-true header rc=1/len/cap, memory.grow, OOM traps loud),
  int_to_string, concat. Memory map: null guard · itoa scratch · pool ·
  line buffer (global 0) · heap (mutable global 1).
- **Gate policy sharpened**: a successful emit against a NONZERO-exit
  oracle row is classified `gate:abort-parity-pending`, not claimed and
  not skippable-as-divergence — abort parity (exit + stderr) is its own
  future slice. The div-by-zero/overflow family (×9) lands there.
- **Burn-up**: supported 1 → **18 / 590**, floor re-pinned 18, zero
  divergence. Claims include short-circuit, `i64::MIN`, mutual recursion,
  while loops, concat chains, default params. Next walls, measured:
  `bind-ty:Applied ×216 → bind-ty:Named ×65 → expr:Match ×30 →
  bind-ty:Bytes ×23` (heap aggregates + match = the next slice).
- **Mutation evidence** (aviation rule: every gate proves it can catch):
  (1) concat copies b-first → 3 fixtures red; (2) itoa sign write dropped
  → 4 fixtures red (`i64_min_literal` among them). Both reverted; suite
  green. Strict-tier clippy extended to `-p almide-layout -p almide-wasm`
  in CI (zero findings).

---

## Unit 6 — stage 3: the scalar-sum slice (Option/Result + match) (2026-08-19)

Driven by the measured map (stage 2's next walls: Applied ×216, Match ×30).
All new construction:

- **Sum layout ratified into `almide-layout`** (§6.6: no ad-hoc repr in the
  emitter): Option's `none` IS `NULL_ADDR` — no block, no tag (nested
  Option therefore unrepresentable and refused by construction);
  Result is a tagged block (`SUM_TAG` 0=Ok 1=Err, value at `SUM_FIELD`,
  8-aligned slot) — the shape user variants will generalise. Layout digest
  deliberately RE-PINNED (8782915244131330720 → 6642309021484683773), the
  crate's documented intentional-change procedure.
- **Type-hint flow**: `none`/`ok(x)`/`err(x)` have no self-contained type,
  so `lower(e, want)` carries the expectation DOWN from binds, args,
  returns, match arms. First bug of the slice proved the wall works: the
  hinted `none` returned its type without pushing `i32.const NULL_ADDR` —
  wasmparser validation refused all three affected modules (never
  instantiated), exactly the invariant the harness promises.
- **`match`**: if/else chain over pattern tests; subjects live in shared
  scratch locals (safe: a subject is only read during its own tests,
  which finish before any nested match in a selected arm's body runs).
  Arm-bind patterns load slots from the subject; the FINAL arm keeps its
  test with a LOUD `unreachable` behind it — the checker's exhaustiveness
  promise is verified at runtime, never assumed silently. Guards refused
  (own reason) for now.
- **`!` semantics honoured, not approximated**: in a pure fn returning
  Option/Result the oracle PROPAGATES on none/Err (#1410 family) — those
  bodies are refused (`unwrap-propagating`), only the ABORT form lowers
  (null/Err → trap). A mis-lowering here would have been a silent
  divergence class; the refusal keeps the histogram honest instead.
- **`$str_eq`** helper (byte compare): unlocks `==`/`!=` on String and
  string-literal match arms.
- **Burn-up**: 18 → **27 / 590**, floor re-pinned 27, zero divergence.
- **Mutation evidence**: Result tag inverted → 5 fixtures red
  (letbound_variant_match + tm_res_int family). Reverted; suite green.
- Next walls, measured: `bind-ty:Applied ×193` (now almost all List) →
  `Named ×69` (user records/variants) → `Bytes ×23` → `Float ×16` →
  list module calls. The LIST slice (layout for element arrays, ForIn,
  list.len/join) is the next big mechanism.
