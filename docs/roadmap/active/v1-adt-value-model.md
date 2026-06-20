# v1 ① — custom ADT (variant) as a first-class value

The cross-repo wall lever #1 (~41 walls: recursive `to_string`/pretty-print over `type Expr = Lit(Int)
| Add(Expr,Expr) | …`) is the tip of a real gap: **custom variants do not render in v1 at all.**

## Evidence (byte-match-first, /tmp/adt.almd, /tmp/adt3.almd)

- `classify(e) = match e { Lit(n) => n, Add(_,_) => 100, Neg(_) => 200 }` (non-recursive, scalar) v0 = 7/200,
  v1 = **WALL** `unlinked stdlib/runtime call(s): Lit, Neg` — the **constructors have no wasm impl**.
- `eval` (scalar match) is counted `in-profile` by classify, but render WALLS on the same unlinked ctors —
  so **classify overstates ADT support**; render (byte-match) is the truth. (This also means the
  org-trust-status `in-profile` is optimistic for any ADT-using repo.)
- Root in the MIR: `build_record_layouts` (lower/mod.rs:142) **skips variant decls** ("variant / alias
  decls carry no flat record layout"). There is no `IrPattern::Constructor` tag-dispatch in the MIR lower;
  the existing `try_lower_variant_*` are Option/Result-only.

## Target repr — match v0's tagged heap block

v0 wasm (emit_wasm: `variant_alloc_size`, the per-ctor record layout, `control.rs:1485` "bind named fields
from the variant's record layout") lays a variant value as a heap block `[rc][tag][ctor fields…]`: tag =
ctor index, payload = that ctor's fields in its own (record-like) slot layout. v1 must byte-match this.

## Brick sequence (each ends in a v0==v1 byte-match; protected by `proofs/diff-fuzz.sh`)

1. **Variant layout registry** — extend `build_record_layouts` to register, per `IrTypeDecl::Variant`, the
   tag + each ctor's field layout (reuse the record-field modeling from brick #54). Infra only; not yet
   byte-verifiable on its own.
2. **Ctor construct** — lower `Lit(7)` / `Add(l,r)` to `Alloc` a tagged block + `store32` the tag +
   `store` each field (scalar copy / heap-handle move, masked like a record). Mirrors
   `try_lower_scalar_record_construct` + the heap-field tuple construct. FIRST byte-verifiable point with…
3. **Tag-dispatch match (scalar result)** — `match e { Ctor(binds…) => arm }`: `handle → tag@4`, dispatch
   to N arms (chained `IfThen`/`Else` on tag==i, or a switch), bind each ctor's fields from its layout
   slots. Byte-match `classify`/`eval` (non-recursive first, then recursive `eval`). This is the
   N-variant generalization of `try_lower_variant_value_match` (which is the 2-variant Option case).
4. **Heap-result match** — the same dispatch with `lower_heap_result_arm` + subject-drop-before-arms, so a
   String/heap-returning arm works. Byte-match recursive `to_string` (the #1 lever) → /tmp/adt.almd.
5. **Recursive drop** — a layout-driven recursive free: free each ctor's heap fields by the tag's mask,
   recursing into child variants (`Add(Expr,Expr)`). The ownership-sensitive part — verify with a
   create+drop **leak loop** (like DropListStrValue), AND keep diff-fuzz green. One cert `d`, trusted
   recursion (same shape as `$__drop_value`).

## Why this is ① (value-model unification), not a one-off

Record / tuple / enum / list are all "a tagged?/masked heap block". Bricks 1-5 make the variant the LAST
member to join that model; once done, the per-surface-shape bricks (the List[(String,Value)] hand-drop,
the Option-tuple-match, the heap-accumulator special cases) are all instances of ONE masked/tagged-block
construct+match+drop — the path to retiring accidental complexity. byte-match-first throughout, with the
per-PR diff-fuzz gate (commit 33e4237f) catching any drift the moment a generated program hits it.
