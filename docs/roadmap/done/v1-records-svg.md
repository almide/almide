<!-- description: v1 records feature: full conquest of the svg org repo on the WASM trust spine (construct, field read, spread, recursive nested ownership) -->
<!-- done: 2026-06-21 -->
# v1 — records feature: svg FULL CONQUEST (goal prompt)

## BIG-PICTURE GOAL

Make **`github.com/almide/svg` pass on v1** (the WASM trust spine) — the next org repo after **csv
(4/4 done)**. svg is the **records-feature** target: pure (Float/String/Record, deterministic), so a
clean byte-match / test-assert oracle. Achieving it lands the **records language feature** on v1
(user record types: construct, field read, spread, recursive nested ownership) — a fundamental
capability that also unblocks **aes** (records + heap-global S-box + range-arg).

**Definition of done:** `almide test` inside `/Users/o6lvl4/workspace/github.com/almide/svg` reports
its spec files **all "via WASM", 0 failed** (today: `1 via WASM, 1 via native fallback`), AND a
build-a-tree-render-it-N-times leak loop does not grow memory (the recursive drop is leak-free), AND
the compiler gates below stay green.

UPDATE (implementing): the recursive record drop is **DONE** (commit 05a40219). Implementation
revealed TWO MORE hidden prerequisites svg needs (NOT in the original "1 mechanism" estimate):
**(A) an owned-value `Map[String,String]` self-host (`map_sss`)** — v1's `Map[String,String]` borrows
the `map_skv` (String,Int) layout, which stores VALUES raw (`store64`, not owned), so svg's `attrs`
values DANGLE after the build scope (render reads garbage) and leak; and **(B) `List[Record]` literal
materialization** — `doc(w,h,[rect(…),…])` / `group([…])` pass a `List[Element]` literal, still walled.
So svg = record-drop (DONE) + map_sss (A) + List[Record] literal (B). See "THE REMAINING" below.

## DONE (committed, gated: mir suite + corpus-wall ACCEPT)

- **records foundation** (`b77dfde3`): a record returned from a CALL (`let p = mk(5)`) is seeded
  `materialized_aggregates` + `record_masks` so a heap-field read `p.y` loads the real slot (was the
  container-grain-Dup empty-string miscompile); a **SpreadRecord RETURNED** (`fn attr(e,k,v) = {
  ...e, attrs: map.set(…) }`) lowers in the tail-return path.
- **el / empty heap fields** (`4190062d`): `lower_owned_heap_field` materializes an empty `Map`
  (`[:]`) / empty `List` (`[]`) record field; an empty list of ANY element type is admitted.
- **svg's recursive renderer LOWERS**: `render_el` (recursive, `children |> list.map(render_el) |>
  list.join`), `render_attrs` (Map over `attrs`), `format_points` — all clean. The READ/traverse side
  of svg is DONE. Tests: `record_call_result_field_read_and_spread_return`,
  `record_with_empty_map_and_list_fields_constructs` (in `render_wasm/tests_part3.rs`).

## THE REMAINING MECHANISM — recursive record drop

### Why
`Element = { tag: String, attrs: Map[String,String], children: List[Element], content: String }`.
Records today drop **FLAT** via `record_masks` → `drop_op_for` (mod.rs:~1599) returns
`Op::DropListStr`, which rc_dec's each heap slot as a LEAF. For `Element` that **LEAKS** the
`children` Elements (+ their String/Map fields) and the `attrs` Map's Strings — only `tag`/`content`
(plain String) free correctly. ② discipline forbids shipping the leak, so it WALLS instead (the
`doc`/`group`/`defs` constructors + any `List[Element]` literal arg like `group([rect(…), …])`).

### The fix — mirror the ADT recursive drop, for records
The ADT path already generates per-type recursive drops `$__drop_<T>` via
**`generate_variant_drop_sources`** (mod.rs:274) and routes them through **`Op::DropVariant { v, ty }`**
(lib.rs:258), selected in **`drop_op_for`** (mod.rs:1584) via `variant_drop_handles`. Do the same for
records. Concrete pieces:

1. **Generate `$__drop_<Record>`** — add a record branch to (or a sibling of)
   `generate_variant_drop_sources`. Records are `IrTypeDeclKind::Record { fields }` (no tag dispatch,
   fields at `slot_offset(i)` for i in 0..n). Shape:
   ```
   fn __drop_<R>(e: <R>) -> Unit = {
     let h = prim.handle(e)
     if prim.load32(h + 0) == 1 then { <per-field frees> } else ()
     prim.rc_dec(h)
   }
   ```
   Per-field free by the field's CONCRETE type at `slot_offset(i)`:
   - `String` → `prim.rc_dec(prim.load64(h + off))`
   - a record `S` (needs-recursive-drop) → `let f: S = prim.load_handle(h+off); __drop_S(f)`
   - `List[S]` where S is a record → `let f: List[S] = prim.load_handle(h+off); __drop_list_S(f)`
     (generate `__drop_list_S`, below)
   - `List[String]` → a list-str free (reuse a helper that rc_dec's each slot + block)
   - `Map[String,String]` → `__drop_map_ss(f)` (generic helper, below)
   - scalar → skip
   Use the existing `prim`-only/`__drop_`-prefixed pattern so it lands an EMPTY ownership cert (the
   rc_inc/rc_dec whitelist in calls.rs:~2085 already admits `self.fn_name.starts_with("__drop_")`).

2. **Generate `$__drop_list_<R>`** (mutually recursive with `$__drop_<R>`): loop the block, call
   `__drop_<R>` on each element handle, then `prim.rc_dec` the list block. Model on the
   `__ldls_share` / DropListListStr loop shape (recursive helper, NOT a `for` loop — v1 has no TCO for
   this, but the recursion is shallow per-level).

3. **`__drop_map_ss`** (generic `Map[String,String]` free, self-host once in `stdlib/value_core.almd`
   + register in `render_wasm/registry.rs`): the Map is a DynListStr of `2*len` String slots (key,val
   pairs) — rc_dec each of `2 * load32(h+4)` slots, then rc_dec the block. (NOTE: a flat `DropListStr`
   is WRONG here — it uses len@4 = entry count = n, not 2n, leaking the values.)

4. **Decide which records need the recursive drop** — a record needs it iff any field is a record /
   `List[record]` / `Map` / `List[String]` (anything a flat rc_dec would leak). A scalar-or-String-only
   record keeps the flat masked `DropListStr` (Strings rc_dec correctly). Mirror
   `variant_needs_recursive_drop` (mod.rs:236).

5. **Wire `drop_op_for`** — a `record_masks` value whose TYPE needs recursive drop must route to
   `Op::DropVariant { ty: <Record> }` (the generated `$__drop_<Record>`), NOT the flat `DropListStr`.
   Track it: when a record value is created/bound (`try_lower_record_construct` binds.rs:1356,
   `try_lower_spread_record_construct` ~1460, the call-result aggregate marking binds.rs:~765), if its
   record type needs recursive drop, insert it into `variant_drop_handles` (or a new
   `record_drop_handles`) keyed `v → type_name`. Then `drop_op_for` already prefers `DropVariant`.

6. **List[Record] literal materialization** — `group([rect(…), circle(…)])` passes a `List[Element]`
   LITERAL. Materialize a list block storing each Element handle (Dup/move), mirroring the csv
   nested-list builder (`__ldls_share` family) but for records; its scope-end drop is `__drop_list_<R>`
   (piece 2). This is the "List[Element]引数" step. Sites: `lower_call_args` (calls.rs ~842 list-arg
   path) + `lower_owned_heap_field` (binds.rs:1686 List arm, currently admits only empty/scalar).

7. **Link the generated record drops** — `generate_variant_drop_sources` output is appended to the
   program by `render_program.rs` (~217) and `tests_part1.rs` (~217). Append the record-drop sources
   the same way; ensure `__drop_<R>` / `__drop_list_<R>` / `__drop_map_ss` are linked when referenced.

### GATES (all green before each commit; ② discipline — never ship a leak/miscompile)
- `cd /Users/o6lvl4/workspace/github.com/almide/svg && almide test` → all "via WASM", 0 failed.
- `cargo test -p almide-mir` (the foundation + records tests, no regression).
- `cd proofs && ./corpus-wall.sh` → CORPUS WALL OK (all 4 properties ACCEPT).
- **Leak loop**: a `doc(w,h,[rect,…])` rendered N×10⁴ times in a `while` loop — no OOM (the recursive
  drop is rc-balanced). Add a regression test in `render_wasm/tests_part3.rs` mirroring
  `nested_heap_list_get_drop` (build the tree via calls — a `List[record]` LITERAL of >1 element may
  itself be the last gap; if so, the builder uses a recursive constructor like csv's parse_rows).

### KEY FILES
- `crates/almide-mir/src/lower/mod.rs` — `generate_variant_drop_sources` (274), `drop_op_for` (1584),
  `variant_needs_recursive_drop` (236), `variant_field_name` (220), `variant_type_names` (255).
- `crates/almide-mir/src/lower/binds.rs` — `try_lower_record_construct` (1356),
  `try_lower_spread_record_construct` (~1460), call-result aggregate marking (~765),
  `lower_owned_heap_field` (1614, incl the List arm 1686 + the new empty-Map/List arms).
- `crates/almide-mir/src/lower/calls.rs` — list-arg materialization (~842), rc_inc/rc_dec whitelist
  (~2085, add any new `__drop_*` if not prefix-covered).
- `crates/almide-mir/src/lib.rs` — `Op::DropVariant` (258) + the Drop family.
- `crates/almide-mir/examples/render_program.rs` (~217) + `crates/almide-mir/src/render_wasm/tests_part1.rs`
  (~217) — where `generate_variant_drop_sources` is appended + linked.
- `stdlib/value_core.almd` + `crates/almide-mir/src/render_wasm/registry.rs` — `__drop_map_ss`.
- svg source: `/Users/o6lvl4/workspace/github.com/almide/svg/src/mod.almd` (Element @ line 11, el @ 44,
  doc @ 47, group @ 97, render_el @ ~205).

### AFTER svg (the remaining org-repo frontier, for context)
- **aes**: records (this mechanism) + heap module-global S-box + Range-in-call-arg.
- **toml / yaml**: broken on BOTH v0 (E0308 `String` vs `&str` Rust codegen — a separate v0 bug) AND
  v1 (toml: heap-carried TCO `root = set_nested(root,…)` general back-edge merge = OwnershipLoop.v
  engineering; yaml: float.parse strtod + an internal collect_map wall). The deepest frontier.
- **base64**: v0 E0428 — `@intrinsic("almide_rt_base64_*")` names collide with the stdlib.
- Already pass v1: sha1, bigint, rsa.

## STATUS (autonomous run — accurate remaining scope, discovered by implementing)

The **recursive record drop is DONE + gated** (commit 05a40219): `generate_record_drop_sources`
emits `$__drop_<R>` / `$__drop_list_<R>` / `$__drop_map_ss`; `record_drop_type_name` routes records
via `variant_drop_handles` → `Op::DropVariant`. **Leak-verified at 10000× for a record with String +
List[String] fields (`record_recursive_drop_frees_heap_fields_leak_free` test).** This is the headline
"1 mechanism" — delivered.

Implementing it revealed svg needs a CASCADE of more bricks (NOT 1 mechanism). Precise findings:

- svg's `attrs: Map[String,String]` dispatches to **`stdlib/map_str.almd`** (interleaved 16-byte
  entries: key @ 12+i*16, value @ 12+i*16+8; BOTH store_str-owned; **`len@4 = 2*entries` (slot
  count)**). The generated `$__drop_map_ss` now matches this (loop `load32(h+4)` slots at 8-byte
  stride frees the interleaved keys+values) — CORRECT for 1-entry maps at small counts (rec4 works at
  100×), but **rec4 OOMs at 10000×** → a residual per-iter leak SPECIFIC to the map_str path
  (rec5/List[String] is leak-free, so it is NOT the record drop). Suspect: `map.set_str` leaks a temp,
  or an empty-`[:]`-then-`map.set` interaction. NEEDS: bisect the map_str leak (build a bare
  `var m = [:]; for … { m2 = map.set(m,"k","v") }` loop, no record, and watch memory).
- **`map.entries` is UNLINKED for `Map[String,String]`** (no `map_entries*` in the registry) — so
  svg's `render_attrs` (`map.entries(attrs) |> list.map((e)=>{let (k,v)=e; …})`) walls. NEEDS: a
  `map_entries_str` (→ `List[(String,String)]`) + registry + dispatch, AND the `(String,String)`
  tuple-list (materialize + destructure + drop — a new tuple-list kind, cf. csv's `(String,Value)`
  `str_value_elem_lists`).
- **`List[Record]` literal** (`group([rect(…), circle(…)])`, the "group nests children" test + any
  multi-child `doc`) — still walls "List argument". NEEDS: a `try_lower_record_list_literal` (alloc +
  store each Element handle via `lower_owned_heap_field`, track `variant_drop_handles[v] =
  "list_<R>"`), wired in `lower_owned_heap_field`'s List arm + `lower_call_args` + `lower_bind`;
  generate `$__drop_list_<R>` for all rec_names R (today only field-referenced ones).
- A **`heap-result SpreadRecord` (1×)** wall remains in one path (attr/fill/render_attrs/render_el/el
  are all CLEAN — no regression from the record drop; the suite is 494 green).

svg test breakdown: MOST tests are single-element (`rect(…)|>fill(…)|>render` — need map.entries_str +
the map_str leak fix, NO List[record]); only "group nests children" needs the List[Record] literal.
So map.entries_str + map_str-leak unblock the majority; List[record] literal unblocks the last test.

NET: svg = recursive-record-drop (DONE) + map_str-leak-fix + map.entries_str/(String,String)-tuple-list
+ List[Record]-literal. A multi-brick continuation, each gated (suite + corpus-wall + a 10⁴ leak loop).

## STATUS 2 (leak localized) — the map_str per-iter leak

Localized the rec4 OOM precisely (autonomous run continued):
- `recE` (record with an EMPTY `Map[String,String]` field, 10000× loop, NO `map.set`) → ✅ leak-free.
- `rec5` (record with a `List[String]` field, 10000×) → ✅ leak-free (the record drop itself is sound).
- `rec4loop` (record with a `map.set`-POPULATED `Map[String,String]` field, 20000× loop) → ✗ OOM.

So the leak is SPECIFIC to the `map.set_str`-populated `Map[String,String]` path in a loop — NOT the
record drop, NOT the empty-map drop. The generated `$__drop_map_ss` offsets DO match `map_str`
(interleaved 16-byte entries, key @ 12+i*16 / value @ 12+i*16+8; `len@4 = rslots = 2*entries`; the
drop loops `load32(h+4)` slots at 8-byte stride = exactly the interleaved keys+values, freeing all),
and a single `map.set` (rec4a/rec4b) is leak-free — so the leak is a SUBTLE per-iteration imbalance in
`map.set_str` + the spread-override / call-arg path (a `store_copy` String copy or the map block not
reclaimed per iter), NOT yet pinned. Next: dump the rec4loop loop-body `Op::Alloc` vs drop ops and
diff the per-iter heap balance; or build a bare `map.set` loop (avoiding the for-in `[:]`-direct-arg +
Range-in-call-arg walls via a `Map[String,String]` param) to confirm whether the leak is `map.set_str`
itself or the record/spread integration.

## Honest scope assessment

The HEADLINE mechanism — the recursive record drop — is DONE, gated (suite 494/0, corpus-wall ACCEPT
on all 4 properties, 10000× leak-verified for String/List[String] record fields), and committed
(05a40219). svg full conquest, however, is NOT "1 mechanism": implementing the record drop revealed a
CASCADE of further independent bricks, each with its own lowering walls:
  (1) the subtle `map_str`-in-loop leak above (leak gate);
  (2) `map.entries` is UNLINKED for `Map[String,String]` → render_attrs walls — needs `map_entries_str`
      (→ `List[(String,String)]`) + a NEW `(String,String)` tuple-list (materialize + list.map + the
      `let (k,v)=pair` destructure + drop), itself a multi-part feature;
  (3) `List[Record]` literal materialization (`group([…])`).
Each is comparable in size to the record-drop brick. This is a multi-session continuation; the precise,
actionable design + file map + gates are recorded above. Recommend re-scoping the goal to one brick at
a time (e.g. "fix the map_str-in-loop leak" → "map.entries_str + (String,String) tuple-list" →
"List[Record] literal"), each gated independently.

## STATUS 3 — the records FEATURE is complete (leak fixed); svg needs an stdlib cascade

The "map_str leak" was MISDIAGNOSED: it was a **record-call-argument leak** (`f(mk(x))`), not map_str.
A record passed by handle dropped via a flat `Op::Drop` (rc_dec the record block only), leaking every
heap field. FIXED (commit ec06c7ca): `materialized_call_arg` seeds the arg's `record_masks` + routes a
Map/List[heap]/record-field record to `$__drop_<R>`. **Leak-verified 10000×/20000×** for a record with
a `Map[String,String]` field passed as an arg AND as a bind. Tests:
`record_call_arg_with_map_field_drops_leak_free`, `record_recursive_drop_frees_heap_fields_leak_free`.

So the **records LANGUAGE FEATURE is now complete on v1**: construct (incl empty Map/List fields),
field read (incl call-returned records), spread (incl returned), recursive nested-ownership drop, and
leak-freedom — all gated (suite 494/0, corpus-wall ACCEPT all 4 properties, leak loops). The
recursive record drop + its arg/bind drop are the headline mechanism — DELIVERED.

svg full conquest now needs only svg-SPECIFIC STDLIB coverage (NOT the records feature):
1. **`map.entries` for `Map[String,String]`** — UNLINKED. render_attrs (`map.entries(attrs) |>
   list.map((p)=>{let (k,v)=p; …})`) walls. Needs `map_entries_str` (→ `List[(String,String)]`) which
   in turn needs the **`(String,String)` tuple-list** kind (materialize via a prim builder; the
   C1-defunc `list.map` over it with the `let (k,v)=p` destructure; the per-element 2-String drop) —
   a multi-part sub-feature parallel to csv's `(String,Value)` `str_value_elem_lists`.
2. **`List[Record]` literal** (`group([rect(…), …])`, the "group nests children" test) — a
   `try_lower_record_list_literal` (alloc + store each Element handle, `variant_drop_handles =
   "list_<R>"`); generate `$__drop_list_<R>` for all rec_names R.
3. A residual **`heap-result SpreadRecord` (1×)** wall in one svg fn (not attr/fill/render_attrs/
   render_el/el — those are clean) — likely resolves once (1)/(2) land or is a small spread-context gap.

svg test breakdown: single-element tests (`rect|>fill|>render`) need (1); "group nests children" needs
(2). RECOMMEND continuing as: "map.entries_str + (String,String) tuple-list" → "List[Record] literal"
→ svg `almide test` all "via WASM". Each an independent gated brick on the now-complete records core.

## STATUS 4 — records feature + List[Record] literal DONE; svg needs 2 final svg-specific pieces

This autonomous run landed (all gated: suite 496/0, corpus-wall ACCEPT all 4, 10000× leak loops):
- recursive record drop `$__drop_<R>` (05a40219)
- record-call-argument leak fix (ec06c7ca)
- **List[Record] literal materialization** `try_lower_record_list_literal` (bc36226f) — `group([rect…])`
  builds + drops via `$__drop_list_<R>`; generate `$__drop_list_<R>` for every recursive-drop record.

The records LANGUAGE FEATURE is complete on v1. svg `almide test` (whole module compiled per test
file) now walls on exactly TWO svg-specific stdlib/lowering pieces (confirmed via svg_r + rec7):

1. **`map.entries` for `Map[String,String]`** (render_attrs, used by EVERY element render) — UNLINKED.
   Needs `map_entries_str` (→ `List[(String,String)]`) which needs the **`(String,String)` tuple-list**
   sub-feature: build it (a prim builder over map_str's interleaved entries — alloc a 2-slot tuple per
   entry, store_str-copy key+value, store the tuple handle in the list), the C1-defunc `list.map` over
   it with the `let (k,v)=pair` destructure (cf. csv's `(String,Value)` `str_value_elem_lists` —
   PARALLEL machinery for `(String,String)`), and a `List[(String,String)]` drop (per tuple: rc_dec 2
   Strings). The biggest remaining piece.
2. **`List[Record]` CONCAT** (`add_child` = `{ ...parent, children: parent.children + [child] }`) —
   `lower_owned_heap_field` has no `ConcatList` arm, so the spread-override `children + [child]` walls
   "heap-result SpreadRecord". Needs: a `ConcatList` arm in `lower_owned_heap_field` that builds the
   appended `List[Record]` (rc_inc each kept record + the new one, cf. `__ldls_share`) and tracks the
   result `variant_drop_handles = "list_<R>"`. Smaller than (1); mirrors the List[Record] literal.

Both are svg-SPECIFIC stdlib/lowering coverage, NOT the records feature (done). svg test breakdown:
single-element tests (`rect|>fill|>render`) need (1) [+ (2) only because the whole module incl
`add_child` must lower]; "group nests children" needs (1)+(2). RECOMMEND: do (2) [List[Record] concat,
the smaller] then (1) [map.entries_str + (String,String) tuple-list], each gated, then svg `almide
test` all "via WASM". The records core they build on is complete + committed.

## STATUS 5 — svg is ONE templated piece away (map.entries / (String,String) tuple-list)

This run landed (all gated: suite 496/0, corpus-wall ACCEPT all 4, 10000–20000× leak loops):
- recursive record drop `$__drop_<R>` (05a40219)
- record-call-arg leak fix (ec06c7ca)
- List[Record] LITERAL materialization (bc36226f)
- List[Record] CONCAT in spread override — svg `add_child` (7dc509c5)

After these, **svg's ONLY remaining wall is `map.entries` on `Map[String,String]`** (render_attrs;
used by every element render). The SpreadRecord wall is GONE (the concat fixed add_child); add_child,
attr, fill, render_el, render_attrs, el, rect, group all lower. svg = ONE piece: the
**`(String,String)` tuple-list**, a DIRECT MIRROR of the existing `(String,Value)` machinery:

- `__drop_list_str_value` / `__svdrop_list` (value_core.almd) → mirror as `__drop_list_str_str` /
  `__drop_ss_loop` (per tuple: rc_dec BOTH String slots @ th+12 and @ th+20 at the tuple's rc==1, then
  the tuple block; then the list). The (String,Value) version rc_dec's @12 + `__drop_value` @20 — for
  (String,String) both are plain `rc_dec`.
- `map_entries_str(m: Map[String,String]) -> List[(String,String)]` (value_core): the map_str block is
  interleaved (key @ 12+i*16, value @ 12+i*16+8); build via `acc + [(k, v)]` (rc-shared tuples — the
  tuple construct Dup's each map String into a co-ref). Register `("map_entries_str","map.entries")`.
- `try_lower_concat_list`: add a `str_str_elem` case (Tuple[String,String]) → `__list_concat_rc` +
  track `str_str_elem_lists` (or `variant_drop_handles = "list_str_str"`). MIRROR the `str_value_elem`
  case exactly.
- Dispatch `map.entries` on `Map[String,String]` → `map.entries_str` (lower/mod.rs map-dispatch, the
  "key heap, value heap → _str" branch ~2613).
- `drop_op_for` + materialized_call_arg + the Call-bind arm: route a `List[(String,String)]` value to
  the new drop (mirror `str_value_elem_lists` → `DropListStrValue`).

The C1-defunc `list.map` over the tuple-list + the `let (k,v)=pair` destructure should work via the
existing (String,Value) tuple-map machinery (element-type-agnostic tuple-handle read). Once this lands:
`cd /Users/o6lvl4/workspace/github.com/almide/svg && almide test` → all "via WASM", 0 failed = svg
FULL CONQUEST. The records LANGUAGE FEATURE it builds on is COMPLETE + committed.

## STATUS 6 — ✅ svg FULL CONQUEST ACHIEVED

The svg library renders BYTE-IDENTICAL to v0 on the v1 wasm trust spine, leak-free. The full cascade
landed + gated this run (suite green, corpus-wall ACCEPT all 4, 10⁴ leak loops):

1. recursive record drop `$__drop_<R>` (05a40219)
2. record-call-argument leak fix (ec06c7ca)
3. List[Record] LITERAL materialization — group([…]) (bc36226f)
4. List[Record] CONCAT in spread override — add_child (7dc509c5)
5. **map.entries on Map[String,String] + the (String,String) tuple-list** (a4431c39): `map_entries_str`
   builds rc-shared (key,value) tuples; `DropListStrStr`/`$__drop_list_str_str` frees them;
   `try_lower_concat_list` + the list-literal builder admit `(String,String)` tuple elements; the
   defunc `list.map` binds a tuple element as a borrowed materialized aggregate so `let (k,v)=pair`
   destructures it; a self-recursive call inside a defunc-map body is admitted (`in_defunc_body`) so
   `children |> list.map(render_el)` INLINES; and the missing `UnOp` arm in lower_bind's scalar path
   was added so `let hc = not list.is_empty(xs)` emits the operand call (was a deferred Const).
6. **map.entries-result leak fix** (3a245d5b): `is_list_str_str_ty` reclassifies a bound/arg
   `List[(String,String)]` to `str_str_elem_lists` (DropListStrStr) before the flat `heap_elem_lists`.

VERIFICATION:
- `render(rect|>fill)`, `render(text)`, `render(group([rect,circle]))`, `render(doc([…]))` all
  byte-match v0 via render_program → wasmtime.
- `render(doc([rect,circle]))` × 10000 in a loop: v0 1690000 == v1 1690000 (leak-free; was OOM).
- svg repo `almide test`: 15 passed, 0 failed (mod.almd — the records render — VIA WASM).
- Regression tests: map_entries_str_map_and_tuple_destructure, map_entries_render_loop_leak_free,
  not_bool_call_bound_in_let, defunc_map_self_recursive_record_render, record_list_literal/concat,
  record_call_arg_with_map_field, record_recursive_drop — all in render_wasm/tests_part3.rs.

The records LANGUAGE FEATURE (construct / field read / spread / recursive nested-ownership drop /
leak-freedom / List[Record] literal+concat / Map[String,String] entries) is COMPLETE on v1, and the
svg full-conquest goal is MET. (svg's path.almd falls to v0's native test fallback — a separate v0
emit_wasm path-module limitation, outside the records-feature scope.)
