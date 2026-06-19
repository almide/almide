# v1 dynamic Value model — the yaml keystone (path A: self-host + ONE trusted recursive-drop routine)

**Status: DESIGN (2026-06-19). CEO chose path A ("Aでいくぞ"): keep the trusted base minimal, PROVE the Value model (constructors/extractors/serializer self-hosted in .almd, cert-verified), with the recursive free as the ONE trusted runtime routine (like `DropListStr` already is). Coq-free; byte-match-verified vs v0 `runtime/rs/src/value.rs` per brick.**

## Why this is the keystone

yaml's remaining 22 v1 walls are dominated by the dynamic `Value` model: `value.array`/`value.object` build it (collect_seq/collect_map), `value.as_array` binds the array payload (yaml:249 `ok(items) => emit_seq(items, ind)`), `value.stringify` + the yaml `emit` serializer read it. v1 self-hosts SCALAR Values (value_core.almd: Null/Bool/Int/Float tags 0-3, Str tag 4) but NOT the COMPOUND ones (Array tag 5, Object tag 6). The blocker is the RECURSIVE FREE: a `List[Value]`/Array's elements are themselves Values with their own heap payloads, so the existing `DropListStr` (a FLAT per-slot `rc_dec`) LEAKS them.

## The pieces (a COUPLED unit — observable only end-to-end via stringify, so build + byte-verify together)

1. **`List[Value]` materialize** — extend `try_lower_str_list_literal` (binds.rs:103) to admit Call elements (`[value.int(1), value.int(2)]`) like the tuple/ResultOk Call paths; AND mark the list as a VALUE-element list (new set `value_elem_lists`) so its drop is the recursive value-aware one, NOT the flat `DropListStr` (which would leak str/array/object elements). Same for the call-arg position (calls.rs, mirror the f89cbfca List[String] brick).

2. **Recursive `$__drop_value` trusted runtime routine** (preamble, render_wasm.rs ~1040 — the ONE trust addition, like `$rc_dec`/`$alloc`/the inline DropListStr). At rc==1 (last ref):
   - tag < 4 (scalar): nothing nested → `rc_dec(p)`.
   - tag == 4 (Str): `rc_dec(payload@12)` (frees the String) → `rc_dec(p)`. (== today's inline DropValue.)
   - tag == 5 (Array): payload@12 = a `List[Value]`. `for i in 0..len(list): $__drop_value(elem_addr(list,i))`; then `rc_dec(list)`; then `rc_dec(p)`.
   - tag == 6 (Object): payload = the key/value store (match v0's Object layout — `Vec<(String, Value)>`). `for each pair: rc_dec(key String); $__drop_value(value)`; then `rc_dec(store)`; then `rc_dec(p)`.
   The cert sees ONE `d` (an `Op::DropValue`, opaque). The recursion is the trusted routine — verified by the rt-oracle-registry DIFFERENTIAL test (a `value.stringify` round-trip fixture proves no leak/no double-free vs v0). A `value_elem_lists` List drops via a sibling `$__drop_list_value` (loop `$__drop_value` per element + free list).

3. **`value.array`/`value.object` self-host** (value_core.almd): `alloc_value`, `store32(h+4, 5/6)`, `store64(h+12, <list/store handle>)` — MOVE the items in (v0 does `Array(items.clone())`, so a DEEP COPY: the self-host either copies items or takes ownership; match v0's bytes via the stringify round-trip). Register in render_wasm/registry.rs.

4. **`value.as_array`/`value.as_object`** — read tag; tag 5/6 → `Ok(payload list/store)` (BINDS the heap payload — yaml:249), else `Err`. A heap-Ok Result (cap-as-tag, like value.as_string — reuse the str-result machinery from commit 7b24ef8f).

5. **`value.stringify`** self-host — recursive: scalar → "null"/"true"/n/float.to_string; Str → `"\"" + escape(s) + "\""` (escape `\\ " \n \r \t` EXACTLY as v0 value.rs:228); Array → `"[" + (items |> list.map(stringify) |> list.join(",")) + "]"`; Object → `"{" + (pairs |> list.map((k,v) => "\"" + escape(k) + "\":" + stringify(v)) |> join(",")) + "}"`. The self-call is inside `list.map` (defunctionalized by C1, NON-tail) so the self-rec GUARD does not apply — it should lower. float via the self-hosted float.to_string (#63). VERIFY: `value.stringify(value.array([value.int(1), value.str("a")]))` byte-matches v0 (`[1,"a"]`).

## §4.1-COMPLIANT RECURSIVE DROP — the design BREAKTHROUGH + its soundness boundary (2026-06-19)

The `handwritten_wasm_runtime_does_not_grow` gate (§4.1) FORBIDS a hand-written WAT `$__drop_value`
(I tried — reverted). The §4.1-compliant path: SELF-HOST `__drop_value` in Almide, operating on RAW
HANDLES (Int), exactly like `string_eq`/`__streq_at` — `fn __drop_value(v: Int) -> Unit = { let tag
= prim.load32(v + 4); if tag == 5 { <loop: __drop_value(prim.load_handle(elem_addr)) > over the
List[Value]>; prim.rc_dec(list) } else if tag >= 4 { prim.rc_dec(payload) }; prim.rc_dec(v) }`.
Because it operates on raw Ints (no heap-TYPED values), its ownership cert is EMPTY (no i/a/d/m) —
cert-clean like string_eq. The render of `Op::DropValue` becomes a `CallFn` to it; the Op KEEPS its
cert `d` (so the Value's alloc still balances). Needs ONE new prim: `prim.rc_dec(addr: Int) -> Unit`
→ a `PrimKind::RcDec` that emits `(call $rc_dec …)` — REUSES the existing `$rc_dec`, NO new WAT func,
§4.1-OK.

**SOUNDNESS BOUNDARY (the catch — this is task #30 "runtime Almide化" territory, soundness-critical):**
`verify_ownership` treats `Op::Prim` as a no-op (lib.rs:705). So `prim.rc_dec` is an UNTRACKED free —
a self-host fn using it can double-free WITHOUT the checker catching it (a cert HOLE). This is the
SAME trust level as `DropListStr`'s inline per-element rc_dec (untracked, gated rc==1, trusted), BUT
exposing `prim.rc_dec` to ALL self-host widens the trusted surface: ANY .almd fn could misuse it.
CONTAINMENT options (design-first, do NOT rush): (a) gate `prim.rc_dec` to a whitelist (only
`value_core.__drop_value` may call it), enforced like the purity registry; (b) a dedicated
`Op::DropValueRecursive` the lowering emits ONLY for the drop routine; (c) prove the drop routine
total/safe separately. This is the soundness-critical core of self-hosting the Value drop — the
CEO's "design-first, don't-rush" discipline applies. The BREAKTHROUGH is real (raw-handle self-host
sidesteps the borrow-by-default/consuming-convention problem); the rc_dec EXPOSURE is the gated step.

## ★ IMPLEMENTED + EMPIRICALLY VALIDATED 2026-06-19 (prim infra committed; self-host staged with 4 confirmed blockers)

COMMITTED (83176420): `prim.rc_dec`/`prim.rc_inc` (PrimKind::RcDec/RcInc, reuse the proven `$rc_dec`/`$rc_inc`,
NO new WAT func = §4.1-OK), WHITELISTED in lower_prim_call to `fn_name ∈ {__drop_value, __varr_copy}` (the
untracked-free containment — a Prim is a cert no-op, so an unrestricted rc_dec would let any fn double-free
unseen). The whitelist + `prim.load_handle` (b50853ef) are the floor.

The self-host (below) was WRITTEN, BUILT, and the recursive drop RAN CORRECTLY in render_program
(`let a = value.int(5); match value.as_int(a) {…}` → "5", with `$__drop_value` linked + called, line 758 of
the emitted wat). It is STAGED (reverted from value_core.almd to keep the tree green) pending 4 EMPIRICAL
blockers found by implementing it:

1. **value.stringify is TCO-blocked AND poisons value_core.** Its array case `__vstr_arr(v,i,n,acc) = if i>=n
   then acc else __vstr_arr(v,i+1,n, acc+sep+value_stringify(e))` is a heap-result (String) SELF-RECURSION →
   hits the self-rec GUARD → does NOT lower → an unlinked `$__vstr_arr` → and because value_core auto-links
   ITS WHOLE SOURCE when any value.* is called, EVERY Value program then walls. FIX: restructure stringify as
   a Unit-extract-to-List[Value] (Unit self-rec is NOT guarded) + `list.fold(",", value_stringify)` (fold is
   C1-defunctionalized, so value_stringify's recursion is indirect, guard-free); OR land TCO first.
2. **Test-harness vs production linking diverge.** render_program (corpus-wall/output-parity) auto-links
   `__drop_value` (rides on value_core); the unit-test `lower_source` (tests_part1.rs:195, via
   `lower_function_all_with_types`) did NOT → the json unit tests failed `unknown func $__drop_value` while
   production ran fine. Align lower_source's auto-link with render_program before wiring DropValue→__drop_value.
3. **List[Value] materialize is needed first.** `value.array([value.int(1),…])`'s arg walls:
   `try_lower_str_list_literal` (binds.rs:103) admits LitStr/Var/Record/Tuple/ConcatStr but NOT Call elements.
   Add a Call-element arm (materialize each via the CallFn, Consume into the slot) gated to a `List[Value]`.
4. **The drop is UN-GATE-VERIFIABLE (the soundness crux).** A wrong tag-5 drop = a silent LEAK: the output is
   still correct (leak happens after the print) and the cert sees DropValue as one balanced `d` — so neither
   output-parity NOR corpus-wall catches it. Needs a dedicated LEAK test (loop create+drop, assert no memory
   growth / address reuse) BEFORE the drop can be trusted. This is the ②-critical reason it was NOT rushed.

THE WORKING SELF-HOST CODE (value_core.almd, ready to re-paste once 1-3 are cleared; self-contained tag-5
`[rc][tag=5 @4][len @8][elem@12…]`, shallow-copy via rc_inc, raw-handle recursive drop = empty cert):
```almide
fn __varr_copy(src: Int, dst: Int, i: Int, n: Int) -> Unit =
  if i >= n then () else { let e = prim.load64(src + 12 + i * 8); prim.rc_inc(e); prim.store64(dst + 12 + i * 8, e); __varr_copy(src, dst, i + 1, n) }
fn value_array(items: List[Value]) -> Value = {
  let ih = prim.handle(items); let n = prim.load32(ih + 4); let v = prim.alloc_value(n); let vh = prim.handle(v)
  prim.store32(vh + 4, 5); prim.store32(vh + 8, n); __varr_copy(ih, vh, 0, n); v }
fn __vdrop_arr(v: Value, i: Int, n: Int) -> Unit =
  if i >= n then () else { let h = prim.handle(v); let e: Value = prim.load_handle(h + 12 + i * 8); __drop_value(e); __vdrop_arr(v, i + 1, n) }
fn __drop_value(v: Value) -> Unit = {
  let h = prim.handle(v)
  if prim.load32(h + 0) == 1 then { let tag = prim.load32(h + 4); if tag == 5 then __vdrop_arr(v, 0, prim.load32(h + 8)) else if tag == 4 then prim.rc_dec(prim.load64(h + 12)) else () } else ()
  prim.rc_dec(h) }
```
The render wiring (reverted): `Op::DropValue { v } => format!("    (call $__drop_value (local.get {}))\n", local(*v))`
+ register `("value_array","value.array")` and (once stringify is restructured) `("value_stringify","value.stringify")`.
Almide gotcha CONFIRMED: the Unit literal is `()` NOT `unit`.

## PRIM-FLOOR layer found 2026-06-19 (the foundation below piece 1)

The prim floor (load_str/store_str/load64/store64/handle) has NO typed `load_list` to read a
`List[Value]` payload back out of a Value's @12 slot (load_str is hardcoded to return String). So
`value.array` storing a `List[Value]` payload + `value.as_array`/`stringify` reading it back needs a
prim-floor addition first: a `prim.load_list` (a typed `LoadHandle` returning `List[Value]`, the list
sibling of `load_str`), wired in `lower_prim_call` (calls.rs:1968 maps load_str→LoadHandle) + the prim
stdlib type sig. (Alternatively: represent a Value-array as a SELF-CONTAINED tag-5 block holding the
element Value handles inline — no separate List payload, no load_list — but then value.array must COPY
items's elements in, and stringify/drop iterate the inline slots; weigh vs the load_list approach.)
The LAYERS, foundation-up: `prim.load_list` → recursive `__drop_value` → List[Value] materialize →
value.array/object/as_array/stringify self-host → registry. Each layer is revealed by implementing the
one below it (empirically confirmed this session).

## Gates (per brick, the proven methodology — 9 bricks this session)

corpus-wall ACCEPT (3 props, ownership = the leak/double-free check that catches a bad recursive drop) + a Value-model byte-match probe (build + stringify, compared to v0) + cargo test + output-parity baseline. A drop bug = a leak/double-free → corpus-wall REJECTS or the probe diverges → revert (never ship). After the Value model: TCO (mid-loop-break-with-result, docs/roadmap/active/v1-tco-self-recursion.md) + float.parse → yaml 0 walls.

## Honest scale

This is a COUPLED unit (~5 pieces, recursive runtime drop + serializer), genuinely a focused multi-brick push, not a single-turn task — but every piece is Coq-free and byte-verifiable, and the recursive-drop is the ONLY trusted-base addition (one routine, like DropListStr). Path A keeps the Value model PROVEN, which is the v1 differentiator.
