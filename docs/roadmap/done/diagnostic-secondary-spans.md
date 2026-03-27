<!-- description: Activate secondary spans showing declaration sites in error messages -->
<!-- done: 2026-03-25 -->
# Diagnostic Secondary Spans

**Completed:** 2026-03-25

## Implementation

Activated secondary spans (displaying declaration sites that caused the error).

### Changes
- Removed `#[allow(dead_code)]` from `with_secondary()`, `at()`, `at_span()`
- Added `fn_decl_spans: HashMap<Sym, (usize, usize)>` to `TypeEnv` to track function declaration positions
- Extended `check/registration.rs` `register_fn_sig()` to pass span information
- Record `Let`/`Var` statement declaration positions in `var_decl_locs` in `check/infer.rs`

### Errors using secondary spans (3 locations)
- **E006** (effect isolation) — shows effect fn definition site
- **E005** (argument type mismatch) — shows function definition site
- **E009** (immutable reassignment) — shows variable declaration site

### JSON diagnostics
- Added `end_col` and `secondary` fields to `to_json()`

## Remaining → [active/diagnostic-end-col.md](../active/diagnostic-end-col.md)
