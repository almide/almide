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
