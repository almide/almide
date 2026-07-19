<!-- description: Goal prompt: finish v1 ADT brick 5 (heap-field bind + recursive drop) to close the #1 cross-repo wall lever -->
# GOAL PROMPT — finish ADT brick 5 (heap-field bind + recursive drop) to the #1 lever

You are continuing the v1 trust-spine ADT value-model work on branch **`develop-v1`**. Bricks
1–4 + 5a are DONE and pushed (see `git log`). Your job: **land bricks 5c (heap-field match
bind) and 5b (recursive drop), so the recursive `Expr` to_string byte-matches v0 — the #1
cross-repo wall lever (~41 walls).** Read `docs/roadmap/active/v1-adt-value-model.md` first; it
is the authoritative design + status.

## The goal, concretely

This program must byte-match v0 (`almide run`) on the v1 trust spine
(`render_program` → wat → `wasmtime`):

```almide
type Expr = Lit(Int) | Add(Expr, Expr) | Neg(Expr)
fn tos(e: Expr) -> String = match e {
  Lit(n)    => int.to_string(n),
  Add(l, r) => "(" + tos(l) + " + " + tos(r) + ")",
  Neg(x)    => "-" + tos(x),
}
fn main() -> Unit = { println(tos(Add(Lit(1), Neg(Lit(2))))) }   // v0: (1 + -2)
```

It currently WALLS (honest). Done = v0==v1 byte-match for this AND a create+drop leak loop
over `Expr` AND `bash proofs/corpus-wall.sh` ACCEPT (all 3 properties) AND `cargo test -p
almide-mir` shows no NEW failures (2 pre-existing `self_hosted_json_*` `$__drop_value` fails
are unrelated).

## NON-NEGOTIABLE discipline (the ② cardinal rule — already re-proven twice this arc)

**Never ship MIR the kernel-proven `[ownership]` checker rejects.** A heap-field variant
lowering can byte-match on `wasmtime` yet be a LATENT DOUBLE-FREE — only `corpus-wall.sh`'s
proven checker catches it. Two naive 5c attempts (borrow `LoadHandle`+`param_values` + the
Option-style move-out auto-Dup in `lower_heap_result_arm`) BOTH got `[ownership] REJECT`
(ownership would rise 16085→16162). So: corpus-wall ACCEPT is the gate for EVERY step; if it
REJECTs, REVERT (do not ship), don't reason your way past it. Work in small increments, run
corpus-wall after each.

## Where the code is

- Construct: `try_lower_variant_ctor` in `crates/almide-mir/src/lower/binds.rs` (5a = scalar +
  leaf `String` fields already land; the `String` field is moved in + masked via `record_masks`,
  freed by the existing DropListStr). Heap NON-String / nested-variant fields WALL.
- Match: `crates/almide-mir/src/lower/control.rs` —
  `try_lower_custom_variant_match` (value), `lower_custom_variant_unit_match` (unit-stmt),
  `parse_variant_arms` (shared arm parser — currently rejects heap-field binds),
  `bind_variant_arm` (scalar binds only), `emit_variant_arm_chain` / `emit_variant_unit_chain`.
  Wired at: tail.rs (scalar+heap), binds.rs (let-bind), calls.rs (operand/arg + ctor construct
  in arg position), control.rs:~122 (unit statement).
- v0 reference layout: `crates/almide-codegen/src/emit_wasm/collections.rs::emit_record`,
  `equality.rs::{variant_alloc_size,find_variant_tag_by_ctor}`, `mod.rs` variant registration.
  (v1 uses its OWN uniform-i64-slot block — tag@slot0, fields@slot1.., padded to slot_count —
  NOT v0's byte-packed layout. Only the observable stdout must match v0.)

## Step 5c — heap-field match bind (the blocker; USE-AWARE binding is the answer)

DIAGNOSIS DONE (this session). The exact corpus reproducer + the two failure modes are known:

- The rejecting corpus function is **`spec/wasm_cross/generic_fn_in_inferred_lambda.almd`** —
  `fn unbox[T](b: Box[T]) -> T = match b { B(x) => x }` monomorphized at `Box[String]`: a
  String field bound and **moved out** (`B(x) => x`) over a borrowed param. Its single
  ownership.cert lifecycle pinpoints the bug:
  - **borrow bind** (`LoadHandle`+`param_values`, + `lower_heap_result_arm`'s Var auto-Dup):
    move-out `B(x)=>x` is fine (`am`), BUT a **consuming re-use** `Ctor(s) => Other(s)` emits a
    `Consume`/`m` on the borrow at rc 0 → REJECT.
  - **Dup'd owned bind** (`LoadHandle`+`Op::Dup`, push `live_heap_handles`, dropped by the arm
    frame): consuming re-use is fine, BUT move-out `B(x)=>x` **double-acquires** — the bind `a`
    + `lower_heap_result_arm`'s auto-Dup `a` + its `m` + the arm-frame `d` + the return `m` =
    `aamdm`, **net −1 over-release** → REJECT.
  Reproduce: `cargo run -q -p almide-mir --example classify_corpus -- --out /tmp/cw
  spec/wasm_cross/generic_fn_in_inferred_lambda.almd` then `cat /tmp/cw/ownership.cert`
  (`aamdm` with Dup; balanced `am` with borrow).

THE FIX = **USE-AWARE binding**. A bound heap field needs the bind mode chosen by how the arm
BODY uses it:
  - read-only / borrow-passed to a borrowing callee (`string.len(x)`, `tos(l)`): **borrow**
    (`LoadHandle`+`param_values`, NOT in `live_heap_handles`, no drop) — `ad`-free, balanced.
  - moved out as the arm result (`B(x) => x`): **borrow + the existing auto-Dup** in
    `lower_heap_result_arm`'s Var case (`am`) — do NOT pre-Dup (that double-acquires).
  - consumed into a new ctor / a consuming call (`Ctor(s) => Other(s)`): **`Dup` then move**
    (`am`) — pre-Dup so the move is of an owned ref, not the borrow.
  Implement by classifying each bound field's use in the arm body (a small use-visitor:
  borrowed-pass vs moved-out-as-result vs consumed), then bind accordingly. The per-arm-frame
  helpers `lower_variant_arm_value` / `lower_variant_unit_arm` already exist (they drop owned
  binds at arm end via `drop_arm_locals` from a mark taken BEFORE the bind — conditional-safe).
  NOTE: do NOT move an owned local out of `live_heap_handles` inside a CONDITIONAL arm — lhh is
  compile-time, the branch is runtime; the other arm still needs the scope drop. Keep drops
  per-arm (inside the IfThen/Else region) as the helpers do.

VERIFY: corpus-wall ACCEPT (the gate) + byte-match `unbox`-style + `tos`-style + a leak loop.

## Step 5c — STATUS: ✅ DONE (commit `b50e95a3`)

Heap-field (String) match binds land for MULTI-arm matches (borrow + auto-Dup on move-out +
Dup-on-consume). The blocker turned out NOT to be the bind model but the **single-arm
direct-ret double-move** (`unbox` newtype `B(x)=>x`: `Op::Consume`→`m` + heap `func.ret`→`m`);
that single-arm heap case is now WALLED, multi-arm proceeds. corpus-wall ACCEPT. Only 5b
remains for the recursive-`Expr`-to_string lever.

## Step 5b — recursive drop (the LAST piece) — CONCRETE RECIPE (designed this session)

KEY INSIGHT: `$__drop_value` (stdlib/value_core.almd) is SELF-HOSTED ALMIDE built from `prim.*`
ops (`prim.handle`, `prim.load32`, `prim.load_handle`, `prim.rc_dec` + Unit self-recursion).
A `prim` is a CHECKER NO-OP, so the drop fn has an EMPTY ownership cert — it is a "trusted
routine", and its leak/double-free correctness is the LEAK LOOP's burden, not the cert's. So a
custom-variant recursive drop is NOT a render/runtime change — it is a GENERATED per-type
Almide fn in exactly that shape, auto-linked like value_core, cert-clean by construction.

RECIPE: for each variant type with a nested-variant heap ctor field, GENERATE (v1 layout =
`[rc@0][len@4][cap@8][tag=slot0@12][field slot i @ 12+(1+i)*8]`, tag stored width-8):

```almide
fn __drop_Expr(e: Expr) -> Unit = {
  let h = prim.handle(e)
  if prim.load32(h + 0) == 1 then {        // last ref
    let tag = prim.load64(h + 12)          // slot 0 = tag
    if tag == 1 then {                      // Add: fields slot1@20, slot2@28
      __drop_Expr(prim.load_handle(h + 20))
      __drop_Expr(prim.load_handle(h + 28))
    } else if tag == 2 then {               // Neg: field slot1@20
      __drop_Expr(prim.load_handle(h + 20))
    } else ()                               // Lit (tag 0): scalar, nothing to free
  } else ()
  prim.rc_dec(h)
}
```
(Only VARIANT/heap fields recurse; a leaf String field is `prim.rc_dec(prim.load64(h+off))`;
a scalar field is skipped. Mirror `__drop_value`'s tag-5 array case + the `__vdrop_arr` helper
shape if a ctor needs a loop — variants don't, fixed arity per ctor.)

WIRING:
1. `Op::DropVariant { v, ty }` in MIR — cert = ONE `d` (lib.rs + certificate.rs, alongside the
   other Drop* ops). Render → `(call $__drop_<ty> (local.get v))`.
2. GENERATE the `__drop_<ty>` Almide source per recursive variant type at program assembly and
   auto-link it (the cleanest hook is alongside `self_host_runtime()` in render_wasm/registry.rs
   — but that registry is `include_str!` of fixed files, so add a DYNAMIC generated-source path:
   produce the source string from the `VariantLayouts`, run it through the same frontend feeder
   `lower_source` uses, rename its fn to `__drop_<ty>`, add to the program functions). Both
   render_program (trust spine) and the render_wasm `lower_source` test feeder must link it.
3. CONSTRUCT: `try_lower_variant_ctor` — admit a nested-variant heap field (move the child handle
   in via `lower_owned_heap_field` — its Var case already `Dup`s — + mask, exactly the String
   path). Track the constructed value so its scope-end drop is `Op::DropVariant{ty}` (a new
   tracking set, like `value_handles` → `DropValue`).
4. MATCH-BIND: extend 5c's `Ty::String` heap-field bind in `parse_variant_arms` to ALSO admit a
   variant-typed field (borrow — `tos`'s `l`/`r` are borrow-passed to the recursive call, never
   moved out, multi-arm; the single-arm gate already covers the degenerate newtype).
5. VERIFY (each step): corpus-wall ACCEPT, a create+drop LEAK LOOP over `Expr` (a freelist makes
   a leak an OOB trap), and the `tos(Add(Lit(1), Neg(Lit(2)))) == "(1 + -2)"` byte-match.

## Step 5b — original notes

A nested-variant ctor field (`Add(Expr, Expr)`) cannot be freed by a flat `rc_dec` of the child
slot — that leaks the grandchildren. You need a **tag-driven recursive free**, the shape of the
existing `Op::DropValue` / `$__drop_value` (the dynamic-Value recursive drop). Likely design:

- Allow nested-variant heap ctor fields in `try_lower_variant_ctor` (move child handle in, mask).
- Emit, per recursive variant type, a `$__drop_<Type>(ptr)` runtime fn: read tag@slot0, switch,
  per ctor recursively `$__drop_<FieldType>` each variant field + `rc_dec` each leaf field, then
  free the block. (Or a generic `$__drop_variant` driven by an emitted per-type layout table.)
- A new `Op::DropVariant { v, type }` (or reuse the DropValue mechanism) that the render lowers
  to a call to that fn; the proven checker accepts it as ONE trusted `d` (like `$__drop_value`).
- Track a constructed recursive-variant value so its scope-end drop emits `Op::DropVariant`.
- Verify: the `Expr` to_string byte-match AND a create+drop leak loop AND corpus-wall ACCEPT
  (ownership) — the checker must accept the recursive `d` as leak/double-free-free.

## Verification commands (run after every increment)

```
cargo build -q -p almide-mir --example render_program
# byte-match: almide run <f>  vs  render_program <f> | wasmtime
bash proofs/corpus-wall.sh        # MUST stay ACCEPT (all 3) — the soundness gate
bash proofs/diff-fuzz.sh 40       # generative v0==v1 (variant templates already present)
cargo test -q -p almide-mir       # no NEW failures (2 pre-existing json fails are unrelated)
```

When the `Expr` lever byte-matches with corpus-wall ACCEPT: add it to a deterministic wasmtime
cargo test (`crates/almide-mir/src/render_wasm/tests_part3.rs`, near
`custom_variant_*`) + a diff-fuzz recursive-variant template, mark bricks 5b/5c DONE in
`docs/roadmap/active/v1-adt-value-model.md`, commit (English, one line,
`Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`), and push `develop-v1` only when asked.

## Guardrails

- Branch `develop-v1`. NEVER `git checkout`/`restore`/`stash` files you didn't modify (other
  agents work concurrently); manual-Edit reverts instead.
- The `?? p03*.*` / other untracked files are not yours — don't touch them.
- Don't grow the §4.1 hand-written WAT runtime.
- If a step can't be made cert-sound, WALL it honestly and document — a wall beats a latent
  double-free. Partial-but-sound (e.g. 5a construct) is a real win; checker-rejected is not.
