# v1 — the parser-TCO lever (the real "heap-result-expr" cross-repo lever)

## csv full-conquest status (4 public fns, v0-vs-v1 byte-match audit)

Audited each `almide/csv` public fn end-to-end (inline the source, v0 `almide run` vs v1
render_program+wasmtime). **✅ FULL CONQUEST — 4 of 4 byte-match** (verified: byte-match + 100000×
leak loop clean + corpus-wall ACCEPT + mir suite). Three distinct mechanisms, fixed at the root:
- ✅ **stringify** (commit 6fb48108 — needed the non-capturing heap-map inline, closing the lift
  path's nested-map silent miscompile that returned `,`).
- ✅ **stringify_records** (commit b129ad45 — the capturing heap-map / map-closure-over-Value).
- ✅ **parse** (commit 646aa233 — the TCO result-accumulator fix: parse_rows_rec's `paf(…, cur+[field])`
  base, which reads the loop-body-local `field`, is now computed IN the loop via a result accumulator
  instead of the post-loop dispatch where `field` was dead. THE drop-placement bug, fixed at the root).
- ✅ **parse_records** (commits fba2e960 + af8dcdf7 — the nested-heap-list element ops the parse trap
  had masked: `list.get(rows,0)` / `list.drop(rows,1)` over a `List[List[String]]`. The `_str`
  variants deep-copy the inner list via `string.repeat` (its length word read as a byte count) — a
  silent miscompile for get, a double-free trap for drop. New handle-SHARE `list.get_liststr` /
  `list.{take,drop}_liststr` + `option.liststr_unwrap_or`: each inner list co-owned by rc_inc + raw
  store64 (`__ldls_share`, whitelisted like `__varr_copy`), freed once at the last ref).
- ❌ **parse_records** — the map machinery (enumerate+map fusion + the tuple-element map, commit
  a9aecee5) and the parse_rows_rec double-free (the TCO fix, 646aa233) are BOTH done; what remains is a
  THIRD, separately-revealed issue the TCO trap had been masking: **`list.get` / `list.drop` over a
  NESTED-heap list `List[List[String]]`** (the `rows` parse_rows returns). Isolated by reducing
  parse_records body (`va*` repros):
  - `let header = list.get(rows, 0) ?? []` → **SILENT MISCOMPILE** (`va2`: v0 `[2]`, v1 `[0]`). The
    dispatch (lower/mod.rs:2354) routes ANY heap element to `list.get_str`, which DEEP-COPIES the
    element via `string.repeat` — correct for a leaf `String` element, but a COMPOUND-heap element
    (`List[String]`, `List[_]`) must be SHARED by handle (like the `is_value_ty` → `list.{f}_value`
    branch at :2350) and tracked by its REAL drop type (`List[String]` → DropListStr, not `_value`'s
    DropValue). Needs a compound-heap accessor + element-drop tracking, not just the dispatch line.
  - `let data = list.drop(rows, 1)` → **TRAP** (`va3`). The sublist op is not heap-element-aware for
    `List[List[String]]` (the moved element handles' rc is mishandled).
  So parse_records → 4/4 needs the nested-heap-list element accessors (`list.get`/`first`/`last`
  handle-share + drop-type tracking for compound elements) and a heap-element-aware `list.drop`. The
  `va2` silent miscompile is a ②-discipline item (mis-dispatch to `_str`); minimum sound step = WALL a
  compound-heap-element `list.get` (vs the quiet `_str` deep-copy) until the accessor lands.
- ❌ **parse** (+ the rest of parse_records) — the SHARED `parse_rows_rec`/`parse_after_field`
  double-free (rc_dec → `unreachable` in the prr↔paf mutual recursion). The cause is an INTERACTION,
  narrowed by three minimal repros (an honest correction of an earlier single-cause guess):
  - **dd** (direct self-recursion `prr(…, cur + [field])` with `let (field,np)=pf(…)`, NO paf) → ✅
    works. So the destructure + `cur + [field]` ALONE is sound (my earlier "destructure drops the
    tuple before the field's use" was WRONG — dd has exactly that and is fine).
  - **B** (prr↔paf MUTUAL recursion, NO destructure) → WALLS (out of subset, not a trap).
  - **prrep** (BOTH: prr↔paf mutual recursion + the `let (field,np)=pf(…)` destructure + `cur +
    [field]`) → TRAPS (double-free).
  FINAL trigger (regression test `parse_rows_rec_destructure_mutual_recursion_double_free`, #[ignore]):
  the precise combination is a **SELF-recursive sibling arm + a destructure-in-the-nested-else**. In
  prr's inner `if c == "," then prr(…) else { let (field,np)=pf(…); paf(…, cur+[field]) }` the THEN is
  SELF-recursive (`prr(…)`) and the ELSE destructures an owned tuple then uses the borrowed `field` —
  the owned pf-tuple is dropped (rc_dec'ing its String slot) BEFORE the `cur+[field]` concat reads it
  (wat: drop at 479 < concat at 551) → freed String copied → double-free. CONFIRMED trigger: the `ns`
  repro — same code but the inner THEN is `paf(…)` instead of the SELF-recursive `prr(…)` — does NOT
  trap (the whole prr then lowers as the executable heap-result-if). So the self-recursive sibling arm
  is what pushes the destructure-else onto the drop-misplacing path. dd (a FLAT `else { destructure;
  prr(…) }`, self-recursive but no nested if) is also fine (drop at 374 AFTER concat 347). Dup-ing the
  field does NOT fix it (Dup dropped at the same misplaced point). A focused but deep drop-placement
  fix for the destructure × self-recursive-sibling-arm × nested-if interaction; high regression risk.
  map machinery DONE (a9aecee5); both parse + parse_records trap ONLY here. NOT in corpus-wall (csv ∉ corpus).

## THE LAYOUT BRICK read side — OPEN (commits 5b7efec7, e43db65f)

The heap-Result-of-X read/`??` is the layout brick the value-subsystem flagged. Now landed for the
full Result family — each byte-matches v0, leak-loop clean, corpus-wall ACCEPT, suite green:
- **value.get** → `Result[Value,String]` (self-hosted: linear-scan `__vobj_find` index, then a
  non-recursive `Ok(@12 borrow)`/`Err("missing field '<k>'")` wrap). `match` reads tag@16 + binds the
  @12 Value (classified `value_result_results` → recursive `DropResultValue`); was garbling ok:0|err:0.
- **value.as_array** → `Result[List[Value],String]`; **value.as_string** → `Result[String,String]`.
- The `??` routes each Ok-payload kind to a self-hosted helper reusing the working match read:
  `result.value_unwrap_or` / `result.list_value_unwrap_or` / `result.str_unwrap_or` (the Ok arm Dup's
  @12; precise `is_result_str_str_ty` gates the str helper vs result.zip's tuple-Ok). count_ir_calls
  credits the synthetic call so mir==ir. The handle else-if + Var-case admit the Value/List operands.

- ✅ **Option-of-Value read** — DONE (commit cab0924b). `list.get` on a List[Value] dispatches to
  self-hosted `list.get_value` (NOT the `_str` variant, which `string.repeat`-copied the element,
  corrupting an Object to `{}`); it SHARES the element via `Some(@i)` (the `Some(Value)` ctor Dup's the
  borrowed Value like value.get's Ok), and the `??` routes Option[Value] to the prim-based
  `option.value_unwrap_or` (the value-match Some-arm's scalar_bind rejects a heap payload, so the
  helper reads len-tag@4 + @12 directly). PLUS a leak fix: a `value.as_array ?? []` operand OWNS its
  inner list → reclassified to value_result_lists (recursive `DropResultListValue`) in
  materialized_call_arg (the flat drop leaked the element Values, a loop OOMed); a Result[Value,String]
  Ok stays flat (CO-OWNED). Verified byte-match incl Object elements, 2000x leak-clean, corpus ACCEPT.

- ✅ **map-closure-over-Value** — DONE (commit b129ad45/264976e8). csv **stringify_records byte-matches
  v0** (incl CSV quoting `LA, CA` → `"LA, CA"`). The C1 inline `list.map` was extended to a HEAP-element
  source AND result: a CAPTURING closure over a List[Value]/List[String] (no liftable env) inlines as a
  specialized loop — the element is read by `LoadHandle` (a borrowed i32 the body uses, e.g.
  `value.get(row, h)`), the per-element body lowers via `lower_heap_result_arm` (a general heap expr:
  call / concat / nested `list.map … list.join` / the new `??` arm routing `value.as_string ?? ""`
  through the unwrap helpers), and the fresh owned result is Handle-extended + stored into a DynListStr
  slot tracked `heap_elem_lists` (recursive drop). The nested cell projection (outer map over rows
  capturing `header`, inner map over header capturing `row`) inlines BOTH levels. Gated to CAPTURING
  closures (non-capturing heap maps keep the proven `list.map_str` lift path) + STRING-element results.
  Verified: all map-closure-over-Value shapes byte-match, 2000x leak-clean, corpus-wall ACCEPT (16514
  heap objects, caps-transitive), suite 487/0, diff-fuzz green. **The full layout-brick lever is closed:
  heap-Result/Option READ family + Option[Value] + map-closure-over-Value all land csv stringify_records.**


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
