<!-- description: Let stdlib/<module>.almd extend TOML modules (codegen dispatch fix) -->
<!-- done: 2026-04-16 -->
# Bundled-Almide Dispatch for Stdlib Modules

Motivation: dojo run #10 proved `almide fix` retry-loop integration is a
force multiplier. Writing
auto-rewrite rules, helper primitives, and other stdlib extensions in
Almide (vs Rust runtime + TOML templates) would let contributors move
faster — but today, extending `list.*` / `string.*` / etc. via a bundled
`.almd` file does NOT work: codegen emits `almide_rt_<module>_<fn>` for
every stdlib call regardless of source, so a bundled `fn binary_search`
ends up trying to call a non-existent Rust runtime function.

## Current behavior (the bug)

```
stdlib/list.almd: fn bundled_probe() = 42     # Almide source
↓ parser loads file (BUNDLED_MODULES opt-in)
↓ canonicalize: bundled fn is NOT merged into the `list` namespace
↓ lowering: no IrFunction named `bundled_probe` exists in the program
↓ codegen: user code calls `list.bundled_probe()` → rt_list_bundled_probe
↓ rustc: error[E0425]: cannot find function `almide_rt_list_bundled_probe`
```

**Investigation 2026-04-16** (during the "ideal form" pass): adding
`list` to `BUNDLED_MODULES` + `AUTO_IMPORT_BUNDLED` and shipping
`stdlib/list.almd` with a probe fn revealed the problem is not *just*
codegen — bundled fns don't land in the IR's `functions` list at all
when the module name overlaps with a TOML-backed module. The `option`
/ `result` bundled sources work today because the frontend resolution
for Tier-1 modules follows a different path than Tier-2 / TOML
stdlib, and `list` sits on the TOML side.

So the fix is two-layer:

1. **Frontend side**: when a bundled `.almd` file shares a module name
   with a TOML-registered stdlib module, merge its functions into the
   module's public surface (env.fn registrations) so call sites
   resolve to real `IrFunction`s.
2. **Codegen side**: when a `list.<func>` call resolves to a user
   IrFunction (i.e. the function is defined in IR), bypass
   `almide_rt_<module>_<func>` emission and use the normal user-fn
   call path.

Either step alone is insufficient. The roadmap entry was originally
scoped to codegen only; it's now both layers.

## Proposed fix

Make the pass aware of "this fn has a TOML template" vs "this fn is
bundled-Almide". When the fn is bundled-only, skip the `rt_` emission
and fall through to the non-stdlib `Module` call path; the walker then
renders it as a normal function reference which resolves to the
bundled IR function.

Implementation sketch:

```rust
// pass_stdlib_lowering.rs
let is_stdlib = is_stdlib_module(&module);
let is_toml_fn = is_stdlib && toml_module_has(&module, &func);
if !is_toml_fn {
    // Either a non-stdlib module, or a stdlib module whose `func`
    // comes from bundled Almide source. Leave as Module call.
    return leave_as_module_call(...);
}
// rt_ emission (unchanged)
```

The `toml_module_has(module, func)` lookup has to be available to the
codegen crate. `module_functions()` today lives in
`almide-frontend::stdlib` and is built from TOML during the frontend's
build script — codegen has no access.

### Plumbing options (pick one)

1. **Move the TOML-fn inventory to `almide-types`** (or `almide-lang`)
   where both frontend and codegen can read. Requires relocating the
   build-script generation step. Cleanest long-term.

2. **Pass the inventory down as codegen input.** Add a field on
   `CodegenCtx` (or the `Target`-level config) seeded from the
   frontend's `stdlib::module_functions`. Smaller diff, but every
   entry point that kicks off codegen needs to thread it through.

3. **Inline the check via IR**: after the bundled Almide file is
   lowered to IR, each bundled `IrFunction` could carry a
   `source: FunctionSource::{Bundled, User, Extern}` tag. The pass
   then checks "is there an `IrFunction` named `<module>.<func>` with
   `Bundled` tag?" to skip rt_ emission. Cleanest semantically —
   source-of-truth is the IR itself. Requires an IR struct addition.

Recommendation: **option 3**. It makes the IR self-describing (codegen
doesn't need external data), and the check at lowering time becomes
trivial (HashMap lookup on `program.functions`).

## What this unlocks

Once bundled Almide can extend stdlib modules:

- **Auto-rewrite rules in Almide**: the `almide fix` fix functions
  currently live in `src/cli/fix.rs` as Rust code. With bundled
  dispatch working, rewrite rules could be expressed as Almide
  fns that read-and-return AST (once we expose an AST API — a further
  spec). Short-term: even Rust-implemented rewrites benefit if the
  HELPER primitives (`list.binary_search`, `string.run_length_encode`,
  etc.) can ship as Almide instead of TOML + Rust + WASM.
- **Dogfooding tightens**: the `almd-outline` demo (examples/) already
  shows `ide outline`'s formatter can be Almide. Next layer: the
  formatter's stdlib-outline subroutine could use bundled Almide list
  primitives instead of routing through the compiler. Meta.
- **Stdlib WASM becomes free**: TOML + Rust + WASM is the current
  stdlib shape — three places to edit, easy to forget one. Bundled
  Almide is one file that compiles to both Rust and WASM targets
  from the same source.

## Non-goals

- Replacing existing TOML templates. The fast-path intrinsics
  (`try_inline_intrinsic`, `try_lower_to_iter_chain`) are Rust-specific
  optimizations; they stay.
- Removing the `almide_rt_*` runtime. All existing fns continue to use
  it; this change only affects NEW fns added to stdlib modules via
  bundled Almide.

## Testing

After the fix:

1. Add `stdlib/list.almd` with `fn binary_search_v2(xs: List[Int], target: Int) -> Option[Int] = ...`
   (simple dispatch test, doesn't need to be the same as the existing
   TOML version).
2. Add `list` to `BUNDLED_MODULES` and `AUTO_IMPORT_BUNDLED` (already
   present from the earlier failed attempt, can revert or reuse).
3. User code `list.binary_search_v2([1, 3, 5], 3)` must:
   - Type-check clean.
   - `almide run` compile and execute correctly.
   - `almide build --target wasm` produce valid WASM.
4. Existing TOML-backed `list.binary_search` continues to work
   unchanged (regression guard).

## Estimated scope (revised post-investigation)

- **Frontend merge**: 1-2 hours. Identify where bundled .almd fns
  currently fail to merge into TOML-module namespaces. Likely in
  `canonicalize::registration` or `import_table`. The existing
  `option` / `result` bundled pattern is the reference — tracing why
  that works gives the merge point.
- **Codegen bypass**: 1 hour. Once IR has the bundled fn, check
  `program.functions` at `pass_stdlib_lowering` entry for
  `<module>.<func>`-style names; skip rt_ emission when present.
- **Tests**: 1 hour. Regression guard: add a new bundled fn to
  `stdlib/list.almd`, verify it type-checks, runs (Rust target),
  compiles (WASM target), and doesn't break existing TOML-backed
  `list.*` callers.

Total: 3-4 hours in a dedicated session. This is one of the
architectural blockers for `diagnostic-snippet-externalization` and
the general "stdlib-in-Almide" dogfooding story.

No dojo measurement delta expected on landing (infrastructure, not a
diagnostic); downstream work that depends on it will move the needle.

## When to implement

Now (elevated priority per dojo run #10 recommendation: "`almide fix`
が劇的に効いたので、auto-rewrite rules を Almide で書けるようにする
価値が確定"). Block on: user approval of option 3 vs alternative.

## Resolution (2026-04-16)

Implemented in `llm-first-phase2`. The realised design diverged from the
spec in two material ways — both came out of the implementation
investigation:

1. **option 3 (IR `FunctionSource` tag) was NOT taken.** Pre-scanning
   `program.modules` for `is_bundled_module(m.name)` at
   `pass_stdlib_lowering` entry achieves the same self-describing
   property with a smaller diff (no IR struct change, no per-fn lowering
   tag plumbing). Stored in a `thread_local!` cell scoped to the pass
   run. Revisit option 3 if a second consumer (other passes, other
   targets) needs the same lookup.

2. **The spec mis-characterised the option/result baseline.** The
   investigation said option/result bundled fns "work today because the
   frontend resolution for Tier-1 modules follows a different path than
   Tier-2 / TOML stdlib." Re-checking with this fix in place:
   option/result `.almd` sources never produce **codegen output**
   (every fn collides with a TOML entry and gets pruned), but they ARE
   consumed by the type checker and **override the TOML signatures**.
   Specifically, the TOML uses `Fn[Unit] -> X` while the bundled
   `.almd` uses `fn() -> X`, and every real-world caller writes the
   latter. Deleting the bundled `.almd` breaks `coverage_misc_test`
   immediately. The two halves are accidentally co-dependent —
   bundled = signatures, TOML = runtime dispatch. See
   `roadmap/active/option-result-bundled-cleanup.md` for the path to
   un-tangle this.

### Surfaced gaps not in the original spec

- **Verifier** (`almide-ir::verify`) treats `known_module_functions` as
  authoritative. Lowering bundled list to IR caused
  `result.collect`'s call to `list.is_empty` to fail verify (TOML fn
  invisible to verifier). Fix: skip bundled stdlib modules entirely in
  the registry.
- **TOML duplicate prune required.** Bundled `.almd` and TOML defs both
  declare e.g. `option.map`. Lowering both produces two
  `almide_rt_option_map` definitions. The `src/main.rs` lowering loop
  prunes IR fns whose name overlaps `module_functions(name)` from the
  generated stdlib_sigs.

### Files touched

- `crates/almide-types/src/stdlib_info.rs` — `list` ∈ BUNDLED + AUTO_IMPORT
- `crates/almide-frontend/src/stdlib.rs` — `get_bundled_source("list")`
- `stdlib/list.almd` — smoke fn
- `src/main.rs` — bundled lowering pass-through + TOML prune
- `crates/almide-ir/src/verify.rs` — bundled module skip
- `crates/almide-codegen/src/pass_stdlib_lowering.rs` — `BUNDLED_FNS`
- `spec/stdlib/list_bundled_test.almd` — regression guard

### Estimated 3-4h, actual ~3h.
