<!-- description: Let stdlib/<module>.almd extend TOML modules (codegen dispatch fix) -->
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
stdlib/list.almd: fn binary_search(xs, target) = ...   # Almide source
↓ parser/checker: resolves list.binary_search → IrFunction
↓ pass_stdlib_lowering (codegen):
  if is_stdlib_module("list"):
    rt_name = "almide_rt_list_binary_search"    # ← always
↓ rustc: error[E0425]: cannot find function `almide_rt_list_binary_search`
```

The frontend correctly loads the bundled Almide source and produces a
valid IR function for `binary_search`. But `pass_stdlib_lowering.rs:95-110`
hardcodes the `is_stdlib_module(module)` branch to emit runtime-function
names — it has no signal that an individual `func` comes from bundled
Almide instead of TOML.

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

## Estimated scope

- Option 3 implementation: 2-3 hours including IR change, frontend
  tagging pass, codegen check, tests across all three targets.
- No dojo measurement delta expected (this is infrastructure, not a
  diagnostic).

## When to implement

Now (elevated priority per dojo run #10 recommendation: "`almide fix`
が劇的に効いたので、auto-rewrite rules を Almide で書けるようにする
価値が確定"). Block on: user approval of option 3 vs alternative.
