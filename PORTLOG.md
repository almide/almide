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

---

## Unit 6 — stage 4: the List slice (2026-08-19)

The biggest wall (`bind-ty:Applied ×193`, dominated by lists) — and the
slice where the burn-up net earned its keep three times in one sitting:

- **Repr**: `List[scalar]` = one block, payload = the element array,
  len = count × stride (stride = the scalar's slot size), cap = capacity
  bytes. `xs ++ ys` is `$concat` UNCHANGED — byte-concat of payloads IS
  element concat. `==` on List[Int/Bool] is `$str_eq` (bytes are values);
  List[String] holds addresses, so equality is refused, not faked.
- **`infer` now reads the checker's annotation** (`IrExpr.ty`) instead of
  re-deriving structurally — one authority, and `none`/`ok`/`err` work in
  any typed position without the hint plumbing.
- **Hold pools**: stack-disciplined scratch locals for constructs that
  keep an address live ACROSS sub-expression lowering (list literals,
  index bases, for-in state). Depth beyond the pool is an honest unsup —
  never a silent clobber.
- **Three bugs the net caught, in order**:
  1. `$list_get_4` declared `(i32,i32)` but its body treats the index as
     i64 → wasmparser refused EVERY module (helpers are omnipresent).
     Fix: the index is ALWAYS i64; only the slot width differs.
  2. `list.push` lowered as copy-append with the result dropped — but the
     oracle MUTATES through the `mut` param (the 100k growth fixture went
     len=0 and trapped out-of-bounds). Fix: write-back form
     `var = $push(var, v)`.
  3. Copy-per-push is quadratic in BYTES — the 100k fixture needs ~40GB
     and hit the $alloc OOM trap. Fix: amortized growth (in-place while
     `cap - len >= stride`, else doubled capacity) — the layout's `cap`
     field earning its seat.
- **Value semantics by construction**: every List bind/assign deep-copies
  (`$block_copy`), so a local's block is uniquely its own and in-place
  growth is unobservable through aliases (the checker already restricts
  `push` to mut vars). The corpus does NOT yet witness this (the
  bind-copy mutation stayed green — alias fixtures are still refused), so
  **tests/alias_semantics.rs is the dedicated referee**: alias → push →
  len observation vs the interpreter, mutation-verified red/green.
- **Burn-up**: 27 → **43 / 590**, floor re-pinned 43, zero divergence.
  ForIn (list + `..`/`..<` ranges), list.len/get/get_or/push/join,
  ConcatList, list literals all landed.

---

## Unit 6 — stage 5: user types (records + variants) (2026-08-19)

- **Field packing ratified into `almide-layout`** (`pack_fields`: 8-byte
  slots 8-aligned, tail pad to 4, pinned by `field_packing_is_pinned`).
  Records are field blocks; variants are tagged blocks — SUM_TAG then
  fields packed after SUM_FIELD's pad, the direct generalisation of the
  stage-3 Result shape.
- `TypeTable` resolves declarations to layouts; a declaration ANY part of
  which is outside the slice (generic, recursive/boxed, record-shaped
  case, non-slice field) is EXCLUDED whole — its uses then refuse with
  the honest `bind-ty:Named` reasons, never a partial lowering.
- Record literals / spreads (`{...r, x: v}` = block-copy + overwrite) /
  member reads; variant constructors (`Call` targets checked against the
  ctor map first); `Constructor` patterns with tag test + field binds.
  FieldAssign refused — the same no-in-place-mutation doctrine as List.
- **Burn-up: 43 → 50 / 590**, floor re-pinned 50, zero divergence.

## Unit 6 — codopsy A-rank directive (2026-08-19)

Mid-flight user directive: "codopsy A ランクは当然維持". Measured with the
ratified `.codopsyrc.json` (now committed at the greenfield root):

- almide-layout **A(100)**, almide-base **A(100)**, almide-diag **A(96)**
  — already A.
- almide-spine **B(81) → A(94)**: the deficit was `.unwrap()` density in
  parity tests and probe bins — replaced with `.expect("...")` (which
  also names each invariant).
- almide-wasm **C(70) → A(96)**: the 2,500-line monolith split into
  `lib/emitter/calls/patterns/collect/runtime/types_table` (every file
  under the 800-line rule); the cc-61 `lower` decomposed along its real
  seams (`lower_control`/`lower_data`/`lower_sum`/`lower_record`,
  `lower_cmp`, `lower_forin`, `lower_list_get_or`, `test_ctor_pattern`,
  `add_record`/`add_variant`); `lower_fn`'s seven params bundled into
  `Ctx`. Pure mechanical restructuring — the 590-fixture net, first
  light, and the alias referee ran green after every batch, and strict
  clippy stayed at zero.

---

## Unit 6 — differential fuzzing of the wasm leg (flight-gap A-1) (2026-08-19)

User ○: quality gaps before more features, fuzzing first. The first
verification net INDEPENDENT of the hand-picked corpus:

- `tests/fuzz_differential.rs`: a zero-dependency seeded xorshift RNG +
  type-directed generator producing Almide SOURCE over the supported
  surface (scalars, Option, List[Int], interp println, if/for/match,
  push, concat, int.to_string). FIXED seed range 0..200 in CI — a green
  run is a ratchet, not a dice roll; `ALMIDE_FUZZ_ITERS`/`_BASE` extend
  for exploration. Policy mirrors the burn-up gate: checker-reject and
  emit-refusal are counted classes, interp exit≠0 is abort-pending, and
  a compared-count floor (≥ iters/4) stops silent generator drift.
- **It caught a real miscompile on day one** (seed 79, in the fixed CI
  range): `some(A - (some(10) ?? 10))` — sum construction held its block
  base in the SHARED tmp local, and the inner `some(10)`'s construction
  clobbered it, so the outer `some` returned the INNER block. The
  emitter comment ARGUED this impossible ("nested sums are untyped") —
  wrong: nested constructors appear as subexpressions of the inner
  VALUE. Exactly the "comment-argued invariant" class the flight-gap
  review flagged. Fix: OptionSome/ResultOk/ResultErr now use the
  stack-disciplined hold pool like every other aggregate; 590-corpus +
  3,000-seed burst green after.
- Generator falsehoods fixed along the way (its own honest costs): bare
  `a..b` is not surface syntax (`..<` only), `match none` has no type
  context, `i64::MIN` is not writable as one literal.
- **Mutation evidence, and the net-vs-net comparison**: the
  `$append_bool` select swap is caught by the fuzzer at seeds 1/3/4
  (three hits in five seeds) and by the corpus in exactly ONE fixture of
  590 — the fuzzer's detection density on the shared surface is orders
  denser, which is the point of independence.

---

## Verification survey → outlook ratified into the queue (2026-08-19)

User directive: "../almide-references から解決方法の見通しを立てておく".
Three-lens survey (backend-correctness nets / CI gate culture / spec
traceability) across the 9 reference compilers, synthesized as
`../almide-references/RESEARCH-verification.md` (V-1..V-15 canon, SHAs
pinned). Key validations and adoptions:

- Much of greenfield's net is ISOMORPHIC to the field's best practice:
  the burn-up gate is roc's `parallel_runner` in 2-leg form, the fuzzer
  is roc's typed-generator `fuzz-build` + rustlantis' compare-legs shape,
  per-slice mutation evidence is roc's `*_mutation_check` done manually.
- Adopted same-day: roc's coverage instrumentation-broken guard (zero
  measured lines must FAIL, never pass) into check-wasm-coverage.sh.
- Queued, priority order: V-4 finding auto-reduction, V-5 finding→
  fixture permanence, V-1 release-shape test lane, V-6 mutation-gate
  permanence (`ci/mutations/`), V-7 declared-vs-tested surface diff with
  named shrink-only exceptions (= the requirements matrix), V-10
  debug-assert density on hold/scr discipline, V-15 maxrss gate.
- The field's answer to comment-argued safety is to make the argument
  EXECUTABLE (verifier passes, certifiers, mutation gates) — never
  thicker review. seed-79 is this survey's living example.

---

## V-1 + V-4 landed: release-shape lane, finding auto-reduction (2026-08-20)

- **V-1**: CI `release-shape` job runs the full almide-wasm net (590-fixture
  parity, 200-seed fuzz, first light, alias referee) with `--release` — a
  debug/release divergence (optimizer, overflow-checks, wasmtime tier)
  cannot ship unseen. Locally the release parity sweep runs 60s → 5s.
- **V-4**: the differential fuzzer now SHRINKS every finding (rustlantis'
  `--reduce` shape): drop line windows (8/4/2/1) to fixpoint while the
  divergence survives; pipeline refusals reject a removal for free, so
  soundness costs nothing. Mutation-verified end-to-end: the planted
  `$append_bool` swap's ~10-statement random program reduced to the
  1-line minimal repro `println("${(42 >= 42)}|${false}")`.
- **V-5** is now a stated rule in the finding message itself: a reduced
  divergence lands as `spec/wasm_cross/fuzz_found_*.almd` in the fixing PR.

---

## V-6 landed: the mutation gate is standing, not manual (2026-08-20)

The five slice-proven mutations (concat b-first, itoa sign drop, Result
tag invert, List bind-copy skip, bool select swap) are now pre-authored
patches in `ci/mutations/`, and `scripts/check-mutation-gate.sh` (CI job
`mutation-gate`) applies each against the release-shape net and requires
RED. A surviving mutant = the net lost a tooth; a patch that stops
applying = code drift must REFRESH the mutant, never silently retire it
(roc's discipline). First full run: 5/5 caught, tree clean after.
New-slice rule going forward: each slice's mutation evidence lands as a
new patch here in the same PR, so the manual step cannot be forgotten.

---

## V-7 landed: the exercised-surface manifest (requirements matrix) (2026-08-20)

`tests/surface_matrix.rs` + `tests/golden/wasm-exercised-surface.txt`
(61 constructs): for every fixture the backend can emit, the IR is walked
and each lowered construct recorded (expr kinds, operators, stdlib
calls — fixture-local fn/ctor names normalized to `call:user-fn` so the
manifest carries surface, not noise; patterns; stmt kinds). The golden is
the committed, reviewed matrix of what the wasm leg CLAIMS to exercise:

- a construct DISAPPEARING is a failure — the silent-regression class
  the supported-count floor cannot see (rust's `target_policy.rs` shape:
  declared-vs-tested diff, holes must be named);
- growth requires a deliberate `ALMIDE_UPDATE_SURFACE=1` regeneration —
  a reviewed diff, never drift.
- The refused half of the matrix needs no second registry: the burn-up
  histogram already names every wall.
- Mutation-verified in both directions (planted `binop:FakeOp` → red).

---

## Fuzzer surface: records + variants (coverage witness) (2026-08-20)

The generator now produces the fixed preamble types `Pt`/`Tr`, record
literals, spreads, member reads, variant constructors, and variant
matches (binding + literal-ctor arms). Purpose delivered: the coverage
inventory's "hint-absent constructor paths" are now WITNESSED — emitter
line coverage 86.9% → 90.0%, package total 92.8% → 94.0%, never-run
functions 3 → 2; floor re-pinned 90 → 92. Fixed-range claim rate 84% →
89%; a 2,000-seed exploration burst on the extended surface found no
divergence.

---

## Unit 6 — stage 6: interned element types (the nested-Applied slice) (2026-08-20)

The Applied wall's remaining mass was NESTING (List[Named], List[List[Int]],
Option[composite], Result with composite sides, composite record fields).
One architecture ends it piecemeal-free: element types are now INTERNED —
`SliceTy`'s composite payloads hold `ETy` arena handles (dedup on intern,
so handle equality IS type equality and every existing `==` comparison
stays exact across arbitrary nesting; the arena lives in the TypeTable
behind interior mutability so no signature moved). rustc's TyCtxt shape,
at slice scale.

- List/Option/Result/record-fields are now parametric in ANY slice type;
  the wasm level only ever cared about slot width (i64 vs i32-word), so
  the runtime helpers needed zero changes.
- Byte-equality rule recomputed honestly: `==` stays for Str and lists
  whose payload bytes ARE values (Int/Bool elements); any address-holding
  element makes byte-compare an identity test — refused, not faked.
- Literal patterns inside some()/ok()/err() require scalar slots — the
  non-scalar case is its own honest reason, not a wrong compare.
- **Corrected a layout-doc overclaim**: nested Option was ALWAYS
  representable (`some(none)` = a block holding NULL_ADDR ≠ outer NULL);
  the comment said otherwise and slice_ty_of's refusal had hidden it.
  Comment fixed (no constant changed — digest untouched).
- Mutant 003 drifted in the refactor and was REFRESHED, not retired
  (the gate doctrine working as designed on its second day).
- **Burn-up: 50 → 57 / 590**, floor re-pinned 57, zero divergence; fuzz
  fixed range green. Next walls: bind-ty:Applied ×80 (Map/Set/remaining),
  Named ×46 (record-shaped variant cases, generics), Bytes ×24,
  list.map ×20 (closures), Tuple ×17, Float ×16, StringInterp-as-value ×12.

---

## Unit 6 — stage 7: value-position interpolation + string.len (2026-08-20)

- **`"${...}"` as a VALUE**: the line buffer gained a stack-disciplined
  BUILD CURSOR global — a nested value-position build starts after the
  outer's partial content and restores on exit, so interpolations nest to
  any depth; the finished region is captured as a real block
  (`$buf_to_block`). The append helpers now TRAP on buffer overflow
  (bounds against `G_LINE_END`) instead of corrupting the heap behind it.
- **`string.len`** — the oracle counts CODEPOINTS (`chars().count()`),
  so `$str_len_chars` counts non-continuation bytes, never the byte len.
- The fuzzer generates value-position (and therefore nested)
  interpolations; 1,500-seed burst clean.
- **Burn-up: 57 → 64 / 590**, floor re-pinned 64, zero divergence;
  surface manifest +`call:string.len`.

---

## Unit 6 — stage 8: HOF inlining (map/filter/fold) (2026-08-20)

The corpus's callback idiom is the LITERAL lambda (153 lambda sites vs 31
other lines) — so the dominant list HOFs land with ZERO closure
machinery: the lambda is inlined at the call site, its params become
locals (collected like any bind), and captures are simply the enclosing
locals already in scope. `list.map` mallocs the result up front (same
count), `filter` builds through the amortized push helper, `fold`
threads the accumulator param. Fn-typed VALUES (closures as data) remain
their own honest refusal — a later mechanism, not a partial fake.

- **Burn-up: 64 → 85 / 590** (+21, the largest single-slice jump), floor
  re-pinned 85, zero divergence; `call:list.map` left the histogram.
- The fuzzer now generates map/filter callbacks and fold chains;
  1,500-seed burst clean. Surface manifest +3 (lambda, map/filter/fold).
- Remaining walls: Applied ×80 (Map/Set), Named ×45, Bytes ×24,
  Tuple ×19, Float ×16, Matrix ×12, process.exit ×10, RuntimeCall ×9,
  Fn-typed values ×8.

---

## Stage-8 CI red → return_call (C-292) (2026-08-20)

The stage-8 push went RED in CI: `ref_gleam_tail_deep` (first claimed by
the HOF slice) runs 200k-deep tail recursion — it survived a laptop's
generous wasmtime stack and overflowed the runner's. Environment-shaped
depth bugs are exactly what the C-292 contract exists for; the fix is
the real mechanism, not a bigger stack:

- Tail position now propagates through function bodies, block tails,
  if arms and match arms (one-shot `in_tail` marker, TAKEN at `lower`
  entry so it can never leak into operand lowering); a direct call in
  tail position with a matching return type emits **`return_call`** —
  constant stack for arbitrarily deep, including mutual, recursion.
- New referee `tests/tail_calls.rs` pins a DELIBERATELY TINY 64 KiB wasm
  stack so depth-vs-environment can never hide again;
  mutation-verified (disabling return_call → red) and promoted to
  standing mutant `006-return-call-disabled.patch` (gate suites now
  include the referee).

---

## Unit 6 — stage 9: Map/Set core (2026-08-20)

Insertion-ordered entry blocks — the oracle's own semantics (`Value::Map`
is an insertion-ordered entry vec). Entry layout from `pack_fields`;
lookup via three shared `$scan_*` helpers (i64 / raw-i32 / string-bytes
key classes) reused by Map (key offset) and Set (offset 0). Keys and set
elements are scalars (defined equality); values any slice type.

- map.new/set/insert(mut→write-back)/get/get_or/contains/len;
  set.new/from_list(dedup loop)/insert/contains/len/to_list (layout-
  identical to List, so to_list is the identity — sharing unobservable
  under the no-in-place doctrine; binds deep-copy extended to Map/Set).
- `map.new()`/`set.new()` have no argument to type them: the checker's
  annotation flows in as a ret-type hint through `lower_call_at`.
- First hold-arithmetic draft ("reborrow" index guessing) was REJECTED
  mid-slice as the same comment-argued class seed-79 falsified — scans
  now RETURN every hold explicitly.
- **Fuzzer finding about the fuzzer**: map programs made the interp
  oracle ABSTAIN (exit -2, the #1226 heap-bridge boundary) and the old
  tally silently filed that under abort-class — a growing blind spot.
  Now a VISIBLE `ORACLE-ABSTAINED` class (17/200 on the fixed range);
  the corpus manifest remains the true referee for map fixtures.
- **Burn-up: 85 → 87 / 590**, floor 87, zero divergence. The map wall's
  remainder is compound: tuples (map.from_list pairs ×24, bind-ty:Tuple
  ×20) and map HOFs — the tuple slice is next.

---

## Unit 6 — stage 10: tuples (the keystone slice) (2026-08-20)

Tuples unlock everything that carries pairs: `map.from_list`,
`list.enumerate`, `let (a, b) = …` (BindDestructure), `for (i, x) in …`
(ForIn tuple vars), tuple literals/`.0`-indexing, tuple match patterns.
Shapes are INTERNED in the TypeTable's tuple arena (`SliceTy::Tuple(id)`
— handle equality is shape equality), layout from `pack_fields` like
records.

- **The gate caught 6 divergences mid-slice** — two mirror-image bugs of
  one class: field loads computed ABSOLUTE addresses but used the
  base-relative (+PAYLOAD) accessor in `from_list` (header read as key),
  and `get`/`get_or` used the base-relative accessor on the scan's
  absolute entry address (read 12 bytes past the value). The class —
  two addressing conventions distinguishable only by discipline — is
  now named in collections.rs; making them type-distinct is queued
  hardening.
- **Generator archaeology**: the fuzzer had emitted the REMOVED `++`
  operator since the List slice — every concat program silently
  checker-rejected (waste, not blindness; the corpus still covered
  concat). Fixed to `+`: rejects 41 → 8, compared 137 → 166.
- Fuzzer generates destructures, enumerate loops, and from_list maps;
  1,500-seed burst clean (197 oracle-abstained = the known map-bridge
  blind spot, visible by class).
- **Burn-up: 87 → 109 / 590** (+22), floor re-pinned 109, zero
  divergence; surface manifest 69 → 81 constructs.

---

## Unit 6 — stage 11: two-phase type table (forward refs + recursion) (2026-08-20)

The Named wall's biggest component was an ORDERING artifact: the type
table built in declaration order, so `type A = { b: List[B] }` before
`type B` excluded BOTH (and mutual recursion deadlocked structurally).
The insight that ends it: a composite field is a SLOT (an i32 address) —
a layout never needs its pointee's layout, only its NAME.

- Phase 1 registers every declaration (Excluded placeholder); phase 2
  builds definitions in place. Forward references and arbitrary
  (mutual) recursion now resolve; the variants' `boxed_args` exclusion
  is lifted (boxing is a Rust-target concern the wasm slot never sees).
- `Excluded` stays name-resolvable so OTHER layouts can hold slots of
  it: constructing a value of an excluded type is impossible (its ctors
  refuse), so such slots are unreachable — refusal stays sound without
  poisoning whole type graphs.
- Defaulted record fields: a literal supplying fewer fields than the
  layout is refused (`record-defaults`) until the checker's omission-
  filling is verified — a missing store would leave header garbage.
- **Burn-up: 109 → 122 / 590** (+13), floor 122, zero divergence;
  Named 48 → 29. Remaining walls: Named ×29 (generics + record-shaped
  cases), Applied ×28, Bytes ×24, Float ×19, Matrix ×12, Fn ×10.

---

## Unit 6 — generic TYPE instances (groundwork) (2026-08-20)

Generic declarations (`type Box[T]`, `type Either[L, R]`, mutually
recursive `Tree[A]`/`Forest[A]`) now monomorphize ON DEMAND: instances
are keyed by (name, resolved args), their index RESERVED before fields
build (the same trick that ended the forward-reference wall, so
recursive generic instances resolve), and `Ty::TypeVar` substitution
happens at the Ty level before field resolution. Variant constructor
resolution moved from the global name map to BY NAME WITHIN THE
SUBJECT'S/ANNOTATED type — exact for concrete types and the only
unambiguous route for generic instances (ctor names repeat across them).

Honest yield note: +0 fixtures — every generic-type fixture also calls
generic FUNCTIONS, which fn_signature still refuses. Function
monomorphization (the R3 intra-CU mono decision, driven by the IR's
call-site `type_args`) is the next slice; this commit is its type-side
half, kept green under the full net (122/590 parity, fuzz, mutants).

---

## Generic instances live: the Named-param encoding discovery (2026-08-20)

Three probe rounds pinned why generic instances stayed Excluded:
1. `monomorphize` ALREADY RUNS — `link_ir` = optimize_half + link_half,
   and link_half is `monomorphize + ir_link`. My planned "port the mono
   pass into s5" was a double-run and was reverted before landing;
   generic FUNCTIONS arrive as concrete specializations (`unbox__Int`).
2. What was actually missing: generic TYPE params are encoded by the
   decl lowerer as bare `Ty::Named("T", [])`, NOT `Ty::TypeVar` — the
   substitution only replaced TypeVar, every instance built as Excluded,
   and `instance()` still returned the index (refusals honest, cause
   invisible). One arm fixed it: a bare Named matching a param SHADOWS
   any like-named type in the declaration's scope.
3. `debug hooks that print Some(Named(i))` prove nothing about the DEF —
   the probe had to inspect def-kind to see EXCLUDED.

Burn-up: 122 → **124 / 590**, floor 124, zero divergence.

---

## Unit 6 — Float values groundwork (2026-08-20)

f64 is now a first-class scalar: literals, arithmetic (f64.add family),
comparisons and equality (f64.eq — the oracle's own == semantics, NaN
included), aggregate slots, match scratch, and a dedicated f64 hold
pool. The parity net immediately caught the first plumbing gap (a
generic instance carrying a Float field routed f64 through the i32 slot
arm — wasmparser refused the module) and the two-way val_type dispatches
are now three-way everywhere.

The REST of the Float wall (float.to_string, `${float}`) is gated on the
self-host linkage arc: the oracle formats floats via the SELF-HOSTED
Dragon4 in stdlib/float_to_string.almd (pure Almide over the prim
floor — load/store/alloc/bit ops, all direct wasm mappings), but s5 does
not yet link self-host module bodies into the IR (probe: a float
fixture's IR carries ONE function). That linkage is the next major arc:
it unlocks Float, Bytes, and the self-hosted string/int surface at once.

Burn-up: 124 → **125 / 590**, floor 125, zero divergence.

---

## Unit 6 — linked-module functions (self-host arc, part 1) (2026-08-20)

Probing the self-host plan surfaced where module functions actually LIVE:
`ir.modules` (the interp resolves `CallTarget::Module` against them at
runtime) — `ir.functions` never held them, which is why even fully-linked
modules (url) stayed refused. The emitter now flattens every linked
module's functions into the call table under their QUALIFIED name
("url.encode_component") — exactly the Module-call lookup key — and
module-owned type declarations join the type table. Modules carrying
top-level lets are excluded whole (module init order is its own slice).

Burn-up: 125 → **126 / 590** (imported pure modules like url). The big
half of the arc remains: s5 loading self-host REGISTRY modules (they are
fully-bodied but never imported, so resolve never sees them) plus the
prim floor (~25 direct-mapping ops) — that combination unlocks the
oracle's own Dragon4 float formatting, Bytes, and the self-hosted
string surface.

---

## Unit 6 — stage 12: the self-host arc lands (2026-08-20)

The oracle's own Dragon4 now formats floats on the wasm leg, byte-exact
under fuzz (random float arithmetic → formatting → 1,000+ compared runs
clean). The arc surfaced more real bugs than any before it, each caught
by a referee:

- **prim floor** (~22 direct-mapping ops: handle/loads/stores/allocs/
  bit ops/f64 ops, `f2i` as trunc_sat = Rust `as` semantics, `die` as
  eprint+trap) — `src/prim.rs`.
- **s5 loads registry modules on demand** (worklist to closure over the
  IR's module calls + implicit demands like `${float}` →
  float.to_string_compound), and — after the first attempt REWROTE call
  sites and broke the interp leg 468 → 254 — the ratified shape is
  LOADING ONLY: one IR, two sound resolutions (interp keeps its bridge,
  the emitter resolves surfaces through the same registry).
- **The layout boundary is real and now explicit**: the incumbent keeps
  Result tags in the len slot and packs EVERY list element into 8 bytes;
  ours differ deliberately. After the signature heuristic missed two
  coupling classes (result.unwrap_or returned the default for ok(5);
  string.join trapped on 4-byte list slots), linked-impl resolution is a
  VERIFIED WHITELIST — Dragon4 + its closure (math_log family) — grown
  one impl at a time with parity evidence.
- **Named-call resolution is module-scoped** (a global simple-name index
  collided across modules' helper names and called the WRONG module's
  fn — nondeterministically, until self-host load order was also made
  deterministic). Reachability BFS now walks table INDICES, closing the
  registry-resolved escape it had.
- **`${float}` formats via float.to_string_compound** — the oracle's
  interpolation form drops the ".0" suffix; to_string does not. Two
  formatters, both linked, each used where the oracle uses it.
- **slot_size was still two-way** — F64 slots computed as 4 bytes (the
  parity net showed zeroed float lists; the disassembly showed a div-by-4
  against an f64 load). Every 8-byte VALUE type is an 8-byte slot now.
- Float list elements cross the 8-byte helper boundary as BIT PATTERNS
  (i64.reinterpret_f64) — memory is bytes; only call-boundary value
  types need bridging.

**Burn-up: 128 → 137 / 590**, floor 137, zero divergence; fuzz surface
includes float arithmetic + formatting; interp leg re-verified at
468/121/0 after every s5 change.

---

## Unit 6 — stage 13: Bytes core (2026-08-20)

Bytes is String's layout twin (byte-packed block, len = byte count),
implemented as native special forms — the incumbent's Bytes travels as
an 8-byte-slot List[Int] behind the bridge, so registry impls stay
excluded and the ops mirror ORACLE OUTPUT directly: new (zero-filled by
the bump allocator's fresh-page guarantee), from_list/from_string,
len/get_or/read_u8, in-place set_at/set_u8/set_f32_le (sound under the
bind-deep-copy doctrine), and read_f16_le through `$f16_to_f64` — EXACT
half-float widening by bit construction (normals and inf/nan re-based
into f64 fields; subnormals via exact m × 2⁻²⁴ scaling).

Referees earned their keep three times in one slice:
- the validator killed the first f16 draft (an inner `if` block starts
  with an EMPTY stack — f64_neg reached for a value outside it);
- the f16 fixture caught the select's inverted sign order;
- `mutable_global_repeat_writes` (a bytes-snapshot fixture) exposed that
  the Bind/Assign deep-copy rule in the MAIN path still said List-only —
  the Map/Set extension had only landed in the top-let prelude. All four
  container classes now copy in both paths; mutant 004 refreshed to the
  new shape.

**Burn-up: 137 → 140 / 590**, floor 140, zero divergence.

---

## Unit 6 — stage 14: the slice/repeat cluster (2026-08-20)

`string.slice` (CODEPOINT indices — `$cp_off` scans to the idx-th
codepoint start, clamped like the native rt: negatives → 0, past-end →
len, start ≥ end → ""), `string.repeat` (n ≤ 0 → "", the 2 GiB cap traps
in the abort-pending class), and `list.slice` (element indices with the
native `start as usize` semantics: negative start → empty; end clamps to
count). The `end` parameter's surface default (i64::MAX = "to the end")
is materialized at the call site when the third argument is omitted.

**Burn-up: 140 → 149 / 590**, floor 149, zero divergence — codepoint
semantics matched the oracle on the first gate run.

---

## Unit 6 — stage 15: record-shaped variant cases (2026-08-20)

`| Scroll { dy: Int, fast: Bool }` — the last variant SHAPE. One shared
`build_case` now produces every CaseDef (tuple cases carry positional
names, record cases their declared names), so construction
(`Record{name: Some("Scroll")}` literals whose type is a Variant become
tagged-case builds), the tag test, and NAMED field binds
(`Scroll { dy, fast } =>` RecordPatterns) all run through the same
machinery as tuple cases. Generic variants with record cases come free
through the same builder.

**Burn-up: 149 → 150 / 590** (a quarter of the corpus), floor 150, zero
divergence. The Named wall's remainder is compound with other refusals;
the shape machinery is now complete for every declared-type form the
corpus uses.

---

## Unit 6 — stage 16: abort parity is a CLAIMED surface (2026-08-20)

The `gate:abort-parity-pending` class is RETIRED. `almide.exit(code)` is a
third host import — the harness records the code before the unwind, so every
run yields (stdout, stderr, exit) and a nonzero-exit oracle row is claimed
only when the wasm leg reproduces both the stdout-before-abort hash AND the
exit code. `process.exit(n)` lowers to the import + `unreachable`; the
ALS-T18 assert desugar (eprintln + process.exit) then unlocks the whole
assert-abort family with no assert-specific code.

**Activation-day catch (C-002, the mission thesis in one fixture):** wasm
`i64.rem_s` DEFINES `MIN % -1 = 0` — no trap — so `int_mod_overflow` printed
`0`/exit 0 against the oracle's abort. The incumbent's guard had NOT been
ported; the gate found the carried-over gap the moment it activated. div/rem
now guard both operands and abort with the exact native frame
("Error: division by zero" / "Error: integer overflow" + exit 1), making the
raw wasm traps unreachable for arithmetic.

Fuzzer: abort rows are compared (stdout + exit), not skipped, and the
generator now produces sometimes-zero computed divisors — 6 abort-compared
per 200 seeds, all agreeing. Mutants 007 (exit neutered) and 008 (div guard
removed) are the slice's teeth.

**Burn-up: 150 → 167 / 590**, floor 167, surface 97 → 103 constructs, zero
divergence.

---

## Unit 6 — stage 17: match guards + slice-syntax delegation (2026-08-20)

Guards: an arm's verdict is `pattern-test AND (binds; guard)` — binds run
before the guard (it references them) and locals are function-scoped, so a
guarded arm's body needs no re-bind and a failed guard's binds are
harmlessly overwritten. Irrefutable-with-guard keeps the chain (no
unconditional short-circuit). Mutant 009 (guard fail-open).

Slice syntax: `xs[a..b]` desugars to `almide_rt_list_slice` — delegated to
the one `list.slice` lowering, as native rt shares one impl. Mutant 010
(slice start boundary). The RuntimeCall histogram's real remainder is the
fan budget/timeout machinery (×14), now visible under its own name.

**Burn-up: 167 → 171 / 590**, floor 171, zero divergence.

---

## Unit 6 — stage 18: nested patterns + a net-tooth lesson (2026-08-20)

**Mutant 010 SURVIVED on first landing attempt** — `a<0 → a<=0` in the slice
empty-check went uncaught because NO exercised program sliced from zero (the
everyday form: existing fixtures use starts 2/10, the claimed rt-slice rows
nonzero starts). The gate blocked the push, exactly as designed. Tooth added:
the fuzz generator now produces `list.slice(xs, 0|1, k)` — mutant 010 is
caught by 4 seeds. Standing gap noted: landing NEW spec fixtures requires the
oracle-pinned run-manifest regen procedure (also the V-5 permanence path) —
not yet exercised in greenfield.

Nested patterns: `some(ok(3))`, `Nd(Lf, x)`, ctor-in-ctor — every inner
pattern now recurses through a typed hold (`test_nested`/`bind_nested`), so
the outer subject's scratch survives later fields. The Literal-only special
cases collapsed into the general form. Mutant 011 (nested test forced true).

**Burn-up: 171 → 175 / 590**, floor 175, zero divergence.

Reference survey landed: ../almide-references/RESEARCH-wasm-backends.md
(W-1..W-9, zig/roc/grain SHA-pinned) — Fn-value slice design ratified there
(W-1 +1-biased table of address-taken fns + W-2 closure blocks; corpus HOFs
stay inlined). Roc/zig have NO return_call — C-292 keeps us ahead.

---

## Unit 6 — stage 19: structural list equality (2026-08-20)

`==` on lists generalizes past byte-equality: Int/Bool payloads stay
byte-compared ($str_eq — the bytes ARE the values), while address-carrying
elements (Str, Float via f64_eq NaN/-0.0 semantics, nested lists) compare
ELEMENT-WISE through a recursive `emit_val_eq` — same length, then every
element equal, b's element addressed as `cur - a + b` (five holds per
nesting level; the pool bound turns absurd nesting into an honest refusal).
Mutant 012 (element mismatch never flips the verdict).

**Burn-up: 175 → 180 / 590**, floor 180, zero divergence.

Effect-convention probe (for the Unwrap ×6 wall): effect fns keep their RAW
ret_ty (`Int`) + `is_effect=true`; the `!` marker node carries a
Result-typed operand and the raw result type itself. The wasm effect
convention (does an effect fn RETURN a Result block, or trap-propagate?) is
ours to define against the interp's Flow::Return(Err) semantics — next
design decision before the Unwrap slice.

---

## Unit 6 — stage 20: the effect convention (2026-08-20)

Effect fns stop being a wholesale refusal. The wasm convention: an effect
fn's value is ALWAYS one Result block — the interp's raw-value-or-
Flow::Return(Err) pair becomes tag dispatch on one static type. Bodies
yield the RAW ok value and wrap `ok(..)` at the tail; `!` now has the
interp's three enclosing shapes: effect fn → PROPAGATE (return the err
block as-is — err blocks of any Result(_,E) share one layout; slot-type
guards refuse mismatched E), main → ABORT with the exact native frame
("Error: {msg}" + exit 1, via the abort-parity machinery), pure fn → same
frame (stderr contract now emitted, was a bare trap). C-216 identity
(marker typed Option = effect-layer strip) honored. Known debt: effect
bodies drop the tail marker (`f()!`-in-tail return_call peephole queued),
and `-> Unit` effect helpers wait on a Unit repr.

Whitelist growth with a SECOND tier: `string_to_int`/`int_from_hex` trip
the coupled-type proxy (Result in signature) but their bodies are audited
raw-write-free — sums built via language-level ok()/err(), lowered by THIS
emitter with THIS layout; the proxy guards hand-written block internals
and misfires on constructor-built sums. `string_trim` (String→String, raw
stores build string blocks only — digest-shared layout) joins tier 1.
`string_split` stays refused: it hand-builds List[String] with 8-byte
incumbent slots — the REAL coupling class.

Tuple patterns (refutable positions) + tuple equality (field-wise through
emit_val_eq) complete the composite-comparison matrix for tuples.

**Burn-up: 180 → 195 / 590** (effect +1+7 via parse/trim, tuples +3+4),
floor 195, surface 105, zero divergence. Mutant 013 (ok-wrap tag).

---

## Unit 6 — stage 21: the equality matrix + declared-sum effect ABI (2026-08-20)

Probe-settled ABI corner (the `ty-mismatch:result-vs-Scalar(Int)` class):
a declared-Result effect fn is SINGLE-layer — its body yields the Result
value itself (`ok(h(p)! + …)`), no wrap; declared-Option and raw-T effect
fns wrap (call sites are checker-annotated `Result[T?, E]` / `Result[T, E]`).
The four cells of effect_option_explicit_bang (never-err/can-err ×
scalar/heap payload) and unwrap_in_callarg all claim.

Equality matrix completed for the current type surface: Option (null-ness
agreement + recursive payload), records (field-wise), unit variants (tag),
payload variants (tag + per-case field dispatch — an if/else chain per
case, unit cases settled by the tag). The wasmparser wall caught BOTH
authoring bugs in the variant chain (then/else inversion, missing outer
end) before any instantiation — four fixtures briefly divergent, zero
landed. Mutant 014 (variant equality degraded to tag-only).

Whitelist: + list_repeat (8-byte Int slots, the shared list class; carries
its own C-169 ceiling die), + string_to_upper (byte-level string builder;
its tuple helpers lower through THIS emitter).

**die stderr fix (invisible-divergence class):** the die convention carries
its own trailing `\n` and the host print appended another — every die
line was doubled on the wasm leg, invisible because no gate compares
stderr yet. Now printed verbatim (trailing newline stripped at the call).

**Burn-up: 195 → 216 / 590**, floor 216, surface 107, zero divergence.

---

## Unit 6 — stage 22: anonymous records as synthetic Named defs (2026-08-20)

`Ty::Record` shapes intern by (name, type) field list into synthetic Named
record defs — construction, member access, equality, and RecordPatterns
all reuse the Named machinery with zero new runtime code. Mutant 015
(field lookup degraded to positional zip).

Process note: the stage-21 landing's mutation gate ABORTED with "tree
dirty after gate" because this slice was drafted onto the tree WHILE the
gate ran — the gate's clean-tree invariant blocked the push exactly as
designed. Standing rule reaffirmed: no tree edits while a landing task
runs; drafts go to the scratchpad.

**Burn-up: 216 → 221 / 590**, floor 221, zero divergence.

---

## Stage 22 addendum: the refusal-shaped mutant escape (2026-08-20)

Original mutant 015 (by-name field lookup → positional zip) SURVIVED even
after the fuzzer gained shuffled anonymous-record literals — not because
the nets lack teeth but because the mutation degrades into a TYPE MISMATCH
inside `lower`, i.e. an HONEST REFUSAL: affected programs drop out of the
compared class instead of diverging, and 13 extra refusals breach neither
the parity floor nor the compared-count floor. **Refusal-shaped mutants
are invisible to correctness nets by construction** — the mutant class
worth keeping is the one that yields WRONG VALUES. Replaced with 015
layout-reversal (anon field offsets reversed): baseline green, 13 fuzz
findings under the mutant. Fuzzer additions stay: anonymous-record binds
print both members immediately, so any placement error is observable in
every program that binds one.

---

## Unit 6 — stage 23: function VALUES (2026-08-20)

The W-1/W-2 design (RESEARCH-wasm-backends.md) lands: a fn value is an
i32 funcref-table slot, +1-biased so 0 is the permanently-null trap slot;
only value-referenced functions enter the table (insertion order = slot
order, deterministic). Named refs resolve module-qualified-first like
calls; a PURE fn filling an EFFECT slot synthesizes an ok-wrap ADAPTER
(C-221 carrier semantics); non-capturing lambdas LIFT into extra
functions through a fixed-point loop (a lifted body may register further
lambdas); computed calls go через call_indirect — return_call_indirect in
tail position — with signature types interned after the per-fn types.
Capturing lambdas refuse honestly (`fn-value-capture` ×9, the closure-
block mechanism is the queued follow-up). Reachability BFS extended: a
table entry's target is a root like any call.

Two authoring bugs the nets caught before landing: the encoder's
call_indirect argument order is (table, type) — swapped args put the TYPE
index in the table slot (first_light red, "unknown table 391"); and the
funcref table must exist whenever ANY body emits call_indirect, entries
or not — a stdlib body calling its fn param needs the table even when no
entry was ever registered (221 fixtures briefly red). Mutant 016 (the +1
bias dropped).

**Burn-up: 221 → 231 / 590**, floor 231, surface 108, zero divergence.

---

## Unit 6 — stage 24: container interpolation + float.parse (2026-08-20)

`${list}` parts route through the SAME linked display impls the oracle
uses — `list.to_string` (List[Int]) / `list.to_string_f` (List[Float],
whose ".0"-dropping element form the impl's own header documents) — for
the layout-SHARED element classes only; Bool/String lists keep the
incumbent's 8-byte slots and stay walled. Mutant 017 (display surfaces
swapped). float_parse joins the sum-builder tier (raw stores target a
scratch buffer's PAYLOAD — offset 12 is layout-shared; sums built via
ok()/err()).

Wall-owner probe: `var:unmapped` ×5 = MODULE GLOBALS (module top_lets
are excluded from flattening — the init-order slice is real and next);
`expr:Unwrap` ×6 is a first-refusal label over deeper walls (fs, Matrix,
mut-param, module globals, Unit ABI) — not an unwrap gap.

**Burn-up: 231 → 234 / 590**, floor 234, surface 109, zero divergence.

---

## Unit 6 — stage 25: the fuzzer's second real catch + gate incrementalization (2026-08-20)

**Fuzz finding (the generator learned `${list}` and caught stage 24's
wiring the same hour):** the linked list_to_string impls read the len
header as ELEMENT COUNT — the incumbent wasm's convention; ours is BYTES
— so `${[1, 1]}` printed garbage elements from past the block's end. A
READ-side layout coupling: the whitelist audit checked raw WRITES only.
The corpus never exercised `${list}` (the +3 at stage 24 came from
float_parse), so the burn-up stayed green — the independent net earned
its keep. Fix: the display impls are RETRACTED from the whitelist; the
list shell now builds NATIVELY in the line buffer ('[' + per-element
F_APPEND_I64 / compound-float + ', ' + ']'), byte-matching the oracle.
Audit criterion extended: read-side header semantics count as coupling.
Mutant 017 (separator shortened) — 68 fuzz findings under the mutant.

**Mutation gate incrementalization (ratified ○):** locally the gate runs
only mutants whose patched files intersect the work in flight
(`ALMIDE_MUTATION_SCOPE=incremental`); CI's mutation-gate job keeps the
FULL sweep on every push. Verification total unchanged; landing cycles
lose the ~15-minute re-proof of untouched mutants.

**Burn-up: 234 → 236 / 590**, floor 236, zero divergence.

---

## Unit 6 — stage 26: top-lets are wasm GLOBALS (2026-08-20)

Top-lets (root + module) leave main's locals and become zero-initialized
mutable wasm globals, set by main's prelude in DEPENDENCY order — the
SAME `dependency_init_order` the interp uses (C-077: `BANNER` declared
first but reading `APP_NAME` through a fn must see it initialized), so
the order matches by construction. Functions read globals across
function boundaries — the class main-local top-lets could never serve
(the whole `var:unmapped` wall). Modules WITH top_lets are no longer
excluded from flattening. Var/Assign fall back to the globals map;
container semantics unchanged (binds from globals deep-copy as always).
Mutant 018 (dependency order degraded to declaration order — exactly the
#632 regression the fixture pins).

**Burn-up: 236 → 240 / 590**, floor 240, surface 110, zero divergence.

---

## Unit 6 — stage 27: CLOSURES — the uniform env convention (2026-08-20)

Fn values graduate from bare table slots to closure BLOCKS
`[slot:i32][captures packed…]` under ONE convention: every table-entered
function is `(env, params…) -> ret` with env as arg 0 (W-2, grain's
doctrine). Capture-free blocks are POOL STATICS (dedup by content, zero
runtime alloc); capturing lambdas alloc + snapshot their captured locals
BY VALUE (the interp's closure semantics); the lifted fn's prelude loads
captures from env into fresh locals, so the body lowers unchanged. Plain
named fns get a forwarding shim (return_call — constant stack); the
C-221 ok-wrap adapter merges into the same shim form. Computed calls
push env first and fetch the callee slot from the block's first field.
`fn-value-capture` is RETIRED as a wall: compose-style closures
(capturing OTHER fn values) work — a captured fn value is just an i32
block address like any other capture. Mutant 019 (capture snapshot
skipped).

**Burn-up: 240 → 249 / 590**, floor 249, zero divergence.

---

## Unit 6 — stage 28: Unit as a value + copy-on-write IndexAssign (2026-08-20)

`SliceTy::Unit` (one i32 zero) covers where Unit must FLOW — binds,
params, and the effect ok payload — while pure Unit-returning fns keep
the void convention. The declared-Unit effect ABI (C-135's four shapes)
claims: a Unit-effect body is statement-shaped and the `ok(())` payload
materializes after it runs.

`xs[i] = v` lands as COPY-ON-WRITE — semantically identical to the
interp's `Rc::make_mut` COW (C-033), alias-safe by construction whatever
escapes; evaluation order (index, value, bounds) and the OOB abort frame
("Error: index out of bounds" + exit 1) match the interp verbatim. The
unconditional copy is a correctness-first cost; ownership-guarded
in-place stores are a perf-war slice. Mutant 020 (store misses the
payload offset — header corruption every fixture catches).

**Burn-up: 249 → 256 / 590**, floor 256, surface 111, zero divergence.

---

## Unit 6 — stage 29: the dynamic Value model, REBUILT (2026-08-20)

Ratified ○ (2026-08-20): the Value/json/Codec arc REBUILDS on this
backend's layout instead of adopting the incumbent's len-as-tag
convention (which bit twice today — list_to_string's len-as-count, and
the whole class the layout doctrine exists to kill). A Value is a
16-byte tagged block at the SAME offsets Result uses (tag@SUM_TAG,
payload@SUM_FIELD); tags 0=Null 1=Bool 2=Int 3=Float 4=Str 5=Array
6=Object; Str/Array payloads are OUR block addresses (4-byte-slot
lists). The registry's value/json impls stay unlinked by design — their
algorithms (json parse/stringify) get PORTED onto this representation in
the arc's later stages, not their layout.

Stage 1 (src/value.rs): scalar constructors (null/int/bool/float/str)
+ array + the as_* accessor family with the incumbent's exact err lines
("expected Int" …) and the #658 Int→Float widening. The `Value` name
resolves as a builtin fallback AFTER user declarations. The bind-ty:Named
wall collapsed with it. Mutant 021 (constructor tags shifted).

**Burn-up: 256 → 263 / 590**, floor 263, surface 117, zero divergence.
Next in the arc: value.object + MapLiteral (×13), then json.stringify
(×8, port the serializer), then json.parse (×12, port the parser).

---

## Unit 6 — stage 30: map literals + value.object (2026-08-20)

`["k": v, …]` lowers by SYNTHESIS: the emitter builds the pairs-list IR
and delegates to the same insertion-ordered upsert `map.from_list` runs
— duplicate keys keep the interp's last-write-wins for free. Objects are
tag-6 Values whose payload is the (String, Value) pairs list itself:
insertion order IS the block, the interp's ordered-object model.

One divergence caught and fixed before landing: as_string's err line is
"expected Str" (the impl's exact text), not "expected String" — the gate
flagged the newly-claimed C-108 fixture on the first run. Mutant 022
(literal entries reversed — insertion order flips).

**Burn-up: 263 → 265 / 590**, floor 265, surface 118, zero divergence.

---

## Stage 30 addendum: the observer-less mutant (2026-08-20)

Mutant 022 (map-literal entries dropped/reversed) survived BOTH nets — and
the investigation shows why no tooth can exist yet: the parity corpus's
MapLiteral fixtures all sit behind deeper walls, and the FUZZ oracle
ABSTAINS on map programs (#1226 interp heap-bridge debt — forcing map
observers into the generator just moved 14 programs into the
ORACLE-ABSTAINED class, compared 168→155). An unobservable mechanism must
not carry a fake mutant: 022 is WITHDRAWN, the generator additions
reverted, and the gap recorded as an ORACLE debt (the interp's map
abstention), not a backend one. The literal synthesis path itself reuses
`map.from_list`'s upsert, which claimed fixtures do exercise. When the
MapLiteral fixtures' remaining walls fall, the mutant returns WITH its
observers.

---

## Unit 6 — stage 31: the native JSON serializer (2026-08-20)

`json.stringify`/`value.stringify` land as PER-PROGRAM emitted helpers
(`$vjson` + `$vjson_quote`, recursive, assembled right after main with
their indices promised during lowering): tag switch over the rebuilt
Value blocks, the incumbent's exact surface — 5-escape quoting (\\ \" \n
\r \t, no \u escapes), separators without spaces, floats through the
LINKED float.to_string with any trailing ".0" stripped (the `{}` form:
2.0→"2", 2.5→"2.5"), unknown tags render "null". Build runs over the
line buffer under the nested-cursor discipline and captures via
buf_to_block. Probe end-to-end:
`{"name":"a\"b\\c","ns":[1,2],"f":2,"g":2.5,"t":true,"z":null}`.
The stringify mutant waits for its observers (only one fixture claims so
far — the 022 doctrine), arriving with value.field/keys/merge.

**Burn-up: 265 → 266 / 590**, floor 266, zero divergence.

---

## Unit 6 — stage 32: value.field/keys + the parser links (2026-08-20)

`value.field` (the Codec-derive accessor: tag check → first-match scan →
the exact err lines, missing-field message built at runtime via concat)
and `value.keys` land as emitted helpers. `json_parse` joins the
sum-builder tier on a decisive audit: its raw ops build its OWN string
buffers (layout-shared) and every Value comes through the public value.*
surface — which THIS emitter now lowers natively, so the whole 387-line
recursive-descent parser is layout-consistent by construction. Fixture
count unmoved (+0): the remaining json chains block deeper (Codec derive
bodies, value.merge, fs) — mechanisms first, the claims follow.

**Burn-up: 266 / 590 held**, zero divergence.

---

## Unit 0 — re-based onto the almide/als mount (2026-08-20)

The judge left the tree. `almide/als` (https://github.com/almide/als) was
extracted from `almide@53e2a2ab7` (develop) with `git filter-repo` — 1,321
commits of history over the judge-owned paths — and is mounted here as the
submodule `als/`, pinned by commit (R6, ARCHITECTURE.md §6).

- **Deleted from this tree (now read from the mount):** `spec/lang`,
  `spec/stdlib`, `spec/integration`, `spec/programs`, `spec/wasm_cross`,
  `spec/wasm_cross_pkg`, `spec/wasm_fail`, `docs/contracts/`, `docs/specs/als/`,
  `scripts/check-contracts.sh`, `scripts/lib/contract-classes.txt`.
  **Kept (implementation-resident):** `spec/churn`, `spec/pass_isolated`.
- **Single indirection:** `crates/almide-corpus` (greenfield-authored, strict
  tier) resolves a corpus-relative path to this tree first, then `als/`, and
  walks both roots as a partition (a path in both panics). The parity tests
  keep handing the parser/checker/interpreter the corpus-relative name, so
  every oracle hash is unchanged.
- **Generators** (`gen-ast-manifest.sh`, `gen-check-manifest.sh`,
  `gen-run-manifest.sh`) run the a877d2138 oracle with cwd = the fixture's
  root. Regenerated: ast 1,095 → **1,099** manifest rows (+4, 3 exclusions
  unchanged), check 1,095 → **1,099** (+4, 3 exclusions unchanged), run
  590 → **591** (+1: `effect_tco_err_rewrap`; exclusions 1 → 4). Every
  pre-existing row is byte-identical — the diff is exactly the four fixtures
  the judge gained between a877d2138 and 53e2a2ab7.
- **What the four do at the port SHA (run parity/burn-up):** three of them
  have NO referee in the a877d2138 oracle and sit in the new shrink-only
  register `scripts/lib/run-oracle-exclusions.txt` (merged into the
  exclusions by the generator, re-entering as the port catches up):
  `map_literal_ctor_values` and `map_upsert_str` — the oracle's wasm leg
  WALLS at build (exit 1, empty stdout: a refusal, not a run; its interpreter
  is #1226-unsupported), and the greenfield backend ALREADY emits and runs
  `map_literal_ctor_values`, so judging against the wall row would punish
  being ahead of the oracle; `record_default_field_omitted` — the a877d2138
  interpreter aborts (`internal: no field \`tag\` on record`) where its wasm
  leg prints. The fourth, `effect_tco_err_rewrap`, IS refereed (oracle runs
  it, exit 0) and the verbatim interpreter spins to fuel exhaustion on it —
  the contract pins a TCO on the err-rewrap path the a877d2138 interpreter
  does not perform: fuel ceiling 1 → **2**, the pin advance's one ceiling
  raise. Rule (same as the deviation register): a pin advance may raise a
  ceiling by exactly what it brings, each named here; otherwise ceilings only
  lower.
- **Port gate** now runs `als/scripts/check-contracts.sh --impl-root "$PWD"`
  (the judge's gate in two-repo mode: judge evidence required inside the
  mount, implementation evidence required here). Verdict: 42 forward-reference
  findings, all registered, 0 unexplained — GREEN.
- **Deviation register:** 50 → **51**. The pin advance brought exactly one new
  forward reference — C-300 cites `tests/heap_cap_test.rs` (unit 7). The
  shrink-only rule gains its one sanctioned exception (gate header): a pin
  advance may raise the ceiling by precisely the references it brings, each
  named here. Outside a pin advance the ceiling still only lowers.
- **Oracle and judge are two pins.** The port-SHA oracle stays a877d2138 (the
  code being ported); the judge pin is als@main at the time of this entry.
  Advancing the judge is a reviewed commit of its own.
- **Incumbent `develop` untouched** — it keeps its copies until its own cutover
  is decided (user decision 2026-08-20: Stage B for greenfield only).

---

## Unit 6 — stage 33: `?` joins `!`, and the parser's last dependency (2026-08-20)

The oracle dispatches `Try | Unwrap` to ONE eval arm — so does the
emitter now: `?` gets the same three-shape lowering `!` has (propagate /
main-abort / pure-abort, C-216 identity included). `string_from_codepoint`
(the JSON parser's \uXXXX dependency — UTF-8 encoding into its own fresh
buffer, layout-shared writes only) joins tier 1. The json.parse wall fell
with it.

**Burn-up: 266 → 269 / 590**, floor 269, zero divergence.

---

## Unit 6 — stage 34: map.fold (2026-08-20)

Fold over entries in insertion order — the (acc, k, v) callback inlines
through the same HOF machinery, the walk reads entries absolutely at
their packed offsets. The incumbent's pinned heap-accumulator wall does
not bind this backend; the manifest hash judges.

**Burn-up: 269 → 270 / 590**, floor 270, zero divergence.

---

## Unit 6 — stage 35: native string.split, judged mid-slice (2026-08-20)

The incumbent's string_split builds List[String] with 8-byte slots (the
real coupling class) — so split lands NATIVE: a `$split` helper, Rust
semantics (byte-level full-separator match, non-overlapping, empty
pieces kept, count = separators + 1), two passes (count → alloc+fill,
each piece a fresh owned string). The corpus judged the first version
mid-slice: `split_empty_sep` (C-100) rejects the empty-separator trap —
Rust's char-boundary split (leading "" + each CHAR, multibyte whole, +
trailing "") is IN contract, and now emitted as a dedicated path off the
same helper. A latent per-helper type bug fell out too: the helper
assembly gave every helper one (i32,i32)→i32 type — ValueKeys takes one
param; types are per-helper now. Mutant 024 (final piece mis-based).

First slice landed on the als-mounted corpus: **denominator 590 → 591**.

**Burn-up: 270 → 272 / 591**, floor 272, surface 126, zero divergence.

---

## Unit 6 — stage 36: the gleanings batch (2026-08-20)

Four small honest walls fall together: statement-position `f()!` / `f()?`
(the marker machinery runs — propagation/abort — and the ok payload
drops; the ×14 expr:Try histogram entry was THIS, not a marker gap),
`{}` empty-map literals (a zero-entry map block), `option.unwrap_or` /
`result.unwrap_or` as module surfaces (the `??` machinery verbatim), and
`list.first` (= get(xs, 0), the same Option-returning helper). Mutant
025 (unwrap_or's null test inverted).

**Burn-up: 272 → 278 / 591**, floor 278, surface 131, zero divergence.

---

## Unit 6 — stage 37: the display engine (2026-08-20)

`${part}` grows from five scalar arms into ONE recursive display engine
over the type shape (emit-time recursion, depth-capped — runtime-
recursive data displays are a queued helper): the oracle's exact forms
for records (`Nm { f: v }`), variants (`Case(v)` / bare unit names),
tuples, Options (`some(v)`/`none`), Results (`ok(v)`/`err(v)`), lists
(`[a, b]`), with Rust-Debug string quoting in NESTED positions sharing
the JSON 5-escape walker. Three oracle rulings landed mid-slice:
anonymous records print as `{ f: v }` with NO name-space, an INFERRED
structural record that matches a DECLARED record IS that type (name and
declared field order — r5's contract), and truly-anonymous records
display their fields in NAME order. Mutant 026 (record separator
shortened).

**Burn-up: 278 → 285 / 591**, floor 285, zero divergence.

---

## Unit 6 — stage 37 hotfix: the fuzzer's THIRD real catch (2026-08-20)

CI red on the display-engine landing: 21 seeds showed a NESTED
interpolation (`${"${a}~${b}"}`) printed TWICE when followed by more
appends in the same line. Root cause: the value-position StringInterp
capture restored the GLOBAL line cursor but left the shared cursor LOCAL
at the inner build's end — the old per-part stack discipline had
protected the outer cursor on the operand stack, and the display-engine
refactor removed that push. The corpus never nests an interp beside
further parts, so the burn-up stayed green; the fuzzer caught it within
one CI cycle. Fix: restore the cursor local to the captured region's
start alongside the global. Process lesson re-learned the hard way: the
amend cycle skipped the full-net rerun — landing checklist is FULL nets
after ANY amendment, no exceptions. V-5 permanence: the reduced case
goes to almide/als as a PR (the two-repo rule), pin bump to follow.

Fuzz 146→167 compared, zero findings; parity 285/591 unchanged.

---

## Unit 6 — stage 38: the matrix floor opens (2026-08-20)

Ty::Matrix lands as a FLAT block — payload `[rows:i32][cols:i32][f64
row-major]`, no row-pointer array (the native Vec-of-rows shape is an
implementation detail; the 2026-08-10 fuzz night's OOM-by-row-headers is
exactly why the dims rule bounds the ROW count alone). Stage-1 ops:
zeros / ones / shape / rows / cols with `almide_rt_matrix_dims`
transcribed verbatim — negative dims clamp to 0 (C-034/C-161), then
`r > 2^28 || r*c > 2^28` aborts in the T6 form ("Error: matrix
dimensions too large" + exit 1) before any allocation. The select-order
clamp bug (negative KEPT the dimension) was caught by the guard fixtures
on the first parity run. Mutant 027 (the row-alone bound dropped — the
exact class the fuzz night found). The greenfield interp abstains on
matrix arithmetic, so the manifest rows are the arc's only oracle —
noted for the heavy ops ahead (bit-exact f32 accumulation, quant
schedules; a dedicated-session arc).

**Burn-up: 285 → 287 / 591**, floor 287, surface 135, zero divergence.

---

## Unit 6 — stage 39: codopsy A restored (2026-08-20)

The burn let the crate slip to B(79) — three files over 800 lines,
lower_call_at at cc 72 — caught by the user's standing check-in, not by a
gate: measurement was manual and skipped during the run. Restoration by
the stage-5 playbook (mechanical splits, behavior judged by the FULL nets
after): calls.rs → display.rs/list.rs + a thirds-split module dispatch;
emitter.rs → data.rs/stmts.rs/equality.rs + FnRef/Lambda/IndexAssign
extractions; lib.rs → work.rs/ty.rs/func.rs/assembly.rs +
collect_program_fns/build_globals/resolve_extras phases; prim float
family split; the fuzz generator's expr dispatcher split per type.
**B(79) → A(90)**, zero behavior change (full parity 287/591 + fuzz +
alias + tail + surface all green, clippy 0). Two surgery lessons for the
tooling: brace-tracking must strip STRING LITERALS before counting, and
`rindex("}")` splices must land inside the right impl block.

---

## Unit 6 — stage 40: mutation-fleet repair + matrix stage 2 (2026-08-21)

**CI red from stage 39, and the mechanism that ends the class.** The
eleven-module split stranded 8 mutant patches (003/004/010/012/014/016/
017/020 — their context lines moved to data/stmts/list/equality/work/
display); CI's full sweep caught it, the local landing did not, because
the amend cycle skipped the gate entirely — the same lesson as fuzz
catch #3, re-learned on a different net. Three fixes, one of them
structural: (1) all 8 patches refreshed against the split layout, same
semantics; (2) the incremental scope now treats a CHANGED PATCH FILE as
in scope even when its target file is not in the diff; (3) a pre-push
hook (shared hooks dir, greenfield-refs-only) runs the incremental gate
on every push — a checklist can be skipped, a hook cannot.

**Matrix stage 2** — the index domain + list bridge, semantics verbatim
from runtime/rs: `get` aborts out-of-range in the unified T6 form,
i64-compared before any cast (C-282, both halves); `row_dot`/`dot_row`
answer the empty-sum identity 0.0 out of range (the accessor/reduction
split), sequential mul-then-add, bit-exact; `from_lists` takes cols from
the FIRST row (native from_iter), short rows zero-fill; `to_lists` emits
fresh row blocks; `transpose` is a pure permutation, either-dim-zero →
the (0,0) matrix. Constructors now normalize rows==0 → cols=0 — the
stage-1 header kept the clamped cols and `cols(zeros(0, 9))` would have
answered 9 against native's 0 (latent: no claimed fixture reached it).
New mutant 028 pins `get`'s col bound through the wasm_fail row.

**Burn-up: 287 → 289 / 591**, floor 289, surface 139, zero divergence.

---

## Unit 6 — stage 41: bound ranges + repeat's copy wall + fuzz catch #4 (2026-08-21)

**Bound ranges (C-238), the design that deletes the analysis debt.** The
front end types `let a = 0..<4` as `List[Int]` with the Range expr as
initializer, so value-position Range now MATERIALIZES the real block —
span by native list_range's `saturating_sub().max(0)` (the saturation is
genuine i64-overflow detection: `(i64::MIN)..<3` is the C-197 die, not an
empty list), with "Error: out of memory" + exit 1 past the wasm leg's own
structural bound (success between the two legs' bounds is the contracted
divergence). Head-only binds never materialize: ranges.rs scans the fn
body (exhaustive-match visitor — a miss disqualifies toward the SAFE
side, materialization) and every for-in over a deferred var counts
between two i64 locals — the 4294967295-iteration cell passes by
construction.

**Fuzz catch #4 — the new range-bind arm bit the INTERP.** Three seeds
(47/83/89) found `[1, 2] + r` aborting "internal: list concat" in the
reference evaluator while native 0.58.0 and this backend both answer the
materialized list. ConcatList now takes List|Range operands through
as_iter_items (over-cap abstains like index/len — C-197 territory).
The A/B against the released binary settled who was wrong (the memory
rule exists for exactly this).

**list.repeat's copy wall.** Routing repeat to the VERIFIED linked impl
failed the C-169 boundary cell: the impl BINDS its buffer, the bind
deep-copy doubles the footprint, and 2×2^31 bytes cannot exist in
wasm32 — a raw trap where the contract requires success. Root-caused by
probe (harness now surfaces swallowed traps under ALMIDE_DBG_TRAP; its
host reads also zero-extend ptr/len — i32 sign-extension broke reads
past 2 GiB). repeat is now a native one-allocation fill, semantics
verbatim from list_make.almd (die > 2^28 slots, negative clamps empty).
int.to_float is f64.convert_i64_s (IS Rust's `as f64`).

**Verification honesty note**: the interim "297 green" I reported to
myself was a grep that filtered the burn-up line and DROPPED the test
verdict — the run was already red on the boundary cell. Full-line
checking is now part of the drill.

**Burn-up: 289 → 299 / 591**, floor 299, surface 141, zero divergence,
workspace suite 0 failures.

---

## Unit 6 — stage 42: the option/result combinator matrix (2026-08-21)

The API-family doctrine applied to sums: every INTRINSIC cell of the
option/result surface lands in one module (sums.rs) — result
is_ok/is_err/map/map_err/flat_map/unwrap_or_else/to_option/to_err_option,
option is_some/is_none/map/flat_map/flatten/unwrap_or_else/or_else/
filter/zip/to_list — while the source-level cells (flatten, to_list, zip
on the result side, …) keep compiling from their stdlib match bodies
through the linked path (probed working before writing a line).
Callbacks are the literal-lambda hof_lambda idiom; pass-through sides
REUSE the subject block (sums are never mutated in place). Result types
come from the lambda BODY's IR type, not the ret hint, so nested
combinator chains type without context. string.join is list.join spelled
the other way (same F_LIST_JOIN). 13/15 cells probe-matched the interp;
flatten/zip (interp abstains) A/B'd against native 0.58 byte-for-byte.
result.partition stays an honest wall for now.

**Burn-up: 299 → 308 / 591**, floor 308, zero divergence.

---

## Unit 6 — stage 43: string.replace, sort, chunk/windows, with_capacity (2026-08-21)

Two new fixed helpers (types reused, so only F_FN_BASE moved 28→30):
$str_replace carries Rust str::replace/replace_first byte-for-byte —
the C-100 empty-pattern rule (`to` at every CHAR boundary, a leading
`to`, multibyte chars whole; replace_first("") = to ++ s) plus the
count-pass/fill-pass general form. $str_cmp is byte-lexicographic with
length tiebreak (String: Ord), feeding list.sort — an insertion sort
over a FRESH copy (stable; for scalars any correct sort is
value-identical to native `v.sort()`); Float order is IEEE totalOrder
via the bits ^ ((bits >>s 63) >>u 1) key compared signed, so
`-0.0 < 0.0` and the NaN positions hold. chunk keeps the
ceiling-division-without-the-overflow-trick form and the v0
negative-n one-chunk reading; windows: n > len (negatives included) →
empty; both die "…size must be positive" on 0. with_capacity is the
empty list (capacity is a hint — native clamps it; ret_hint now flows
into lower_list_call). The select-operand footgun struck TWICE in
chunk (probe caught garbage reads immediately) — mutant 031 pins the
exact inversion permanently.

**Burn-up: 308 → 319 / 591**, floor 319, zero divergence, workspace 0
failures.

---

## Unit 6 — stage 44: the Try routing hole + effect TCO see-through (2026-08-21)

The ×15 "expr:Try" wall was ONE MISSING LINE: lower_sum has handled
`Try | Unwrap` in a single arm since the effect-ABI stage, but the
routing predicate `is_sum_shape` listed only Unwrap — every `?` fell to
the generic refusal. Routing it exposed a real divergence the burn-up
caught at once: effect_tco (C-069) traps at depth ~1e5 because
`Try{Call self}` in tail position evaluated the call on O(n) stack.

The fix is the see-through the old comment had parked ("the
`f()!`-in-tail peephole is a later slice"): the effect body now lowers
WITH the tail marker — a RAW-typed tail call stays a plain call (the
Named arm's ret == fn_ret guard), while `f(…)!` whose callee's WASM
Result type equals the frame's return_calls directly:
propagate-err-or-rewrap-ok on an identical Result is the identity, so
O(1) stack (1e6/2e6-deep fixtures pass, value-exact). The callee's
Result type comes from the TABLE, not the IR node (the IR `ty` is the
raw ok type — the effect ABI is a backend layer; the first guard
compared against it and never fired). No mutant this stage: the
behavior pin is effect_tco's own stack-depth cell, which is exactly
what caught the miss.

**Burn-up: 319 → 330 / 591**, floor 330, zero divergence, workspace 0
failures.

---

## Unit 6 — stage 45: find/min/max/sort_by, string.take, PowInt (2026-08-21)

Gleaning sweep three. list.find is filter with an early break into a
some-block. min/max run the scalar three orders (Int/Str Ord, Float
totalOrder via the sort key transform) with first-wins ties — scalar
ties are value-identical, so unobservable. sort_by evaluates the key
ONCE per element into a parallel array (#560: per-comparison keys were
an observable divergence for side-effectful keys) and the list.sort
insertion sort moves keys and values in lockstep. string.take is
chars().take verbatim — a NEGATIVE n takes the WHOLE string
(deliberately not the C-054 clamp; cp_off clamps past-end). `^` is the
oracle's wrapping square-multiply with the negative-exponent die.
All eight probes matched the interp byte-for-byte, stderr included.

**Burn-up: 330 → 340 / 591**, floor 340, zero divergence, workspace 0
failures.

---

## Unit 6 — stage 46: mutation write-backs, Result equality, map pairs (2026-08-21)

The place-mutation statement family lands as COPY-ON-WRITE write-backs
(the doctrine that keeps in-place mutation unobservable): FieldAssign
copies the record block, replaces one slot, rebinds; MapInsert
(`m[k] = v`) reuses the functional `set` the `map.insert` mut form
already runs; list.pop yields some(last)/none and rebinds the shrunken
copy; list.clear rebinds empty. `m[k]` in EXPRESSION position is
exactly map.get (a miss is `none`, the interp's map_lookup contract).
Result `==` joins emit_val_eq: tags agree, then the ACTIVE side's
payload compares recursively. map.entries walks the insertion-ordered
entry region into fresh pair blocks. list.is_empty is len == 0;
string.push is the concat write-back — the last link in alias_cow's
chain, which then also proved mutant 033 (FieldAssign's copy dropped)
is observable: the first 033 generation SURVIVED because no claimed
fixture aliased a record across the assign — the observer-first rule
from 032, re-learned the same day, now twice underlined.

**Burn-up: 340 → 353 / 591**, floor 353, zero divergence, workspace 0
failures.

---

## Unit 6 — stage 47: runtime-recursive display (2026-08-21)

The display engine's depth counter is GONE, replaced by what it was
standing in for: emit-time inlining now follows the type shape with a
PATH of Named types, and a CYCLE — a genuinely recursive type — is cut
with a call to a per-type `(block, cursor) -> cursor` helper, built by
the same Emitter machinery in a fixed-point phase after each fn lowers
(a body may register more helpers; mutual recursion just works — each
body starts its path at its own type and calls the sibling's promised
index). Non-recursive shapes inline at ANY depth (finite DAG). A body
that fails to build refuses the REGISTERING fn — per-fn granularity
survives — is marked Failed so later callers refuse themselves, and
assembly stubs the promised index loud. Self-recursive lists, trees,
and mutually recursive enums probe-match the interp exactly. int.max /
int.min came along (one select each).

**Burn-up: 353 → 355 / 591**, floor 355, zero divergence, workspace 0
failures.

---

## Unit 6 — stage 48: the deterministic meter (ALS-DT2/DT3) (2026-08-21)

The fan/fuel wall — the burn's largest — falls in one landing, because
the design work was READING, not inventing: the frontend already
desugars time algebra to plain saturating Int IR and brackets regions
with exactly three prims, and the interp's det_* cells ARE the spec.
The wasm leg mirrors them cell for cell: five G_DET_* globals; enter
divides by CM-1 (3ns/unit), min-caps EIP-150 style and RETURNS the
saved fuel (the IR threads it to exit); exit computes verdict/spend and
deducts the region's consumption from the restored outer fuel. Charges:
one unit per loop-head CHECK (n iterations = n+1) on all four loop
shapes, one per non-exempt user-fn entry (the exempt analysis mirrors
det_entry_exempt: loop-free AND not self-reaching through user-fn
edges, SIMPLE-name keyed — deliberately bug-compatible), one per
closure hop, and the T3-5 dynamic concat charge (1 + len/16). Pool
bodies and synthesized helpers never charge. The CUT mirrors
Flow::Return(Int(0)): the current fn returns a zero-shaped value at a
charge site and callers CONTINUE to their own next charge site —
charge sites are identical across legs, so the cut point is identical
by construction, boundary-exact (3006ns ok / 3005ns exhaust pins).

A probe detour worth recording: fourteen "MISMATCH" verdicts were my
probe hashing without the normalization's re-appended newline; the
a877 oracle was rebuilt and the manifest regenerated to prove the
committed goldens byte-identical before the real parity run said 371.

**Burn-up: 355 → 371 / 591**, floor 371, zero divergence, workspace 0
failures.

---

## Unit 6 — stage 49: the fan combinators are sequential (2026-08-21)

fan.map / fan.any / fan.settle / `fan { }` land as what the
deterministic model says they ARE: sequential traversals in LIST ORDER
(fan.race was an E027 tombstone precisely because the model has no
race). Semantics verbatim from the self-hosted stdlib bodies and the
interp: map collects oks and the FIRST err IS the result; any takes the
first Ok (an element's err skips it; all-fail and empty are the
ledger-constant Err; the block form statically unrolls its literal
thunk list, and a PURE arm Ok-adapts and wins on the spot); the
`fan { }` block runs EVERY arm, then aborts with the FIRST err's bare
message (the interp's run-all-then-abort order), one arm bare / many a
tuple. Tuple patterns now compose through the nested machinery, so
`(ok(a), err(e))` destructures — the fan.settle shape.

**Burn-up: 371 → 383 / 591**, floor 383, zero divergence, workspace 0
failures.

---

## Unit 6 — stage 50: the fs host boundary (2026-08-21)

fs crosses ONE generic import — `almide.fs_call(op, a, b) -> i64`
(status in the high half, len/flag in the low) plus `almide.host_read`
pulling parked result bytes — and the HOST (the harness) runs the SAME
std::fs code the native runtime runs, io_err = Display included, so
error strings ("No such file or directory (os error 2)") match
verbatim by construction. Twelve ops land: read_text / write /
write_bytes (the List[Int] payload crosses raw; the host takes the low
byte per native `x as u8`) / write_bytes_raw / exists / is_dir /
is_file / mkdir_p / remove / remove_all / create_temp_dir (host-side
verbatim, temp paths never printed by the corpus) / list_dir (sorted) /
read_lines / read_text_if_exists (status 2 = ok-none) / read_bytes /
append. List-of-strings results travel as u32-LE length-prefixed
frames; fold_lines / for_each_line run GUEST-side over the frames —
observably identical to native's streaming. One probe catch: the frame
walker peeked hold indices by depth arithmetic and a caller's extra
hold shifted them — indices are now passed explicitly (the probe showed
garbage lengths immediately). fan_settle_tuple completes its chain.
list.take and String `<`/`>` comparisons (via $str_cmp) land as
groundwork; tail_calls' mini-harness stubs the new imports.

**Burn-up: 383 → 388 / 591**, floor 388, zero divergence, workspace 0
failures.

---

## Unit 6 — stage 51: aviation discipline restored AND mechanized (2026-08-21)

The user's check-in caught the same slippage class as stage 39: ten
stages of burn had pushed four files past the 800-line discipline and
codopsy to B(84), with lower_list_call at cc 79. Restoration by the
same playbook — list.rs → list_order.rs + list_mut.rs (dispatcher
falls through family-wise), emitter.rs → binop.rs + the Range value
arm joining ranges.rs, runtime.rs → runtime_str.rs, resolve_extras
home to assembly.rs, the harness fs_dispatch split read/write, the
fuzz generator's grown arms to an included file — all behavior-proven
by the full nets at every step (B84 → 86 → 88 → 89 → A 90, 34 files
all under 800, clippy 0).

The difference from stage 39: the failure is now MECHANIZED away.
scripts/check-file-discipline.sh (the 800-line cap — the measured
primary driver of both slippages) runs inside the pre-push hook, so
file growth can no longer reach a push unmeasured. Surgery lessons
banked: multi-line fn signatures need seen-open depth counting; a
`{ pat }` in a match GUARD balances on its own line (count from the
`=>`); include! files in tests/ must live in a subdirectory or cargo
compiles them as their own test target.

---

## Unit 6 — stage 52: the first performance truths (2026-08-21)

The world-best claim got its first NUMBERS. Six kernels, greenfield
wasm (emit once, five runs, best) vs the a877 oracle's wasm leg
(end-to-end minus its ~84ms baseline), all six outputs byte-identical:

| kernel        | before      | oracle≈ | after   |
|---------------|-------------|---------|---------|
| int_loop 30M  | 107ms       | ~84ms   | ~106ms  |
| float_math    | 45ms        | ~24ms   | ~45ms   |
| str_build 3M  | 130ms       | ~59ms   | ~125ms  |
| list_sort     | **608ms**   | ~23ms   | **15ms**|
| recursion     | 126ms       | ~86ms   | ~122ms  |
| list_pipeline | 30ms        | ~8ms    | ~28ms   |

Three fixes the numbers forced, landed with full-net invariance:
merge sort replaces insertion sort (26x behind → now AHEAD of the
oracle on the same kernel); list.filter builds in ONE upper-bound
allocation with a final len/cap rewrite (the push-per-kept form
re-copied the block per element); and the deterministic meter ELIDES
entirely for programs with no region prims — charges are unobservable
without a region, so ordinary programs stop paying the per-iteration
loop-head cost. Plus one correctness-grade item off the "cannot claim"
list: the allocator's failed grow now dies in the C-197 form ("Error:
out of memory" + exit 1), never a raw trap.

Honest remainder, recorded not hidden: ~1.3-2x on tight arithmetic
loops and ~2x on string churn against the incumbent (codegen shape +
allocator; next probe targets), pipeline ~3.5x (HOF body inlining is
fine; fold/map dominate). The probe is now a tracked test
(tests/perf_probe.rs, ignored by default) with its kernels in
tests/perf/ — ratio gating once a second checkpoint exists.

---

## Unit 6 — stage 52b: the cross-language truths (2026-08-21)

The second measurement round answers "are we losing to other
languages?" with data. Same kernels, hand-written ideal WAT (the
machine's wasm ceiling) and Zig 0.16 ReleaseFast
(wasm32-freestanding), ALL under the same wasmtime 47, outputs
verified identical:

| kernel        | ideal WAT | zig  | greenfield | incumbent≈ |
|---------------|-----------|------|------------|------------|
| int_loop 30M  | ~95ms     | ~79  | ~100       | ~95        |
| list_sort     | —         | ~13  | ~13-15     | ~23        |
| str_build 3M  | —         | ~26  | ~120       | ~59        |
| list_pipeline | —         | ~9   | ~26        | ~19        |

Three verdicts. (1) On arithmetic we are AT the WAT ceiling — the
earlier "1.3-2x behind the incumbent" was baseline-estimation error
(measure baselines with an empty module, never by subtraction of a
guess); zig's edge over the ceiling itself localizes the one real
arithmetic gap: LLVM strength-reduces the CONSTANT modulus to
multiply-shift where we emit i64.rem_s — a known, bounded
optimization, not a design flaw. (2) list.sort is at PARITY with
zig's pdqsort — yesterday it was 26x behind our own incumbent.
(3) The string/allocation workload is the real front: 2x behind the
incumbent on identical semantics (implementation debt in the
itoa/concat/alloc path) and 4.6x behind zig-with-zero-allocations
(the immutable-string semantics bill — partly mitigable, honestly not
fully). The pipeline reads 1.4x vs the incumbent baseline-corrected,
2.9x vs a hand-fused zig loop.

Next probe targets, in order: the itoa/concat path, then constant-
divisor strength reduction (exactness-critical — lands only with its
own fuzz arm and oracle sweep).

Process note, banked the hard way: a subshell `cd` plus the harness's
cwd reset landed one PORTLOG commit on the MAIN checkout (develop,
the incumbent session's desk) — caught by the foreign-diff rule,
removed without touching their work. Repo-mutating commands now pin
their worktree path explicitly.

---

## Unit 6 — stage 53: the verifier verifies itself (2026-08-21)

The user's challenge — "is this how you aim for world-best?" — named
the real pattern: the PRODUCT has a world-class net, but the OPERATOR
ran on manual discipline, and every recent incident (two codopsy
slips caught only by human check-in, a probe that hashed without the
manifest's normalization, a benchmark verdict skewed by a guessed
baseline, one commit landed on the wrong desk by a cwd reset) is the
same class: an unmechanized process step. Doctrine applied to self,
three mechanizations landed:

1. `aviation-quality` CI job — codopsy A (>= 90) and the 800-line cap
   are RED conditions on every push, not check-in answers.
2. A wrong-desk pre-commit guard in the shared hooks: greenfield-only
   paths staged on any other branch refuse to commit (the cwd-reset
   class dies at the desk, and it cannot bite the develop session).
3. The perf probe measures its OWN empty-module baseline and prints
   work-only numbers — no more guessed subtractions (re-measured:
   float_math work 32ms vs the 33ms hand-WAT ceiling — at ceiling,
   now with a measured control).

---

## Unit 6 — stage 54: the tiny-copy truth (2026-08-21)

The string-front gap localized by micro-bisection (skeleton / concat /
itoa / full — each 3M iterations): the missing ~50ms was wasmtime
lowering `memory.copy` to an out-of-line libcall whose fixed cost
dwarfs a 2-8 byte move. Concat now branches: len < 16 walks bytes,
else one memory.copy — str_build work 121ms → ~68ms, from 2x behind
the incumbent to PARITY (incumbent ≈ 59ms end-to-end, shared-machine
noise band ±15ms). The same rule lands as one shared `$copy(dst, src,
len)` helper routing append_copy, buf_to_block, str_slice, str_repeat,
append_i64, int_to_string, list-push-grow, and block_copy. Geometric
allocator growth (max(needed, current) pages) came along — measured
neutral here (wasmtime grows cheaply) but strictly dominant, and
memory.size is unobservable from the language so the policy is
behavior-free. Two dead-end hypotheses recorded honestly: grow policy
(~0ms) and let-binds (free); the libcall was the whole story.

Full nets green, clippy 0, workspace 0 failures. The remaining fronts
after this: recursion work 129ms (call overhead — the one arithmetic
kernel still 1.5x off ceiling), and the constant-divisor strength
reduction (exactness-critical, needs its own fuzz arm).

---

## Unit 6 — stage 56: self-tail-recursion becomes a loop (2026-08-21)

The recursion front closes to within 8% of the loop ceiling. tco.rs is
a contained peephole over the ENCODED body — depth comes from parsing
truth (wasmparser), not from bookkeeping threaded through twenty
emitter files: wrap the body in one `loop`, rewrite every
`return_call $self` into reverse-order `local.set` of the params plus
`br` to the head. Values, termination, and constant stack are
unchanged (tail_calls' 1e6-deep cell still passes); the win is
wasmtime's per-tail-call overhead on hot self-recursion — the 30M-call
kernel drops 139 → 105ms work against a 97ms pure-loop ceiling
(1.4x → 1.08x). Mutual recursion keeps genuine return_call. One
silent-bail lesson banked: Function::encode writes a code-entry LENGTH
PREFIX — the first wiring parsed it as a locals count and returned
None quietly; the probe that counts per-body self-return_calls is what
caught it.

---

## Unit 6 — stage 58: pipeline fusion (2026-08-21)

The last named perf front falls: `src |> map* |> filter* |> fold` fuses
into ONE pass with zero intermediate lists — the pipeline kernel drops
19 → 3ms work, PAST the hand-fused zig loop (~9ms) it was 2.9x behind
this morning. Soundness is the whole design: the unfused oracle runs
all maps, then all filters, then the fold, so fusion is legal only when
every callback is OBSERVATION-FREE — the scan is conservative (any
Named or Computed call refuses: user fns are opaque and println IS a
Named call; fs/io/http/process/env/random/fan module calls refuse;
RuntimeCall/Fan/nested-Lambda refuse), and any refusal falls back to
the generic staged lowering. Deterministic fuel is symmetric by the
pool rule (HOF internals never charge on either leg; callback charges
are order-free sums). One validator catch banked: a wasm block cannot
receive operands from outside — the element load moved inside the
skip-block the moment the first parity run said "expected i32 but
nothing on stack".

Scorecard after stages 52-58: arithmetic AT the WAT ceiling, sort at
zig parity, strings at incumbent parity, recursion 1.08x of the loop
ceiling, pipeline AHEAD of hand-fused zig. Remaining known gap: the
constant-divisor strength reduction (bounded, exactness-critical,
needs its own fuzz arm).

---

## Unit 6 — stage 59: the fusion boundary gets its own fuzz arm (2026-08-21)

Mutant 039 was killed by the fuzz alone — no claimed fixture exercises
a fused filter — which made the generator the load-bearing observer of
stage 58's soundness. So the boundary now has DEDICATED coverage: a
generator arm that builds map/filter → fold pipelines whose callbacks
are sometimes pure (the fusion path) and sometimes PRINTING (the
refusal path, where the oracle's all-maps-then-all-filters ordering
must survive verbatim). Both sides compare green against the interp
across the fixed seed range.

---

## Unit 6 — stage 60: constant divisors — the measurement is the boss (2026-08-21)

The last named perf item lands as what the NUMBERS chose, not what the
textbook did. The net came first (a fuzz arm hammering literal
divisors of both signs across the edge lattice, MIN included — green
before any change). Then the full Hacker's Delight signed-magic
multiply-shift, with the emitted op sequence mirrored in Rust and an
exhaustive exactness test (±1000 all divisors, plus 999983 / ±2^40 /
±MAX, against ~260 edge dividends). It was EXACT — and a measured
PESSIMIZATION: aarch64 sdiv is fast, cranelift has no mulhi, and the
32-split sequence cost int_loop +60%. Retired honestly. What survives
is what measured: literal divisors drop their GUARDS (a nonzero
non-minus-one constant makes both die-checks provably dead), positive
powers of two take the 4-op shift form, and /1 and %1 fold. int_loop
lands at the hand-WAT ceiling. Two lessons banked: the hold-pool
refusal class (the magic path's four i64 holds overflowed the pool of
4 inside held expressions — one fixture silently became UNSUPPORTED;
the grow-only floor caught it, pool now 8), and "optimization dogma
loses to the host's silicon — keep the net, measure the candidate,
keep only the winner."

---

## Unit 6 — stage 62: the third oracle (2026-08-21)

The 2-way-oracle gap — the first item on the cannot-claim list —
closes structurally: where the reference interpreter abstains (host
fs, over-cap materializations, matrix), the fuzz now referees against
the RELEASED native binary (`ALMIDE_FUZZ_HOST_ORACLE`; CI downloads
the pinned v0.58.0 asset, locally the installed almide). Arm selection
stays deterministic per (seed, mode) — the env is read, never the
clock — and the fs round-trip arm returns under that mode with every
printed observable path-free. First armed run: 122 interp-compared +
45 HOST-compared, zero abstentions, zero divergences — the fs host
boundary, the fuel meter, and the abort forms now have fuzz coverage
with a real referee on both sides of every program.

Org note, owner-adjudicated: 0.59.x stabilizes the incumbent
architecture; 0.60.x onward IS this compiler — the greenfield burn is
the successor line, with the als canonical ledger and the Stage B
cutover aligned to that frame.
