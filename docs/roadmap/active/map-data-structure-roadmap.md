<!-- description: Design decision and roadmap for Map[K,V]/Set[T]: an O(1) insertion-ordered hash map (compact-ordered-dict) -->
# Map / Set data-structure roadmap

How `Map[K,V]` (and `Set[T]`) are represented, and where they should go.

## Decision

**`Map` is an O(1) insertion-ordered hash map — a compact-ordered-dict (②).** It is the
standard "correct" map (Python dict / `indexmap`), and "do the better structure since
both options are the same big-bang rewrite anyway." `Set` stays a dense list (its
`PartialEq`, possibly-`Float` elements don't want a hash index). Both iterate in
**insertion order**; cross-target equivalence (native iteration == wasm) is the hard
invariant, enforced by the gate.

Shipped so far:
- **native** `Set` + `Map` (`runtime/rs/src/{set,map}.rs`, PR #354): `Vec<T>` /
  `Vec<(K,V)>`, first-seen insertion order, `insert` updates value in place (keeping
  position) / appends new, `remove` preserves survivor order, order-independent
  equality. Key/elem bound is `PartialEq` (not `Eq + Hash`), so `Set[Float]` and maps
  over non-`Hash` keys work. **This is the cross-target reference oracle for iteration
  ORDER** — native stays `Vec` (see ② on why no native hash index).
- **wasm `Set`**: dense `[len][cap][data…]` list (insertion order).

**SHIPPED: wasm `Map` → compact-ordered-dict (②).** Branch `xtarget-wasm-map-order`
(layout consts `709b6764`, helpers `715acc1e`, op switch `89a5833e`). Dense `(key,val)`
entries (insertion order, iterate `0..len`) + a separate hash INDEX region (tags + 1-based
slot pointers) for O(1) lookup; centralized `emit_dict_*` helpers + named `layout::map`
consts put every offset behind a name (the magic-number cure). All map-layout sites
switched together (`calls_map.rs` + `calls_map_closure.rs` + `control.rs` for-in,
`expressions.rs` literal, `equality.rs` eq, `calls_list_closure2.rs` group_by,
`calls_http.rs` headers). `calls_value.rs` was confirmed NOT a map (its `=8` offsets are
`List[(String,Value)]` object payloads). Verified byte-identical native==wasm across the
full spec suite + all `spec/wasm_cross/*.almd`; new `map_insertion_order.almd` locks the
order guarantee into the gate (the prior corpus only checked order-independent counts).
The dead Swiss helpers (`emit_map_resize`/`emit_alloc_table`/`emit_swiss_setup`) are gone;
`set` now grows by load factor (fixing a >16-entry overflow). Full notes: memory
`project_wasm_compact_ordered_dict`.

> ⚠️ The earlier **seq-in-entry** attempt — a wrong hack that changed the entry *stride*
> and so forced touching every hand-coded stride site → magic-number proliferation, 6
> corruption traps — was abandoned. The proper COD does NOT change the entry stride:
> entries stay dense at `ks+vs`; the INDEX is a *separate* region.

### Rejected alternative — ① plain dense list (O(n))

A dense `Vec`-like map (linear-scan lookup), mirroring the wasm Set + native Vec exactly:
simplest, lowest per-op risk, zero magic numbers. Rejected because it makes `Map` secretly
O(n) — a footgun for mutable/read-heavy code — and a hash map *should* be O(1). (The
persistent clone-on-write model makes *building* O(n²) regardless, muting ②'s O(1) win,
which is the only reason ① was ever tempting.)

## ② Compact-ordered-dict — the chosen Map structure

The "correct hash map" — Python 3.7+ `dict`, Ruby `Hash`, Rust `indexmap`: a hash
**INDEX** over a **DENSE insertion-ordered entries** array. O(1) lookup, iteration
still insertion order (walk the dense entries).

- wasm: TAGS (h2 fast-reject) + INDEX (1-based pointer into dense entries) + DENSE
  ENTRIES. Lookup probes the index; iterate walks the dense entries. Entries never
  move on grow (only the index rehashes). Full design + the centralized
  `dict_*`/`layout::map::{INDEX_SLOT_SIZE,EMPTY_INDEX,INITIAL_CAP,GROWTH_SHIFT,LOAD_NUM,LOAD_DEN,H2_SHIFT,H2_MASK}`
  constants already scaffolded (see workflow `wf_1a3614dd`, memory
  `project_wasm_compact_ordered_dict`).
- native: a dense `Vec<(K,V)>` + a hash index. The std `HashMap<K,usize>` index
  forces `K: Eq+Hash` — which **rejects `Map[Float]` and `Map[record-without-derive-Hash]`**
  and so would diverge from wasm (which hashes any key by raw bytes). To keep
  cross-target parity under ②, EITHER adopt the universal **"map keys must be
  hashable+eq"** contract on both targets (clean, but re-restricts today's `PartialEq`
  relaxation), OR give native a custom **byte-hashing** index that matches wasm's hash
  (more code, supports any key — the truly-equivalent option).

**Why chosen:** "keys must be hashable" is the universal standard (every serious language
requires it) and O(n) maps are a footgun — so ② is the right *map* design, and since both
② and ① are the same big-bang rewrite, the better structure wins. **Caveat (accepted):**
under the persistent clone-on-write model the O(n²)-build dominates, so ②'s O(1) lookup
mostly helps mutable `insert`/`delete` chains and repeated `get` on a stable map; the
structural correctness ("a Map is O(1)") is the real reason, not a profile. The magic-number
risk that bit the seq-in-entry hack is handled by doing ② *properly* — separate index
region (entry stride unchanged) + centralized `dict_*` helpers, never inline byte math.
The native side keeps `Vec` (the order oracle) until/unless a custom byte-hashing index is
added; the std `HashMap` index is NOT adopted (its `K:Hash` bound is the regression above).

## ③ HAMT — the true persistent ideal (research)

The persistent-map endgame: a **hash array mapped trie** (Clojure / Scala /
immutable.js) — O(log n) ops with **structural sharing**, so `map.set` does NOT clone
O(n). This is the only structure that fixes the O(n²)-persistent-build that ① and ②
share. Cost: by far the largest implementation, and making it byte-identical across the
native (Rust) and hand-emitted-wasm runtimes is its own project (and a strong argument
for the "one runtime" endgame — compile one Rust runtime to wasm32, or define the map in
Almide itself). Track as the long-term persistent-collections direction.

## Cross-cutting

Whatever the representation, the **hard invariant is observable cross-target equivalence**
(native iteration == wasm iteration, enforced by `tests/wasm_runtime_test.rs::wasm_cross_target_spec`).
Any move ①→②→③ must keep every `spec/wasm_cross/map_*` case byte-identical and add no
`@xt-allow`. Set may stay a dense list (Set[Float] needs only `PartialEq`); the index
question is Map-specific.
