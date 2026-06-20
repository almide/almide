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

## Target repr — v1's OWN uniform-slot block (only the OUTPUT byte-matches v0)

CORRECTION (verified while building brick 1): v1 does **not** replicate v0's internal bytes — only the
OBSERVABLE output (the `classify`/`to_string` result printed to stdout) must byte-match v0. v0 wasm
(emit_wasm: `variant_alloc_size`, `collections.rs::emit_record`, tag @0 via `i32_store(0)`, fields @4 in
type-sized packed slots, padded to `4 + max payload`) is byte-PACKED. But v1 records already use a
DIFFERENT internal model — a uniform i64 slot per field (`layout::slot_offset`, `Store{width:8}`, block
`[rc][len][cap]` + slots) — and read back from the same slots they wrote, so the output matches without
matching v0's bytes. The variant joins THAT model:

- a variant value is a record-like block whose **slot 0 = tag** and **slots 1.. = the active ctor's
  fields** (uniform i64 each, the same `slot_offset` machinery records use);
- `slot_count = 1 + max arity over all ctors`, so every ctor of the type is one block size (uniform
  alloc + a sound whole-block `==`) — the v1 analogue of v0's max-payload padding, NOT a byte copy of it.

Tag = declaration index and tuple fields are named `_0.._n` (matching v0's registration) so the two
backends agree on tag + field identity even though the byte layout differs.

## Brick sequence (each ends in a v0==v1 byte-match; protected by `proofs/diff-fuzz.sh`)

1. **Variant layout registry** — ✅ DONE (commit `c81b0878`). `build_variant_layouts` builds the
   `VariantLayouts` registry (type → `VariantLayout { generics, tag-indexed cases, slot_count }` +
   ctor-name → type reverse index + `lookup_ctor`), threaded into lowering via the new
   `lower_function_all_with_layouts` (the record-only `_with_types` delegates with an empty variant
   registry; render_program + classify_corpus build + pass it). Infra only — nothing consumes the
   registry yet, so **zero output change** (diff-fuzz mismatch=0, full mir suite unchanged). Unit test
   locks tag order, `_N` tuple synthesis, named record fields, `slot_count = 1 + widest arity`,
   `lookup_ctor`. (Confirmed gap: a ctor is `IrExprKind::Call { target: Named(ctor) }` → currently
   WALLED as an "unlinked stdlib/runtime call"; oracle `classify(Lit/Add/Neg)` v0 = 7/100/200.)

   FIRST byte-match slice (bricks 2+3, scalar-only — isolates from brick 5's recursive drop, mirroring
   how #51/#54 started scalar-first):
   ```
   type Tok = Num(Int) | Sym(Int) | Eof
   fn val(t: Tok) -> Int = match t { Num(n) => n, Sym(s) => s * 10, Eof => -1 }
   ```
   No heap fields ⇒ no recursive drop ⇒ brick 2+3 verifiable alone. The recursive/heap-field `Expr`
   (the #1 to_string lever) needs brick 5 and comes after.

2. **Ctor construct (scalar fields first)** — intercept a `Call{Named(ctor)}` whose name is in
   `variant_layouts.ctor_to_type`: `Alloc` a `slot_count`-wide block, `Store` the tag into slot 0, lower
   each arg into slot `1+i` (scalar copy via the `try_lower_scalar_record_construct` slot machinery; a
   HEAP/recursive field is a brick-5 concern — wall it for now). FIRST byte-verifiable point with…
3. **Tag-dispatch match (scalar result)** — `match t { Ctor(binds…) => arm }`: `handle → tag@slot0`,
   dispatch to N arms (chained `IfThen`/`Else` on tag==i, or a switch), bind each ctor's scalar fields
   from its `1+i` slots. Byte-match `val`/`classify` (scalar). N-variant generalization of
   `try_lower_variant_value_match` (the 2-variant Option case).
4. **Heap-result match** — the same dispatch with `lower_heap_result_arm` + subject-drop-before-arms, so a
   String/heap-returning arm works. Byte-match recursive `to_string` (the #1 lever) → /tmp/adt.almd.
5. **Recursive drop + heap/recursive ctor fields** — admit heap-handle ctor fields (a nested
   `Add(Expr,Expr)` block MOVED into a parent slot, cert `m`, tracked in `record_masks`) AND a
   layout-driven recursive free: free each ctor's heap fields by the tag's mask, recursing into child
   variants. The ownership-sensitive part — verify with a create+drop **leak loop** (like
   DropListStrValue), AND keep diff-fuzz green. One cert `d`, trusted recursion (same shape as
   `$__drop_value`).

## Why this is ① (value-model unification), not a one-off

Record / tuple / enum / list are all "a tagged?/masked heap block". Bricks 1-5 make the variant the LAST
member to join that model; once done, the per-surface-shape bricks (the List[(String,Value)] hand-drop,
the Option-tuple-match, the heap-accumulator special cases) are all instances of ONE masked/tagged-block
construct+match+drop — the path to retiring accidental complexity. byte-match-first throughout, with the
per-PR diff-fuzz gate (commit 33e4237f) catching any drift the moment a generated program hits it.
