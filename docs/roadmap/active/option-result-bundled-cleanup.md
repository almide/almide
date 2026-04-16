<!-- description: Decide whether option/result bundled .almd should be deleted or made authoritative -->
# Option/Result Bundled `.almd` Cleanup

## Background

`stdlib/option.almd` and `stdlib/result.almd` declare the same fn surface
as `stdlib/defs/option.toml` / `stdlib/defs/result.toml`. Until
`bundled-almide-dispatch` (2026-04-16, see `done/`) the bundled `.almd`
sources were silently discarded — `src/main.rs:338` skipped bundled
lowering whenever `is_stdlib_module(name)` returned true.

After the dispatch fix, bundled stdlib modules ARE lowered to IR, but
the lowering loop now prunes IR fns whose name collides with the TOML
runtime (otherwise `almide_rt_<m>_<f>` is double-defined). So the
`option.almd` body is parsed, lowered, then dropped — no behavior change
versus the pre-fix world, but now the dead-weight is observable.

## The two fns are not equivalent

The TOML+Rust runtime has fns the bundled `.almd` doesn't: e.g.
`option.unwrap_or_else_throw`, `result.collect`. The bundled `.almd` has
fns the TOML doesn't: nothing currently — it's a strict subset.

So today the bundled `.almd` is dead source. It compiles. It does
nothing.

## Two viable resolutions

### A. Delete the bundled `.almd` (-1 layer)

- `stdlib/option.almd`, `stdlib/result.almd` → removed.
- `BUNDLED_MODULES` keeps `args`, `path`, `list`. Drop `option`,
  `result`.
- `AUTO_IMPORT_BUNDLED` keeps `list`. Drop `option`, `result`.
- `get_bundled_source` drops `option`/`result` arms.
- The "bundled-Almide can extend stdlib" story holds for `list` (and
  any future module).

Pros: minimum surface, no dead code. Bundled `.almd` is genuinely
optional — the TOML stays the source of truth for any module that has
a TOML.

Cons: gives up on the dogfooding pitch ("stdlib could be in Almide")
for option/result specifically.

### B. Make bundled `.almd` authoritative for option/result

- Delete `stdlib/defs/option.toml`, `stdlib/defs/result.toml`.
- Delete `runtime/rs/src/option.rs`, `runtime/rs/src/result.rs` (or
  thin them to non-overlapping helpers).
- Bundled `.almd` becomes the single source. Codegen falls through to
  the bundled IR fn via the dispatch fix landed in this branch.

Pros: one source of truth for option/result. Demonstrates the bundled
path on something more interesting than a probe.

Cons: HUGE risk surface. WASM emit (`emit_wasm/calls_option.rs`) has its
own dispatch, `arg_transforms` is regenerated from TOML, the runtime
has fns the `.almd` doesn't (`unwrap_or_else_throw`, `collect`,
`partition`). Migrating without regression requires writing the
missing fns in Almide AND verifying WASM target keeps working. Likely
1-2 weeks of careful work.

## Recommendation

**A in the near term, B as a much later milestone.** Option A is one
small commit and removes a confusing dead code path. Option B is a
worthwhile north-star but only after the bundled-Almide story has
proved itself with `list` (real fns, not just `bundled_probe`).

## Decision needed

User confirms before either is shipped. A could land alongside the
first real bundled-Almide list fn ship.
