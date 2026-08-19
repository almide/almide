# Unit 4 stage 1 — the REAL checker behind a per-file query: gate (g) VERDICT

Date: 2026-08-19. Harness: `cargo run --release -p almide-spine --bin s3_bench`.

## What was ported to make this real (all `almide@a877d2138`, verbatim)

`almide-types` (incl. build.rs stdlib embedding + `stdlib/` 309 sources as
build inputs), `almide-lang`, `almide-ir`, `almide-frontend` (25,327 lines:
check, canonicalize, import_table, lower, ir_link, stdlib tables), plus
`src/resolve.rs` + `src/project.rs` into a new root facade crate `almide`
that reproduces the incumbent's re-export map. **The whole ~35k-line stack
compiled with zero source edits** — two Cargo deps (toml, semver) and the
facade were the only additions. One helper (`infer_module_capturing`) is a
verbatim copy out of the incumbent's CLI crate (unportable whole: it sits
beside codegen), attributed in s3.rs.

`check_file_json` (crates/almide-spine/src/s3.rs) reproduces
`resolve_and_typecheck_for_check` (src/cli/check.rs:46-88) faithfully —
resolve → canonicalize → `Checker::from_env` → `refresh_module_toplets`
(#785) → `infer_program` → the #862 module loop — as one memoized per-file
query. Purity contract: only stdlib-only-import files qualify (resolve must
not touch the file system inside a query); the harness prefilters and counts
exclusions.

## Numbers

Corpus: **1,062 qualifying files (53,943 lines)**, 36 excluded with reason.

| measurement | result |
|---|---|
| batch full check, fresh db per round (per-file-compile model: what `almide check` / the test runner pays today) | **6,807 ms**; 100 real diagnostics produced |
| warm re-derive after a 1-file edit | **4.42 ms** (median/30) |
| **(g) warm ≥ 10x batch** | **PASS — 1539x** |
| checks per warm round | max **1**; no-edit sweep **0** (invalidation exact with the real checker) |

The keystroke-latency claim is now measured with the production checker in
the loop: an edit costs one file's full front end + check (~4.4 ms here),
independent of project size — against the incumbent's whole-world,
superlinear (`slope_check 1.144`) re-check.

## Scoreboard across the spike series

S1 (a)(b)(c) + S2a (d)(e)(f) + stage 1 (g)+(a3): **8/8 PASS.** Every §6.5
criterion is green; the "旨み" question is answered with the check phase
included.

## What unit 4 still owes before LANDING (unchanged obligations)

- Corpus **diagnostics parity** vs the oracle (`almide check --json` A/B,
  manifest + exclusions, same pattern as AST parity).
- **Per-decl** granularity inside a file (S2a's firewall shape fused into the
  real checker) — today's stage is per-file.
- Zig-style **incremental diagnostic stability** scenarios.
- E1 wiring: misspelling catalogue + recoverable codes into the
  diagnostic-emitting path.

## Side effect: first shrink of the deviation register

Porting `stdlib/` satisfied 29 registered forward references; the port gate
flagged every one as a stale deviation (designed mechanic, first firing).
Register: 96 → **67**, ceiling lowered accordingly.
