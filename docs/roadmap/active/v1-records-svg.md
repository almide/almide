# v1 — records feature (svg full-conquest target)

The next org-repo frontier after csv (4/4 done). A comprehensive survey of `github.com/almide`
repos found:

- **Already pass v1 (WASM)**: sha1, bigint, rsa (pure algorithmic — Int/Bytes/String).
- **csv**: ✅ 4/4 (this session — TCO result-accumulator + nested-heap-list accessors).
- **svg**: the records target — pure (Float/String/Record, deterministic), 1 of 2 test files already
  via WASM; the walled file needs the **records** feature.
- **aes**: v0 ✅ / v1 walls (records + heap module-global S-box + Range-in-call-arg).
- **toml, yaml**: broken on BOTH v0 (E0308 `String` vs `&str` Rust codegen) AND v1 (toml: heap-carried
  TCO in `parse_doc` `root = set_nested(root,…)` = general heap back-edge merge, the OwnershipLoop.v
  engineering; yaml: float.parse strtod + an internal collect_map wall). The deepest frontier.
- **base64**: v0 E0428 — its `@intrinsic("almide_rt_base64_*")` names collide with the stdlib.
- **porta**: a multi-module CLI app (process effects), not a single-file codec target.

## svg's record type

```
type Element = { tag: String, attrs: Map[String, String], children: List[Element], content: String }
```
String + Map + **recursive List[Element]** fields — the full nested-ownership record surface.

## Done (committed b77dfde3, gated: suite 491 + corpus-wall ACCEPT)

- **Record returned from a CALL** (`let p = mk(5)`) is now seeded `materialized_aggregates` +
  `record_masks`, so a heap-field read `p.y` loads the real slot instead of the container-grain Dup
  that returned the whole record (the `mk(5).y` empty-string miscompile), and the owned scope-end drop
  frees the heap fields. (binds.rs, the Named-call bind arm.)
- **SpreadRecord RETURNED** (`fn attr(e,k,v) = { ...e, attrs: map.set(…) }` — the svg builder shape)
  added to the tail-return path → builds + moves out a fresh same-layout block. (tail.rs.)
  → svg's `attr` now lowers clean.

## Remaining for svg full conquest (the recursive-record frontier)

1. **`el`** — `Element { tag, attrs: [:], children: [], content: "" }`: record construct with an EMPTY
   `Map` (`[:]`) and EMPTY `List[Element]` (`[]`) field. `lower_owned_heap_field` must materialize an
   empty Map / empty List as a record field (today it handles LitStr/ConcatStr/Var/Call).
2. **List[Element] argument** (doc / group / render call `f(children)`): passing a `List[Element]` to a
   call walls ("would borrow an empty deferred heap value") — the nested-heap-list arg, analogous to
   the csv `list.get`/`drop` over `List[List[String]]` but in arg position.
3. **Recursive Element drop** — a populated `children: List[Element]` field needs a per-field-type
   masked record drop where the List[Element] slot frees each child Element RECURSIVELY (the
   nested-ownership record drop; el's empty fields drop flat, but `doc`'s populated children do not).
4. **Map field ops** — `map.set` (attr), `map.entries` (render) over the `attrs: Map[String,String]`
   field; `map.entries` is currently unlinked in the v1 self-host registry.

These are the records nested-ownership frontier — a multi-brick continuation, the same class as the
csv `List[List[String]]` crossing but a wider surface (Map + recursive records).
