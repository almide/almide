<!-- description: v1 dynamic Value model — the yaml keystone: self-hosted constructors/extractors/serializer with one trusted recursive-drop routine (path A) -->
# v1 dynamic Value model — the yaml keystone (path A: self-host + ONE trusted recursive-drop routine)

**Status: DESIGN (2026-06-19). CEO chose path A ("Aでいくぞ"): keep the trusted base minimal, PROVE the Value model (constructors/extractors/serializer self-hosted in .almd, cert-verified), with the recursive free as the ONE trusted runtime routine (like `DropListStr` already is). Coq-free; byte-match-verified vs v0 `runtime/rs/src/value.rs` per brick.**

## ★★★★ LANDED 2026-06-19: the recursive Value-drop FOUNDATION (value.array + 3-level recursive free)

The path-A keystone — the ONE trusted-base addition — is BUILT and LEAK-VERIFIED (no longer staged):
- **value.array** (value_core.almd): a SELF-CONTAINED tag-5 Value `[rc@0][tag=5 @4][len@8][elem@12+i*8]`,
  shallow-copying each element handle in via `__varr_copy` (`prim.rc_inc`). Registered.
- **__drop_value** (recursive, raw-handle, EMPTY cert): tag-dispatched at the last ref (rc==1) — tag 5
  frees each element Value via `__vdrop_arr`→`__drop_value`, tag 4 frees the String, scalar nothing.
  `Op::DropValue` now renders to `(call $__drop_value …)` (REPLACES the flat inline drop that leaked an
  Array's element Values). Verified NO-regression (output-parity 64→65, all existing scalar/Str Value
  programs byte-match — they drop via $__drop_value identically).
- **__drop_list_value** + **Op::DropListValue** + the `value_elem_lists` set: a `List[Value]` frees each
  element via `$__drop_value` (a flat `DropListStr` would leak each element Value's payload). The cert
  treats `DropListValue` as one `d` (added to lib.rs + certificate.rs groupings). Wired into BOTH
  `emit_scope_end_drops` AND `drop_arm_locals` (the per-iteration/arm teardown — the bug found by the
  leak loop: the loop body used `drop_arm_locals`, which initially still emitted the flat `DropListStr`).
- **List[Value] materialize**: `try_lower_str_list_literal` admits `[value.int(1), value.str("a")]` (Call
  elements) + marks `value_elem_lists`.
- `prim.rc_dec`/`rc_inc` whitelist += `__drop_list_value`.

§4.1-COMPLIANT: NO new hand-written WAT runtime function — `$__drop_value`/`$__drop_list_value` are
self-hosted Almide (auto-linked with value_core) reusing the proven `$rc_dec`/`$rc_inc`. The 3-LEVEL
recursive free (Array → element-Value → String) is LEAK-VERIFIED by `spec/wasm_cross/value_array_leak_loop.almd`
(a 20000-iter create+drop loop with a Str element — completes "done"; a leak would OOB-trap past 64KB
well before ~320 iters since the allocator has a free list). GATES: corpus-wall ACCEPT (14491 heap, no
leak/double-free) + cargo test (incl. §4.1 + cert) + output-parity 65/65. This is the soundness-core the
CEO gated "design-first" — now empirically verified by the leak loop (the cert canNOT see a tag-5 leak).
REMAINING for the 8 Value walls: `value.as_array` (read-back, needs a Result-of-List[Value] rep whose
drop is value-aware — another tag-dispatched drop) + `value.stringify` (recursive serializer) + value.object.

## ★★★★★ LANDED 2026-06-19: value.as_array — yaml 15→13 (is_compound + emit_non_int)

The READ side of the Value-array model. `value.as_array(v)` returns `Result[List[Value], String]` in
the cap-as-tag rep (REUSING the str-result `materialized_results_str` MATCH machinery — tag @16, payload
@12 — so the Camp-4 borrow-bind `ok(items) => …` is the str_heap_bind borrow+drop-after from fab14729,
NO new match code). The DROP is the new part: a value-array Result frees its `List[Value]` payload
RECURSIVELY (`Op::DropResultListValue` → self-hosted `$__drop_result_lv`: tag@16 0→`$__drop_list_value`,
1→rc_dec; a flat `DropListStr` would leak the list's element Values). Tracking is TYPE-DRIVEN
(`is_result_listval_ty`: Ok-arm is a `List`) so it's sound at EVERY str-result marking site
(try_lower_variant_value_match, lower_branch, seed_variant_param, lower_bind) — `value_result_lists`
when the Ok is a List, `heap_elem_lists` when a String. Drop-op selection unified into one
`drop_op_for` helper (emit_scope_end_drops + drop_arm_locals + the variant-match subject drop). ALSO
fixed a latent leak: `materialized_call_arg` now marks a `Value` call-arg `value_handles` (→ recursive
`DropValue`), not a flat `Drop` — a `f(value.array([…]))` arg was leaking its element Values.
VERIFIED: `value_as_array_roundtrip.almd` byte-matches v0 (arr_len 3/-1/0, is_compound 1/0 — value.array
STORES + value.as_array READS correctly + the Ok(items) borrow + tag dispatch) + `value_as_array_leak_loop.almd`
(the 3-LEVEL Result→List[Value]→Value→String drop completes "done"); corpus-wall ACCEPT + cargo test +
output-parity. REMAINING Value walls (6): collect_map/collect_seq/seq_item/parse_lines/parse_nested/map_entry
(build value.array/object during parsing) + value.object/value.stringify.

## ★★★★★★ LANDED 2026-06-19: list-iterator TCO (heap-loop-carried escape) — yaml 13→11 (oct_rec, bin_rec)

The FIRST heap-loop-carried recursion cleared WITHOUT a cert extension — a cert-clean TRANSFORM
(extends scalar-TCO). A tail-self-recursive fn over a SHRINKING list (`f(.., cs)` matched on
`list.first(cs)`, recursing with `list.drop(cs,1)`) is rewritten so `cs` is an INVARIANT borrowed
list + a synthetic scalar INDEX `idx`: `match list.first(cs) { none => BASE, some(ch) => BODY }` →
`if idx < list.len(cs) then { let ch = cs[idx]; BODY } else BASE`, and each `f(list.drop(cs,1), …)`
bumps `idx += 1` (in `tco_rewrite`). `cs` is never reassigned → the loop is the proven cert-clean
scalar form; NO heap back-edge merge, NO `verify_ownership` change. `try_list_iter_rewrite` (lower/mod.rs)
runs BEFORE `tco_collect` (which bails on a `match` body). VERIFIED: `spec/wasm_cross/list_iter_tco.almd`
byte-matches v0 (bin_rec 5/15/0/-1 direct-arm + a block-arm `cnt` len 5/0); corpus-wall ACCEPT (14548
heap) + cargo test + output-parity 67/67.
2 BUGS FIXED en route:
- `max_var_id` skipped PATTERN-bound vars (`some(ch)`), so the synthetic `rk`/`idx` COLLIDED with `ch`
  → the renderer reused one local for an i32 handle AND an i64 flag = invalid wasm. Now counts pattern
  binds (also hardens the existing scalar-TCO).
- (commit 1601af5e) A scalar `??` in a BinOp OPERAND position (`(int.parse(s) ?? 0) - 48`,
  `(codepoint(ch) ?? 0) - 48`) fell through `lower_scalar_value` to a `Const 0`, so the WHOLE BinOp
  silently read 0. This was a GENERAL pre-existing silent miscompile (mis-diagnosed at first as a
  "codepoint-borrow" gap — codepoint(cs[i]) alone is FINE; the bug was the `?? · - 48`). Added a
  scalar-`??` arm to `lower_scalar_value_inner` (`try_lower_option_unwrap_or`, gated on a scalar
  fallback). With it, oct_rec/bin_rec now BYTE-MATCH v0 end-to-end (oct 15/511/.../83, bin 5/15) —
  `spec/wasm_cross/unwrap_or_operand.almd` + the oct/bin probe. So oct_rec is FULLY correct (lowering +
  render), not just walled-clear; it only still needs float.parse to render inside a full yaml program.
NEXT: the APPEND-accumulator TCO variant (collect_seq/flow_rec `acc + [x]` → in-place push) +
mutual-recursion (flow_rec↔flow_step), then value.object/stringify + tuple-heap (block_*).

## yaml wall countdown (the live tally)

74 functions → walls: 22 (session start) → 21 (float.parse recognition) → **19** (scalar-arg TCO, commit 77c91648) → **17** (nested heap-result `match` arm, commit 5510dc47) → **16** (str-result heap-payload bind = `emit`, commit fab14729) → **15** (let-bound variant-call read-shape seed = `num_signed_base`, below). Remaining 15: oct_rec, bin_rec (heap-loop-carried + match-leaf), flow_rec, flow_step (mutual-rec + heap acc), block_scalar/block_line/block_nonblank (tuple-heap + let-bound heap-if), parse_lines, map_entry, parse_nested, collect_map, collect_seq, seq_item, emit_non_int, is_compound (the Value-array model: value.array/as_array/object/stringify + recursive drop).

### LANDED 2026-06-19: let-bound variant-call read-shape seed — yaml 16→15 (num_signed_base)

`num_signed_base`'s `let parsed = if kind=="o" then parse_oct(d) else parse_bin(d); match parsed {…}`
walled even though (a) the let-bound-heap-`if` tail-duplication (`desugar_let_bound_heap_branch`)
distributes the `match` into each arm and (b) the nested-`match`-arm brick lowers a match arm. The
remaining gap: a LET-BOUND user-function Result/Option var was NOT seeded with its variant READ-shape,
so `match parsed` saw an untracked subject and walled — even though the DIRECT call-arg position
(`lower_call_args`'s Named arm, calls.rs:1075) already seeds it via `seed_variant_param`. FIX: call
`seed_variant_param(dst, ty)` on a variant-returning Named user call in `lower_bind` too — the exact
mirror of the direct path. `seed_variant_param` adds ONLY layout/read knowledge (the bound `dst` is
already an owned heap value dropped at scope end), so NO ownership/cert change. VERIFIED:
`spec/wasm_cross/letbound_variant_match.almd` byte-matches v0 (both a plain `let p = mk(b); match p`
and the full `let parsed = if … ; match parsed` num_signed_base shape); corpus-wall ACCEPT; output-parity
+ cargo test green. (block_scalar shares the let-bound-heap-`if` but ALSO returns a `(Value, Int)` heap
tuple — the tuple-heap brick — so it stays walled.)

### LANDED 2026-06-19: str-result heap-payload bind — yaml 17→16 (emit)

The Camp-4 frontier for a str-result (`Result[String, String]` from `value.as_string` — slot-0 @12
owns the ONE String, the Ok/Err tag at @16, read via `(i32.load)` = low-32 handle only). `emit`
(`match value.as_string(v) { ok(s) => emit_scalar(s), err(_) => emit_non_string(v, ind) }`) binds the
Ok String AND returns a heap result — the case `try_lower_variant_value_match` gated out at 936 (the
subject-drop-BEFORE-arms desugar can't borrow a dropped subject). RESOLUTION (cert-clean, no checker
change): for a str-result heap bind, bind the payload as a BORROW (`LoadHandle` @12, in `param_values`),
DEFER the owned subject's drop to AFTER the branch-join (so the borrow is live through both arms), and
rely on the bare-Var arm's auto-acquire (`Op::Dup`) — so the drop-after frees slot-0 exactly once
whether an arm borrows the payload (a call arg) or returns it. Also widened the Ok-arm bind from
`scalar_bind` to `heap_or_scalar_bind` (self-gated on `heap_elem_lists`, so a scalar Result still
rejects a heap bind — no regression). A NON-str heap payload (heap-Result-of-list, Array element) has
no single-slot borrow rep yet → still the true Camp-4 frontier (the Value-array model). VERIFIED:
`spec/wasm_cross/str_result_heap_bind.almd` byte-matches v0 for BOTH the borrow-payload and
return-payload shapes (5/5); corpus-wall ACCEPT (4234 fns, 14392 heap objects — the ownership cert IS
the leak/double-free check, so accept ⟹ the drop-after is leak-free); output-parity 62/62 + new fixture;
the 3 non-baseline MISMATCH files remain pre-existing.

### LANDED 2026-06-19: nested heap-result `match` arm — yaml 19→17 (try_decimal, parse_number)

`lower_heap_result_arm` (control.rs) handled Module-call / Named-call / nested-`if` / ctor arms but
fell to `_ => None` on a nested `match` arm, walling any heap-result `if`/`match` whose arm is itself a
`match` — the `try_decimal` shape (`match int.parse(c) { ok(n) => value.int(n), err(_) => match
float.parse(c) {…} }`) and `parse_number`'s then-arm (`match int.from_hex(..) { ok(n) => value.int(n),
err(_) => value.str(raw) }`). FIX: add a `Match` arm case that recurses through the SAME proven
machinery the tail position uses — `try_lower_variant_value_match` for a variant subject (subject-drop-
before-arms over a scalar payload + a heap-result-`if` skeleton), `desugar_match_to_if` +
`lower_heap_result_if_inner` for an Int-literal subject. The recursion already `Consume`s each leaf and
returns the merged if-result `dst`, so the arm adds NO extra `Consume` (exactly like the nested-`If`
arm). CERT-CLEAN: it composes two already-proven, internally-balanced lowerings — NO new MIR op, NO
checker change. VERIFIED: a focused fixture (`spec/wasm_cross/nested_match_heap_arm.almd`, linkable
int.parse/int.from_hex only) byte-matches v0 (5/5 cases); corpus-wall ACCEPT (4231 fns, 14370 heap
objects, 3 props); output-parity baseline 61/61 (no regression) + the new fixture ratcheted in; the 3
non-baseline MISMATCH files are PRE-EXISTING (confirmed by an A/B stash test — unrelated to this brick).
NOTE: try_decimal/parse_number now LOWER (wall gone) but a full-yaml program using them still needs
float.parse (strtod) to RENDER — the lowering wall and the link wall are independent counts.

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
4. **The drop is UN-GATE-VERIFIABLE by the EXISTING gates (the soundness crux) — but a LEAK TEST works.**
   A wrong tag-5 drop = a silent LEAK: the output is still correct (leak happens after the print) and the cert
   sees DropValue as one balanced `d` — so neither output-parity NOR corpus-wall catches it. **RESOLVED
   2026-06-19: the allocator HAS a free list (`$freelist`: `$rc_dec` at rc 0 pushes the block, `$alloc`
   reuses a same-size block), so a LEAK TEST is feasible — a `while i < 100000 { let xs = [..]; … }`
   create+drop loop COMPLETES if freed (freelist keeps memory in page 1) and TRAPS (OOB store past 64KB) if
   leaked. PROVEN: a 100000-iter List[String] create+drop loop runs to "done" → DropListStr is leak-free.**
   So the Value drop CAN be trusted: leak-test `while i<100000 { let a = value.array([value.int(i)…]); … }`.
   Self-recursion can NOT drive the loop (no TCO → stack overflow at ~100k depth); a `while` loop must.

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

## ★★ FULL BUILD ATTEMPT 2026-06-19 — built end-to-end; the ONE remaining gate is the LAYOUT BRICK (as_array usage)

Drove the entire coupled batch this session and CONFIRMED each piece by build + run, then re-staged
(unverifiable end-to-end → not shipped, per ②). What WORKS (verified):
- `prim.rc_dec`/`rc_inc` + `prim.load_handle` (COMMITTED) + the whitelist (to `__drop_value`/`__varr_copy`/`__vfill`).
- **List[Value] materialize** — `[value.int(1), …]` admits Call elements (Module value.* via lower_pure_module_value_call, Named via CallFn), Consumed into the nested-ownership list. BUILT (walled before, renders now).
- **value.array** (self-contained tag-5, shallow-copy via `__varr_copy`+rc_inc) — the emitted wat is CORRECT by inspection (alloc n slots, store tag@4=5 / len@8=n, rc_inc+store each element).
- **__drop_value** (recursive raw-handle, Unit self-rec helpers `__vdrop_arr` — NOT guarded) — RAN in production earlier.
- **value.as_array** (extract to a fresh List[Value] via `__vfill`+rc_inc; `prim.alloc_list_str` is generic `[A]` so `alloc_list_str` typed `List[Value]` needs no new prim) — CONSTRUCTS correctly.
- `Almide gotchas CONFIRMED: Unit literal is `()`; `value.at` is NOT a v0 fn (frontend rejects) — use the real v0 `value.as_array`; the allocator HAS a `$freelist` so the leak-loop gate works.`

THE ONE REMAINING GATE — **the LAYOUT BRICK (payload-precise heap binding)** — blocks as_array USAGE:
`match value.as_array(a) { Ok(items) => items[i] / list.len(items) }` MIS-BINDS. Two coupled causes:
(1) `bind_pattern` (control.rs ~150) binds a HEAP payload by Dup'ing the WHOLE subject (container-grain —
its own comment says "payload-PRECISE identity needs the layout brick"), so `items` = the Result shell,
not the inner list. (2) The heap-Result-of-LIST is not tracked: `materialized_results_str` only marks
`Result[String,_]` (the cap@16 str-result machinery, 7b24ef8f); a `Result[List[Value],String]` is
unrecognized, so the match linearizes/mis-reads. EVIDENCE: `value.array([7,8,9])` then
`Ok(items)=>list.len(items)` prints 1, not 3 (reads the shell, not the list). So the NEXT brick is the
layout brick: payload-precise heap-payload binding + heap-Result-of-list tracking. Then re-paste the
staged value.array/as_array/__drop_value, register them, wire Op::DropValue→$__drop_value, and verify
with a round-trip (`value.as_int(items[i])`) + the leak loop. yaml's emit/collect then unblock.

## ★★★ THE FINAL GATE PINNED PRECISELY 2026-06-19: the Camp-4 frontier (heap-payload bind over a heap-Result)

The "layout brick" for as_array usage is, precisely, the **Camp-4 frontier** in `try_lower_result_match`
(control.rs ~916-939). `match value.as_array(a) { Ok(items) => … }` where `items: List[Value]` is a
HEAP-payload bind over a HEAP-Result. Two hard stops:
- `Ok(inner)` uses `scalar_bind` (line 919) which admits ONLY scalar/wildcard — a heap `items` → rollback.
- Even via `heap_or_scalar_bind`, line 936 ROLLS BACK any heap-payload bind when `heap_res` (the
  "Camp-4 frontier: defer").
ROOT CAUSE (a real cert-design problem, not a one-liner): in the cap-as-tag rep (value.as_string,
7b24ef8f) the Ok payload ALIASES the subject (same handle). The heap-Result match does SUBJECT-DROP-
BEFORE-ARMS (line 965) to make the arms a clean scalar-cond `if` — but then a payload BORROW (`items` =
LoadHandle@12 = the subject) DANGLES. The alternative (MOVE the payload into each arm's bind, arm owns +
drops) makes `verify_ownership` — which processes the if-arms FLAT — see TWO drops of the one subject =
a false double-free (the checker doesn't model then/else mutual exclusion). So neither the borrow nor
the move is cert-clean today.
THE REAL FIX (one of): (a) teach `verify_ownership` then/else MUTUAL EXCLUSION (a drop in `then` and a
drop in `else` of the same object is ONE net drop) — the principled fix, unblocks heap-payload binds
generally; or (b) a NON-SHARED Result rep (a separate Ok shell `[rc][tag][_][payload@12=list]` so the
payload is a distinct owned object the arm moves out, and the shell drop frees only the shell). (a) is
the better long-term (it also lifts the Err-heap-bind + Some-heap-bind deferrals). This is ②-critical
(a wrong choice = UAF or double-free), so it is design-first, NOT a session-end rush. Everything ELSE
in the Value model is built + verified (see the FULL BUILD ATTEMPT section above); THIS is the one gate.

## Camp-4 depth, fully investigated 2026-06-19: a LOWERING-ONLY path exists, but needs a NEW heap-Result-of-list rep

Good news (no kernel/Coq change needed): the cert ALREADY backstops the hard part. If the heap-payload
match binds `items` as a BORROW (LoadHandle of the subject's payload slot) and drops the SUBJECT
**after** the arms (not the current before), `verify_ownership` sees ONE subject drop — flat-cert-clean.
And the no-alias safety is FREE: an arm that RETURNS the borrowed payload (`ok(items) => items`) is a
"return of a non-owned value" the cert already REJECTS (lib.rs:725), so only use-not-move arms (yaml's
`ok(items) => emit_seq(items, ind)`) pass. So Camp-4 is a LOWERING change to `try_lower_result_match`
(borrow-payload + drop-subject-after), NOT a checker/Coq change. (I was over-conservative earlier.)

The REMAINING real work — a NEW heap-Result-of-list REPRESENTATION: `value.as_array : Result[List[Value],
String]` can NOT reuse the str-result cap@16 rep (value.as_string, is_self_host_result_str_module_fn).
The cap-as-tag writes the Ok/Err tag at the payload's @16 — fine for a String (that's its byte region)
but for a `List[Value]` @16 is the HIGH 32 bits of `elem0` (slots at @12,@20,…) → the tag would CORRUPT
element 0. So a list-payload Result needs a SEPARATE Ok-shell rep `[rc][tag@4][_][payload@12=list]` (the
match reads tag@4, binds `items`=LoadHandle@12) — distinct from the str-result cap@16 machinery. THE
PLAN: (1) a `value`-result-of-heap rep + tracking set (separate shell, not cap@16); (2) admit Ok(heap)
bind in try_lower_result_match (scalar_bind→heap_or_scalar_bind for Ok) with borrow + drop-subject-AFTER
for THIS rep only (don't perturb the proven str-result drop-before path); (3) verify the round-trip
(`value.as_int(items[i])`) + corpus-wall + no str-result regression. This is a focused rep+lowering
brick (NOT kernel/Coq) — the cert rejects misuse, so it is verifiable, just delicate (a wrong rep is a
silent-corruption class that corpus-wall's cert may not catch — needs the round-trip + leak gates).

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
