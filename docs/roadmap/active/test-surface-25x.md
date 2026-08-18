# Test-surface 25× — the road to reference-compiler scale

Set 2026-08-17, after the reference-suite mining sweep (#1508) measured the
gap: rust carries 26,515 test files, swift 13,051, lean4 4,461 against our
~1,000 committed .almd programs + 2,198 `#[test]` fns. Target: **25× today's
committed surface**, grown in the order that buys correctness, not count.

## Baseline (2026-08-17)

| tier | count |
|---|---|
| cross-target differential fixtures (spec/wasm_cross) | 500 |
| other .almd programs (spec/integration, …) | 497 |
| rust `#[test]` fns | 2,198 |
| diagnostics fixtures (broken/fixed pairs) | 99 |
| wall corpus (pinned refusals) | 16 |
| contracts / gates | 291 / 54 |

Meter: `bash scripts/test-surface-meter.sh` (append a dated trend row with
`--update`).

## What counts toward 25× (in priority order)

1. **Negative/diagnostic tests** — the thinnest tier vs rust (99 pairs vs
   their ~15k ui tests). Every E-code wants a fixture family: each hint
   variant, each fix-it verdict, each span shape. Target first: 10× this
   tier before anything else doubles. Parser-floor errors (#1471, #1509,
   #1510) belong here.
2. **Cross-target fixtures** — keep growing by DISTILLATION, not bulk:
   every fuzz finding lands as a fixture (the standing rule), every wall
   that graduates lands with a contract, every reference-suite pattern
   that becomes expressible (or-patterns, string patterns — #1508's list)
   gets its port the release it lands.
3. **Expected-output RC snapshots** (koka parc model): commit the
   drop/dup placement for a corpus of RC-critical shapes alongside the
   runtime result, so benign-today placement moves are loud.
4. **Type×operation matrices** — the nested-container matrix
   (nested_*.almd) generalized: generate the grid (construct / index /
   mutate / iterate / equal / print / drop) × (List/Map/Option/Result/
   tuple/variant nestings), commit the CLEAN cells, pin the walls. The
   generator lives in-repo so the grid regrows when a wall graduates.
5. **The uncommitted tier is already ~∞**: xtarget-fuzz generates and
   differentially checks thousands of programs per run — raw committed
   count UNDERSTATES the effective surface. The committed corpus is for
   regression pinning and review; the fuzzer is for discovery. Both grow;
   only the first is in the 25× number.

## Non-goals

- Bulk-committing machine-generated fixtures that no failure ever
  distilled (suite-runtime cost, zero review value).
- Porting reference tests for features Almide lacks (tracked in #1508;
  they enter with the feature).

## Trend

| date | wasm_cross | other almd | #[test] | diag pairs | walls |
|------|-----------|------------|---------|------------|-------|
| 2026-08-17 | 500 | 497 | 2198 | 99 | 16 |
| 2026-08-18 | 501 | 497 | 2199 | 335 | 16 |
