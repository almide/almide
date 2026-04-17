<!-- description: Drive stdlib toward a single source-of-truth: `.almd` + multi-target ABI attributes -->
# Stdlib Declarative Unification — Toward a Single Source of Truth

## Current state after v0.14.7

Every stdlib fn is defined in **three** places:

| Layer | File | Role |
|---|---|---|
| Signature + Rust template | `stdlib/defs/<m>.toml` | type checker sees this; pass_stdlib_lowering substitutes `{xs}` / `{f.body}` into the Rust template |
| Rust runtime body | `runtime/rs/src/<m>.rs` | `almide_rt_<m>_<f>(…)` hand-written Rust impl |
| WASM inline emit | `crates/almide-codegen/src/emit_wasm/calls_<m>.rs` | hand-written `wasm!(self.func, { … })` sequence |

25 modules × ~20 fns = ~500 functions, each touched in 3 layers. The
v0.14.7 Phase 3 arc drove the **dispatch layer** to a single source
(`CallTarget` goes to exactly one place on every backend), but the
**definition** layer stays tripled.

Symptoms of the triple-definition model:

- **Drift**: a Rust runtime tweak (e.g. `list.binary_search` bounds
  check) doesn't propagate to WASM until a human notices.
- **Onboarding cost**: adding `list.intercalate` means authoring the
  TOML signature + Rust template + Rust impl + WASM emit. 4 edits for
  one conceptual change.
- **Bundled-Almide can't compete**: `stdlib/list.almd` can extend the
  stdlib with pure-Almide fns (since v0.14.6), but can't *replace* a
  TOML-backed fn because the TOML signature + dispatch path is
  load-bearing.

## Goal

One source of truth per stdlib fn: `stdlib/<m>.almd`. Multi-target
ABIs are encoded as Almide attributes read by codegen.

```almide
// stdlib/list.almd
@pure
@inline_rust("{xs}.iter().cloned().filter(|x| {f.body}).collect()")
@wasm_intrinsic(
  // declarative WASM recipe; emitter compiles directly
  iter_chain(elem=Auto, filter=f)
)
fn filter[T](xs: List[T], f: fn(T) -> Bool) -> List[T] =
  // pure-Almide fallback body (used when no intrinsic for the target)
  list.fold(xs, [], (acc, x) => if f(x) then acc + [x] else acc)
```

Then:

- Codegen reads `stdlib/list.almd`, sees `@inline_rust` → emits the
  template inline (replaces TOML templates).
- Codegen reads `@wasm_intrinsic` → emits WASM bytecode from the
  recipe (replaces `emit_wasm/calls_list.rs`).
- No attribute for a given target → falls back to the Almide body
  (the bundled-dispatch path from v0.14.6).
- `stdlib/defs/*.toml` and `runtime/rs/*.rs` and `emit_wasm/calls_*.rs`
  all disappear.

## Scope per module

Every module currently triple-defined is a step:

- [ ] math (done: v0.14.5, `StdlibOp::FloatUnaryCall` dispatch)
- [ ] string partial (done: v0.14.5, 10 fns via `StdlibOp`)
- [ ] float partial (done: v0.14.5, `to_string`)
- [ ] list — **highest value, high complexity** (closures + iter chains)
- [ ] option / result — generic, shallow
- [ ] bytes — low-level, memory-specific
- [ ] value — dynamic polymorphism, requires tag dispatch
- [ ] map / set — hash-based, closure-heavy
- [ ] int — simple scalar ops
- [ ] datetime / regex / random / io / process / fs / env / http / json /
  testing / base64 / hex / matrix — misc

Each module needs:

- `stdlib/<m>.almd` with pure-Almide bodies (fallback / WASM default)
- `@inline_rust(...)` attributes for Rust fast-path (where hand-written
  template adds value)
- `@wasm_intrinsic(...)` attributes OR rely on pure-Almide body +
  bundled-dispatch (the v0.14.6 path)
- Codegen support for each new attribute recipe
- Regression: Rust + WASM pass spec/ after migration, dojo MSR
  unchanged

## 移行構造 (Stage 1-4)

- **Stage 1** (完了): define attribute syntax + codegen reader.
  `int.to_string` を `stdlib/int.almd` に `@inline_rust` 経由で migrate。
  TOML / runtime / emit は並列維持。
- **Stage 2** (進行中): migrate 5 non-closure modules (int, float, bytes,
  base64, hex). `int` 22 fn 全移行 + `stdlib/defs/int.toml` 削除 完了。
  残り 4 モジュール。Runtime Rust fns (`runtime/rs/src/<m>.rs`) は維持、
  WASM は `calls_<m>.rs` 経由の既存 dispatch を使う。
- **Stage 3**: closure-bearing modules (list, option, result, map, set).
  Requires attribute recipes that encode closure ABI.
- **Stage 4**: effect modules (fs, http, process, io, env, datetime,
  random, regex, json, testing, matrix). These have WASM runtime
  interactions that may need different attribute shapes.

Target release cadence: one stage per `0.14.N` / early `0.15.x`. Full
unification lands in `0.15.0`.

## Technical debt during migration

The migration intentionally leaves a few patch-like seams that need to
be collapsed before Stage 3:

1. **Duplicate bundled-source parse paths.** Both
   `almide-frontend::bundled_sigs` (for `FnSig` lookups) and
   `almide-codegen::pass_stdlib_lowering::parse_bundled_inline_rust`
   (for `@inline_rust` template lookups) parse every bundled
   `stdlib/<m>.almd` source independently. The source string itself
   is shared (`almide_lang::stdlib_info::bundled_source`), but each
   side maintains its own cache and extraction pass. The ideal end
   state: bundled modules are always lowered to `IrModule` during a
   codegen preamble (even for unit tests that bypass `resolve.rs`),
   so both consumers see the same typed IR.

2. **Two `module_functions` APIs.**
   `almide-frontend::stdlib::module_functions` returns the TOML-only
   list (used by the main-crate prune), while `module_functions_all`
   merges bundled. This split mirrors (1): once all fns flow through
   IR, the distinction collapses to a single "every registered fn"
   query.

3. **`@inline_rust` param borrow inference short-circuit.**
   `pass_borrow_inference::infer_function_borrows` forces all-Own
   on fns with `@inline_rust` / `@wasm_intrinsic` because their
   bodies are holes and inferring against the hole would produce
   spurious `RefStr` / `RefSlice` borrows. The template is the sole
   authority for borrow semantics. If we later grow `@pure` pure-
   Almide fallback bodies alongside templates, this rule needs to
   become "use template when chosen, infer from body otherwise".

## MLIR Backend + Egg arc との関係

本 arc は [mlir-backend-adoption.md](./mlir-backend-adoption.md) の **準備運動**でもある。各アウトプットが次 arc (egg + MLIR) でそのまま活用される:

- `stdlib/<m>.almd` の pure Almide body → Almide dialect (MLIR) への入力
- typed intrinsic (`@intrinsic(rust=..., wasm=...)`) → MLIR FunctionImport
- `@rewrite` declarative rule → egg e-graph rewrite rule に自動コンパイル
- `@schedule` block → affine dialect schedule attribute

本 arc を skip して MLIR arc 直行は技術的に可能だが、stdlib が 3 層定義のままだと MLIR 移植工数が 2 倍になる。順序は動かさない。

## Non-goals

- Not changing the TOML *syntax* for hand-written modules during
  migration (ripping them out wholesale would stall).
- Not removing `runtime/rs` while the bundled-dispatch path still needs
  some Rust runtime fns for non-declarable primitives (`alloc`,
  `panic_hook`, etc.) — only the per-stdlib-fn impl files.

## Success signal

- Adding a new stdlib fn (e.g. `list.intercalate`) is a single-file
  change: edit `stdlib/list.almd`, add fn + optional attributes.
- Grep `stdlib/defs/` → empty (or only left-over infra defs).
- `grep -r "emit_list_call" crates/almide-codegen/src/emit_wasm/` →
  single dispatch point reading the attribute recipe; no per-fn match
  arm.

## Scope estimate

Full arc: **2-4 weeks of concentrated work**, split into four releases.
Not a single-session task.

## Decision points before starting

- Attribute syntax (Almide-native `@...` or `#[...]`?) — bikeshed
  small but ecosystem-visible. Suggestion: `@name(args)` matching
  Python / Swift decorator flavor, to stay LLM-familiar.
- WASM intrinsic recipe format — declarative DSL vs embedded WASM?
  Declarative DSL (e.g. `iter_chain(elem=T, filter=f)`) is more
  maintainable but needs an interpretation layer.
- Migration order: simple scalar first (`int`) vs highest-value first
  (`list`)? Probably scalar first for infra proof, then list for
  real payoff.

Start this arc after dojo run #11 baselines v0.14.7; phase4 delta
measurement requires a clean baseline.
