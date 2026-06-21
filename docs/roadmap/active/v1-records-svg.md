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

## Progress 2 (committed 4190062d, gated: suite 493 + corpus-wall ACCEPT)

- **`el`** — empty `Map` (`[:]`) and empty `List[Element]` (`[]`) record fields now materialize
  (`lower_owned_heap_field` builds a 0-length layout-agnostic block; an empty list of ANY element type
  is admitted — the recursive `children: []`). `Element { tag, attrs: [:], children: [], content: "" }`
  constructs + its field reads load the real empty slots.
- **svg's RECURSIVE RENDERER LOWERS** — `render_el` (recursive, `element.children |> list.map((c) =>
  render_el(c, …)) |> list.join`), `render_attrs` (Map over `attrs`), and `format_points` all lower
  clean. The recursive read/traverse side of svg is DONE.

## Remaining for svg full conquest (the recursive-record-DROP frontier)

Only the List[Element]-children CONSTRUCTORS remain (`doc`/`group`/`defs` = `{ ...base, children:
children }`, and `group([e0, e1])` call sites with a `List[Element]` LITERAL). Two intertwined pieces:

1. **List[Element] literal materialization** — `group([rect(…), circle(…)])`: build a list block
   storing each Element (record) handle (Dup/move), like the csv nested-list builder but for records.
2. **Recursive Element drop** — THE blocker. Records today drop FLAT via `record_masks` (rc_dec each
   heap slot), which would leak a populated `children: List[Element]` (rc_dec the list block, leaking
   the Elements + their String/Map fields). Needs a GENERATED per-record-type recursive drop
   `$__drop_<Record>` — the same shape as the Value-model `$__drop_value` / the ADT `$__drop_<ty>`
   (free each heap field; for a `List[Record]` field, free each element recursively) — wired into the
   masked record drop AND a `List[Record]` drop. This is a substantial mechanism (a record-drop-fn
   generator), the genuine recursive-ownership frontier for static records; NOT a leaking shortcut
   (② discipline). A focused next brick.

`map.entries` is also unlinked in the v1 registry (render_attrs lowered because it walls there too —
re-check once the drop lands).
