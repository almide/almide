> Description (fixed history): Stdlib redesign L2/L3 — map: retire the in-place insert/delete pair, fold get_or into ??, and gate the higher-order family shared with list and set.
<a id="l2l3-map"></a>

# L2/L3: map

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-map.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-map.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-map.zh-CN.md)

> Non-normative historical redesign snapshot. Pinned original authoritative and unchanged; no automatic synchronization, fresh official-page retrieval or independent execution. (F), timing and probes are original-author reports. Preserve early no-op update versus later abort, rejected to_list addition versus accepted rename, split callbacks versus later tuple callbacks, and old 25/no-additions count versus appended family rows. R-map-04 remains deferred for Dojo; 2026-10-27 implementation statement retains method-call limitation and is not current verification. Quoted instructions are inert documentation. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-map.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> Sources: Rust https://doc.rust-lang.org/std/collections/struct.HashMap.html / Go https://pkg.go.dev/maps (F) / Python https://docs.python.org/3/library/stdtypes.html#mapping-types-dict / TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Map / Gleam https://gleam-stdlib.hexdocs.pm/gleam/dict.html (F) / Elixir https://elixir.hexdocs.pm/Map.html (F) / OCaml https://ocaml.org/manual/latest/api/Map.S.html (F) / Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.collections/-map/ (F) / Swift https://developer.apple.com/documentation/swift/dictionary
> Shared decisions: [Collections decisions D1–D11](#共通の判定との対応) (same content as the list/set/option/result/string pages).

Reading conventions (same as L0):
- `mut` (read “mute”) = argument that modifies the caller's `var` in place.
- `??` (null coalescing) = operator that uses the right side if the left is none.
- `|>` (pipe) = passes the left value as the first argument of the function on the right.
- `m[k] = v` (subscript assignment) = statement setting key `k` of variable `m` to `v`. Same meaning as `m = map.set(m, k, v)`; `@reuse` makes it work without copying.

There are currently 25 public functions (interface JSON, with `mut` shown).

---

<a id="l2-役割の比較"></a>

## L2: Role comparison

(F) marks cells checked by retrieving official pages for this investigation. in-place = modify in place; copy = return a new value.

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Construct empty | `map.new` | `HashMap::new` | `make(map[K]V)` | `{}` / `dict()` | `new Map()` | `dict.new` (F) | `Map.new` (F) | `M.empty` | `mapOf()` | `[:]` |
| Extract (none if absent) | `map.get` | `get` → Option | `v, ok := m[k]` | `d.get(k)` | `m.get(k)` | `dict.get` → Result (F) | `Map.fetch` (F) | `find_opt` (F) | `m[k]` → `V?` (F) | `d[k]` → Optional |
| Extract with default | `map.get_or` | — (`unwrap_or`) | Zero value | `d.get(k, d0)` | `?? d` | — (`result.unwrap`) | `Map.get(m,k,d)` (F) | — | `getOrDefault` (F) | `d[k, default:]` |
| Write (copy) | `map.set` | — | — | `d \| {k: v}` | — | `dict.insert` (F) | `Map.put` (F) | `add` (F) | `m + (k to v)` (F) | — |
| Write (in-place) | `map.insert(mut)`・`m[k] = v` | `insert` | `m[k] = v` | `d[k] = v` | `m.set` | — | — | — | `m[k] = v` | `d[k] = v` |
| Remove (copy) | `map.remove` | — | — | — | — | `dict.delete` (F) | `Map.delete` (F) | `remove` (F) | `m - k` (F) | — |
| Remove (in-place) | `map.delete(mut)` | `remove` | `delete(m,k)` | `del d[k]` | `m.delete` | — | — | — | `remove` | `removeValue` |
| Clear all (in-place) | `map.clear(mut)` | `clear` | `clear(m)` | `d.clear()` | `m.clear()` | — | — | — | `clear` | `removeAll` |
| Key exists? | `map.contains` | `contains_key` | `_, ok := m[k]` | `k in d` | `m.has` | `dict.has_key` (F) | `Map.has_key?` (F) | `mem` (F) | `containsKey` (F) | `d[k] != nil` |
| Count/empty check | `map.len`・`map.is_empty` | `len`・`is_empty` | `len` | `len` | `size` | `size`・`is_empty` (F) | `map_size` | `cardinal`・`is_empty` (F) | `size`・`isEmpty` (F) | `count`・`isEmpty` |
| Key/value lists | `map.keys`・`map.values` | `keys`・`values` | `maps.Keys` (F) | `keys()` | `keys()` | `dict.keys`・`values` (F) | `Map.keys`・`values` (F) | — | `keys`・`values` (F) | `keys`・`values` |
| List of pairs | `map.entries` | `iter()` | `maps.All` (F) | `items()` | `entries()` | `dict.to_list` (F) | `Map.to_list` (F) | `to_list`・`bindings` (F) | `entries`・`toList` (F) | Iteration |
| Construct from pair list | `map.from_list` | `from_iter` / `collect` | `maps.Collect` (F) | `dict(pairs)` | `new Map(pairs)` | `dict.from_list` (F) | `Map.new(list)` | `of_list` (F) | `toMap` | `Dictionary(uniqueKeysWithValues:)` |
| Merge two (right wins) | `map.merge` | `extend` | `maps.Copy` (F) | `a \| b` | — | `dict.merge` (F) | `Map.merge/2` (F) | — | `a + b` (F) | `merging` |
| Merge using a combining function | — | — | — | — | — | `dict.combine` (F) | `Map.merge/3` (F) | `union` (F) | — | `merging(_:uniquingKeysWith:)` |
| Transform values only | `map.map`（`(V) -> B`） | — | — | Comprehension | — | `dict.map_values`（`fn(k, v)`）(F) | — | `map` (values)・`mapi`（F） | `mapValues` (F) | `mapValues` |
| Filter | `map.filter`（`(K, V)`） | `retain` | `maps.DeleteFunc` (F) | Comprehension | — | `dict.filter` (F) | `Map.filter` (F) | `filter` (F) | `filter` (F) | `filter` |
| Fold | `map.fold` | `iter().fold` | — | — | — | `dict.fold` (F) | `Enum.reduce` | `fold` (F) | `entries.fold` | `reduce` |
| any / all / count / find | `map.any`・`all`・`count`・`find` | `iter().any` etc. | — | `any(...)` | — | — | `Enum.*` | `exists`・`for_all` (F) | `any`・`all`・`count` (F) | `contains(where:)` etc. |
| Update by function if present (otherwise no-op) | `map.update` | `entry().and_modify` | — | — | — | — | `Map.update!` (exception if absent) | `update`（option→option）(F) | — | — |
| Update if present, initial value if absent | `map.upsert` | `entry().and_modify(f).or_insert(v)` | — | `setdefault` | — | `dict.upsert`（`fn(Option(v)) -> v`）(F) | `Map.update/4` (initial value inserted unchanged)(F) | `update` (F) | `getOrPut` | `d[k, default:] += 1` |
| Fetch and remove | — | `remove` → Option | — | `d.pop(k)` | — | — | `Map.pop` → `{v, map}` (F) | — | `remove` → `V?` | `removeValue` → Optional |

<a id="完全性の規則"></a>

### Completeness rules

1. **Higher-order family (shared by list/map/set)**: all three collections provide the seven functions `map`, `filter`, `fold`, `any`, `all`, `count`, `find` (shared decision D3). Map already has all seven. Set's missing `count` and `find` are added in [L2-set](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-set.md).
2. **Basic set (shared by list/map/set)**: `new`, `from_list`, `len`, `is_empty`, `contains`. Map has them all.
3. **Map callback shape**: functions observing both key and value (`filter`, `fold`, `any`, `all`, `count`, `find`) accept `(K, V)`. Value-only transformations `map.map`, `map.update`, `map.upsert` accept `(V)`. Omitting the key from value-only modification functions is intentional (the key does not change).
4. **Mutation style**: every public map function except `clear` is copy-returning (R-L0-04). The in-place single-key write is the language's subscript assignment `m[k] = v`, not a function.
5. **Intentional omissions**: no fetch-and-remove (`pop`), merge with a combining function (`merge`), or key transformation `map_keys` (R-map-05).
6. **Out-of-range writes**: L0's “positional writes abort out of range” governs index/position writes. Map writes by key, so no position is out of range. A missing key making `map.update` do nothing is an ordinary “key absent” case, like `map.get` returning none, not a contract violation (same as Rust `entry().and_modify` and OCaml `update`). Keep this behavior.

<a id="l2-の判定"></a>

### L2 verdict

- **Style conflict (R-L0-04)**: only `map.insert(mut)` and `map.delete(mut)` mutate in place using copy-side verbs `insert`/`delete`. `set.insert` and `list.insert` return copies → R-map-01.
- **Duplicate default form (R-L0-06)**: `map.get_or` is equivalent to `map.get(m, k) ?? d` → R-map-02.
- **Missing roles (at least 5/9)**:
  - Obtain pairs through `to_list` (Gleam/Elixir/OCaml/Kotlin, 4/9) → not a majority, and `map.entries` serves the role → R-map-03 (rejected).
  - Fetch-and-remove (Rust/Python/Elixir/Kotlin/Swift, 5/9) → expressible in two steps (`get` and `remove`); `pop` is reserved for the in-place “take from end” verb → R-map-05 (rejected).
- **Silently discarded calls (R-L0-05)**: measurements show statement calls `map.set(m, "z", 9)` and `map.remove(m, "b")` do nothing without errors on native or wasm (below). Removing `map.insert` in R-map-01 will increase statement uses of `map.set(m, k, v)` from other-language habits, so R-L0-05 must land first.
- **Family holes**: none in map. R-map-06 establishes the family-wide gate.

Measurements (2026-10-21, `almide 0.68.0 (dev, 437b8f127)`, same output on native and `--target wasm`; `scratchpad/sr/probe-map/a.almd`):

```almide
var m: Map[String, Int] = map.new()
m["a"] = 1
m["b"] = 2
m["a"] = 10                 // 上書き。位置は変わらない → [("a", 10), ("b", 2)]
m = map.remove(m, "a")      // → [("b", 2)]
map.set(m, "z", 9)          // エラーにならず、何も起きない
map.remove(m, "b")          // エラーにならず、何も起きない → len は 1 のまま
map.update(m, "nope", f)    // キーが無ければ何もしない
map.upsert(m, "nope", 0, f) // 無ければ 0 がそのまま入る（f は呼ばれない）。あれば f(old)
map.merge(a, b)             // 同じキーは b が勝つ。順序は a、その後 b の新しいキー
```

The comments say: overwriting retains position; the two discarded calls cause no error or change (length remains 1); update does nothing for an absent key; upsert inserts the initial value unchanged without calling its callback when absent, otherwise applies it to the old value; merge favors the right map and preserves left order followed by new right-hand keys.

`map.to_list` produces E002, whose hint only says “see CHEATSHEET”, not `map.entries`.

---

<a id="案"></a>

## Proposals

<a id="r-map-01-mapinsert-と-mapdeleteその場書き換えを撤去する---採用"></a>

### R-map-01 Remove in-place `map.insert` and `map.delete` — ○ Accepted

- Content:
  - Annotate `map.insert(mut m, k, v) -> Unit` and `map.delete(mut m, k) -> Unit` with `@deprecated(since=15, use=…)`, remove in epoch 16.
  - Replacements: `map.insert(m, k, v)` → `m[k] = v` (when `m` is a variable) or `m = map.set(m, k, v)`; `map.delete(m, k)` → `m = map.remove(m, k)`.
  - Retain `map.clear(mut m)` (`clear` belongs to R-L0-04's verb set).
  - Remove `map.insert` from CHEATSHEET line 193's “in-place stdlib op” list and two locations in `llms.txt` (including E032's line).
- Supporting L0 rules: R-L0-04 (`insert`/`delete` are copy-side verbs explicitly named in L0), R-L0-01, R-L0-05, R-L0-14.
- (a) Among nine languages, those not distinguishing styles by name (Rust/JS/Python) use `insert`/`set`/`delete` for mutation; nonmutating languages (Gleam/Elixir/OCaml Map) use the same verbs for copies. Almide defaults to copies, so aligns with the latter. In-place single-key writes use subscript assignment `m[k] = v`, as in Go/Python/Kotlin/Swift.
- (b) Native/wasm: no impact. Subscript assignment and `map.set` + `@reuse` already work on both paths (same measured output). L0 measured `map.set`, `map.insert`, and subscript assignment all at about 0.07 seconds for 2 million entries.
- (c) Currently `set.insert` copies and `map.insert` mutates, so verbs do not reveal style. After removal, “`insert` always copies”. But a Rust habit of writing `map.insert(m, k, v)` followed by a removal hint recommending statement `map.set` would silently discard it, so the first hint choice is subscript assignment.
- (d) Removal. Compatibility: `@deprecated(since=15, use="m[k] = v")` and `@deprecated(since=15, use="map.remove")`; add `[[deprecation]]` to `proofs/dialect-epochs.toml` (`removal_epoch = 16`). `almide fix`: when the first argument is a variable path (`m`, `r.m`), use `m[k] = v`; rewrite `map.delete(m, k)` to `m = map.remove(m, k)`. After removal, `removed_spellings.rs` hints show these forms.
  - Usage counts (occurrences of `\bmap\.insert\b` / `\bmap\.delete\b`, upper bounds including definitions/generated docs): insert spec 96 / tests 27 / docs 19 / stdlib 1 (comment only); delete spec 16 / tests 19 / docs 10 / stdlib 0.
  - Ordering: R-L0-05 (error on discarded statement values) must land first in the same epoch 15. Otherwise migrated `map.set(m, k, v);` is silently discarded.
- Gate: in R-L0-G's `tests/stdlib_surface_conventions_test.rs`, check `clear` is map's only public function with a `mut` argument. Add native⇄wasm fixtures for subscript assignment and `map.remove` reassignment in `spec/stdlib/`.
- Issue: #3928

<a id="r-map-02-mapget_or-を撤去して--に一本化する---採用"></a>

### R-map-02 Remove `map.get_or`, consolidate on `??` — ○ Accepted

- Content: mark `map.get_or(m, k, d)` with `@deprecated(since=15, use="??")`, remove epoch 16. Replace with `map.get(m, k) ?? d`.
- Supporting L0 rules: R-L0-06 (2), R-L0-01, R-L0-14.
- (a) Default forms exist in Python `get(k, d)`, Elixir `Map.get(m, k, d)`, Kotlin `getOrDefault`, Swift `d[k, default:]`. Rust/JS/Gleam use operators or general functions. No majority; Almide favors CHEATSHEET's `??`.
- (b) No impact. ADR-0005 desugars `??` to `option.unwrap_or_else`, identical on both targets.
- (c) `get_or` always evaluates its default, while `??` evaluates the right side only for none. `??` is safer for costly defaults.
- (d) Removal. Compatibility: `@deprecated(since=15, use="??")`, `removal_epoch = 16`, same window as `#1107`'s `option.unwrap_or` / `result.unwrap_or`. `almide fix`: `map.get_or(m, k, d)` → `map.get(m, k) ?? d` (pipe `|>` form `m |> map.get_or(k, d)` → `m |> map.get(k) ?? d`). Counts: spec 161 / tests 29 / docs 21 / stdlib 0. Batch-rewrite the 161 specs with `almide fix`, regenerate run/check/ast ledgers.
- Gate: add to R-L0-G a check that public names do not end in `_or` (allowlist may only shrink).
- Issue: #3913

<a id="r-map-03-mapto_list-を足す---不採用-r-map-07-で採用に変更"></a>

### R-map-03 Add `map.to_list` — × Rejected (→ accepted in R-map-07)

- Content: do not add pair-list-returning `map.to_list`. Retain `map.entries` as the only spelling. Instead change E002's hint for `map.to_list` to “use `map.entries`” (currently only “see CHEATSHEET”, measured).
- Supporting L0 rules: R-L0-00 (keep when no majority), R-L0-01 (no two spellings for one role).
- (a) `to_list` is 4/9: Gleam/Elixir/OCaml/Kotlin (OCaml also has `bindings`, Kotlin `entries`). JS uses `entries()`, Python `items()`, Rust `iter()`; less than 5/9.
- (b) No impact.
- (c) Because `set.to_list` and `map.from_list` exist, LLMs infer `map.to_list`. Adding it would be a synonym; handle this via hints.
- (d) No change (diagnostic hint text only).
- Mechanism: add to E002's “did you mean” `ALIASES` table (`crates/almide-frontend/src/stdlib.rs:156-230`, consulted only for undefined names): `("map", "to_list", "map.entries")`, `("map", "put", "map.set")`, `("map", "items", "map.entries")`. After R-map-01 removals, also add `("map", "insert", "m[k] = v")` and `("map", "delete", "map.remove")` (removed-spelling hints in `removed_spellings.rs` take precedence; put them in only one place).
- Fix old rows in the same table:
  - `("map", "count", "map.len")` (line 164): `map.count` is public for counting predicate matches, so the row is never consulted. Delete the incorrect `count` = `len` mapping.
  - `("map", "new", "[:] (empty map literal)")` (line 223): `map.new` is public; another dead row. Delete it.
- Gate: add broken/fixed `map.to_list` cases to `tests/diagnostics/`, verify hint names `map.entries`. Test that every `ALIASES` left-hand name is not a public function, preventing dead rows.
- Issue: #3905 (diagnostic improvement)

<a id="r-map-04-mapmap-を-map_values-に改名するまたはコールバックを-k-v-にする--保留六巡目に--から変更dojo-で測る"></a>

### R-map-04 Rename `map.map` to `map_values`, or accept `(K, V)` callbacks — Deferred (changed from × in sixth round; measure in Dojo)

- Content: keep `map.map(m, f: (V) -> B)` name and callback shape.
- Supporting L0 rules: R-L0-00, R-L0-14 (4: no repurposing meaning).
- (a) Value-only transformations use `map_values` (Gleam)/`mapValues` (Kotlin/Swift), 3/9, and `map` (OCaml), 1/9: no majority. Gleam `map_values`, Kotlin `mapValues`, and OCaml `mapi` also pass the key.
- (c) Since `map.filter` etc. use `(K, V)` while `map.map` uses `(V)`, writing `(k, v) =>` is a possible mistake, but yields an argument-count type error, not silent acceptance. Changing shape under the same name leaves old forms in training data (R-L0-14 item 4).
- (d) No change.
- Gate: none (type checking stops it).
- Issue: none.
- Reason for deferral (sixth round): “mistakes produce type errors” alone does not justify rejection under R-L0-36. Three candidates: current `map.map` with `(V)`, `map.map` with `(K, V)`, or rename to `map_values`. Other languages differ too (Swift/Kotlin `mapValues`, OCaml value-only `map` and key/value `mapi`, Gleam key/value `map_values`). Decide after measuring candidate survival rates on identical tasks/models in Dojo (almide-dojo#64).

<a id="r-map-05-取り出して消す-pop合わせ方つきの-merge_withキーを変換する-map_keys-を足す---不採用"></a>

### R-map-05 Add fetch-and-remove `pop`, custom-combining `merge_with`, and key-transforming `map_keys` — × Rejected

- Content: add none of the three.
- Supporting L0 rules: R-L0-04 (`pop` is in-place take-from-end), L1 addition criterion (at least 5/9), R-L0-00.
- (a)
  - Fetch-and-remove: Rust/Python/Elixir/Kotlin/Swift, 5/9. Four only expose it as a mutating operation's return; only Elixir `Map.pop` is copy-returning. Almide can use two steps `map.get(m, k)` and `m = map.remove(m, k)`. Naming a copy form `pop` would split the in-place take-from-end meaning of `pop` fixed by R-L0-04.
  - Merge with a combining function: Gleam `combine`, Elixir `merge/3`, OCaml `union`, Swift `merging`, 4/9 with differing names. Expressible as `map.fold(b, a, (acc, (k, v)) => map.upsert(acc, k, v, (old) => g(old, v)))`.
  - Key transformation: only Kotlin `mapKeys`.
- (b)(d) No change.
- Issue: none.

<a id="r-map-06-高階関数の-family-を-listmapset-の行列でゲートする---採用"></a>

### R-map-06 Gate the higher-order family as a list/map/set matrix — ○ Accepted

- Content: from interface JSON, check the seven functions `map`, `filter`, `fold`, `any`, `all`, `count`, `find` exist in list/map/set; map predicate callbacks use `(K, V)`, list/set use `(A)`. Map unchanged. Set's `count`/`find` additions are [L2-set R-set-01](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-set.md).
- Supporting L0 rules: R-L0-01, R-L0-10, CLAUDE.md “expand API families as matrices”.
- (a) Among nine languages, inconsistent higher-order collection families (Gleam set lacking `count`/`find`) are places writers try and fail.
- (b) No impact (checks only).
- (c) A form learned for one collection works unchanged in others.
- (d) No additions (gate only).
- Gate: add “collection family matrix” to R-L0-G tests. Missing cells fail; intentional omissions require reasons in a shrinking-only allowlist.
- Issue: #3901

---

<a id="r-map-07-mapentries-を-mapto_list-に改名するr-map-03-の見直し---採用"></a>

### R-map-07 Rename `map.entries` to `map.to_list` (revisiting R-map-03) — ○ Accepted

- Current: `fn map.entries(m: Map[K, V]) -> List[(K, V)]` and `fn map.from_list(pairs: List[(K, V)]) -> Map[K, V]`. R-map-03 rejected adding `to_list` (no majority, synonym).
- Proposal: rename, not add. `map.entries` → `fn map.to_list(m: Map[K, V]) -> List[(K, V)]`. Keep `keys`/`values`.
- New evidence: redesigned container-to-List spellings split between six `to_list` modules (`set`, `deque`, `heap`, `bytes`, `option`, `result`) and only map's `entries`. Map alone has `from_list` but no `to_list`. R-map-03 considered only nine-language majority (R-L0-00 item 2), overlooking internal Almide conflict (item 1).
- Supporting L0 rules: R-L0-00 item 1, R-L0-07 item 2 (construction `from_list` and its inverse).
- (a) `to_list`: Gleam/Elixir/OCaml/Kotlin, 4/9; `entries`: JS/Kotlin, 2/9.
- (b) No impact (same body).
- (c) R-map-03 itself admits LLMs infer `map.to_list`. Aligning the name with inference also makes `map.from_list(map.to_list(m)) == m` readable from names.
- (d) Rename. `@deprecated(since=15, use="map.to_list")`, removal epoch 16, one-to-one `almide fix`. Counts: spec 22 / tests 2 / docs 22 / stdlib 7 / research 2 (55 total).
- Completeness: containers with `from_list` have `to_list`, satisfying `from_list(to_list(x)) == x`.
- Gate: add paired `from_list`/`to_list` check to R-L0-07, round-trip property tests for every container except list.
- Issue: #3969

<a id="r-map-08-mapupdate-はキーが無ければ-abort---採用"></a>

### R-map-08 `map.update` aborts for absent keys — ○ Accepted

- Current: absent keys do nothing and return the same map (measured `almide 0.68.0 dev 437b8f127`). `list.update` aborts for absent indices; the shared “modify existing element” role does not predict absence behavior.
- Proposal: abort if absent. Use `upsert` to create if absent; when existence is unknown, check `map.contains` or use `upsert`.
- Supporting L0 rules: R-L0-33 (existing-element modification aborts), R-L0-06 item 3.
- (a) Elixir `Map.update!` and Kotlin `getValue` throw on absence. Rust `entry().and_modify` does nothing, but “and” makes the absent-case branch visible in the written form.
- (d) Semantic change. Calls currently returning unchanged will abort, so include in epoch 15 `breaks`.
- Issue: #3990

<a id="r-map-09-map-を-k-v-の組の-container-にする---採用"></a>

### R-map-09 Make `map` a `Container` of `(K, V)` pairs — ○ Accepted

- Current: `map.filter`, `any`, `all`, `count`, `find` callbacks take two arguments `(K, V) -> Bool`; `fold` takes three `(A, K, V) -> A`. Other containers' callbacks accept one element, so `map` could not join the protocol hierarchy's `Container`/`Sequence`.
- Proposal: accept one pair element `(K, V)` (`filter(m, f: ((K, V)) -> Bool)`, `fold(m, init: A, f: (A, (K, V)) -> A)`). `map.to_list`/`from_list` already use pair sequences. Introduce the language rule allowing an anonymous function with as many parameters as tuple elements where a single-tuple function is expected, so `map.filter(m, (k, v) => v > 0)` still works (same as Swift `dict.filter { key, value in … }`).
- (a) Swift `Dictionary` is a `Collection` of `(key, value)` pairs. Rust `HashMap` iteration, Kotlin `Map.Entry`, Python `items()` also pass one pair at a time.
- (d) Type change. Two-parameter anonymous functions remain valid; `almide fix` rewrites three-parameter `fold` lambdas to `(acc, (k, v)) =>`.
- Implementation status (2026-10-27): implemented. The six callbacks take one pair (`fold` takes accumulator plus pair). The checker reads a lambda with tuple-element-count parameters as `((k, v)) =>` where only one tuple is expected (based only on expected type, not function name). `fold`'s `(acc, k, v) =>` produces E004 with hint `(acc, (k, v)) =>` (epoch 15 `breaks`). This reinterpretation does not yet work for method-call form (`m.filter((k, v) => …)`).
- Issue: #4004

<a id="l3-関数表"></a>

## L3: Function table

Usage counts are given only for non-keep verdicts (see R-map-01/R-map-02).

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `map.new` | `map.new() -> Map[K, V]` | Rust `HashMap::new` / Gleam `dict.new` / Elixir `Map.new` / OCaml `empty` | Keep | — |
| `map.get` | `map.get(m: Map[K, V], key: K) -> Option[V]` | Rust `get` / Python `get` / JS `get` / Gleam `get`(Result) / OCaml `find_opt` / Kotlin `m[k]` | Keep | — |
| `map.get_or` | `map.get_or(m: Map[K, V], key: K, default: V) -> V` | Python `get(k, d)` / Elixir `get/3` / Kotlin `getOrDefault` / Rust・JS・Gleam — (operator) | Remove | R-map-02 |
| `map.set` | `map.set(m: Map[K, V], key: K, value: V) -> Map[K, V]` | Gleam `insert`(copy) / Elixir `put` / OCaml `add` / Kotlin `m + (k to v)` | Keep | R-map-01（replacement） |
| `map.contains` | `map.contains(m: Map[K, V], key: K) -> Bool` | Rust `contains_key` / JS `has` / Gleam `has_key` / OCaml `mem` / Kotlin `containsKey` | Keep | — |
| `map.remove` | `map.remove(m: Map[K, V], key: K) -> Map[K, V]` | Rust `remove` / Gleam `delete` / Elixir `delete` / OCaml `remove` / Kotlin `m - k` | Keep | R-map-01（replacement） |
| `map.keys` | `map.keys(m: Map[K, V]) -> List[K]` | All languages `keys`（Go `maps.Keys`） | Keep | — |
| `map.values` | `map.values(m: Map[K, V]) -> List[V]` | All languages `values` | Keep | — |
| `map.len` | `map.len(m: Map[K, V]) -> Int` | Rust `len` / Gleam `size` / JS `size` / OCaml `cardinal` / Swift `count` | Keep | — (R-L0-11) |
| `map.entries` | `map.entries(m: Map[K, V]) -> List[(K, V)]` | JS `entries` / Kotlin `entries` / Gleam・Elixir・OCaml `to_list` / Python `items` | Rename → `to_list` | R-map-07 |
| `map.merge` | `map.merge(a: Map[K, V], b: Map[K, V]) -> Map[K, V]` | Gleam `merge` / Elixir `merge/2` / Python `a \| b` / Kotlin `a + b` (all favor right) | Keep | — |
| `map.is_empty` | `map.is_empty(m: Map[K, V]) -> Bool` | Rust `is_empty` / Gleam `is_empty` / OCaml `is_empty` / Kotlin `isEmpty` | Keep | — |
| `map.from_list` | `map.from_list(pairs: List[(K, V)]) -> Map[K, V]` | Gleam `from_list` / OCaml `of_list` / Rust `from_iter` / Go `maps.Collect` | Keep | — (R-L0-07) |
| `map.map` | `map.map(m: Map[K, V], f: (V) -> B) -> Map[K, B]` | Gleam `map_values` / Kotlin・Swift `mapValues` / OCaml `map` | Keep | R-map-04 |
| `map.filter` | `map.filter(m: Map[K, V], f: ((K, V)) -> Bool) -> Map[K, V]` | Gleam `filter` / Elixir `filter` / OCaml `filter` / Kotlin `filter` / Rust `retain` | Semantic change (callback takes one pair; `(k, v) =>` remains valid) | R-map-09 |
| `map.fold` | `map.fold(m: Map[K, V], init: A, f: (A, (K, V)) -> A) -> A` | Gleam `fold` / OCaml `fold` / Elixir `Enum.reduce` | Semantic change (second callback argument is a pair; `almide fix` rewrites it) | R-map-09 |
| `map.any` | `map.any(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool` | Kotlin `any` / OCaml `exists` / Rust `iter().any` | Semantic change (callback takes one pair; `(k, v) =>` remains valid) | R-map-09 |
| `map.all` | `map.all(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool` | Kotlin `all` / OCaml `for_all` / Rust `iter().all` | Semantic change (callback takes one pair; `(k, v) =>` remains valid) | R-map-09 |
| `map.count` | `map.count(m: Map[K, V], f: ((K, V)) -> Bool) -> Int` | Kotlin `count` / Elixir `Enum.count/2` | Semantic change (callback takes one pair; `(k, v) =>` remains valid) | R-map-09 |
| `map.find` | `map.find(m: Map[K, V], f: ((K, V)) -> Bool) -> Option[(K, V)]` | Rust `iter().find` / Elixir `Enum.find` / Swift `first(where:)` | Semantic change (callback takes one pair; `(k, v) =>` remains valid) | R-map-09 |
| `map.update` | `map.update(m: Map[K, V], key: K, f: (V) -> V) -> Map[K, V]` | Rust `entry().and_modify` / OCaml `update` / Elixir `update!` (exception if absent) | Semantic change (abort if key absent) | R-map-08 |
| `map.upsert` | `map.upsert(m: Map[K, V], key: K, init: V, f: (V) -> V) -> Map[K, V]` | Elixir `update/4` (same meaning)/ Rust `entry().and_modify(f).or_insert(v)` / Gleam `upsert`（`fn(Option(v))`） | Keep | — |
| `map.insert` | `map.insert(mut m: Map[K, V], key: K, value: V) -> Unit` | Rust `insert` / JS `set` / Python `d[k] = v` (all in-place) | Remove | R-map-01 |
| `map.delete` | `map.delete(mut m: Map[K, V], key: K) -> Unit` | Rust `remove` / JS `delete` / Go `delete`（in-place） | Remove | R-map-01 |
| `map.clear` | `map.clear(mut m: Map[K, V]) -> Unit` | Rust `clear` / Python `clear` / JS `clear` / Kotlin `clear` | Keep | — (R-L0-04 verb) |
| `map.enumerate` | （Add）`map.enumerate(m: Map[K, V]) -> List[(Int, (K, V))]` | Swift `enumerated` / Rust `enumerate` | Add | R-L0-42 |
| `map.zip` | （Add）`map.zip(m: Map[K, V], ys: List[B]) -> List[((K, V), B)]` | Swift `zip` / Rust `zip` | Add | R-L0-42 |
| `map.zip_with` | （Add）`map.zip_with(m: Map[K, V], ys: List[B], f: ((K, V), B) -> C) -> List[C]` | Kotlin `zip(other, f)` | Add | R-L0-42 |
| `map.filter_map` | （Add）`map.filter_map(m: Map[K, V], f: ((K, V)) -> B?) -> List[B]` | Swift `compactMap` / Rust `filter_map` | Add | R-L0-42 |
| `map.find_map` | （Add）`map.find_map(m: Map[K, V], f: ((K, V)) -> B?) -> B?` | Rust `find_map` | Add | R-L0-42 |
| `map.reduce` | （Add）`map.reduce(m: Map[K, V], f: ((K, V), (K, V)) -> (K, V)) -> (K, V)?` | Swift `reduce` / Rust `reduce` | Add | R-L0-42 |
| `map.scan` | （Add）`map.scan(m: Map[K, V], init: B, f: (B, (K, V)) -> B) -> List[B]` | Rust `scan` / Kotlin `runningFold` | Add | R-L0-42 |
| `map.partition` | （Add）`map.partition(m: Map[K, V], f: ((K, V)) -> Bool) -> (List[(K, V)], List[(K, V)])` | Kotlin `partition` | Add | R-L0-42 |
| `map.group_by` | （Add）`map.group_by(m: Map[K, V], f: ((K, V)) -> B) -> Map[B, List[(K, V)]]` | Kotlin `groupBy` / Swift `Dictionary(grouping:)` | Add | R-L0-42 |
| `map.sort_by` | （Add）`map.sort_by(m: Map[K, V], f: ((K, V)) -> B) -> List[(K, V)]` | Kotlin `sortedBy` | Add | R-L0-42 |
| `map.sort_with` | （Add）`map.sort_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> List[(K, V)]` | Swift `sorted(by:)` | Add | R-L0-42 |
| `map.min_by` | （Add）`map.min_by(m: Map[K, V], f: ((K, V)) -> U) -> (K, V)?` | Kotlin `minByOrNull` | Add | R-L0-42 |
| `map.max_by` | （Add）`map.max_by(m: Map[K, V], f: ((K, V)) -> U) -> (K, V)?` | Kotlin `maxByOrNull` | Add | R-L0-42 |
| `map.min_with` | （Add）`map.min_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> (K, V)?` | Swift `min(by:)` | Add | R-L0-42 |
| `map.max_with` | （Add）`map.max_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> (K, V)?` | Swift `max(by:)` | Add | R-L0-42 |

L3 row count: 25 (no additions). Breakdown: keep 22, remove 3 (`get_or`, `insert`, `delete`).

---

<a id="共通の判定との対応"></a>

## Correspondence with shared decisions

Shared decisions relevant here, aligned with list/set/option/result/string pages:

| Shared decision | Treatment on this page |
|---|---|
| D1 `_or` removal | R-map-02 |
| D2 `map.insert` / `map.delete` removal、`map.clear` retained | R-map-01 |
| D3 Higher-order family and `map.to_list` × | R-map-06、R-map-03 |
| D8 Compatibility window（since=15、removal 16） | R-map-01、R-map-02 |
| D9 R-L0-05 stops these calls | `map.set`・`map.remove`・`map.update`・`map.upsert`・`map.merge`・`map.filter` used as statements (measurements show silent discarding currently) |

<a id="l0-への差し戻し"></a>

## Referrals back to L0

None.
