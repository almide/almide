# v1 — the parser-TCO lever (the real "heap-result-expr" cross-repo lever)

## ⭐ decode_chunks ONION — FULLY MAPPED (2026-06-22, turn 5, effort=max). The remaining "base64 decode / toml" frontier is ONE coherent slice: **TCO over a `match` whose result is a `Result`**, with an ownership-critical core. The unwrap-`!` desugar turns `let x = f(p)!; recurse(.., acc+[x])` into `match f(p) { ok(x) => recurse(.., acc+[x]), err(e) => err(e) }` — recursion inside a match arm, returning `Result[heap, String]`. To lower it (verified, repro `umr` = a 1-unwrap version; build it first, it is the unit test for the whole slice):

1. **TCO-over-match** — `tco_collect` + `tco_rewrite` must recurse `match` arms (today: only `if`/`Block`). Small, symmetric with the `if` arms. (Prototyped + reverted — sound but incomplete alone.)
2. **`tco_empty_for(Result[_,String])` → `err("")`** — a placeholder result-accumulator (a base overwrites it). Small. **This is what makes the TCO FIRE** (without it `tco_empty_for` returns None → TCO declines → "heap-result if" wall). Prototyped: the wall MOVED to layer 4, proving 1+2 work.
3. **loop-body match EXECUTION** — the TCO loop body becomes `match f(p) { ok(x)=>{acc=acc+[x]; p=..}, err(e)=>{rk=..; result=err(e)} }`; this user-`Result` STATEMENT match must execute not linearize → the lower_branch subject-tracking (control.rs:104) needs a USER named-call `Result` arm (scalar-Ok→materialized_results+heap_elem_lists; heap-Ok→materialized_results_str cap-tag). Also fixes a SEPARATE latent linearize-miscompile (hok2: a user heap-Ok `Result` in a Unit statement match runs BOTH arms today).
4. **⚠ scalar-loop Result-slot Assign (OWNERSHIP-CRITICAL — the cardinal-sin layer)** — `result = ok(acc)` / `result = err(e)` in the loop (mod.rs:1532 scalar-loop Assign handles only `acc=acc+[x]` append + `[]`/`""` reset; a `ResultOk`/`ResultErr` RHS defers → try_lower_scalar_while declines → "while body heap-accumulator reassignment" wall). MUST materialize via `materialize_result_str` AND **`Dup` the carried payload `acc` before move-in** — else the loop-end drop of `acc` double-frees the list `result` now owns. The cert MUST verify this (the reason this is a careful slice, not a force-through).
5. **REAL decode_chunks** then adds: 4-deep NESTED unwrap-matches (TCO-over-match handles nesting by recursion) + the `new_acc` nested-if accumulator INSIDE the innermost ok-arm (the landed flatten-let-block + inline-tail-accumulator + let-bound-if pre-desugar must compose with the match nesting — verify on the extracted function).

Do 1–4 together with a v0==v1 byte-test on `umr` BEFORE touching the corpus (corpus-wall does NOT catch byte-miscompiles). This single slice clears base64 decode (3 walls) + the bulk of toml (22). Prototyped 1+2+3 this turn (TCO fires); reverted because 4's ownership Dup + 3's coupling need careful joint implementation under the cert, not piecemeal.


## cross-repo conquest scoreboard (real `github.com/almide` repos on the v1 spine)

- ✅ **csv** — 4/4 public fns byte-match (see below).
- ✅ **svg** — FULL CONQUEST (2026-06-21). The records-based renderer (rect/text/group/doc/nested
  children + `map.entries` attrs) renders BYTE-IDENTICAL to v0 and is leak-free at 10⁴. The records
  language feature (construct / field read / spread / recursive nested-ownership drop / List[Record]
  literal+concat / `Map[String,String]` entries via the new `(String,String)` tuple-list) is complete
  on v1; `almide test` 15/0 (mod.almd via WASM). Full design + commit trail: [[v1-records-svg]]
  (STATUS 6). Cross-cutting fixes that also help every repo: the `not <bool-call>` let arm (lower_bind
  UnOp), the defunc-map self-recursion admission (`in_defunc_body`), `DropListStrStr` for
  `List[(String,String)]`.
- ✅ **yaml — RUNS ON V1 (2026-06-22).** `parse` compiles to a ~13.7k-line WAT (0 walls) and parses
  real YAML byte-correctly on wasmtime — maps, sequences, nesting, strings/ints/floats/bools all
  round-trip through `parse`+`stringify` (`a: 1 / b: hello / c:\n  - x\n  - y` → identical). The full
  chain executes: list.enumerate + find-over-(Int,String)-with-closure, list.set_str, the effect-fn
  (Value,Int) tuple-result TCO, the heap-result-if Match-arm temp frame, and heap-Value tail/bind drop
  fixes. REMAINING (not blocking "runs"): a leak long-tail over MANY parses (map ~ok to 1000×, the
  sequence path leaks sooner) — small per-call under-frees in the deep parser; see STATUS (2026-06-22,
  runs) below.
- 🟢 **bigint / rsa / sha1 / porta** — 0 lower-walls (all fns lower); `sha1` byte-matches its test
  vectors out of the box (2026-06-21 probe). Need only a byte-match audit + leak loop to bank.
- 🟡 **base64 (9/13) / aes (17/30) / almide-sqlite (20/28) / toml (22/48)** — mechanism walls remain.

## yaml status — the 1 wall is the heap-result-tuple TCO (option C engineering integration)

`collect_map` walls (`heap-result if … would move out an empty deferred heap value`). ROOT: it is a
MUTUAL-recursive parser (`collect_map ↔ map_entry ↔ after_colon`) that returns a `(Value, Int)` TUPLE
and accumulates `pairs: List[(String, Value)]`. Its tail SELF-call (`collect_map(…)!`, the blank-line
skip) is rejected by the heap-result-arm self-call gate — CORRECTLY, because v1 has no TCO for
heap-result tail recursion and a deep yaml would overflow the wasm stack. So admitting the self-call is
UNSOUND; the right fix is the **option-C append-accumulator TCO** (turn the same-level entry loop into a
real loop; the Coq loop-ownership soundness proof is already landed, commit 7f673b4c — this is the
*engineering integration* that remains).

PROGRESS (2026-06-21, then REVERTED to keep the tree sound): extending the list-literal builder +
`lower_owned_heap_field` Tuple arm to `(String,Value)` tuples (`DropListStrValue`/`str_value_elem_lists`
already exist from csv) makes the `[(k, value.str(…))]` accumulator literal MATERIALIZE — a minimal
`cm2` (literal-key `(Value,Int)` TCO accumulator) moves from WALL → **TRAP**. The TRAP is the CRUX and
why it was reverted (②discipline — do not ship a reachable miscompile):

- The TCO fires (the recursion becomes a real `while`), and `pairs = pairs + [(k, value.str("x"))]`
  lowers via the append-accumulator Assign. BUT `value.str(arg)` in the loop body MOVES its String arg
  into the Value, which escapes into the accumulator (Value → tuple → list → loop-carried `pairs`) — yet
  the per-iteration teardown (`drop_arm_locals`) STILL `rc_dec`s that String (it stayed in
  `live_heap_handles`), DOUBLE-FREEING it (`rc_dec(v13)` trap). This is a **loop-body-escape drop-balance
  bug**: a value-constructor argument that escapes into the loop accumulator is not Consumed out of the
  per-iteration frame. (`sv1`/`sv2` — the SAME concat/tuple OUTSIDE a loop — are leak-free, so the bug
  is specifically the loop-escape interaction.)
- Separately, the real `collect_map` (key = `list.get(lines, pos) ?? ""`, not a literal) still WALLs —
  a second issue (the `??`-keyed element / the mutual-inline of `map_entry` into a self-recursive shape).

ROOT CAUSE (fully de-risked 2026-06-21): `tco_empty_for` (lower/mod.rs:2952 — the result-accumulator's
initial empty value) handles ONLY `String`/`List`; for a `(Value, Int)` tuple it returns `None`, so the
TCO declines the in-loop RESULT-ACCUMULATOR path (the csv parse fix 646aa233) and falls back to the OLD
POST-LOOP DISPATCH, which recomputes the base `(value.object(pairs), pos)` AFTER the loop reading STALE
values → `pos` reads its entry value (`p=0`, not 3). So the fix is to make the result-accumulator path
available for tuple results. THE SETTLED 5-BRICK PLAN (implement fresh from this plan, gate each):

1. **`tco_empty_for` → Value + Tuple.** `Value → value.null()` (a clean empty Value); a scalar →
   `0`/`0.0`/`false`; `Tuple[a,b] → (tco_empty_for(a), tco_empty_for(b))` recursively. This routes
   `(Value,Int)` results onto the in-loop result-accumulator (no stale post-loop dispatch).
2. **The tuple result-accumulator DROP.** The accumulator slot holds a `(Value, Int)` tuple; each base
   reassignment (and the initial empty) drops the old via a NEW `Op::DropTuple`-style op (free slot-0
   `Value` via `$__drop_value`, slot-1 scalar no-op, then the tuple block). Generalize the existing
   `record_masks`/per-slot recursive drop to a 2-slot tuple with a Value slot. (Only ONE base is hit per
   run, so this drops the empty once — but must be correct for multi-base parsers.)
3. **Re-land the `(String,Value)` / `(String,String)` list-literal materialization** (the reverted
   `try_lower_record_list_literal` StrValue + `lower_owned_heap_field` Tuple-arm), so the `pairs +
   [(k, value.str(...))]` accumulator literal builds. SOUND on its own (sv1/sv2 verified).
4. **Loop-body-escape Consume.** In the loop body, `value.str(arg)` COPIES `arg` (runtime
   `string.repeat(s,1)`), so `arg` stays the caller's and is correctly `rc_dec`'d at the per-iteration
   teardown — BUT the trap (`rc_dec(v13)`) shows it is freed TWICE: pin whether the double-`rc_dec` is
   the per-iter frame + a stale carry, or the arg is mis-shared with the accumulator copy. (Re-derive
   from a fresh `cm2.wat` with bricks 1-3 in place; the prior trap was BEFORE the result-accumulator
   path, so it may dissolve once brick 1 routes it correctly.)
5. **Mutual-inline for the real `collect_map`.** `inline_mutual_tail_recursion` must inline `map_entry`
   (and the `after_colon` chain's same-level tail) into `collect_map` so it is purely self-recursive at
   one nesting level (the `parse_nested` deeper call stays a bounded regular call). Then bricks 1-4 apply.
   The `??`-keyed element (`list.get(lines,pos) ?? ""`) must also lower in the loop body.

Then: byte-match audit `parse`/`stringify` + 10⁴ leak loop + corpus-wall + mir suite, per brick.
GATING NOTE: brick 1 alone may already fix `cm2` (it routes to the result-accumulator) — verify before
assuming bricks 2/4 are needed. The Coq loop-ownership soundness (option C, 7f673b4c) underwrites the
whole; this is the extraction/lowering integration only.

### CORRECTED DIAGNOSIS (2026-06-21, sharper — supersedes the "all tuple results broken" framing)

Two disambiguating probes narrowed the bug precisely:
- `cm4` (a self-rec parser returning `List[(String,Value)]`, accumulating `acc + [(k, value.str(…))]`)
  **byte-matches v0** (len 3). So the str_value list-literal materialization, the append-accumulator
  TCO, AND `value.str`'s copied arg are ALL SOUND. (brick 3 is correct; re-land it freely.)
- `cm2` (the SAME but returning the `(Value, Int)` TUPLE `(value.object(pairs), pos)`) gives garbage
  (`pos=0`, value garbage) + a teardown trap.
- The existing `parse_rows_rec` test (csv — a TUPLE-returning self-rec parser) PASSES. **So tuple-result
  TCO is NOT universally broken.** The difference: `parse_rows_rec`'s base reads LOOP-BODY-LOCALS (`let
  (field,np)=…`) → `base_reads_loop_local=TRUE` → the in-loop RESULT-ACCUMULATOR path (works). `cm2` /
  `collect_map` / `collect_seq` bases read ONLY CARRIED PARAMS (`pairs`,`pos`) →
  `base_reads_loop_local=FALSE` → the POST-LOOP DISPATCH, which recomputes the tuple base and reads
  STALE values → garbage.

So the REAL bug is the **post-loop dispatch's recomputation of a TUPLE base** (it does not see the loop's
final carried-param values the way a plain-Var base like `cm4`'s `acc` does). A broad "decline tuple
results" guard is WRONG — it regresses `parse_rows_rec` (verified: its test FAILED under the guard).

REVISED FIX (narrower): make carried-param-only TUPLE bases use the in-loop result-accumulator (route
`result_var` for tuple results too, needing `tco_empty_for(Value/Tuple)` + the `(Value,Int)` tuple drop)
— OR fix the post-loop dispatch to lower a tuple base reading the loop-final carried locals. Either way,
`parse_rows_rec` (loop-local base) must keep working. The minimal repro is `cm2`; gate against both
`cm2` (must become correct) and `parse_rows_rec` (must stay correct).

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

## STATUS (2026-06-22) — the 5-brick TCO WORKS in isolation; blocked on the corpus caps-gate

Implemented the full 5-brick effect-fn tuple-result TCO + PROVED it in isolation, then REVERTED
(②discipline — it breached corpus-wall). What WORKS (verified, then reverted to keep the tree sound):
- `cm2` (effect fn `(Value,Int)` self-rec parser, `pairs + [(k, value.str(x))]` accumulator) →
  **byte-matches v0** + `cm2leak` 10000× **leak-free**.
- `ftup` (effect fn returning a tuple + `let (v,p)=f()!` destructure) → ✅; `cm2p` (pure) ✅; `cm4`
  (List result) ✅; `parse_rows_rec` (csv, List-tuple) ✅ (not regressed); mir suite green.
- **yaml module → 0 lower walls** (collect_map/collect_seq lowered).

The 5 bricks (all needed together):
1. `tco_empty_for` → Value (`value.null()`) / scalar (`0`) / Tuple (recursive) — the result-accumulator empty.
2. `result_var` routing for a **Value-containing tuple** result (`use_result_acc = base_reads_loop_local
   || tuple-with-Value`) — the post-loop dispatch reads a tuple base's sibling SCALAR carry stale when a
   `value.object(..)` CALL is in the base. (A Value-FREE tuple like csv `pf`'s `(acc,pos)` must NOT be
   routed — it works via the dispatch and routing it regresses parse_rows_rec.)
3. The `(Value,scalar)` / `(String,Value)` tuple in `lower_owned_heap_field` (via try_lower_tuple_construct).
4. `__drop_value_tuple` (value_core) — a SINGLE `(Value,Int)` tuple's recursive drop (DropValue the Value
   slot @12 + block); routed via `variant_drop_handles="value_tuple"` (the flat record_masks leaks the
   Value payload → 10⁴ OOM).
5. The destructure-seed (`lower_destructure`): derive the heap-slot mask from the PATTERN (not `value.ty`)
   for `value.kind = Unwrap|Call` (effect-fn `f()!` / the never-err-stripped `f()`, whose `.ty` is the
   effect Result, not a Ty::Tuple) — else the destructure container-grains (`p` reads 0). KEEP the
   value.ty path for a Tuple value, and the no-seed container-grain for a plain Var (the
   `tuple_destructure_aliases_components` unit test).

**WHY REVERTED — the corpus caps gate (the remaining work, de-risked):**
- **mir > ir caps breach (1 fn)**: `tco_empty_for`'s `value.null()` is a SYNTHETIC CALL the TCO inserts
  that the IR does not have, so `count_ir_calls` (IR) < mir CallFn count → the `mir<=ir` gate breaks
  (the [[project_v1_gate_count_vs_lower]] class). FIX: teach `count_ir_calls` (or the gate) to credit the
  TCO's synthetic `value.null` (desugar-before-both, or count the empty-init call), like the existing
  TCO-synthesized ops are accounted.
- **A corpus fn PANICS (totality breach)** under the changes — find it (corpus-wall caps step names it)
  and harden the new tuple path against its shape (likely a tuple/aggregate shape the (Value,scalar)
  arm or the seed mis-handles; gate it out cleanly rather than panic).
- Separately, yaml's full `parse` still needs **float.parse / list.enumerate / string.to_lower**
  self-hosted (stdlib gaps the parse path references — independent of the TCO).

NEXT SESSION (settled): re-apply the 5 bricks (this section is the exact recipe), then fix the two
corpus-gate issues (synthetic-call count + the panic) BEFORE re-testing corpus-wall, then the 3 stdlib
gaps, then yaml `almide test`. Gate against cm2 (must byte-match) + parse_rows_rec + tuple_destructure
+ corpus-wall (4 ACCEPT) at every step.

## STATUS (2026-06-22, later) — mir>ir breach SOLVED (better than planned); suite-breaking recursion FIXED

Two foundation commits landed on develop-v1 (gate: mir suite 501/0 single-thread + parallel,
corpus-wall 4/4 ACCEPT):

1. **mir>ir caps breach is GONE — `value.null()` inlined, NOT a gate change** (commit 6ca50e85,
   `lower/calls.rs`). Instead of teaching `count_ir_calls` to credit the synthetic call (which would
   add lowering-dependent logic to the trust gate — the [[project_v1_gate_count_vs_lower]] anti-pattern
   the 守る系 KPI forbids), `value.null()` now lowers INLINE to a tag-0 Value block (Alloc + store32),
   exactly like the String/List empties. So it is NO CallFn: the TCO's synthetic empty adds zero mir
   calls (gate-neutral), while an explicit `value.null()` source node still counts in the IR (mir < ir,
   allowed). corpus-wall ACCEPT unchanged. **This retires corpus-gate blocker (a) entirely without
   touching the gate.** When re-applying brick 1, `tco_empty_for`'s `Value → value.null()` is now free.

2. **A pre-existing infinite recursion that had been silently breaking the WHOLE mir suite is fixed**
   (commit 4536f33c, `render_wasm/tests_part1.rs`). The `lower_source` test helper recursively links
   self-host sources; the Value-drop force-link (a40c1332) set `any_called` for value_core when lowering
   value_core ITSELF (its drop helpers emit DropValue ops), but `any_defined` only matched the call name
   (`value.null`) not the impl name (`value_null`), so it re-lowered value_core forever → stack overflow
   that aborted the test process (masking the suite, and any cargo-test-gated work). Fix: `any_defined`
   now matches EITHER the call name OR the impl name. The `capturing_heap_map_over_value` test (and the
   suite) is green again. NOTE: this was NOT the value.null inline's fault — it reproduced with the
   inline disabled; it is orthogonal test-helper infra.

REMAINING for yaml: re-apply bricks 1–5 (mir>ir now free), find+harden the **corpus panic** (blocker b,
still open — corpus-wall caps step names it), then the 3 stdlib gaps (float.parse / list.enumerate /
string.to_lower), then yaml byte-match + leak loop.

### Brick re-application probe (2026-06-22) — bricks 1+2 drafted, then REVERTED (sharper blocker map)

Re-applied bricks 1 (`tco_empty_for` → Value/scalar/Tuple) + 2 (`tuple_with_value` routing to
result_var) and probed `cm2`. Findings (all reverted to keep the tree sound — they regress without 3–5):

- **bricks 1+2 ALONE regress corpus-wall coverage** (still ACCEPT/0-panic, but counts drop:
  ownership 16556→16549, names 3850→3849). A Value-tuple-returning corpus fn that previously lowered via
  the post-loop dispatch now routes to result_var and WALLS (because bricks 3–5's tuple
  construct/drop/destructure are absent). So bricks 1–5 are atomic — never commit 1+2 alone.
- **`cm2`'s real blocker after bricks 1+2 is NOT tuple construction** — `try_lower_tuple_construct`
  (binds.rs:1121) ALREADY lowers heap-typed tuple elements (calls `lower_owned_heap_field` per heap
  slot, records `record_masks`). The wall is in `lower_while` (control.rs:3338, `body_reassigns_heap`):
  the TCO produces a `while` whose body does `pairs = pairs + [("k", value.str("x"))]` — a HEAP-APPEND
  accumulator over a `List[(String,Value)]` element. `try_lower_scalar_while` (control.rs:1940) DECLINES
  it (its append-accumulator path doesn't admit a `(String,Value)` tuple element yet), so the
  model-one-iteration fallback walls. **So brick 3 is really "admit a (String,Value)/(Value,scalar)
  tuple ELEMENT in `try_lower_scalar_while`'s heap-append accumulator", not the standalone
  `try_lower_tuple_construct` (which already works).** Re-scope brick 3 accordingly.
- v0 oracle for `cm2` (`local effect fn cm2(n,pos,pairs)->(Value,Int)`): `{"k":"x","k":"x","k":"x"}` then
  `3`. Test via the v1 MIR spine ONLY (`examples/render_program` → WAT → wasmtime, or `lower_source`),
  NOT `almide run --target wasm` (that is the OLD almide-codegen pipeline, a different backend; it
  panics earlier in its ANF pass on `List[(String,Value)]` args — unrelated to the MIR spine).

## STATUS (2026-06-22, yaml) — TCO wall GONE; yaml lowers to 14128-line WAT, 1 stdlib + 1 render bug left

The effect-fn tuple-result TCO (the 5 bricks) is LANDED + gate-green (commit 141f6c09; cm2 byte-match +
10⁴ leak-free, corpus-wall 4/4 ACCEPT 0 panics, mir suite 501/0). With it + the two self-hosted leaf
stdlib fns this session, **`almide/yaml`'s `parse` lowers on the v1 MIR spine with ZERO TCO walls** —
the structural blocker the whole roadmap targeted is resolved.

Self-hosted this session (committed, byte-match in isolation, PURE_MODULES-classified):
- **string.to_lower** (commit, ASCII fold) — `stdlib/string_to_lower.almd`.
- **float.parse** (commit, sign/int/.frac/e-exp, single exact pow10 scale) — `stdlib/float_parse.almd`.
  Tuple-free (single-Int/Float helpers); byte-matches `s.trim().parse::<f64>()` for the yaml grammar.

yaml driver = `mod.almd` + a `main` calling `parse("a: 1\nb: hello\nc:\n  - x\n  - y\n")` (test via
`examples/render_program` → WAT → wasmtime; NOT `almide run --target wasm` = old codegen). After the
above, `render_program` reports ONLY `unlinked: list.enumerate`; with a (reverted) list.enumerate
prototype it reaches 0 walls (14128-line WAT) but FAILS at runtime — TWO remaining bugs:

1. **`list.enumerate` (the last stdlib gap).** The (Int,String) tuple-list **builder** machinery WORKS
   (a tuple-free `__enum_str_rec` append accumulator over `List[(Int,String)]`; the `(Int,String)`
   element added to `try_lower_concat_list`/`try_lower_str_list_literal`/`lower_owned_heap_field`; a
   `$__drop_list_int_str` recursive drop in a `list_enumerate.almd`; the `enumerate→enumerate_str`
   dispatch; `__isdrop_list` in the rc_dec allowlist; `list_enumerate` in PURE_MODULES — all gate-green,
   corpus-wall 4/4 with coverage +50). REVERTED because the CONSUMER mis-compiles: yaml uses
   `lines |> list.enumerate |> list.find((e) => not is_blank(e.1))` and `list.find` over a
   `List[(Int,String)]` WITH A CLOSURE reading the tuple's `.1` is NOT C1-defunc'd (find ∉ the
   map/filter/fold defunc set) and mis-dispatches to `find_str` (reads each tuple as a String → garbage,
   the `none`/`0:` output). The REAL remaining work is find-over-a-heap-tuple-element-with-a-closure
   (the [[v1-selfhost-machinery]] Machinery-3 closure surface), NOT the builder. Re-land the builder
   (it's sound) THEN solve the closure-find.
2. **A `seq_map` WASM type mismatch (i64 vs i32)** surfaced only in the FULL yaml lowering
   (`local effect fn seq_map -> (Value,Int)` = a tuple-returning tail call to `parse_mapping(list.set(…))!`).
   Not self-recursive (not the committed TCO); likely a render bug in `list.set` / the tuple-returning
   tail-call path. Isolate with a seq_map-shaped fixture.

NOTE — v0 native ORACLE is BROKEN for yaml: the current native Rust backend mis-borrows `find_colon`
(`find_colon_at(s.to_string())` E0308), so `almide test`/`almide run` can't compile yaml on v0. Use the
yaml_test.almd expected values (or fix that native borrow bug) as the oracle. The v1 spine is unaffected.

## STATUS (2026-06-22, late) — list.enumerate + closure-find DONE; yaml emits VALID wasm + RUNS the parser

The user's "re-land the builder then solve the closure-find" is COMPLETE + committed + gate-green:
- **`60deaf24` list.enumerate + find-over-(Int,String)-with-closure.** The (Int,String) tuple-list
  BUILDER (append-accumulator TCO + the (Int,String) element in try_lower_concat_list /
  try_lower_str_list_literal / lower_owned_heap_field + the recursive `$__drop_list_int_str`),
  `list_find_int_str` (loads each tuple HANDLE, hands it to the predicate closure reading `e.1`), the
  heap-payload `Option[(Int,String)]` construct (`materialize_opt_int_str_some`, co-own Dup) +
  materialize-recognition + the value-result match `some((idx,line))` destructure, AND the
  `List[(Int,String)]` call-result drop routing (`is_list_int_str_ty` → `variant_drop_handles`). The
  `enumerate |> find((e)=>…e.1…)` shape byte-matches v0 (`1:b`) + **10⁴ leak-free**; corpus-wall 4/4
  ACCEPT (coverage 16620); mir suite 501/0. KEY: yaml's match arms are VALUE-result (`value.null()`/`v`)
  → `try_lower_variant_value_match` (which has the tuple-payload desugar), NOT the Unit-body
  `try_lower_variant_match`; a Unit-body test linearizes, but that is not yaml's shape.
- **`96e94de7` list.set over List[String] (`list.set_str`).** The generic `list.set`'s i64 val param
  mismatched a String element (i32 handle) → invalid wasm in `seq_map`. set_str rc-copies every slot
  (co-own) + replaces slot i (rc_dec old, co-own new); result drops `DropListStr`. Byte-matches v0.

**yaml NOW COMPILES TO VALID WASM (≈13.7k-line WAT, 0 walls) AND EXECUTES the full parser** —
`parse → parse_lines → dispatch → parse_mapping → collect_map → map_entry → after_colon → parse_inline
→ parse_scalar → scalar_typed → scalar_value → scalar_numeric → parse_number`. It TRAPS in
`parse_number` (`rc_dec`, a double-free) on a MAP-ENTRY value (`parse("a: hello")` traps; a BARE scalar
`parse("hello")` does NOT trap, it returns `{}`). The trap is NOT reproducible by faithful repros (the
`match int.parse(c){ok=>value.int,err=>value.str(raw)}` shape, the collect_map (String,Value)
accumulator with a parse_number value, a loop-local `after` threaded through an effect-fn `(Value,Int)`
`!` — ALL byte-match in isolation). So it is a SUBTLE interaction in the full number-parsing chain.
SUSPECT (from the WAT): a `Result[Int,String]` drop (e.g. `int.from_hex` / `int.parse`) that per-element
`rc_dec`s the payload slot — fine for an `Err(String)` but a double-free / scalar-as-handle for an
`Ok(Int)`; OR a borrowed-`raw`/`after` double-drop where the value escapes into the accumulator. NEXT:
dump `$parse_number`'s exact trapping `rc_dec` local in the `parse("a: hello")` WAT, trace it to the
source drop, fix the Result/borrow drop routing — then yaml runs end-to-end (NOTE: v0-native oracle is
still broken by the `find_colon` borrow E0308, so byte-match against yaml_test.almd's expected values).

## STATUS (2026-06-22, runs) — 🎉 almide/yaml RUNS ON V1

`almide/yaml`'s `parse` lowers (0 walls) AND executes byte-correctly on the v1 MIR→WAT→wasmtime spine.
Verified round-trips (parse → yaml's own stringify, identical to input): `a: 1\nb: hello\nc:\n  - x\n  - y`,
`name: Alice\nport: 8080\npi: 3.14`, `items:\n  - one\n  - two\n  - three`, `nested:\n  a: 1\n  b: 2\nflag: true`.
The goal "almide v1 で almide/yaml を動かす" is met. Commits this push (all gate-green, corpus-wall 4/4,
mir suite 501/0):
- `60deaf24` list.enumerate + find-over-(Int,String)-with-closure (the heap-payload Option machinery).
- `96e94de7` list.set_str (List[String] set; the i64-val generic list.set made invalid wasm).
- `6f2128f3` the heap-result-if **Match arm** per-arm frame (its subject-eval temp `string.drop(c,2)`
  leaked to the function scope-end → an unconditional `rc_dec(0)` trap when the OTHER arm ran — the
  parse_number 0x crash); + `drop_arm_locals` clamp.
- `59a5164a` two heap-Value LEAKS: (1) a heap-result tail `f(string.replace(s,..),s)->Value` bypasses
  scope-end drops so its owned-temp args leaked — free them after the CallFn; (2) a let-bound Named-call
  Value result had no `value_handles`/DropValue marking (the Module path had it).

REMAINING — leak long-tail (production-hardening, NOT "runs"): repeated parsing eventually OOMs (a map
input is fine to ~1000×, the SEQUENCE path — collect_seq/seq_item/dash_item/dash_after — leaks sooner).
The OOM SURFACES wherever the next alloc lands (parse_number → find_colon_at as fixes land), so it is an
accumulating per-parse under-free, not one site. PATTERN: every leak found so far is a heap value
(temp / let-local / call result) NOT freed because a heap-result tail/arm bypasses the function
scope-end drops — audit the seq path (dash_after's `(parse_inline(after), pos+1)` tuple + the collect_seq
List[Value] accumulator + the `??`-operand Option temps from `string.get(s,pos) ?? ""`) for the same
class. The single/few-shot parse is correct + balanced; this is depth-of-loop hardening. (v0-native
oracle still broken by the `find_colon` borrow E0308 — use round-trip + yaml_test expectations.)

## STATUS (2026-06-22, base64/toml/aes) — aes CONQUERED, base64 9→4, the two deep frontiers

This push targeted the three repo-specific walls (base64 / toml / aes). Landed, all gate-green
(corpus-wall 4/4 ACCEPT, mir 501/0), each a REUSABLE mechanism:

- ✅ **aes RUNS** (FIPS 197 `encrypt_block` byte-matches v0). Lever = **CONST module-global
  materialization**: a heap module-level global with a const initializer (`let SBOX = bytes.from_list
  ([…])`, a string literal, an int-list literal) now lowers to a DIRECT `Alloc` — new `Init::Bytes`
  (arbitrary-byte block, ≠ `Init::Str` because the S-box has 0x00–0xFF). A COMPUTED init keeps walling
  (materializing it would inject a `CallFn` the gate's IR-side `count_ir_calls` can't see → mir>ir).
  `global_inits` threaded through render/gate/cert so all three agree. (commit ac7fae5c)
- ✅ base64 `char_to_val` — a heap-result `Result` **Err arm whose message is a `${}` interpolation**
  (`err("bad '${ch}'")`): the ResultErr arm folds the interp + frees its intermediates per-arm. (fec44e1a)
- ✅ base64 `encode_chunks` self-append — **multi-concat accumulator** `acc + c0 + c1 + …`: the TCO's
  is_self_append recurses the ConcatStr left-spine to its leftmost leaf. (4f2bfdeb)

base64 is now 9→4 walls; aes 0. **The remaining 4 base64 walls + most of toml's 22 bottleneck on TWO
DEEP FRONTIERS — design slices, NOT incremental bricks:**

### Frontier A — the let-bound heap-result `if` in a loop/recursive body (HIGHEST leverage)
`let c2 = if cond then A else B` inside a scalar loop body (base64 encode_chunks/decode_chunks; toml
parse_val/read_basic/…). binds.rs WALLS the general let-bound heap-result `if`: a let-bound value is
held + dropped at scope end, but the merged `IfThen` dst has no sound scope-end drop in the FLAT cert
(attributing ONE drop to exactly-one-of-two arm allocs needs a checker/Coq change). At FUNCTION TAIL it
already works via `desugar_heap_branches` (tail-duplication into each arm). **DESIGN (no Coq change):
extend the tail-duplication to a let-bound `if` whose continuation is the recursion** — rewrite
`{ let c = if k then A else B; recurse(acc+…+c) }` → `if k then recurse(acc+…+A) else recurse(acc+…+B)`,
duplicating the continuation (the recursive call + accumulator update) into each arm so the let-bound
`if` is ELIMINATED before lowering. Then each arm has a literal `c` and its own self-call; `tco_collect`
must admit BRANCHED self-calls (one per arm, each a conditional accumulator SetLocal — already supported
by the loop-body Assign). Risk: TCO composition with branched calls; gate-protected (any double-free →
cert REJECT). This single lever unblocks base64 ×2 + a large fraction of toml.

### Frontier B — the heap-payload `Result`/variant match (Camp-4)
`match bs { ok(bytes) => …, err(e) => … }` over a `Result[Bytes, String]` (base64 decode/decode_url).
`try_lower_variant_value_match` admits SCALAR-payload variants (subject-drop-before-arms) but GATES OUT
a HEAP payload (the arm borrows the subject's heap slot — `Option[String]`/`Result[Bytes,_]`). This is
the documented Camp-4 frontier ([[v1-selfhost-machinery]] Machinery 2 extended to match-read). Needs the
heap-payload extract-then-arm machinery (the arm reads slot-0 as a borrow, the subject drops after).

### toml — also `unwrap !` in a call-arg position + while-body heap-accumulator (on top of Frontier A).

## STATUS (2026-06-22, turn 2 — base64 ENCODE conquered, Camp-4 is the LAST frontier)

This turn landed THREE reusable lowering levers (all gate-green, corpus-wall 4/4, mir 501/0):
- ✅ **multi-concat self-append TCO** (`acc + c0 + c1 + …`) — is_self_append recurses the ConcatStr
  left-spine. (4f2bfdeb)
- ✅ **let-bound-heap-result-`if` pre-desugar before the TCO** — `desugar_heap_branches` on func.body
  BEFORE `try_tco_rewrite`, so `{ let c = if k then A else B; recurse(acc+c) }` becomes branched
  recursion `if k then recurse(acc+A) else recurse(acc+B)` (tco_collect recurses both arms). (8cf460e5)
  → **base64 `encode_chunks` lowers; base64 ENCODE RUNS** (`encode("hello")=aGVsbG8=`, canonical
  byte-match, standard + url). (1a871eed + fixture spec/wasm_cross/base64_encode.almd)
- ✅ **unwrap-`!`-bound-to-a-let desugar** — `{ …; let v = e!; rest }` → `{ …; match e { ok(v) =>
  {rest}, err($x) => err($x) } }` (the `!` IS early-return-on-Err; the match becomes the tail).
  corpus ownership coverage 16744→**16877 (+133)** — the TOP cross-repo wall (unwrap-bound-to-let)
  cleared for scalar/str-result shapes; str-result unwrap byte-matches v0. (a564a12e)

**base64 = 9→3 walls (ENCODE done); the last 3 (decode/decode_url/decode_chunks) + ALL of toml's 22
now bottleneck on ONE frontier: Camp-4 — the heap-payload `Result`/variant match.**

### Camp-4 design (the last frontier — precise mechanism, NOT yet implemented)
`match e { ok(v) => …, err(x) => … }` where the bound payload (`v` or `x`) is HEAP (a `String`/`List`).
Two sub-cases, increasing difficulty:
1. **scalar-Ok / heap-Err** (decode_chunks's `let v0 = char_to_val(c)!` over `Result[Int,String]`):
   the err-arm binds the slot-0 String + the unwrap-desugar's `err($x)` re-wraps it. NEEDS: track a
   `Result[scalar, String]` subject as `heap_elem_lists` too (so `heap_or_scalar_bind` admits the
   String Err bind AND `drop_op_for` uses `DropListStr` — correct: Ok=len0 frees nothing, Err=len1
   frees slot-0) WHILE keeping the `materialized_results` len@4 tag read; the err-arm must `Op::Dup`
   the BORROWED slot-0 String before re-wrapping (else subject-drop-after double-frees it). VERIFY the
   tag-offset (len@4) and the drop (DropListStr) compose — the risk is a wrong-offset read or a
   double-free, both gate-caught.
2. **heap-Ok / heap-Err** (decode's `match decode_with(..) { ok(bytes) => ok(string.from_bytes(bytes)) }`
   over `Result[List[Int],String]`): the FULL Camp-4 — the Ok arm borrows + consumes the heap List.
   This is the documented borrowed-slot move-out ([[v1-selfhost-machinery]] Machinery 2 for match-read).

Camp-4 is double-free territory (the memory's explicit fatigue-warning zone): implement it FRESH with
the gate (ownership cert REJECTs any double-free) as the safety net. It is the single lever that
finishes base64 (decode) AND unblocks the bulk of toml.

## STATUS (2026-06-22, turn 3) — 7 levers total, +289 corpus coverage; heap-Ok Result is the LAST frontier

Added this turn (all gate-green, corpus-wall 4/4, mir 501/0):
- ✅ **unwrap-`!`-bound-to-let desugar** (`let v=e!` → `match e {ok(v)=>rest, err(x)=>err(x)}`, +133).
- ✅ **Camp-4 sub-case 1** (scalar-Ok / heap-Err `Result[Int,String]` match: heap_elem_lists tracking
  for the Err-String bind + DropListStr + ResultErr-arm Dups the BORROWED payload, +137).
- ✅ **unwrap-in-if-arm desugar** (`let v=if c then e! else d` → `let $r=if c then e else ok(d); $r!`).

Session total: corpus ownership coverage 16744→17033 (+289), 7 reusable lowering levers (module-global
Init::Bytes, Err-interp arm, multi-concat TCO, let-bound-if pre-desugar, unwrap-in-let, Camp-4 sub-case
1, unwrap-in-if-arm). **aes RUNS; base64 ENCODE RUNS.**

### THE LAST FRONTIER — heap-Ok Result (Camp-4 sub-case 2): `Result[List[Int], String]`
base64 `decode_chunks` walls on `ok(acc)` where `acc: List[Int]` — a HEAP-Ok Result. base64 `decode`/
`decode_with` + toml's heap-Ok matches need the same. This is a NEW LAYOUT: the existing heap-Ok Result
handling is str-result-only (`Result[String,String]`/`Result[Value,String]`/`Result[List[Value],String]`
via materialize_result_str + cap-tag@16). `Result[List[Int], String]` has a `List[Int]` Ok payload that
is NOT a String/Value, so it needs: (a) a layout + tag for a List-Ok Result, (b) `ok(list)` ctor
materialization in a heap-result-if arm, (c) the `match r { ok(bytes) => … }` heap-List-Ok bind +
drop. This is the single remaining lever finishing base64 DECODE and toml's heap-Ok matches — a
genuine new-layout slice (do it FRESH with the gate as the double-free net).

## STATUS (2026-06-22, turn 4, effort=max) — Camp-4 LANDED; decode_chunks needs TCO-over-match

Landed this turn (all gate-green, corpus-wall 4/4, mir 501/0; session coverage 16744→17088, +344):
- ✅ **unwrap-in-if-arm desugar** (`let v = if c then e! else d` → lift the `!`).
- ✅ **flatten-let-block** (`let v = { s..; tail }` → `s..; let v = tail`) + **inline-tail-accumulator**
  (a single-use let-bound `acc + …` inlined into the recursion arg so is_self_append admits it) — a
  nested let-bound-`if` List accumulator now lowers (the decode_chunks `new_acc` shape, in isolation).
- ✅ **Camp-4 sub-case 1** (scalar-Ok / heap-Err `Result[Int,String]` match — the err-arm String bind
  + ResultErr Dup, +137 coverage).
- ✅ **Camp-4 sub-case 2 (VALUE match)** — a value-match over a heap-Ok `Result[List[Int],String]` reads
  cap-tag @16 (matching materialize_result_str's construction) instead of len-tag @4; FIXED a layout
  mismatch (the match was reading the wrong tag — a latent silent-miscompile vector) so it byte-matches.

### decode_chunks's LAST blocker — TCO over a `match` (recursion in a match arm)
base64 decode_chunks STILL walls. ROOT (precisely localized this turn, repro `umr`): the unwrap-`!`
desugar turns `let x = f(pos)!; recurse(.., acc+[x])` into `match f(pos) { ok(x) => recurse(..,
acc+[x]), err(e) => err(e) }` — the RECURSION is in a match arm. But the TCO's `tco_collect`/
`tco_rewrite` only recurse `if`/`Block` tails, NOT `match` arms, so the self-call is seen as a buried
non-tail leaf → TCO declines → walls. Adding `Match` to tco_collect/tco_rewrite (attempted, reverted as
unverified) is necessary but NOT sufficient: it ALSO needs (a) the result-accumulator / post-loop
dispatch to handle the two Result bases `ok(acc)`/`err(e)` (the err base reads the match-bound `e`), and
(b) the TCO loop-body `match` over a USER Result (char_to_val) to EXECUTE not LINEARIZE — i.e. the
statement-match subject tracking must cover a user named-call Result (a separate latent linearize-
miscompile, hok2). These 3 interlock; do them together FRESH with byte-tests (the corpus-wall gate does
NOT catch byte-miscompiles, so each needs a v0==v1 fixture). This is the single lever finishing base64
decode + the bulk of toml's heap-Ok / unwrap-in-call-arg walls.
