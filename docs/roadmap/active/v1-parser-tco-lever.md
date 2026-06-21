# v1 — the parser-TCO lever (the real "heap-result-expr" cross-repo lever)

The org-trust dashboard's top wall reason (~40, blocking toml/svg/aes/base64/csv) reads as the
"heap-result-expr family" (`heap-result if`/`match` … "would move out an empty deferred heap
value"). Targeting csv (a working v0 oracle, unlike toml — see below) revealed the TRUE cause.

## Finding: it is NOT the heap-result ARM shapes — those already lower

`lower_heap_result_arm` (control.rs) already handles tuple-construct arms, Named/Module-call arms,
concat arms, nested if/match, blocks, Option/Result ctors. csv's `heap-result if` walls come from
ONE deliberate guard: a **self-recursive call arm is walled** (control.rs ~2162):

```
if name.as_str() == self.fn_name { return None; }  // v1 has NO TCO → deep recursion traps
```

csv's parser is all tail-self-recursion: `parse_unquoted_field(text, pos+1, acc+c)`,
`parse_quoted_field`, `parse_rows_rec`, `parse_after_field` — each recurses, so each heap-result
`if` hits the self-rec guard. So the lever is **TCO of self-recursive heap-result parser
functions**, NOT the arm shapes.

## What TCO already covers (`try_tco_rewrite`, mod.rs:2734) vs the gap

Covers: (1) a list-iterator forward scan (`list.drop(cs,1)` carried), (2) APPEND ACCUMULATORS
(`acc + [x]`, `ConcatList`) → an owned loop-carried slot (option C, cert `check_cert_lc`). yaml
(byte-verified) lowers because its parser fits these.

GAP (csv/toml parser-combinator shape):
- **String accumulator** `acc + c` (`ConcatStr`, not `ConcatList`) — extend the append-accumulator
  to a String slot (the same drop-old/alloc-new-per-iter, cert `i(id)m`).
- **Tuple-result base** `(acc, pos)` — the base returns a `(String, Int)` carrying the accumulator
  + the scalar position, not the carried type directly.
- **Multi-accumulator + tuple-destructure self-calls** (`parse_rows_rec`: carries `rows`,
  `current_row` both `List`, and a self-call's arg is `current_row + [field]` where `let (field, np)
  = parse_quoted_field(...)`).

## Plan (byte-match-first; csv has a WORKING v0 oracle)

Oracle: `parse("a,b,c\n1,2,3\n\"x,y\",4,5\n")` → v0 native = `[["a","b","c"],["1","2","3"],["x,y","4","5"]]`
(confirmed). Driver = csv/src/mod.almd + an `effect fn main` calling `parse` (single file →
render_program). Target: v1 == that.

1. Extend the append-accumulator in `try_tco_rewrite` to a **ConcatStr (String) accumulator** +
   the smallest tuple-result base — unblock `parse_unquoted_field`/`parse_quoted_field`. Gate:
   corpus-wall ACCEPT (the loop-carried cert `check_cert_lc`) + a String-accumulator leak loop +
   byte-match.
2. Multi-accumulator + tuple-destructure self-calls — `parse_rows_rec`/`parse_after_field`.
3. Then `parse` (the `ok(value.array(...))` ResultOk) + `parse_records` (a `list.map` closure)
   lower in cascade. csv → byte-match `[["a"…]]`.

EACH step gated on corpus-wall ACCEPT (TCO is correctness/leak-prone — the loop-carried-slot cert
is the gate) AND the csv v0==v1 byte-match. The lever clears the same class across toml/svg/aes/
base64 (all parser-shaped).

## PROGRESS (commit 63a7a1a6) — step 1 DONE + a pre-existing miscompile fixed

While wiring the ConcatStr accumulator the byte-match surfaced a PRE-EXISTING silent miscompile (the
② cardinal violation): a TCO loop body is `{ if base then … else step }`, so the base-check arrives
as a BLOCK-TAIL `if`, and that tail fell STRAIGHT to `lower_branch` (run BOTH arms with the cond
record-elided) — turning `if done then {rk:=k} else {step}` into an UNCONDITIONAL `rk:=k`, so the
loop ran exactly ONCE. ANY recursive parser with a heap `let c = peek(...)` in its body hit it (v0
`hello`, v1 `h`). **Fix**: route the block-tail if/match through `try_lower_unit_if` FIRST (a real
branch); fall to `lower_branch` only when it cannot execute. This both kills the miscompile AND makes
the scalar-index append-accumulator parser loops EXECUTE.

DONE in this commit:
- ✅ block-tail base-check now branches (the run-once miscompile fixed — list AND string).
- ✅ ConcatStr (String) accumulator + tuple-result base `(String, Int)` — `is_self_append` matches
  ConcatStr, the upfront slot-copy is String-aware (`acc + ""`). Leak-loop verified (2000×).
- ✅ corpus-wall ACCEPT (ownership 16303), diff-fuzz green, the 4 `*_loop_reclaims` tests still pass,
  a new wasmtime cargo test (`string_accumulator_parser_tco_executes_on_wasmtime`).

## PROGRESS (commit 1d8bdd92) — step 2 partial: multi-accumulator reset + cross-read

The multi-accumulator gap decomposed into FOUR sub-gaps (minimal repros each). Two are now DONE:
- ✅ **RESET** a heap accumulator to a fresh empty (`cur = []` / `acc = ""`) — admitted as a
  loop-carried slot update (the parser resets the current-row acc after a delimiter).
- ✅ **heap-acc-reads-heap-acc** (`out = out + cur` while `cur = ""`) — per-iteration heap assigns
  emitted in READ-DEPENDENCY topological order (reader before readee); only a CYCLE walls.
  A two-String-accumulator parser now byte-matches v0 (leak-loop verified, cargo test
  `multi_accumulator_reset_and_cross_read_tco_executes_on_wasmtime`).

DONE (commit cd8ad5e6): ✅ **scalar-var list literal** `[pos]` — `lower_call_args` materializes it via
`try_lower_scalar_list_construct` (flat `DynList` + store64).

DONE (commit fc4d8425) — THE BOSS: ✅ **nested heap-element list** `List[List[String]]`. New
`Op::DropListListStr` renders a NESTED wasm loop (free each row's cells, each row, then the outer
block); `try_lower_concat_list` admits a `List[String]` element (`rows + [cur]`, `__list_concat_rc`);
`try_lower_str_list_literal` builds the `[cur]` singleton; the in-loop assign handles a RESET
(`cur = []`); EVERY value of this type routes to a new `list_list_str_lists` set (via
`is_list_list_str_ty`, checked BEFORE `is_heap_elem_list_ty`) so its drop is the nested one. The leak
loop first OOM-trapped (call-result temps routed to the flat drop) → fixed by routing at all tracking
sites. csvcore byte-matches v0, 2000× leak loop clean, corpus-wall ACCEPT, csv classify **5/6 → 7/4**
(parse_rows_rec + parse_after_field now lower).

DONE (commit b871b73d): ✅ **`[]` heap-result-if arm** — `lower_heap_result_arm` materializes an empty
list arm (`if is_empty(t) then [] else parse_rows_rec(...)`). csv 7/4 → 8/3 (parse_rows lowers).

FINDING (probes): the **`list.map` closure** lever the dashboard suggested is LARGELY ALREADY DONE —
scalar / String / Value / block-body / nested-map / map|>join closures all byte-match. The actual
remaining csv walls are narrower (specific value-construction), not a general closure gap.

DONE (commit 47301322): ✅ **`Result[Value, String]` ok/err wrapper** (csv `parse`'s
`ok(value.array(...))`). New `Op::DropResultValue` → self-hosted `$__drop_result_value` (tag-dispatch:
Ok → `$__drop_value`, Err → `rc_dec`); `try_lower_result_value_ctor` (in lower_tail + the if-arm)
materializes Ok via `lower_owned_heap_field` (handles `value.*` + the nested `list.map`), routed to a
new `value_result_results` set (`is_value_result_ty`). ok/err + match-read round-trips byte-match;
corpus-wall ACCEPT; 2000× v1 no OOM. **csv classify 8/3 → 11/2 (parse lowers).**

## Value/JSON subsystem (the csv-records dependency) — Phase 1 DONE

The last 2 csv functions need the dynamic Value OBJECT + JSON ops SELF-HOSTED (none were in the v1
trust spine). A multi-fn subsystem, decomposed:

DONE (commit a0dcf31d): ✅ **value.object + json.keys** (the tag-6 Object). 2-slot-per-pair block
(key String + value Value, rc_inc'd in), recursive `__vdrop_obj` via `__drop_value`'s new tag-6 arm.
KEY bug caught: `@8` must hold the SLOT count (2·pairs = the alloc size the freelist reclaims) — the
pair count there leaked 2 slots/iter (a 2-pair OOM the leak loop trapped). round-trips byte-match v0,
2000× multi-pair leak loop clean, corpus-wall ACCEPT.

STILL TODO (the deeper Value pieces — each a real sub-gap, NOT a quick add):
- ❌ **value.get** — built the scan (scalar self-rec → index) + Ok/Err wrap, but (a) a self-recursive
  `Result[Value,String]` body produces invalid wasm (TCO×Result interaction), (b) the `?? value.null()`
  UNWRAP and the `match ok(v)` READ of a Value-Ok Result mis-bind / wall (the unwrap_or + match-read
  of a Value-Ok Result are unwired — only String-Ok is), (c) v0's Err message is `missing field
  '<k>'`, not a fixed string (byte-match needs the interp). Reverted (kept unbuilt, not a broken commit).
- ✅ **value.stringify** — DONE (commit 85e05369). The full recursive JSON serializer, self-hosted in
  value_core, byte-identical to v0 (null/bool/int/float/quoted-escaped-string/`[…]`/`{…}`). The
  array/object recursion is a String accumulator whose separator is `string.repeat(",", k)` with a
  SCALAR-if `k` — this SIDESTEPS the heap-result-if-in-loop-body that walled the first attempt (the
  real gap = `let sep = if i==0 then "" else ","` in a scalar-while body; the workaround needs no TCO
  extension). Two enabling fixes: (1) the auto-link (render_program + lower_source) now rewrites
  internal impl-name calls to the renamed call_name (so `__vstr_arr` recursing through
  `value_stringify` resolves to the renamed `value.stringify` def); (2) a `prim.load_str` borrow used
  as a Module-call arg is passed by Handle WITHOUT a scope-end drop (dropping it double-freed the Str
  Value's tag-4 payload). Plus a pre-existing fix: value_core is now force-linked on any Value-drop op
  (json.*-built Values' `__drop_value` was dangling — fixed json_scalar/json_string). All 7 value
  kinds + nested + escaping byte-match v0; 2000x leak loop clean; corpus-wall ACCEPT; suite 484/0.
  NOTE: the heap-result-if-in-loop-body is still a general gap (the TCO extension, route (a)) — only
  AVOIDED here, not closed.
- ❌ **parse_records closure** — `data |> list.map((row) => { … nested list.map → (key, value.str) …
  value.object(pairs) })`: a block-body closure building `List[(String,Value)]` then a Value object —
  the closure walls ("unliftable closure"), independent of value.object now existing.

## value.get + stringify_records — blocked by THE LAYOUT BRICK (heap-Result-of-Value/List)

ATTEMPTED value.get + reverted (walls + garbles; never shipped broken). Precise findings:
- v0 spec: `value.get(o,k)` → `Ok(v)` / `Err("missing field '<k>'")` (interp message). Used in
  stringify_records as `value.get(row,h) ?? value.null()`.
- value.get's CTOR walls: the body `if idx<0 then Err(msg) else { rc_inc; let val; Ok(val) }` is a
  heap-result `if` returning `Result[Value,String]`, and it walls "heap-result if outside the
  executable subset" — the `Ok(<rc_inc'd borrowed Value>)` + `Err(<concat/let message>)` arms aren't
  in `try_lower_result_value_ctor`'s subset (it handled Ok over a fresh `value.*` / list.map, not a
  borrowed-then-co-owned Object slot).
- value.get's READ garbles: a `match value.get(…){ ok(v)=>…, err(e)=>… }` returns `ok:0|err:0` instead
  of `ok:7|err:'missing field'` — the **Result[Value,String] read** (match + `??`) is unwired
  (`materialized_results_str` tracks only `Result[String,_]`; a Value-Ok payload read mis-binds).
- stringify_records' FIRST line is `value.as_array(v) ?? []` = a `Result[List[Value],String]` unwrap —
  the SAME layout-brick blocker (as_array mis-bind, value_core lines 101-112), reached BEFORE value.get.

So value.get AND stringify_records both need THE LAYOUT BRICK: payload-precise heap-payload
binding + heap-Result-of-Value/List ctor & read (match/`??`). A substantial, documented machinery
brick — NOT a quick add. (value.object/json.keys/value.stringify, which need only Value CONSTRUCTION
+ borrowed reads, all landed; the gap is specifically heap-Result-of-Value/List round-tripping.)

STILL WALLS (csv full byte-match — the last 2, both value.object-building closures):
- ❌ `parse_records` — `data |> list.map((row) => { … value.object(pairs) … })`: a block-body closure
  building a `value.object` from `header`/`row` (a `list.zip`/pairs shape) — a more complex Value
  construction than the `value.array(value.str)` map.
- ❌ `stringify_records` — heap-result `if` whose arms use `json.keys` + `value.object` + nested
  `list.map(... ) |> list.join` over Value objects.
NOTE: the byte-match DRIVER also needs `value.stringify` self-hosted (currently unlinked) — a
separate stdlib-runtime lever orthogonal to the lowering.

## SEPARATE blocker: toml's v0 oracle is BROKEN

toml was the first proposed target but `almide run` (native v0) emits INVALID Rust for it
(`error[E0308]: expected String, found &str`, 2×) — so toml has NO byte-match oracle until that v0
Rust-codegen bug is fixed. csv was chosen instead (v0 test passes). The toml v0 bug is a
v0-backend issue to fix separately before toml can be byte-verified.
