<!-- description: Cross-target (native vs wasm) closure-codegen divergences found by the adversarial differential sweep — all 8 fixed -->
<!-- done: 2026-06-02 -->
# Closure Codegen Cross-Target Gaps

> Beyond [Closure Architecture v2](closure-architecture-v2.md) (capture *cells*),
> a second adversarial cross-target differential sweep (re-run of the saved
> `p6-cross-target-sweep` workflow: machine-generate diverse closure programs,
> diff `almide run` vs `wasmtime`, completeness critic) found a batch of
> divergences in how closures are **typed / parsed / stored** — orthogonal to the
> capture-cell mechanism. The method: a program that compiles+runs correctly on
> one target but errors / traps / silently differs on the other is a bug, even
> when the source is identical. Reuse the sweep for any capture/codegen change.

Status: **Done** — all 8 fixed and landed on `develop`. 5 in PR #334; the 3 deep
ones (map-stored closures, anon-record closure fields, non-Copy-element IndexAssign
through a closure) in the follow-up. spec 240/240, cargo 791/0, and a re-run of the
sweep all clean after each.

## Fixed (PR #334, `develop`)

| Gap | Symptom | Fix |
|-----|---------|-----|
| WASM `string.push` / `string.clear` | no WASM dispatch arm → emitter ICE (native fine) | added arms in `emit_wasm/calls_string.rs` (push via `string_append` + `emit_mutator_writeback`; clear sets len 0) |
| Closure in a tuple → `let (g,_)=…` | native `E0562` (impl-Trait in a binding type) | erase the let-type's nested `Fn` subtree to `_` (`erase_fn_types`, walker) so rustc infers the concrete closure type |
| `fs[0]()` (call a closure indexed out of a list) | parsed as const-generic `fs::<0>()` → native `E0425` / wasm trap | `parser/helpers.rs peek_type_args_call` now requires a `TypeName` token in the brackets — `[0]` is an index, `[Int]` is type-args |
| Typed param `(k: String)=>` on a capture-clone-wrapped closure | param annotation dropped (`move |k|`) → `E0282` | walker Bind annotates the tail lambda's params through the `{ let __cap=…; move|k|… }` block (HOF-arg lambdas stay un-annotated — spliced inline by `render_iter_chain`) |

Verified: spec 240/240, cargo 784/0, +5 cross-target regression tests, parser regression-clean.

## Also fixed (the 3 deep ones)

### Closures stored in a Map → `E0308`

```almide
var m: Map[String, () -> Unit] = map.new()
map.insert(m, "a", () => { list.push(acc, 1) })
let g = map.get_or(m, "a", () => {})   // was: error[E0308] mismatched types
```

Fix: `pass_rust_lowering` now `RcWrap`s the closure arg of `map.insert` / `map.get_or`
on a `Map[K, Fn]`. The map's value type stays the erased `_`, which then infers the
one uniform `Rc<dyn Fn>` from the boxed args — exactly how a `List[Fn]` literal works
(the `_`-infers-from-boxed-elements trick), so no type-rendering change was needed.
A tuple keeps the `_`-erase path (independent slots).

### Closure in an ANONYMOUS record → `E0277`

```almide
let r = { run: () => { list.push(acc, 1) } }
r.run()                                  // was: error[E0277] (closure isn't Debug/PartialEq)
```

Fix: the generic anon-record struct demanded `T: Clone + Debug + PartialEq`; a closure
fails the latter two. Anon records with a closure field now derive `Clone` only and
drop those generic bounds (derive(Clone) re-adds `T: Clone`) — the same `has_fn_fields`
relaxation a `type`-declared record already gets. Fn-field keys are tracked during
`collect_anon_records`. (A `type`-declared record always worked.)

### IndexAssign of a non-Copy element through a closure → WASM trap

```almide
var xs: List[String] = ["a", "b"]
let f = () => { xs[0] = xs[0] + "!" }    // was: wasm trap (native "a!!")
```

Root cause: a captured shared cell is `rc_inc`'d with a PLAIN `rc_inc` on the cell ptr,
but its `RcDec` ran a TYPED rc_dec that walked the cell ptr AS the list — reading
`cell[0]` (the object ptr) as an element count and decref'ing garbage addresses.
`List[Int]` (Copy elems, no element-drop loop) survived; `List[String]` trapped. Fix:
`RcDec` of a `mutable_capture` is now a PLAIN `rc_dec` on the cell, matching the
`rc_inc` (`inc == dec` — the Lean-proven cell invariant).

## Fragility / elimination targets (surfaced while fixing the above)

1. `emit_wasm/statements.rs` derives list element width from the VALUE's type
   (`byte_size(value.ty)`), not the list's declared element type — drift-prone. OPEN.
2. Cell-deref is re-implemented per op (Var read / IndexAccess / IndexAssign / each
   mutator / RcInc / RcDec) instead of one "deref once → normal ptr" abstraction. OPEN.
3. ~~Parser `[X]()` ambiguity~~ — eliminated (#334).
4. ~~Closure-boxing covered only `List[Fn]` literals~~ — extended to `Map[K, Fn]`
   (insert/get_or) and anon-record closure fields. `Set[Fn]` and general
   container-element positions still rely on the `_`-infer trick; a single
   "box any closure entering a uniform container" pass would subsume all of them. OPEN-ish.
5. ~~`IndexAssign` of a non-Copy element didn't coordinate the cell deref with the
   drop~~ — fixed via the `RcInc`/`RcDec` cell consistency above. The deeper item
   remains: a cell's RC-drop frees the cell but NOT the object inside (a leak shared
   by every cell type, `List[Int]` included) — the cell needs a typed destructor.
   OPEN (benign: leak, not a crash).
