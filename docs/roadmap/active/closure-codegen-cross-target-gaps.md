<!-- description: Cross-target (native vs wasm) closure-codegen divergences found by the adversarial differential sweep — 5 fixed, 2 deep remaining -->
# Closure Codegen Cross-Target Gaps

> Beyond [Closure Architecture v2](closure-architecture-v2.md) (capture *cells*),
> a second adversarial cross-target differential sweep (re-run of the saved
> `p6-cross-target-sweep` workflow: machine-generate diverse closure programs,
> diff `almide run` vs `wasmtime`, completeness critic) found a batch of
> divergences in how closures are **typed / parsed / stored** — orthogonal to the
> capture-cell mechanism. The method: a program that compiles+runs correctly on
> one target but errors / traps / silently differs on the other is a bug, even
> when the source is identical. Reuse the sweep for any capture/codegen change.

Status: **Active** — 5 fixed and landed on `develop` (PR #334); 2 deep gaps
root-caused with a fix plan but NOT yet implemented (they are multi-site changes
with real regression surface — take them one at a time, verify spec 240 + cargo
+ the sweep after each).

## Fixed (PR #334, `develop`)

| Gap | Symptom | Fix |
|-----|---------|-----|
| WASM `string.push` / `string.clear` | no WASM dispatch arm → emitter ICE (native fine) | added arms in `emit_wasm/calls_string.rs` (push via `string_append` + `emit_mutator_writeback`; clear sets len 0) |
| Closure in a tuple → `let (g,_)=…` | native `E0562` (impl-Trait in a binding type) | erase the let-type's nested `Fn` subtree to `_` (`erase_fn_types`, walker) so rustc infers the concrete closure type |
| `fs[0]()` (call a closure indexed out of a list) | parsed as const-generic `fs::<0>()` → native `E0425` / wasm trap | `parser/helpers.rs peek_type_args_call` now requires a `TypeName` token in the brackets — `[0]` is an index, `[Int]` is type-args |
| Typed param `(k: String)=>` on a capture-clone-wrapped closure | param annotation dropped (`move |k|`) → `E0282` | walker Bind annotates the tail lambda's params through the `{ let __cap=…; move|k|… }` block (HOF-arg lambdas stay un-annotated — spliced inline by `render_iter_chain`) |

Verified: spec 240/240, cargo 784/0, +5 cross-target regression tests, parser regression-clean.

## Remaining (deep — pick up next)

### 1. Closure as a uniform value in a Map / anonymous record

```almide
var m: Map[String, () -> Unit] = map.new()
map.insert(m, "a", () => { list.push(acc, 1) })
let g = map.get_or(m, "a", () => {})   // native: error[E0308] mismatched types
```

A `Map[K, Fn]` value (and an anonymous-record field of `Fn` type) must lower to a
single uniform `Rc<dyn Fn>`, with each stored closure boxed. The machinery exists
— `render_type_rc_fn` / the `type_fn_field` template (`Rc<dyn Fn>`), the `RcWrap`
IR node, and `pass_rust_lowering.rs:42` which wraps closure elements — but is
applied ONLY to `List[Fn]` *literal* bindings.

- The current tuple fix (erase `Fn` → `_`) is correct for a TUPLE (each slot has
  an independent inferred type) but WRONG for a Map (one value type for all
  entries → heterogeneous closures can't unify to `_`, hence `E0308`).
- Plan: for `List` / `Map` / `Set` *uniform-element* `Fn` positions, render the
  element/value as `Rc<dyn Fn>` and `RcWrap` the inserted closures AND the
  `map.get_or` default; keep tuples on the `_`-erase path.
- A `type`-declared record already works (struct fields use `render_type_field_fn`);
  only ANONYMOUS records (`{ run: … }`) hit `E0277`, same family.

### 2. IndexAssign of a non-Copy element through a closure

```almide
var xs: List[String] = ["a", "b"]
let f = () => { xs[0] = xs[0] + "!" }   // wasm: trap (native: "a!!")
```

The WASM trap is in the Perceus RC-drop of the OLD element (the trapping function
is the decref/free helper). `List[Int]` (Copy element, no drop) works; `List[String]`
(heap element that needs a drop) traps. The no-closure version AND read-in-closure
both work, so it's the **cell-deref + non-Copy-element-store** combination: the
`IndexAssign` cell deref (`emit_wasm/statements.rs:411`) and the drop-of-old-element
address are not coordinated.

## Fragility / elimination targets (surfaced while fixing the above)

1. `emit_wasm/statements.rs:404` derives list element width from the VALUE's type
   (`byte_size(value.ty)`), not the list's declared element type — drift-prone.
2. Cell-deref is re-implemented per op (Var read / IndexAccess / IndexAssign / each
   mutator) instead of one "deref once → normal ptr" abstraction.
3. ~~Parser `[X]()` ambiguity~~ — eliminated in #334.
4. Closure-boxing covers only `List[Fn]` literals — generalize to every
   Fn-in-uniform-container position (see Remaining #1).
5. `IndexAssign` of a non-Copy element doesn't coordinate Perceus drop with the
   cell deref (see Remaining #2).
