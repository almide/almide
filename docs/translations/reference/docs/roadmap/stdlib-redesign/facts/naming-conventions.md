<a id="l0-9-言語の標準ライブラリ命名規約事実のみ"></a>

# L0: Standard-library naming conventions in nine languages (facts only)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](naming-conventions.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](naming-conventions.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](naming-conventions.zh-CN.md)

> Non-normative fixed-source historical survey reader. Original pinned source remains unchanged; no autosync, API execution or independent web re-verification. The October 21 date and F marks describe the original author’s checks, not checks newly performed for this translation. Unmarked cells retain their original evidential status. Preserve all APIs, versions, proposal stages, in-place/copy distinctions and error behavior as reported at that source snapshot; they are not current recommendations. Internal tallies/corrections are retained (size 5/9 then strictly 4, get 6 then 5, set add 3 then 4, to_string 5 then 6); the final no-majority list includes sort’s 6-versus-3 split as in the original. The JS trim alias row and deprecation summary also coexist without reconciliation. Quotations are source quotations or translations, not freshly authenticated quotations. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/naming-conventions.md)

Survey date: 2026-10-21. No design decisions included.
Legend: **(F)** = cells verified by WebFetch of official documentation during this survey. Unmarked cells are known facts (stable official API names).
in-place = modifies the caller's value / copy = returns a new value / “—” = absent from the standard library.

<a id="公式ドキュメント-url"></a>

## Official documentation URLs

| Language | URL |
|---|---|
| Rust | https://doc.rust-lang.org/std/ ・ API Guidelines https://rust-lang.github.io/api-guidelines/naming.html (F) ・ str https://doc.rust-lang.org/std/primitive.str.html (F) |
| Go | https://pkg.go.dev/std ・ slices https://pkg.go.dev/slices (F) ・ strings https://pkg.go.dev/strings ・ strconv https://pkg.go.dev/strconv ・ os https://pkg.go.dev/os |
| Python | https://docs.python.org/3/library/stdtypes.html (F) ・ https://docs.python.org/3/library/functions.html |
| TS/JS | https://tc39.es/ecma262/ ・ MDN https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects (Set: F) |
| Gleam | https://gleam-stdlib.hexdocs.pm/gleam/list.html (F) ・ dict / string / set / int / float in the same directory (F) |
| Elixir | https://elixir.hexdocs.pm/naming-conventions.html (F) ・ https://hexdocs.pm/elixir/Enum.html |
| OCaml | Stdlib https://ocaml.org/manual/latest/api/ (List, String: F; version 5.5) ・ Base https://ocaml.org/p/base/latest/doc/Base/List/index.html (F) |
| Kotlin | https://kotlinlang.org/api/core/kotlin-stdlib/ (collections: F) ・ conventions https://kotlinlang.org/docs/coding-conventions.html (F) |
| Swift | https://developer.apple.com/documentation/swift ・ API Design Guidelines https://www.swift.org/documentation/api-design-guidelines/ (F) |

---

<a id="1-大きさsize--length--count"></a>

## 1. Size (size / length / count)

| Language | list | string | map | set |
|---|---|---|---|---|
| Rust | `v.len()` → usize | `s.len()` = **bytes** (F); character count is `s.chars().count()` | `m.len()` | `s.len()` |
| Go | `len(xs)` builtin | `len(s)` = bytes; `utf8.RuneCountInString(s)` = runes | `len(m)` | (No set type; use map) |
| Python | `len(xs)` builtin | `len(s)` = code points | `len(d)` | `len(s)` |
| TS/JS | `a.length` (property) | `s.length` = UTF-16 code units | `m.size` | `s.size` |
| Gleam | `list.length(xs)` → Int (F) | `string.length` = **graphemes** (F); `string.byte_size` = bytes (F) | `dict.size` (F) | `set.size` (F) |
| Elixir | `length(xs)` (Kernel, O(n)) ・ `Enum.count` | `String.length` = graphemes; `byte_size` = bytes | `map_size(m)` ・ `Enum.count` | `MapSet.size` |
| OCaml | `List.length` (F) | `String.length` = bytes | `Hashtbl.length` ・ `Map.cardinal` | `Set.cardinal` |
| Kotlin | `xs.size` (property) ・ `count()` | `s.length` = UTF-16 code units | `m.size` | `s.size` |
| Swift | `xs.count` (property) | `s.count` = **Characters (graphemes)**; `s.utf8.count` = bytes | `d.count` | `s.count` |

Elixir conventions (F) officially distinguish the names: “`size` is constant-time; `length` is linear-time.”
- Prevalent spellings (list): **len family 3 (Rust/Go/Python), length family 4 (JS/Gleam/Elixir/OCaml), size 1 (Kotlin), count 1 (Swift)** → `length` has a plurality, 4/9. For map/set, `size` is 5/9 (JS/Gleam/Kotlin, Elixir MapSet.size/map_size, …) — strictly, size 4 (JS, Gleam, Kotlin, Elixir) + len 3 + count 1 + cardinal/length 1.
- String units: bytes 4 (Rust, Go, OCaml, plus a separate function in Gleam), code points 1 (Python), UTF-16 2 (JS, Kotlin), graphemes 3 (Gleam, Elixir, Swift). **No majority.**

<a id="2-空判定"></a>

## 2. Emptiness checks

| Language | Spelling |
|---|---|
| Rust | `is_empty()` |
| Go | No dedicated function. `len(x) == 0` |
| Python | No dedicated function. `not xs` (truthiness) |
| TS/JS | No dedicated function. `a.length === 0` / `m.size === 0` |
| Gleam | `list.is_empty` / `string.is_empty` / `dict.is_empty` / `set.is_empty` (F) |
| Elixir | `Enum.empty?/1`; strings use `s == ""` |
| OCaml | `List.is_empty` (5.1+, F) ・ `String.is_empty` (5.5+, F) ・ `Map.is_empty`; Base `List.is_empty` |
| Kotlin | `isEmpty()` / `isNotEmpty()` / `isNullOrEmpty()` |
| Swift | `isEmpty` (property) |

- Majority: **is_empty / isEmpty family 5/9** (Rust, Gleam, OCaml, Kotlin, Swift). Three languages have no dedicated function.

<a id="3-所属判定"></a>

## 3. Membership checks

| Language | List contains | String contains substring | Map has key | Set contains element |
|---|---|---|---|---|
| Rust | `v.contains(&x)` | `s.contains(pat)` (F) | `m.contains_key(&k)` | `s.contains(&x)` |
| Go | `slices.Contains(xs, x)` (F) | `strings.Contains(s, sub)` | `_, ok := m[k]` | (`_, ok := m[k]` on map) |
| Python | `x in xs` | `sub in s` | `k in d` | `x in s` |
| TS/JS | `a.includes(x)` | `s.includes(sub)` | `m.has(k)` | `s.has(x)` (F) |
| Gleam | `list.contains(xs, x)` (F) | `string.contains(s, sub)` (F) | `dict.has_key(d, k)` (F) | `set.contains(s, x)` (F) |
| Elixir | `Enum.member?(xs, x)` / `x in xs` | `String.contains?(s, sub)` | `Map.has_key?(m, k)` | `MapSet.member?(s, x)` |
| OCaml | `List.mem x xs` | Stdlib 5.5+: `String.includes ~affix s` (F); absent earlier. `String.contains s c` is for **char** (F). Base: `String.is_substring s ~substring` | `Hashtbl.mem` / `Map.mem` | `Set.mem` |
| Kotlin | `xs.contains(x)` / `x in xs` | `s.contains(sub)` | `m.containsKey(k)` / `k in m` | `s.contains(x)` |
| Swift | `xs.contains(x)` | `s.contains(sub)` | `d[k] != nil` / `d.keys.contains(k)` | `s.contains(x)` |

- Majority (list): **contains 5/9** (Rust, Go, Gleam, Kotlin, Swift).
- Prevalence (map keys): **has_key/has/containsKey/contains_key family** — spellings split. `contains_key`/`containsKey` 2, `has_key`/`has_key?` 2, `has` 1, `mem` 1, operators/subscripts 3. **No majority.**

<a id="4-末尾に-1-要素追加"></a>

## 4. Append one element

| Language | in-place | copy |
|---|---|---|
| Rust | `v.push(x)` (&mut self) | — (No standard function; e.g. `[v, vec![x]].concat()`) |
| Go | `xs = append(xs, x)` (may share underlying array; reassign returned value) | `slices.Concat` (F) |
| Python | `xs.append(x)` → None | `xs + [x]` |
| TS/JS | `a.push(x)` → new length | `[...a, x]` / `a.concat([x])` |
| Gleam | — | `list.append(xs, ys)` (concatenates lists); prepend with `list.prepend(xs, x)` (F) / `[x, ..xs]` |
| Elixir | — | `xs ++ [x]`; prepend `[x \| xs]` |
| OCaml | — (List); `Dynarray.add_last` (5.2+) | `xs @ [x]`; prepend `x :: xs` |
| Kotlin | `MutableList.add(x)` / `xs += x` | `xs + x` (`plus`) |
| Swift | `xs.append(x)` (mutating) | `xs + [x]` |

- In-place spellings: `push` 2 (Rust, JS), `append` 3 (Go, Python, Swift), `add` 1 (Kotlin). **No majority** (append has a plurality).
- Copy spellings: `+` / `++` / `@` operators predominate (Python, JS-spread, Elixir, OCaml, Kotlin, Swift = 6/9). Gleam's `list.append` concatenates two lists, not appends an element.

<a id="5-pop--insert-at--remove-at"></a>

## 5. pop / insert at / remove at

| Language | Remove last | Insert at position | Remove at position |
|---|---|---|---|
| Rust | `v.pop()` → Option<T> | `v.insert(i, x)` (out-of-bounds panic) | `v.remove(i)` → T (panic) ・ `swap_remove` |
| Go | `xs = xs[:len(xs)-1]` | `slices.Insert(xs, i, x...)` → slice (F) | `slices.Delete(xs, i, j)` → slice (F) |
| Python | `xs.pop()` → value (IndexError if empty) | `xs.insert(i, x)` | `xs.pop(i)` / `del xs[i]` |
| TS/JS | `a.pop()` → value or undefined | `a.splice(i, 0, x)`; copy: `a.toSpliced(i, 0, x)` (ES2023) | `a.splice(i, 1)`; copy `toSpliced(i,1)` |
| Gleam | — | — | — |
| Elixir | `List.pop_at(xs, -1)` → {v, rest} | `List.insert_at(xs, i, x)` (copy) | `List.delete_at(xs, i)` (copy) |
| OCaml | — | — | — |
| Kotlin | `removeLast()` (exception if empty) / `removeLastOrNull()` | `add(i, x)` | `removeAt(i)` (F) |
| Swift | `removeLast()` (trap if empty) / `popLast()` → Optional | `insert(x, at: i)` | `remove(at: i)` |

- pop: `pop` 3 (Rust, Python, JS), `removeLast` 2 (Kotlin, Swift). **No majority.**
- insert: `insert` 4 (Rust, Go, Python, Swift) / `insert_at` 1. Plurality: `insert`.
- remove at: `remove` 1, `removeAt` 1, `remove(at:)` 1, `delete_at` 1, `Delete` 1, `pop(i)` 1. **No majority.**

<a id="6-map辞書"></a>

## 6. map (dictionary)

| Language | Insert/overwrite | Delete | Get (Option) | Get (exception) | With default | Update by function / upsert |
|---|---|---|---|---|---|---|
| Rust | `m.insert(k,v)` in-place → Option<old value> | `m.remove(&k)` → Option | `m.get(&k)` → Option<&V> | `m[&k]` panic | `m.get(&k).copied().unwrap_or(d)` | `m.entry(k).and_modify(f).or_insert(v)` ・ `or_insert_with` |
| Go | `m[k] = v` in-place | `delete(m, k)` builtin | `v, ok := m[k]` | — | `m[k]` returns zero value | — |
| Python | `d[k] = v` in-place; copy `d \| {k: v}` (3.9) | `del d[k]` / `d.pop(k[, default])` | `d.get(k)` → None | `d[k]` KeyError | `d.get(k, default)` | `d.setdefault(k, v)`; `collections.defaultdict` |
| TS/JS | `m.set(k, v)` in-place → Map | `m.delete(k)` → bool | `m.get(k)` → undefined | — | `m.get(k) ?? d` | `Map.prototype.getOrInsert` / `getOrInsertComputed` (Stage 3–4 proposal, not settled) |
| Gleam | `dict.insert(d, k, v)` copy (F) | `dict.delete(d, k)` copy (F) | `dict.get(d, k)` → `Result(v, Nil)` (F) | — | `result.unwrap(dict.get(..), d)` | `dict.upsert(d, k, fn(Option(v)) -> v)` (F) |
| Elixir | `Map.put(m,k,v)` copy ・ `Map.put_new` | `Map.delete(m,k)` copy | `Map.fetch(m,k)` → `{:ok,v} \| :error` | `Map.fetch!` raise (F) | `Map.get(m,k,default)` (default nil) (F) | `Map.update(m,k,default,f)` (upsert) ・ `Map.update!` ・ `Map.get_and_update` |
| OCaml | `Hashtbl.replace` in-place (`add` hides the existing value) ・ `Map.add` copy | `Hashtbl.remove` ・ `Map.remove` | `find_opt` | `find` → `Not_found` | — | `Map.update k (option -> option) m` ・ Base `Map.update ~f` |
| Kotlin | `m[k] = v` / `put` in-place; copy `m + (k to v)` | `remove(k)` in-place; copy `m - k` | `m[k]` / `get` → `V?` | `getValue(k)` exception (F) | `getOrDefault(k, d)` ・ `getOrElse(k) { }` (F) | `getOrPut(k) { }` (F) ・ `merge` (JVM) ・ `compute` (JVM) |
| Swift | `d[k] = v` / `updateValue(_:forKey:)` in-place | `d[k] = nil` / `removeValue(forKey:)` → Optional | `d[k]` → Optional | — | `d[k, default: d0]` | `d[k, default: 0] += 1` ・ `merge/merging(_:uniquingKeysWith:)` |

- Insert spellings: `insert` 2 (Rust, Gleam), `put` 2 (Elixir, Kotlin), `set` 1 (JS), `add`/`replace` 1 (OCaml), subscript assignment 3 (Go, Python, Swift). **No majority.**
- Delete: `remove` 4 (Rust, OCaml, Kotlin, plus Swift `removeValue`), `delete` 4 (Go, JS, Gleam, Elixir), Python `del`/`pop`. **Tied, no majority.**
- Option-get spellings: **`get` 6/9** (Rust, Python, JS, Gleam (Result), Kotlin, Swift uses subscripting) — strictly, functions named `get` returning Option/nullable are in Rust, Python, JS, Gleam, Kotlin: 5. Elixir uses `get` for a default and `fetch` for a tagged result.
- Dedicated upsert names: `upsert` 1 (Gleam), `update` 2 (Elixir, OCaml), `entry().or_insert` 1, `getOrPut` 1, `setdefault` 1. **No majority.**

<a id="7-set"></a>

## 7. set

| Language | Add | Delete | Union / intersection / difference |
|---|---|---|---|
| Rust | `insert` in-place → bool | `remove` → bool | `union` / `intersection` / `difference` / `symmetric_difference` (return iterators); `&` `\|` `-` `^` operators create a new set |
| Go | — (no set type) | — | — |
| Python | `add` in-place | `remove` (KeyError if absent) / `discard` | `union` / `intersection` / `difference` / `symmetric_difference` (copy) ・ `\|= &= -=` and `update` / `intersection_update` / `difference_update` (in-place) |
| TS/JS | `add` in-place (F) | `delete` → bool (F) | `union` / `intersection` / `difference` / `symmetricDifference` copy (ES2025, F) |
| Gleam | `set.insert` copy (F) | `set.delete` copy (F) | `union` / `intersection` / `difference` / `symmetric_difference` (F) |
| Elixir | `MapSet.put` copy | `MapSet.delete` copy | `MapSet.union` / `intersection` / `difference` / `symmetric_difference` |
| OCaml | `Set.add` copy | `Set.remove` copy | `Set.union` / `inter` / `diff` |
| Kotlin | `MutableSet.add` in-place; copy `s + x` | `remove`; copy `s - x` | infix `union` / `intersect` / `subtract` (F) (return a new Set) |
| Swift | `insert` (mutating) → (inserted, member) | `remove` → Optional | copy `union` / `intersection` / `subtracting` / `symmetricDifference`; in-place `formUnion` / `formIntersection` / `subtract` / `formSymmetricDifference` |

- Add: `insert` 3 (Rust, Gleam, Swift), `add` 3 (Python, JS, OCaml, Kotlin → 4), `put` 1. → **`add` 4/8, `insert` 3/8**.
- Delete: `remove` 5 (Rust, Python, OCaml, Kotlin, Swift), `delete` 3 (JS, Gleam, Elixir). **Majority: `remove` 5/8**.
- Union: **`union` 8/8**. Intersection: **`intersection` 6/8** (OCaml `inter`, Kotlin `intersect`). Difference: **`difference` 5/8** (OCaml `diff`, Kotlin `subtract`, Swift `subtracting`).

<a id="8-sort--reverse"></a>

## 8. sort / reverse

| Language | sort in-place | sort copy | Key function | Comparison function | reverse |
|---|---|---|---|---|---|
| Rust | `v.sort()` (stable) / `sort_unstable()` | — (absent from std) | `sort_by_key(f)` / `sort_by_cached_key` | `sort_by(\|a,b\| …)` → Ordering | `v.reverse()` in-place; `iter().rev()` |
| Go | `slices.Sort(xs)` (F) | `slices.Sorted(seq)` (1.23, new slice from iter, F) | — | `slices.SortFunc(xs, cmp)` / `SortStableFunc` (F) | `slices.Reverse` in-place (F) |
| Python | `xs.sort(key=, reverse=)` → None (F) | `sorted(xs, key=, reverse=)` | `key=` argument | (cmp retired; `functools.cmp_to_key`) | `xs.reverse()` → None (F); `reversed(xs)` iterator |
| TS/JS | `a.sort(cmp)` in-place (default **string comparison**) | `a.toSorted(cmp)` (ES2023) | — | `(a,b) => number` | `a.reverse()` in-place / `toReversed()` copy |
| Gleam | — | `list.sort(xs, by: compare)` (F) | — | Comparison function required (F) | `list.reverse` (F) |
| Elixir | — | `Enum.sort(xs)` / `Enum.sort(xs, sorter)` | `Enum.sort_by(xs, f)` | `Enum.sort(xs, fn a, b -> a <= b end)` / `:asc`/`:desc` | `Enum.reverse` |
| OCaml | `Array.sort cmp a` | `List.sort cmp xs` / `stable_sort` / `sort_uniq` (F) | — (Base `List.sort ~compare` (F)) | `cmp` required | `List.rev` (F) |
| Kotlin | `sort()` / `sortBy{}` / `sortWith(cmp)` / `sortDescending()` | `sorted()` / `sortedBy{}` / `sortedWith(cmp)` / `sortedDescending()` (F) | `sortBy` / `sortedBy` | `sortWith` / `sortedWith` | `reverse()` in-place / `reversed()` copy / `asReversed()` view (F) |
| Swift | `sort()` / `sort(by:)` (mutating) | `sorted()` / `sorted(by:)` | — | `by: (a,b) -> Bool` (areInIncreasingOrder) | `reverse()` / `reversed()` |

- Languages where “sort” means in-place: Rust, Go, Python, JS, Kotlin, Swift (6). Where “sort” means copy: Gleam, Elixir, OCaml List (3). The copy name `sorted` occurs in Python, Kotlin, Swift, Go (`Sorted`) = 4.
- Key-function spellings: `_by` family 2 (Elixir `sort_by`, Kotlin `sortBy`), Rust `sort_by_key`, Python `key=`. Comparison functions: Rust `sort_by`, Kotlin `sortWith`, Go `SortFunc`, Swift `by:`. **The meaning of `sort_by` reverses across languages** (Rust = comparator; Elixir/Kotlin = key function). No majority.
- reverse: **`reverse` 8/9** (OCaml `rev`).

<a id="9-破壊的--非破壊的の見分け方"></a>

## 9. Distinguishing mutating and nonmutating operations

| Language | Rule | Official wording |
|---|---|---|
| Rust | Distinguished by type: methods taking `&mut self` mutate. No naming rule. Conversion prefixes `as_`/`to_`/`into_` express ownership and cost (F) | API Guidelines C-CONV: `as_` Free, borrowed→borrowed / `to_` Expensive / `into_` Variable, owned→owned (F) |
| Go | No rule. Conventionally in-place functions return nothing (`slices.Sort`), while those producing new values return them (`slices.Sorted`, `Clone`). `append`/`Insert`/`Delete` mix mutation with a return value (F) | — |
| Python | In-place methods return `None`; copy variants are builtins (`sorted`, `reversed`) | “To remind users that it operates by side effect, it does not return the sorted sequence (use `sorted()` to explicitly request a new sorted list instance).” (F) |
| TS/JS | No rule (historical mixture). ES2023 added copy versions with `to` prefix: `toSorted` / `toReversed` / `toSpliced` / `with` | — |
| Gleam | Everything immutable; no distinction needed | — |
| Elixir | Everything immutable. `!` means “exception on failure” (F), not mutation | — |
| OCaml | Distinguished by type (`Array`/`Hashtbl`/`Bytes` mutable; `List`/`Map`/`Set`/`String` immutable). In-place functions return `unit` | — |
| Kotlin | Verb = in-place, past participle = copy (`sort`/`sorted`, `reverse`/`reversed`, `shuffle`/`shuffled`). Also by type (only `MutableList` has in-place variants) | “The name should also suggest if the method is mutating the object or returning a new one. For instance, `sort` is sorting a collection in place, while `sorted` is returning a sorted copy of the collection.” (F) |
| Swift | Verb = mutating, `-ed`/`-ing` = nonmutating; for noun operations, noun = nonmutating and `form` prefix = mutating. Type-level `mutating` keyword too | “Prefer to name the nonmutating variant using the verb's past participle (usually appending \"ed\")” / “name the nonmutating variant using the verb's present participle, by appending \"ing\"” / “use the noun for the nonmutating method and apply the \"form\" prefix to name its mutating counterpart”; examples `x.sort()`/`x.sorted()`, `y.formUnion(z)`/`y.union(z)` (F) |

- Languages distinguishing by name: Kotlin, Swift, Python (function vs method), JS (new APIs only) = 4. By type/return value: Rust, Go, OCaml = 3. No mutable collections in the first place: Gleam, Elixir = 2.

<a id="10-失敗しうるアクセスの命名"></a>

## 10. Naming fallible access

| Language | Returns Option/null | Exception/panic | With default | Convention |
|---|---|---|---|---|
| Rust | `v.get(i)` → Option, `first()`, `last()`, `pop()` | `v[i]` panic, `.unwrap()`, `.expect(msg)` | `.unwrap_or(d)` / `unwrap_or_else` / `unwrap_or_default` | Distinguish by return type; dangerous variants use `_unchecked` (unsafe) (F) |
| Go | `v, ok := m[k]` (comma-ok) | `xs[i]` panic | `m[k]` gives zero value | — |
| Python | `d.get(k)` → None | `xs[i]` IndexError, `d[k]` KeyError | `d.get(k, d0)`, `next(it, d0)` | — |
| TS/JS | `a[i]` / `a.at(i)` → undefined, `m.get(k)`, `find` | Almost none | `?? d` | — |
| Gleam | `list.first`/`last` → `Result(a, Nil)` (F); `dict.get` → Result (F); **`list.at` does not exist** (F) | None (consumer may abort with `let assert`) | `result.unwrap(r, d)` / `option.unwrap` | Returns `Result(_, Nil)`, not Option |
| Elixir | `Map.fetch` → `{:ok, v} \| :error`; `Enum.fetch`; `Enum.at(xs, i)` → nil | `fetch!`, `Map.update!` (F) | `get(m, k, default)` (F); `Enum.at(xs, i, default)` | `!` = raise, `fetch` = tagged, `get` = default (F) |
| OCaml Stdlib | `List.nth_opt`, `find_opt`, `assoc_opt`, `Hashtbl.find_opt` (F) | `List.nth`, `find`, `List.hd` (exceptions) | `Option.value ~default` | **`_opt` suffix = Option variant** (exception variant unmarked) (F) |
| OCaml Base | `List.nth`, `find`, `hd` → option (F) | `nth_exn`, `find_exn`, `hd_exn` (F) | `Option.value ~default` | **`_exn` suffix = exception variant** (Option variant unmarked) (F) — opposite of Stdlib |
| Kotlin | `getOrNull(i)`, `firstOrNull()`, `elementAtOrNull`, `m[k]` → `V?` (F) | `get(i)`/`[i]`, `first()`, `getValue(k)`, `elementAt` | `getOrElse(i) { }`, `getOrDefault`, `elementAtOrElse` (F) | **`OrNull` / `OrElse` / `OrDefault` suffixes** |
| Swift | `first`, `last`, `popLast()`, `d[k]` → Optional, `firstIndex(of:)` | `xs[i]` trap, `removeLast()` trap | `d[k, default:]`, `?? d` | Distinguish by property/return type |

<a id="parse文字列int"></a>

### parse (string→Int)

| Language | Spelling | On failure |
|---|---|---|
| Rust | `s.parse::<i64>()` / `i64::from_str(s)` / `i64::from_str_radix` | `Result<_, ParseIntError>` (F) |
| Go | `strconv.Atoi(s)` / `strconv.ParseInt(s, base, bits)` | `(int, error)`; out of range gives ErrRange and maximum value |
| Python | `int(s)` | Throws `ValueError` |
| TS/JS | `Number.parseInt(s, 10)` / `Number(s)` | Returns `NaN` (`parseInt("12abc")` = 12) |
| Gleam | `int.parse(s)` / `int.base_parse` | `Result(Int, Nil)` (same for float.parse, F) |
| Elixir | `Integer.parse(s)` → `{int, rest} \| :error`; `String.to_integer(s)` | parse returns tuple; to_integer throws |
| OCaml | `int_of_string` (exception `Failure`) / `int_of_string_opt` → option; Base `Int.of_string` (exception) / `Int.of_string_opt` | `_opt` gives Option |
| Kotlin | `s.toInt()` (`NumberFormatException`) / `s.toIntOrNull()` | `OrNull` suffix |
| Swift | `Int(s)` → `Int?` (failable init) | Optional |

- Majority: parse failure returned as a **value (Result/Option/tuple/null) in 7/9** (Rust, Go, Gleam, Elixir parse, OCaml _opt, Kotlin OrNull, Swift); throws only: Python 1; NaN: JS 1. Spellings: `parse` 4 (Rust, Gleam, Elixir, JS parseInt), `of_string` 1, `toInt` 1, constructor 2 (Python, Swift), `Atoi/ParseInt` 1.
- Languages distinguishing “Option/exception variants” by name: OCaml (`_opt`), Base (`_exn`), Kotlin (`OrNull`), Elixir (`!`), Rust (`_unchecked`/`unwrap`) = 5. **Which variant is unmarked reverses across languages** (OCaml Stdlib/Kotlin: unmarked = exception; Base/Rust/Elixir: unmarked = safe).

<a id="11-数値変換損失--検査--飽和--折返し"></a>

## 11. Numeric conversions (lossy / checked / saturating / wrapping)

| Language | Lossy (default) | Checked | Saturating | Wrapping |
|---|---|---|---|---|
| Rust | `x as i32` (integers truncate bits = wrap; float→int **saturates**, NaN→0) | `i32::try_from(x)` → Result; arithmetic `checked_add` → Option | Arithmetic `saturating_add`; conversion `as` (float) | Arithmetic `wrapping_add`; `overflowing_add` → (value, bool) |
| Go | `int32(x)` (integers wrap; out-of-range float→int implementation-dependent) | — | — | The conversion itself |
| Python | `int(f)` (truncate toward zero; arbitrary precision means no overflow) | — | — | — |
| TS/JS | `Math.trunc`, `\| 0` (32-bit wrapping) | Test with `Number.isSafeInteger` | — | `BigInt.asIntN(bits, n)` |
| Gleam | `float.truncate` / `round` → Int (F); `int.to_float` | — | `int.clamp` | — |
| Elixir | `trunc/1`, `round/1` (arbitrary precision) | — | — | — |
| OCaml | `int_of_float` / `Float.to_int` (undefined out of range); `Int32.of_int` (wrapping) | Base: `Int.of_float` (exception), `Int64.to_int` → option, `to_int_exn` | — | Base `to_int_trunc` / Stdlib `Int32.of_int` |
| Kotlin | `Long.toInt()` (truncate bits = wrapping); `Double.toInt()` = **saturating**, NaN→0 (F) | — (not in standard library) | `coerceIn(min, max)` | `Long.toInt()` |
| Swift | `Int(x)` = **traps out of range** | `Int(exactly: x)` → Optional (F) | `Int(clamping: x)` (F) | `Int(truncatingIfNeeded: x)` (F); arithmetic `&+`, `addingReportingOverflow` |

- Swift API Guidelines (F): “In initializers that perform value preserving type conversions, omit the first argument label” — narrowing conversions use labels (such as `truncating`/`saturating`).
- Only Rust and Swift name all four categories separately. Spelling patterns: Rust prefixes `checked_`/`saturating_`/`wrapping_`; Swift argument labels `exactly:`/`clamping:`/`truncatingIfNeeded:`. **No majority** (the other seven have only one or two categories).

<a id="12-変換の向きto_string"></a>

## 12. Conversion direction and to_string

| Language | Direction style | to_string |
|---|---|---|
| Rust | `to_X` / `into_X` / `as_X` (source-type methods) and `X::from(y)` / `TryFrom` / `FromStr` (destination-type associated functions) | `x.to_string()` (Display) |
| Go | `pkg.Func(x)`: `strconv.Itoa`, `FormatInt`, `ParseInt`; type conversion `T(x)` | `strconv.Itoa(n)`, `fmt.Sprint(x)`, `String()` method (Stringer) |
| Python | Constructors `int(x)`, `str(x)`, `list(it)`, `dict.fromkeys` | `str(x)` / `repr(x)` |
| TS/JS | Constructor functions `String(x)`, `Number(x)`, `Array.from(it)`, `Array.of(...)`, `Object.fromEntries` | `x.toString()` / `String(x)` |
| Gleam | Source-module `to_X`: `int.to_string`, `int.to_float`, `list.to_...`; destination-module `from_X`: `dict.from_list`, `set.from_list`, `string.from_utf_codepoints` | `int.to_string` (F) / `float.to_string` (F) / `string.inspect` (F) |
| Elixir | `Integer.to_string`, `String.to_integer`, `Map.new(list)`, `MapSet.new`, `Enum.into` | `to_string/1` (Kernel, String.Chars), `inspect/1` |
| OCaml | Stdlib `int_of_string`/`string_of_int` (`B_of_A`) and `Int.to_string`, `Float.of_string` (`of_`/`to_`); Base standardized on `of_X`/`to_X` | `string_of_int` / `Int.to_string` |
| Kotlin | `x.toX()` (`toInt`, `toList`, `toSet`, `toMap`); construction `listOf`, `mapOf`, `setOf` | `x.toString()` |
| Swift | Destination-type init `Int(x)`, `String(x)`, `Array(seq)`, `Set(xs)` | `String(x)` / `"\(x)"` / `description` |

- Majority: **source-side `to_X` style 5/9** (Rust, Gleam, Elixir, newer OCaml, Kotlin), constructor style 3 (Python, JS, Swift). Also using destination-side `from_X`: Rust, Gleam, JS (`Array.from`).
- to_string spellings: `to_string`/`toString` 5 (Rust, Gleam, Elixir, OCaml `Int.to_string`, Kotlin, JS) → 6/9; `str()`/`String()` constructors 2 (Python, Swift); Go `Itoa`/`String()` 1. **Majority: `to_string` family 6/9**.

<a id="13-述語の命名"></a>

## 13. Predicate naming

| Language | Style | Examples |
|---|---|---|
| Rust | `is_X` / `has_X` / `starts_with` | `is_empty`, `is_some`, `is_ascii_digit` |
| Go | `IsX` / `HasX` / `Contains` | `unicode.IsDigit`, `strings.HasPrefix`, `math.IsNaN` |
| Python | `isX` (**no underscore**) / `startswith` | `str.isdigit`, `str.isupper`, `os.path.isfile`, `math.isnan` |
| TS/JS | `isX` / `has` / `includes` | `Array.isArray`, `Number.isInteger`, `Set.has`, `isSubsetOf` (F) |
| Gleam | `is_X` / `has_key` / `contains` | `list.is_empty`, `dict.has_key`, `set.is_subset` (F), `int.is_even` |
| Elixir | Suffix `?`; guard-capable predicates use `is_X` (do not combine the two) (F) | `Enum.empty?`, `String.contains?`, `Map.has_key?`, `is_list/1`, `Integer.is_even/1` |
| OCaml | Some `is_X`; often `mem` / `exists` / `for_all` | `List.is_empty`, `String.starts_with` (F), `Float.is_nan` |
| Kotlin | `isX` / `hasX` | `isEmpty`, `isNotEmpty`, `isBlank`, `isDigit` |
| Swift | `isX` (read as an assertion about the subject) (F) | `isEmpty`, `isMultiple(of:)`, `contains`, `hasPrefix` |

- Majority: **`is` prefix 8/9** (spelling `is_` in 3: Rust, Gleam, OCaml; `isX`/`IsX` in 5: Go, Python (no separator), JS, Kotlin, Swift). Exception: Elixir (`?` suffix).

<a id="14-引数順データ先頭か関数先頭かとパイプ"></a>

## 14. Argument order (data-first or function-first) and pipes

| Language | Order | Pipe |
|---|---|---|
| Rust | Receiver first (method `xs.iter().map(f)`) | Method chaining substitutes |
| Go | Data first `slices.IndexFunc(xs, f)` | None |
| Python | **Function first** `map(f, xs)`, `filter(f, xs)`; `sorted(xs, key=f)` is data-first → mixed | None |
| TS/JS | Receiver first (methods); `Object.groupBy(items, fn)` is data-first | None (`\|>` is a TC39 Stage 2 proposal) |
| Gleam | **Data first** `list.map(xs, f)` (F, signature verified) | `\|>` inserts into the **first argument** |
| Elixir | **Data first** `Enum.map(xs, f)` | `\|>` inserts into the **first argument** |
| OCaml Stdlib | **Function first** `List.map f xs`, `List.sort cmp xs` (F) | `\|>` goes to the **last argument** (`xs \|> List.map f`) |
| OCaml Base | **Data first + labels** `List.map xs ~f` (F) | Labels also allow `xs \|> List.map ~f` |
| Kotlin | Receiver first (extension `xs.map { }`) | Method chaining |
| Swift | Receiver first (methods) | Method chaining |
| (Reference: F#) | Function first `List.map f xs` | `\|>` goes to the last argument |

- Majority: **data (receiver) first 7/9** (Rust, Go, JS, Gleam, Elixir, Kotlin, Swift; plus Base). Function first: OCaml Stdlib, Python map/filter.

<a id="15-高階関数の派生名_by--_with--_while--_map"></a>

## 15. Derived higher-order function names (_by / _with / _while / _map)

| Language | Key function `_by` | Comparison/combination `_with` | `_while` | `filter_map` | `flat_map` | `find_map` |
|---|---|---|---|---|---|---|
| Rust | `sort_by_key`, `max_by_key`, `min_by_key`, `dedup_by_key` | `sort_by` (comparison), `max_by` (comparison), `zip` + `map` | `take_while`, `skip_while`, `map_while` | `filter_map` | `flat_map` | `find_map` |
| Go | — | `SortFunc`, `IndexFunc`, `ContainsFunc`, `MaxFunc` (**`Func` suffix**) (F) | — | — | — | — |
| Python | `key=` argument (`sorted`, `max`, `min`, `itertools.groupby`) | — | `itertools.takewhile`, `dropwhile` | (Comprehensions) | `itertools.chain.from_iterable` | — |
| TS/JS | — | — | — | — | `flatMap` | — (`find`) |
| Gleam | `list.chunk(xs, by: f)` (F), `group` | `sort(by:)` (comparison), `dict.combine(with:)` (F) | `take_while`, `drop_while` (F) | `filter_map` (F) | `flat_map` (F) | `find_map` (F) |
| Elixir | `sort_by`, `max_by`, `min_by`, `uniq_by`, `group_by`, `chunk_by`, `split_with` (predicate) | — | `take_while`, `drop_while`, `split_while` | (`filter_map` retired; `Enum.flat_map`/`for`) | `flat_map` | `find_value` |
| OCaml | — (Base `List.max_elt ~compare`) | `List.sort_uniq cmp` | `take_while`, `drop_while` (5.3+, F) | `filter_map` (F) | `concat_map` (F) | `find_map` (F) |
| Kotlin | `sortBy`, `sortedBy`, `maxBy`, `minBy`, `distinctBy`, `groupBy`, `associateBy` | `sortWith`, `sortedWith`, `maxWith`, `minWith`, `zip(other){…}` | `takeWhile`, `dropWhile` | `mapNotNull` (F) | `flatMap` | `firstNotNullOf(OrNull)` (F) |
| Swift | — | `sorted(by:)`, `max(by:)` (comparison, argument label) | `prefix(while:)`, `drop(while:)` | `compactMap` | `flatMap` | `first(where:)` + manual work |

- `_by` = key function: Rust (`_by_key`), Elixir, Kotlin, Gleam `chunk(by:)` → Elixir/Kotlin have clear systems. In Rust, Swift, and Gleam `sort(by:)`, `by` means **comparison function**. **No majority meaning for `_by`.**
- `_with` = comparator: only Kotlin is systematic (`sortWith`/`maxWith`). Go uses the `Func` suffix.
- `take_while`/`takeWhile` family: **7/9** (Rust, Python, Gleam, Elixir, OCaml, Kotlin, including Swift `prefix(while:)`).
- `filter_map`: spelling used by 3 (Rust, Gleam, OCaml); other names `mapNotNull`, `compactMap`.
- `flat_map`/`flatMap`: **6/9** (Rust, JS, Gleam, Elixir, Kotlin, Swift); OCaml `concat_map`.
- `find_map`: 3 (Rust, Gleam, OCaml), Elixir `find_value`, Kotlin `firstNotNullOf`.

<a id="16-スライス"></a>

## 16. Slicing

| Language | Extract range | take / drop | Negative index | Half-open range |
|---|---|---|---|---|
| Rust | `&v[a..b]` (panic) / `v.get(a..b)` → Option (F) | `iter().take(n)`, `skip(n)` | No | Half-open `a..b`; closed `a..=b` |
| Go | `xs[a:b]` (panic) | — | No | Half-open |
| Python | `xs[a:b]` (clamps out of range) | — (`itertools.islice`) | Yes | Half-open |
| TS/JS | `a.slice(start, end)`, `s.slice`, `s.substring` | — | Yes for `slice`, no for `substring` | Half-open |
| Gleam | `string.slice(s, at_index:, length:)` = **(start, length)**, negatives allowed (F); list uses `take`/`drop` (F) | `list.take`/`drop` (F); `string.drop_start` (F) | Yes for `string.slice` (F) | — |
| Elixir | `String.slice(s, start, len)` / `String.slice(s, a..b)`; `Enum.slice(xs, a..b)` | `Enum.take`/`drop` (negative means from the end) | Yes | Range is **closed** (`a..b//1`) |
| OCaml | `String.sub s pos len` = **(start, length)**, exception out of range; `Array.sub` | `List.take`/`drop` (5.3+, F) | No | — |
| Kotlin | `subList(a, b)`, `slice(a..b)` (closed), `substring(a, b)` (half-open) | `take`/`drop`/`takeLast`/`dropLast` | No | substring/subList half-open; slice(range) closed |
| Swift | `xs[a..<b]` / `xs[a...b]` (out-of-range trap); String requires `String.Index` | `prefix(n)`, `dropFirst(n)`, `suffix`, `dropLast` | No | Both, by operator |

- Majority: (start, end) half-open **5/9** (Rust, Go, Python, JS, Kotlin substring); (start, length) 2 (Gleam string, OCaml); Elixir has both. take/drop spellings: **`take`/`drop` 6/9** (Gleam, Elixir, OCaml, Kotlin, Rust take + skip, …). Negative indices allowed: Python, JS (slice), Gleam, Elixir = 4.

<a id="17-大文字小文字"></a>

## 17. Letter case

| Language | Spelling |
|---|---|
| Rust | `to_uppercase` / `to_lowercase`; ASCII variant `to_ascii_uppercase` (F) |
| Go | `strings.ToUpper` / `ToLower` |
| Python | `str.upper()` / `lower()`; `casefold` |
| TS/JS | `toUpperCase()` / `toLowerCase()` |
| Gleam | `string.uppercase` / `lowercase` (F) |
| Elixir | `String.upcase` / `downcase` |
| OCaml | `String.uppercase_ascii` / `lowercase_ascii` (Stdlib ASCII only); Base `String.uppercase` |
| Kotlin | `uppercase()` / `lowercase()` (1.5+; `toUpperCase` deprecated) |
| Swift | `uppercased()` / `lowercased()` |

- `to_upper` family (`to_uppercase`, `ToUpper`, `toUpperCase`) 3; `uppercase` family (`uppercase`, `uppercase_ascii`, `uppercased`) 4; `upper` 1; `upcase` 1. **Plurality: `uppercase` 4/9; no majority.**

<a id="18-trim-系"></a>

## 18. trim family

| Language | Both ends | Start | End |
|---|---|---|---|
| Rust | `trim` | `trim_start` (`trim_left` deprecated) (F) | `trim_end` (`trim_right` deprecated) (F) |
| Go | `strings.TrimSpace` (`Trim(s, cutset)`) | `TrimLeft(s, cutset)` / `TrimPrefix` | `TrimRight` / `TrimSuffix` |
| Python | `strip` | `lstrip` | `rstrip` |
| TS/JS | `trim` | `trimStart` (`trimLeft` alias) | `trimEnd` (`trimRight` alias) |
| Gleam | `trim` (F) | `trim_start` (F) | `trim_end` (F) |
| Elixir | `String.trim` | `trim_leading` | `trim_trailing` |
| OCaml | `String.trim` (both ends only) | — (Base `lstrip`) | — (Base `rstrip`; Base both-ends is `strip`) |
| Kotlin | `trim` | `trimStart` | `trimEnd` |
| Swift | `trimmingCharacters(in: .whitespaces)` (Foundation) | — | — |

- Both ends: **`trim` 6/9**. Start/end: **`trim_start`/`trim_end` (`trimStart`/`trimEnd`) 4/9** (Rust, JS, Gleam, Kotlin) has the plurality; `lstrip/rstrip` 2 (Python, Base), `trim_leading/trailing` 1, `TrimLeft/Right` 1. The left/right spelling was deprecated in Rust and JS.

<a id="19-index-of"></a>

## 19. index-of

| Language | Spelling | When absent |
|---|---|---|
| Rust | `s.find(pat)` (F) / `iter().position(f)` | `None` |
| Go | `strings.Index`, `slices.Index`, `IndexFunc` (F) | **-1** |
| Python | `s.find(sub)` / `s.index(sub)`, `xs.index(x)` | find: **-1**; index: ValueError |
| TS/JS | `indexOf`, `findIndex`, `lastIndexOf` | **-1** |
| Gleam | None (neither string nor list has index-of, F) | — |
| Elixir | `Enum.find_index(xs, f)`; strings use `:binary.match` | `nil` / `:nomatch` |
| OCaml | `String.index_opt s c` (char) (F), `String.find_first ~sub s` (5.5+) → option (F); `String.index` throws | `None` / exception |
| Kotlin | `indexOf`, `indexOfFirst`, `lastIndexOf` | **-1** |
| Swift | `firstIndex(of:)`, `firstIndex(where:)`, `lastIndex(of:)` | `nil` (Optional) |

- Returns -1: Go, Python (find), JS, Kotlin = 4. Option/nil: Rust, Elixir, OCaml, Swift = 4. Gleam has no function. **Tied, no majority** (`index`/`indexOf` spelling family: 5).

<a id="20-プロセス--argv--env"></a>

## 20. Process / argv / env

| Language | argv | Read env | Set env | exit | cwd |
|---|---|---|---|---|---|
| Rust | `std::env::args()` (iter) | `std::env::var(k)` → `Result` | `std::env::set_var` (`unsafe` in edition 2024) | `std::process::exit(code)` | `std::env::current_dir()` → Result |
| Go | `os.Args` ([]string) | `os.Getenv(k)` ("" if absent) / `os.LookupEnv` → (v, ok) | `os.Setenv` → error | `os.Exit(code)` | `os.Getwd()` → (string, error) |
| Python | `sys.argv` | `os.environ[k]` (KeyError) / `os.getenv(k, default)` / `os.environ.get` | `os.environ[k] = v` | `sys.exit(code)` | `os.getcwd()` |
| TS/JS | Absent from ECMAScript (Node `process.argv`, Deno `Deno.args`) | Same (`process.env.K`) | Same | Same (`process.exit`) | Same (`process.cwd()`) |
| Gleam | Absent from gleam_stdlib (external `argv` package, `envoy`) | None | None | None | None |
| Elixir | `System.argv()` | `System.get_env(k)` → nil / `System.fetch_env` → {:ok,v}\|:error / `fetch_env!` | `System.put_env` | `System.halt(code)` / `System.stop` | `File.cwd()` → {:ok, p} / `File.cwd!` |
| OCaml | `Sys.argv` (array) | `Sys.getenv` (Not_found) / `Sys.getenv_opt` | `Unix.putenv` (unix library) | `exit code` (Stdlib) | `Sys.getcwd ()` |
| Kotlin | `main(args: Array<String>)` | JVM `System.getenv(k)` → `String?` | (None in JVM standard library) | `kotlin.system.exitProcess(code)` | JVM `System.getProperty("user.dir")` |
| Swift | `CommandLine.arguments` (stdlib) | Foundation `ProcessInfo.processInfo.environment[k]` | `setenv` (libc) | `exit(code)` (libc / Foundation) | `FileManager.default.currentDirectoryPath` |

- Placement: in **process-specific modules such as `env`/`os`/`sys`/`System`** in 7/9 (Rust `env`+`process`, Go `os`, Python `sys`+`os`, Elixir `System`, OCaml `Sys`, Kotlin `kotlin.system`, Swift `CommandLine`). Absent from language standard: JS (ECMAScript), Gleam.
- Spellings: argv is split among `args`/`Args`/`argv`/`arguments` (args 3: Rust, Go, Kotlin / argv 3: Python, Elixir, OCaml / arguments 1). Env retrieval uses `getenv`/`get_env`/`Getenv` in 5 (Go, Python, Elixir, OCaml, Kotlin), Rust `var`. Exit: **`exit` 6/9** (Rust, Go, Python, OCaml, Kotlin `exitProcess`, Swift). Cwd: `getcwd`/`Getwd` 4 (Go, Python, OCaml, plus Rust `current_dir`).

---

<a id="要約多数派が存在するもの--しないもの"></a>

## Summary (where majorities exist and where they do not)

Majorities: emptiness `is_empty` (5/9), list membership `contains` (5/9), set deletion `remove` (5/8), set union `union` (8/8) / intersection `intersection` (6/8) / difference `difference` (5/8), `reverse` (8/9), to_string family (6/9), source-side conversion `to_X` (5/9), predicate `is` prefix (8/9), data-first argument order (7/9), `flat_map` (6/9), `take_while` family (7/9), `trim` (6/9), parse failure returned as a value (7/9), exit (6/9).

No majority: length spelling (length 4 / len 3 / size / count) and string-length units; in-place append (append/push/add); pop / remove-at; map insertion (insert/put/set/subscript) and deletion (remove 4 vs delete 4); upsert; whether sort defaults to in-place or copy (6 vs 3); `_by` meaning (key vs comparison function); uppercase (uppercase 4 / to_upper 3); index-of (-1 in 4 / Option in 4); four numeric-conversion categories (only Rust and Swift cover all); which Option/exception variant is unmarked (OCaml Stdlib/Kotlin: exception; Base/Rust/Elixir: safe).
