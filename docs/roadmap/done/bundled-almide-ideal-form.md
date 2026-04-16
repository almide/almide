<!-- description: Ideal form for bundled-Almide stdlib: one dispatch path, no patch-layer special cases -->
<!-- done: 2026-04-17 -->
# Bundled-Almide Stdlib — Ideal Form

> **Status**: All 5 debt items closed by the v0.14.7-phase3.N arc
> (S1 → A in `codegen-ideal-form.md §Phase 3 Arc`). See
> `CHANGELOG.md §0.14.7-phase3.5` for the patch-layer audit at close.
> Original narrow-scope rationale preserved below.

The `bundled-almide-dispatch` work (shipped in v0.14.6) made
`stdlib/<m>.almd` usable, but the implementation grew several
patch-layer special cases because the first pass treated bundled fns
as a distinct thing from top-level user fns. The ideal form treats
bundled fns as first-class members of their owning module — no side
paths anywhere in the pipeline. This file catalogues the remaining
debt so we can burn it down over 0.14.7-phase3.

## Guiding principle

> A bundled stdlib fn should be indistinguishable from a top-level user
> fn from the perspective of the type checker, monomorphizer, and
> codegen dispatcher. Only the **TOML runtime mapping** knows whether
> a fn has a Rust/WASM runtime body.

Every remaining bullet below is a place where the current code carries
a "bundled vs not" special case that the ideal form should erase.

## Open items

### 1. Single `dispatch_module_call` entry per target

Today codegen has two places to reach a module fn:

- **Rust**: `pass_stdlib_lowering` rewrites `Module { m, f }` →
  `Named { almide_rt_m_f }` when the fn has a TOML entry, leaves it
  alone otherwise (and the walker renders the leftover as a module
  call).
- **WASM**: `emit_wasm/calls.rs` has a hand-written `match module`
  fanning into `emit_list_call` / `emit_int_call` / etc., with
  a per-module fallback that tries `func_map.get("almide_rt_m_f")`.

Ideal: one entry point per target, with a priority order:
1. IR module has a fn named `f` → user-fn call path (handles bundled).
2. TOML registry has `(m, f)` → existing inline emit.
3. Neither → **compile-time ICE**, not a `stub_call → unreachable`
   at runtime.

Rationale: today a typo like `list.catt(xs)` silently compiles and
traps at the first call. The ideal form turns that into a diagnostic.

### 2. Retire `emit_stub_call → unreachable`

`emit_stub_call_named` in `emit_wasm/calls.rs` is always reachable by
construction (WASM emit has a `_` arm in every module dispatch). Those
fall-throughs exist because the WASM emitter is missing a unified
"unknown fn" ICE path and doesn't want to crash during emission of a
half-valid IR.

After (1) lands, the stub path goes away. WASM emit either succeeds or
returns a compile-time error naming the missing fn.

### 3. `ConcretizeTypes` hard postcondition

`ConcretizeTypes` has a soft audit gated by `ALMIDE_AUDIT_TYPES=1` that
flags remaining unresolved types post-pass (see header comment in
`pass_concretize_types.rs`). Two known sources of leftovers exist today
(Call return types in some `list.zip` cases). Close those, make the
audit a hard postcondition (panic in debug, error in release), and
remove the env var.

Why it matters for bundled-Almide: bundled fns can carry TypeVars into
places the WASM emitter falls back to `i32`, which is the class of bug
that masked the split_at failure during this work. A hard
postcondition catches that class at IR-verify time.

### 4. Unify option/result `.almd` signatures with TOML

`stdlib/option.almd` / `stdlib/result.almd` override the TOML signature
(`fn() -> X` vs `Fn[Unit] -> X`). Removing the .almd files breaks the
test suite because every caller writes `() => x`. See
`option-result-bundled-cleanup.md` for the three paths and the
recommendation (option A: normalize the TOML to `Fn[] -> X`).

This is blocking bullet (5) because while the signature override is
alive, we can't claim "bundled fn = first-class member" honestly —
the type checker is still giving them special power over TOML
signatures.

### 5. First-class mono for module fns — done, verify coverage

v0.14.6 added `monomorphize_module_fns` which handles generic bundled
fns (`list.split_at[T]`, `list.iterate[T]`) on both Rust and WASM.
Remaining work is **coverage**: the current scan is narrow (only
`CallTarget::Module` targeting a bundled stdlib). Widen to:

- `CallTarget::Method` + UFCS forms that eventually lower to Module.
- Bundled fns in user packages (not just stdlib).
- Cross-module generic chains (`list.iterate` inside `stdlib/option.almd`
  when we start writing real bundled fns there).

Add regression tests exercising each widened path.

## Why ship v0.14.6 before fixing all of these

Bullet (5) is done. (1)-(4) are polish that doesn't block the MSR
story — dojo harness writes idiomatic code that the current dispatch
already handles. Releasing 0.14.6 now lets downstream consumers move
from `cargo install --git --branch` to a tagged version while we clean
up incrementally on `develop`.

## Scope

Rough estimates for the 0.14.7-phase3 arc:

- (4) option/result signature normalization: 2-3h
- (1) unified dispatch entry + (2) stub retirement: 3-4h combined
- (3) ConcretizeTypes hard postcondition: 2h (close known gaps + flip
  the switch)
- (5) widening: 1-2h per widening direction, driven by dojo evidence

Total ~10h of concentrated work. Can be broken into ~4 commits.
