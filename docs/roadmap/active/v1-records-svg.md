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
