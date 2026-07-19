<!-- description: v1: make custom ADT (variant) values first-class, closing the #1 cross-repo wall lever (~41 walls, recursive to_string/pretty-print over user variants) -->
<!-- done: 2026-06-20 -->
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

2. **Ctor construct (scalar fields first)** — ✅ DONE (commit `62a4a862`). `try_lower_variant_ctor`
   (binds.rs) intercepts a `Call{Named(ctor)}` whose name is in `variant_layouts.ctor_to_type`: `Alloc`
   a `slot_count`-wide block, `Store` the tag into slot 0, lower each arg into slot `1+i` (scalar copy via
   the `try_lower_scalar_record_construct` slot machinery). Wired in the call-ARG path (`val(Num(7))`,
   calls.rs) and the LET-bind path (`let t = Num(9)`, binds.rs). A HEAP/recursive ctor field walls
   (ADT brick 5) — never a wrong-bytes block.
3. **Tag-dispatch match (scalar result)** — ✅ DONE (commit `62a4a862`). `try_lower_custom_variant_match`
   (control.rs): materialize/borrow the subject → `handle → tag@slot0`, emit the right-nested
   `if tag==t_i { bind scalar fields from slots 1+i; arm } else …` chain (the last arm / any wildcard is
   the unconditional else — exhaustiveness guarantees it). Wired at the tail, let-bind, and
   scalar-operand match sites (NOT gated by the Option/Result-only `is_variant_ty`). N-constructor
   generalization of `try_lower_variant_value_match`. Byte-matches v0 across out-of-order arms, wildcard,
   multi-field ctors, and bind/let positions; corpus-wall proven checker accepts all witnesses; a new
   diff-fuzz generative variant template + a deterministic wasmtime cargo test guard it. SCALAR result +
   SCALAR ctor-field binds only (heap-result arm = brick 4, heap/nested ctor field = brick 5).
4. **Heap-result match (borrowed subject)** — ✅ DONE (commit `f37a7314`). A `match t { Ctor(..) =>
   <String> }` with a heap result over a BORROWED param/var subject dispatches on tag@slot0 and lowers
   each arm via `lower_heap_result_arm` (the arm moves out a fresh heap value; a bound SCALAR field reads
   the still-live borrowed subject's slot). `emit_variant_arm_chain` takes `result_ty` and picks
   scalar vs heap arm lowering; wired at the heap-result tail. An OWNED-temp subject with a heap result
   WALLS (needs subject-drop-before-arms, brick 4b) rather than emit cert-failing MIR. corpus-wall
   ownership 15904→16046 (heap-result variant matches now lower, proven checker accepts all).
5. **Heap/recursive ctor fields + recursive drop** — the remaining frontier (unlocks the recursive
   `Expr` to_string, the #1 lever). Split into three sub-steps, each gated on corpus-wall ACCEPT:

   - **5a — leaf String CONSTRUCT** ✅ DONE (commit `f2c32c90`). `try_lower_variant_ctor` moves a
     `String` ctor field into its slot (cert `m`) + tracks `record_masks`, so the block's scope-end drop
     frees it via the SAME masked DropListStr a String-field record uses. corpus-wall ACCEPT (ownership
     16046→16085); a 1000× construct+drop leak-loop cargo test on wasmtime. (Construct-only: the field
     is still matched with a WILDCARD — the heap-field BIND is 5c.)
   - **5c — heap-field match BIND** ✅ DONE (commit `b50e95a3`). A MULTI-arm match binding a leaf
     String ctor field lowers: **borrow** the slot handle (`LoadHandle`+`param_values`) — the move-out
     arm auto-`Dup`s in `lower_heap_result_arm`, a consuming re-use `Dup`s in `lower_owned_heap_field`,
     so the subject (which owns the slot) is never released at rc 0. A **SINGLE-arm heap-result match**
     (a 1-ctor newtype `unbox`, `B(x)=>x`) is WALLED: with no IfThen branch-merge `dst`, the arm value
     rets directly and double-moves with the arm `Consume` (`amm`/`aamdm`, net −1 — the checker REJECT).
     Diagnosed cert-driven: the exact reproducer was `unbox[String]` in
     `spec/wasm_cross/generic_fn_in_inferred_lambda.almd`; the `Op::Consume`→`m` plus the heap
     `func.ret`→`m` double-counted (certificate.rs). corpus-wall ACCEPT (ownership 16085→16161), a
     1000× leak loop, a wasmtime cargo test + a diff-fuzz heap-field template. (`emit_cert_from_source
     <f> <fn> mir` now dumps the MIR ops + cert — the debug aid that found it.)
   - **5b — nested-variant ctor fields + recursive drop** ✅ DONE (commit `c98486be`) — THE #1 LEVER
     IS LIT. `try_lower_variant_ctor` recursively builds a nested-variant ctor field (`Add(Lit(1),
     Neg(Lit(2)))` — a ctor-call arg → recurse, a var → `Dup`) + moves it in, and tracks the block for
     `Op::DropVariant`. The match heap-field bind extends to variant-typed fields (the recursive
     `tos(l)`/`tos(r)` borrow-pass). The recursive free is a GENERATED per-type Almide fn `$__drop_<T>`
     (`generate_variant_drop_sources` — the `$__drop_value` shape: `prim.handle`/`load*`/`load_handle`/
     `rc_dec` + self-recursion), auto-linked via a TWO-PASS `source_to_ir` in render_program +
     lower_source; it is `prim`-only so its ownership cert is EMPTY (a trusted routine — the `rc_dec`
     whitelist admits `__drop_*`). `Op::DropVariant` is one trusted `d` (like `DropValue`). Verified:
     `tos(Add(Lit(1), Neg(Lit(2)))) == "(1 + -2)"` byte-matches v0 + a deep tree; a 2000× build+tos+drop
     LEAK LOOP is clean (the recursion's only correctness gate); corpus-wall ACCEPT (ownership
     16161→16300, the proven checker accepts the recursive corpus variants, zero double-free/leak); a
     deterministic wasmtime cargo test + a diff-fuzz recursive-variant template.

   STANDING LESSON (re-proven this round): a heap-field variant lowering can byte-match on wasmtime yet
   be a latent double-free — only the kernel-proven `[ownership]` checker catches it (5c) / the LEAK
   LOOP catches it for trusted prim-routines (5b). Both a borrow and a Dup'd-owned bind REJECTED until
   the cert-driven diagnosis pinned the *single-arm direct-ret* double-move; gating that (not the bind
   model) was the fix. NEVER ship a checker-rejected witness.

## STATUS: ① ADT value-model COMPLETE (bricks 1–5)

Custom ADTs are first-class in the v1 trust spine: construct + N-arm tag-dispatch match (scalar /
heap-result, all positions incl. Unit-statement) + scalar/String/nested-variant ctor fields + recursive
drop, all byte-matching v0, corpus-wall-sound (proven checker, zero silent miscompiles / double-frees),
and per-PR diff-fuzz + deterministic cargo guarded. The #1 cross-repo wall lever (recursive
`to_string`/pretty-print over `type Expr = Lit | Add | Neg`) is LIT.

### Org-repo verification (the authoritative byte-match)

REAL `github.com/almide` org variant types now byte-match v0==v1 on the v1 trust spine (the ② gate,
not just wall-count): `porta`'s `InstanceState = Created | … | Exited(Int) | Failed(String)`
(unit + scalar + String-payload ctors), `RestartPolicy`/`ValType` (enums), `FdKind` (String payloads)
— all `almide run` (native) == `render_program | wasmtime`, AND a 2000× ctor+match+drop LEAK LOOP over
the String-payload `FdKind` is clean. porta's variant ctors no longer wall as "unlinked call".

CAVEAT (measurement, not correctness): `scripts/org-trust-status.sh` sweeps only each repo's ENTRY
module, so it is BLIND to ADT code living in SUBMODULES (porta's entry is a barrel — 0 in-subset fns).
The dashboard totals are therefore UNCHANGED by the ADT work; the byte-match above is the real
evidence. The org-wide NEXT lever (dashboard top reason, ~40 walls) is now the **heap-result-expr
family** (`match`/`if`/`Range`/`??`/`Try`/`List`/`BinOp` returned-in-tail or in a call-arg) — blocking
toml/svg/aes/base64/csv — NOT ADTs.

## Why this is ① (value-model unification), not a one-off

Record / tuple / enum / list are all "a tagged?/masked heap block". Bricks 1-5 make the variant the LAST
member to join that model; once done, the per-surface-shape bricks (the List[(String,Value)] hand-drop,
the Option-tuple-match, the heap-accumulator special cases) are all instances of ONE masked/tagged-block
construct+match+drop — the path to retiring accidental complexity. byte-match-first throughout, with the
per-PR diff-fuzz gate (commit 33e4237f) catching any drift the moment a generated program hits it.
