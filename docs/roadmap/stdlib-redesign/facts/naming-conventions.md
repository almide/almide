# L0: 9 言語の標準ライブラリ命名規約（事実のみ）

調査日: 2026-10-21。設計判断は含めない。
凡例: **(F)** = 今回公式ドキュメントを WebFetch して確認したセル。無印は既知の事実（公式 API 名として安定しているもの）。
in-place = 呼び出し元の値を書き換える / copy = 新しい値を返す / 「—」= 標準に無い。

## 公式ドキュメント URL

| 言語 | URL |
|---|---|
| Rust | https://doc.rust-lang.org/std/ ・ API Guidelines https://rust-lang.github.io/api-guidelines/naming.html (F) ・ str https://doc.rust-lang.org/std/primitive.str.html (F) |
| Go | https://pkg.go.dev/std ・ slices https://pkg.go.dev/slices (F) ・ strings https://pkg.go.dev/strings ・ strconv https://pkg.go.dev/strconv ・ os https://pkg.go.dev/os |
| Python | https://docs.python.org/3/library/stdtypes.html (F) ・ https://docs.python.org/3/library/functions.html |
| TS/JS | https://tc39.es/ecma262/ ・ MDN https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects (Set は F) |
| Gleam | https://gleam-stdlib.hexdocs.pm/gleam/list.html (F) ・ dict / string / set / int / float 同ディレクトリ (F) |
| Elixir | https://elixir.hexdocs.pm/naming-conventions.html (F) ・ https://hexdocs.pm/elixir/Enum.html |
| OCaml | Stdlib https://ocaml.org/manual/latest/api/ (List, String は F; 5.5 版) ・ Base https://ocaml.org/p/base/latest/doc/Base/List/index.html (F) |
| Kotlin | https://kotlinlang.org/api/core/kotlin-stdlib/ (collections は F) ・ 規約 https://kotlinlang.org/docs/coding-conventions.html (F) |
| Swift | https://developer.apple.com/documentation/swift ・ API Design Guidelines https://www.swift.org/documentation/api-design-guidelines/ (F) |

---

## 1. 大きさ（size / length / count）

| 言語 | list | string | map | set |
|---|---|---|---|---|
| Rust | `v.len()` → usize | `s.len()` = **バイト数** (F); 文字数は `s.chars().count()` | `m.len()` | `s.len()` |
| Go | `len(xs)` 組み込み | `len(s)` = バイト; `utf8.RuneCountInString(s)` = rune 数 | `len(m)` | (set 型なし。map で代用) |
| Python | `len(xs)` 組み込み | `len(s)` = コードポイント数 | `len(d)` | `len(s)` |
| TS/JS | `a.length` (プロパティ) | `s.length` = UTF-16 コード単位数 | `m.size` | `s.size` |
| Gleam | `list.length(xs)` → Int (F) | `string.length` = **書記素(grapheme)数** (F); `string.byte_size` = バイト (F) | `dict.size` (F) | `set.size` (F) |
| Elixir | `length(xs)` (Kernel, O(n)) ・ `Enum.count` | `String.length` = 書記素数; `byte_size` = バイト | `map_size(m)` ・ `Enum.count` | `MapSet.size` |
| OCaml | `List.length` (F) | `String.length` = バイト | `Hashtbl.length` ・ `Map.cardinal` | `Set.cardinal` |
| Kotlin | `xs.size` (プロパティ) ・ `count()` | `s.length` = UTF-16 コード単位 | `m.size` | `s.size` |
| Swift | `xs.count` (プロパティ) | `s.count` = **Character(書記素)数**; `s.utf8.count` = バイト | `d.count` | `s.count` |

Elixir 規約 (F): 「`size` は定数時間、`length` は線形時間」という命名区別を公式に持つ。
- 多数派の綴り（list）: **len 系 3 (Rust/Go/Python)・length 系 4 (JS/Gleam/Elixir/OCaml)・size 1 (Kotlin)・count 1 (Swift)** → `length` が相対多数 4/9。map/set では `size` が 5/9 (JS/Gleam/Kotlin, Elixir MapSet.size/map_size, …) — 厳密には size 4（JS, Gleam, Kotlin, Elixir）+ len 3 + count 1 + cardinal/length 1。
- string 単位: バイト 4 (Rust, Go, OCaml, + Gleam は別関数)、コードポイント 1 (Python)、UTF-16 2 (JS, Kotlin)、書記素 3 (Gleam, Elixir, Swift)。**多数派なし**。

## 2. 空判定

| 言語 | 綴り |
|---|---|
| Rust | `is_empty()` |
| Go | 専用関数なし。`len(x) == 0` |
| Python | 専用関数なし。`not xs` (truthiness) |
| TS/JS | 専用関数なし。`a.length === 0` / `m.size === 0` |
| Gleam | `list.is_empty` / `string.is_empty` / `dict.is_empty` / `set.is_empty` (F) |
| Elixir | `Enum.empty?/1`; 文字列は `s == ""` |
| OCaml | `List.is_empty` (5.1〜, F) ・ `String.is_empty` (5.5〜, F) ・ `Map.is_empty`; Base `List.is_empty` |
| Kotlin | `isEmpty()` / `isNotEmpty()` / `isNullOrEmpty()` |
| Swift | `isEmpty` (プロパティ) |

- 多数派: **is_empty / isEmpty 系 5/9**（Rust, Gleam, OCaml, Kotlin, Swift）。3 言語は専用関数なし。

## 3. 所属判定

| 言語 | list に含む | 文字列が部分文字列を含む | map がキーを持つ | set が要素を持つ |
|---|---|---|---|---|
| Rust | `v.contains(&x)` | `s.contains(pat)` (F) | `m.contains_key(&k)` | `s.contains(&x)` |
| Go | `slices.Contains(xs, x)` (F) | `strings.Contains(s, sub)` | `_, ok := m[k]` | (map で `_, ok := m[k]`) |
| Python | `x in xs` | `sub in s` | `k in d` | `x in s` |
| TS/JS | `a.includes(x)` | `s.includes(sub)` | `m.has(k)` | `s.has(x)` (F) |
| Gleam | `list.contains(xs, x)` (F) | `string.contains(s, sub)` (F) | `dict.has_key(d, k)` (F) | `set.contains(s, x)` (F) |
| Elixir | `Enum.member?(xs, x)` / `x in xs` | `String.contains?(s, sub)` | `Map.has_key?(m, k)` | `MapSet.member?(s, x)` |
| OCaml | `List.mem x xs` | Stdlib 5.5〜 `String.includes ~affix s` (F); それ以前は無し。`String.contains s c` は **char** (F)。Base: `String.is_substring s ~substring` | `Hashtbl.mem` / `Map.mem` | `Set.mem` |
| Kotlin | `xs.contains(x)` / `x in xs` | `s.contains(sub)` | `m.containsKey(k)` / `k in m` | `s.contains(x)` |
| Swift | `xs.contains(x)` | `s.contains(sub)` | `d[k] != nil` / `d.keys.contains(k)` | `s.contains(x)` |

- 多数派（list）: **contains 5/9**（Rust, Go, Gleam, Kotlin, Swift）。
- 多数派（map キー）: **has_key/has/containsKey/contains_key 系**——綴りは割れる。`contains_key`/`containsKey` 2, `has_key`/`has_key?` 2, `has` 1, `mem` 1, 演算子/添字 3。**多数派なし**。

## 4. 末尾に 1 要素追加

| 言語 | in-place | copy |
|---|---|---|
| Rust | `v.push(x)` (&mut self) | — (標準関数なし。`[v, vec![x]].concat()` 等) |
| Go | `xs = append(xs, x)`（下層配列を共有しうる、戻り値を再代入） | `slices.Concat` (F) |
| Python | `xs.append(x)` → None | `xs + [x]` |
| TS/JS | `a.push(x)` → 新 length | `[...a, x]` / `a.concat([x])` |
| Gleam | — | `list.append(xs, ys)`（リスト連結）; 先頭は `list.prepend(xs, x)` (F) / `[x, ..xs]` |
| Elixir | — | `xs ++ [x]` ; 先頭 `[x \| xs]` |
| OCaml | — (List)；`Dynarray.add_last` (5.2〜) | `xs @ [x]` ; 先頭 `x :: xs` |
| Kotlin | `MutableList.add(x)` / `xs += x` | `xs + x` (`plus`) |
| Swift | `xs.append(x)` (mutating) | `xs + [x]` |

- in-place の綴り: `push` 2 (Rust, JS), `append` 3 (Go, Python, Swift), `add` 1 (Kotlin)。**多数派なし**（append が相対多数）。
- copy の綴り: `+` / `++` / `@` 演算子が多数 (Python, JS-spread, Elixir, OCaml, Kotlin, Swift = 6/9)。Gleam の `list.append` は「2 リストの連結」であり要素追加ではない。

## 5. pop / insert at / remove at

| 言語 | 末尾削除 | 位置へ挿入 | 位置を削除 |
|---|---|---|---|
| Rust | `v.pop()` → Option<T> | `v.insert(i, x)` (範囲外 panic) | `v.remove(i)` → T (panic) ・ `swap_remove` |
| Go | `xs = xs[:len(xs)-1]` | `slices.Insert(xs, i, x...)` → slice (F) | `slices.Delete(xs, i, j)` → slice (F) |
| Python | `xs.pop()` → 値 (空で IndexError) | `xs.insert(i, x)` | `xs.pop(i)` / `del xs[i]` |
| TS/JS | `a.pop()` → 値 or undefined | `a.splice(i, 0, x)`; copy は `a.toSpliced(i, 0, x)` (ES2023) | `a.splice(i, 1)`; copy `toSpliced(i,1)` |
| Gleam | — | — | — |
| Elixir | `List.pop_at(xs, -1)` → {v, rest} | `List.insert_at(xs, i, x)` (copy) | `List.delete_at(xs, i)` (copy) |
| OCaml | — | — | — |
| Kotlin | `removeLast()` (空で例外) / `removeLastOrNull()` | `add(i, x)` | `removeAt(i)` (F) |
| Swift | `removeLast()` (空で trap) / `popLast()` → Optional | `insert(x, at: i)` | `remove(at: i)` |

- pop: `pop` 3 (Rust, Python, JS)、`removeLast` 2 (Kotlin, Swift)。**多数派なし**。
- insert: `insert` 4 (Rust, Go, Python, Swift) / `insert_at` 1。相対多数 `insert`。
- remove at: `remove` 1, `removeAt` 1, `remove(at:)` 1, `delete_at` 1, `Delete` 1, `pop(i)` 1。**多数派なし**。

## 6. map（辞書）

| 言語 | 挿入/上書き | 削除 | 取得 (Option) | 取得 (例外) | 既定値つき | 関数で更新 / upsert |
|---|---|---|---|---|---|---|
| Rust | `m.insert(k,v)` in-place → Option<旧値> | `m.remove(&k)` → Option | `m.get(&k)` → Option<&V> | `m[&k]` panic | `m.get(&k).copied().unwrap_or(d)` | `m.entry(k).and_modify(f).or_insert(v)` ・ `or_insert_with` |
| Go | `m[k] = v` in-place | `delete(m, k)` 組み込み | `v, ok := m[k]` | — | `m[k]` は零値を返す | — |
| Python | `d[k] = v` in-place; copy `d \| {k: v}` (3.9) | `del d[k]` / `d.pop(k[, default])` | `d.get(k)` → None | `d[k]` KeyError | `d.get(k, default)` | `d.setdefault(k, v)`; `collections.defaultdict` |
| TS/JS | `m.set(k, v)` in-place → Map | `m.delete(k)` → bool | `m.get(k)` → undefined | — | `m.get(k) ?? d` | `Map.prototype.getOrInsert` / `getOrInsertComputed` (Stage 3〜4 提案, 未確定) |
| Gleam | `dict.insert(d, k, v)` copy (F) | `dict.delete(d, k)` copy (F) | `dict.get(d, k)` → `Result(v, Nil)` (F) | — | `result.unwrap(dict.get(..), d)` | `dict.upsert(d, k, fn(Option(v)) -> v)` (F) |
| Elixir | `Map.put(m,k,v)` copy ・ `Map.put_new` | `Map.delete(m,k)` copy | `Map.fetch(m,k)` → `{:ok,v} \| :error` | `Map.fetch!` raise (F) | `Map.get(m,k,default)`（既定 nil）(F) | `Map.update(m,k,default,f)` (upsert) ・ `Map.update!` ・ `Map.get_and_update` |
| OCaml | `Hashtbl.replace` in-place (`add` は既存を隠す) ・ `Map.add` copy | `Hashtbl.remove` ・ `Map.remove` | `find_opt` | `find` → `Not_found` | — | `Map.update k (option -> option) m` ・ Base `Map.update ~f` |
| Kotlin | `m[k] = v` / `put` in-place; copy `m + (k to v)` | `remove(k)` in-place; copy `m - k` | `m[k]` / `get` → `V?` | `getValue(k)` 例外 (F) | `getOrDefault(k, d)` ・ `getOrElse(k) { }` (F) | `getOrPut(k) { }` (F) ・ `merge` (JVM) ・ `compute` (JVM) |
| Swift | `d[k] = v` / `updateValue(_:forKey:)` in-place | `d[k] = nil` / `removeValue(forKey:)` → Optional | `d[k]` → Optional | — | `d[k, default: d0]` | `d[k, default: 0] += 1` ・ `merge/merging(_:uniquingKeysWith:)` |

- 挿入の綴り: `insert` 2 (Rust, Gleam), `put` 2 (Elixir, Kotlin), `set` 1 (JS), `add`/`replace` 1 (OCaml), 添字代入 3 (Go, Python, Swift)。**多数派なし**。
- 削除: `remove` 4 (Rust, OCaml, Kotlin, + Swift `removeValue`)、`delete` 4 (Go, JS, Gleam, Elixir)、Python `del`/`pop`。**拮抗、多数派なし**。
- Option 取得の綴り: **`get` 6/9**（Rust, Python, JS, Gleam(Result), Kotlin, Swift は添字）——厳密に関数名 `get` が Option/nullable を返すのは Rust, Python, JS, Gleam, Kotlin の 5。Elixir は `get`=既定値、`fetch`=タグ付き。
- upsert 専用名: `upsert` 1 (Gleam), `update` 2 (Elixir, OCaml), `entry().or_insert` 1, `getOrPut` 1, `setdefault` 1。**多数派なし**。

## 7. set

| 言語 | 追加 | 削除 | 和 / 積 / 差 |
|---|---|---|---|
| Rust | `insert` in-place → bool | `remove` → bool | `union` / `intersection` / `difference` / `symmetric_difference`（イテレータを返す）; `&` `\|` `-` `^` 演算子で新 set |
| Go | — (set 型なし) | — | — |
| Python | `add` in-place | `remove` (無ければ KeyError) / `discard` | `union` / `intersection` / `difference` / `symmetric_difference` (copy) ・ `\|= &= -=` と `update` / `intersection_update` / `difference_update` (in-place) |
| TS/JS | `add` in-place (F) | `delete` → bool (F) | `union` / `intersection` / `difference` / `symmetricDifference` copy (ES2025, F) |
| Gleam | `set.insert` copy (F) | `set.delete` copy (F) | `union` / `intersection` / `difference` / `symmetric_difference` (F) |
| Elixir | `MapSet.put` copy | `MapSet.delete` copy | `MapSet.union` / `intersection` / `difference` / `symmetric_difference` |
| OCaml | `Set.add` copy | `Set.remove` copy | `Set.union` / `inter` / `diff` |
| Kotlin | `MutableSet.add` in-place; copy `s + x` | `remove`; copy `s - x` | infix `union` / `intersect` / `subtract` (F)（新 Set を返す） |
| Swift | `insert` (mutating) → (inserted, member) | `remove` → Optional | copy `union` / `intersection` / `subtracting` / `symmetricDifference`; in-place `formUnion` / `formIntersection` / `subtract` / `formSymmetricDifference` |

- 追加: `insert` 3 (Rust, Gleam, Swift), `add` 3 (Python, JS, OCaml, Kotlin → 4), `put` 1。→ **`add` 4/8, `insert` 3/8**。
- 削除: `remove` 5 (Rust, Python, OCaml, Kotlin, Swift)、`delete` 3 (JS, Gleam, Elixir)。**多数派 `remove` 5/8**。
- 和: **`union` 8/8**。積: **`intersection` 6/8**（OCaml `inter`, Kotlin `intersect`）。差: **`difference` 5/8**（OCaml `diff`, Kotlin `subtract`, Swift `subtracting`）。

## 8. sort / reverse

| 言語 | sort in-place | sort copy | キー関数 | 比較関数 | reverse |
|---|---|---|---|---|---|
| Rust | `v.sort()` (stable) / `sort_unstable()` | — (std に無し) | `sort_by_key(f)` / `sort_by_cached_key` | `sort_by(\|a,b\| …)` → Ordering | `v.reverse()` in-place; `iter().rev()` |
| Go | `slices.Sort(xs)` (F) | `slices.Sorted(seq)` (1.23, iter から新 slice, F) | — | `slices.SortFunc(xs, cmp)` / `SortStableFunc` (F) | `slices.Reverse` in-place (F) |
| Python | `xs.sort(key=, reverse=)` → None (F) | `sorted(xs, key=, reverse=)` | `key=` 引数 | (cmp 廃止; `functools.cmp_to_key`) | `xs.reverse()` → None (F); `reversed(xs)` イテレータ |
| TS/JS | `a.sort(cmp)` in-place（既定は**文字列比較**） | `a.toSorted(cmp)` (ES2023) | — | `(a,b) => number` | `a.reverse()` in-place / `toReversed()` copy |
| Gleam | — | `list.sort(xs, by: compare)` (F) | — | 比較関数必須 (F) | `list.reverse` (F) |
| Elixir | — | `Enum.sort(xs)` / `Enum.sort(xs, sorter)` | `Enum.sort_by(xs, f)` | `Enum.sort(xs, fn a, b -> a <= b end)` / `:asc`/`:desc` | `Enum.reverse` |
| OCaml | `Array.sort cmp a` | `List.sort cmp xs` / `stable_sort` / `sort_uniq` (F) | — (Base `List.sort ~compare` (F)) | `cmp` 必須 | `List.rev` (F) |
| Kotlin | `sort()` / `sortBy{}` / `sortWith(cmp)` / `sortDescending()` | `sorted()` / `sortedBy{}` / `sortedWith(cmp)` / `sortedDescending()` (F) | `sortBy` / `sortedBy` | `sortWith` / `sortedWith` | `reverse()` in-place / `reversed()` copy / `asReversed()` view (F) |
| Swift | `sort()` / `sort(by:)` (mutating) | `sorted()` / `sorted(by:)` | — | `by: (a,b) -> Bool` (areInIncreasingOrder) | `reverse()` / `reversed()` |

- 「sort」= in-place な言語: Rust, Go, Python, JS, Kotlin, Swift (6)。「sort」= copy な言語: Gleam, Elixir, OCaml List (3)。copy 版の名前 `sorted` は Python, Kotlin, Swift, Go(`Sorted`) = 4。
- キー関数の綴り: `_by` 系 2 (Elixir `sort_by`, Kotlin `sortBy`)、Rust `sort_by_key`、Python `key=`。比較関数: Rust `sort_by`、Kotlin `sortWith`、Go `SortFunc`、Swift `by:`。**`sort_by` の意味が言語で逆転**（Rust=比較関数、Elixir/Kotlin=キー関数）。多数派なし。
- reverse: **`reverse` 8/9**（OCaml `rev`）。

## 9. 破壊的 / 非破壊的の見分け方

| 言語 | 規則 | 公式文言 |
|---|---|---|
| Rust | 型で区別：`&mut self` を取るメソッドが書き換える。名前規則なし。変換接頭辞 `as_`/`to_`/`into_` は所有権・コストを表す (F) | API Guidelines C-CONV: `as_` Free, borrowed→borrowed / `to_` Expensive / `into_` Variable, owned→owned (F) |
| Go | 規則なし。慣例として in-place 関数は戻り値なし（`slices.Sort`）、新値を返すものは戻り値（`slices.Sorted`, `Clone`）。`append`/`Insert`/`Delete` は書き換えつつ戻り値も返す混合型 (F) | — |
| Python | in-place メソッドは `None` を返す、copy は組み込み関数（`sorted`, `reversed`） | 「To remind users that it operates by side effect, it does not return the sorted sequence (use `sorted()` to explicitly request a new sorted list instance).」(F) |
| TS/JS | 規則なし（歴史的に混在）。ES2023 で copy 版を `to` 接頭辞で追加: `toSorted` / `toReversed` / `toSpliced` / `with` | — |
| Gleam | 全て不変。区別不要 | — |
| Elixir | 全て不変。`!` は「失敗時に例外」(F) であり破壊の印ではない | — |
| OCaml | 型で区別（`Array`/`Hashtbl`/`Bytes` は可変、`List`/`Map`/`Set`/`String` は不変）。in-place 関数は `unit` を返す | — |
| Kotlin | 動詞 = in-place、過去分詞 = copy（`sort`/`sorted`, `reverse`/`reversed`, `shuffle`/`shuffled`）。型でも区別（`MutableList` のみ in-place 版を持つ） | 「The name should also suggest if the method is mutating the object or returning a new one. For instance, `sort` is sorting a collection in place, while `sorted` is returning a sorted copy of the collection.」(F) |
| Swift | 動詞 = mutating、`-ed`/`-ing` = 非破壊；名詞操作は名詞 = 非破壊、`form` 接頭辞 = mutating。型でも `mutating` キーワード | 「Prefer to name the nonmutating variant using the verb's past participle (usually appending "ed")」/「name the nonmutating variant using the verb's present participle, by appending "ing"」/「use the noun for the nonmutating method and apply the "form" prefix to name its mutating counterpart」 例 `x.sort()`/`x.sorted()`, `y.formUnion(z)`/`y.union(z)` (F) |

- 名前で区別する言語: Kotlin, Swift, Python（関数/メソッドの別）, JS(新 API のみ) = 4。型/戻り値で区別: Rust, Go, OCaml = 3。そもそも可変コレクションが無い: Gleam, Elixir = 2。

## 10. 失敗しうるアクセスの命名

| 言語 | Option/null を返す | 例外/panic | 既定値つき | 規約 |
|---|---|---|---|---|
| Rust | `v.get(i)` → Option, `first()`, `last()`, `pop()` | `v[i]` panic, `.unwrap()`, `.expect(msg)` | `.unwrap_or(d)` / `unwrap_or_else` / `unwrap_or_default` | 戻り型で区別；危険側は `_unchecked`（unsafe）(F) |
| Go | `v, ok := m[k]` (comma-ok) | `xs[i]` panic | `m[k]` は零値 | — |
| Python | `d.get(k)` → None | `xs[i]` IndexError, `d[k]` KeyError | `d.get(k, d0)`, `next(it, d0)` | — |
| TS/JS | `a[i]` / `a.at(i)` → undefined, `m.get(k)`, `find` | ほぼ無し | `?? d` | — |
| Gleam | `list.first`/`last` → `Result(a, Nil)` (F); `dict.get` → Result (F); **`list.at` は存在しない** (F) | 無し（`let assert` で利用側が落とす） | `result.unwrap(r, d)` / `option.unwrap` | Option ではなく `Result(_, Nil)` を返す |
| Elixir | `Map.fetch` → `{:ok, v} \| :error`; `Enum.fetch`; `Enum.at(xs, i)` → nil | `fetch!`, `Map.update!` (F) | `get(m, k, default)` (F); `Enum.at(xs, i, default)` | `!` = raise, `fetch` = タグ, `get` = 既定値 (F) |
| OCaml Stdlib | `List.nth_opt`, `find_opt`, `assoc_opt`, `Hashtbl.find_opt` (F) | `List.nth`, `find`, `List.hd` (例外) | `Option.value ~default` | **`_opt` 接尾辞 = Option 版**（例外版が無印）(F) |
| OCaml Base | `List.nth`, `find`, `hd` → option (F) | `nth_exn`, `find_exn`, `hd_exn` (F) | `Option.value ~default` | **`_exn` 接尾辞 = 例外版**（Option 版が無印）(F) — Stdlib と逆 |
| Kotlin | `getOrNull(i)`, `firstOrNull()`, `elementAtOrNull`, `m[k]` → `V?` (F) | `get(i)`/`[i]`, `first()`, `getValue(k)`, `elementAt` | `getOrElse(i) { }`, `getOrDefault`, `elementAtOrElse` (F) | **`OrNull` / `OrElse` / `OrDefault` 接尾辞** |
| Swift | `first`, `last`, `popLast()`, `d[k]` → Optional, `firstIndex(of:)` | `xs[i]` trap, `removeLast()` trap | `d[k, default:]`, `?? d` | プロパティ/戻り型で区別 |

### parse（文字列→Int）

| 言語 | 綴り | 失敗時 |
|---|---|---|
| Rust | `s.parse::<i64>()` / `i64::from_str(s)` / `i64::from_str_radix` | `Result<_, ParseIntError>` (F) |
| Go | `strconv.Atoi(s)` / `strconv.ParseInt(s, base, bits)` | `(int, error)`；範囲外は ErrRange と最大値 |
| Python | `int(s)` | `ValueError` を投げる |
| TS/JS | `Number.parseInt(s, 10)` / `Number(s)` | `NaN` を返す（`parseInt("12abc")` = 12） |
| Gleam | `int.parse(s)` / `int.base_parse` | `Result(Int, Nil)`（float.parse も同, F） |
| Elixir | `Integer.parse(s)` → `{int, rest} \| :error` ; `String.to_integer(s)` | parse はタプル、to_integer は例外 |
| OCaml | `int_of_string` (例外 `Failure`) / `int_of_string_opt` → option; Base `Int.of_string` (例外) / `Int.of_string_opt` | `_opt` で Option |
| Kotlin | `s.toInt()` (`NumberFormatException`) / `s.toIntOrNull()` | `OrNull` 接尾辞 |
| Swift | `Int(s)` → `Int?`（failable init） | Optional |

- 多数派: parse 失敗を **値（Result/Option/タプル/null）で返す 7/9**（Rust, Go, Gleam, Elixir parse, OCaml _opt, Kotlin OrNull, Swift）；投げるのみ Python 1、NaN の JS 1。綴りは `parse` 4 (Rust, Gleam, Elixir, JS parseInt), `of_string` 1, `toInt` 1, 構築子 2 (Python, Swift), `Atoi/ParseInt` 1。
- 「Option 版/例外版」を名前で区別する言語: OCaml(`_opt`), Base(`_exn`), Kotlin(`OrNull`), Elixir(`!`), Rust(`_unchecked`/`unwrap`) = 5。**ただし無印がどちらかは言語で逆**（OCaml Stdlib/Kotlin: 無印=例外、Base/Rust/Elixir: 無印=安全）。

## 11. 数値変換（損失 / 検査 / 飽和 / 折返し）

| 言語 | 損失あり（既定） | 検査つき | 飽和 | 折返し |
|---|---|---|---|---|
| Rust | `x as i32`（整数は bit 切り捨て=折返し、float→int は **飽和**・NaN→0） | `i32::try_from(x)` → Result; 演算 `checked_add` → Option | 演算 `saturating_add`; 型変換は `as`（float）| 演算 `wrapping_add`; `overflowing_add` → (値, bool) |
| Go | `int32(x)`（整数は折返し、float→int 範囲外は実装依存） | — | — | 変換そのもの |
| Python | `int(f)`（0 方向切り捨て、多倍長なので溢れない） | — | — | — |
| TS/JS | `Math.trunc`, `\| 0`（32bit 折返し） | `Number.isSafeInteger` で判定 | — | `BigInt.asIntN(bits, n)` |
| Gleam | `float.truncate` / `round` → Int (F); `int.to_float` | — | `int.clamp` | — |
| Elixir | `trunc/1`, `round/1`（多倍長） | — | — | — |
| OCaml | `int_of_float` / `Float.to_int`（範囲外は未定義）; `Int32.of_int`（折返し） | Base: `Int.of_float` (例外), `Int64.to_int` → option, `to_int_exn` | — | Base `to_int_trunc` / Stdlib `Int32.of_int` |
| Kotlin | `Long.toInt()`（bit 切り捨て=折返し）; `Double.toInt()` = **飽和**・NaN→0 (F) | — (標準なし) | `coerceIn(min, max)` | `Long.toInt()` |
| Swift | `Int(x)` = **範囲外は trap** | `Int(exactly: x)` → Optional (F) | `Int(clamping: x)` (F) | `Int(truncatingIfNeeded: x)` (F); 演算 `&+`, `addingReportingOverflow` |

- Swift API Guidelines (F):「In initializers that perform value preserving type conversions, omit the first argument label」— 狭める変換にはラベル（`truncating`/`saturating` 等）。
- 4 種を全て別名で持つのは Rust と Swift の 2 言語のみ。綴りパターン: Rust = 接頭辞 `checked_`/`saturating_`/`wrapping_`、Swift = 引数ラベル `exactly:`/`clamping:`/`truncatingIfNeeded:`。**多数派なし**（他 7 言語は 1〜2 種のみ）。

## 12. 変換の向き・to_string

| 言語 | 方向の流儀 | to_string |
|---|---|---|
| Rust | `to_X` / `into_X` / `as_X`（元型のメソッド）と `X::from(y)` / `TryFrom` / `FromStr`（先型の関連関数） | `x.to_string()`（Display） |
| Go | `pkg.Func(x)`: `strconv.Itoa`, `FormatInt`, `ParseInt`; 型変換 `T(x)` | `strconv.Itoa(n)`, `fmt.Sprint(x)`, `String()` メソッド (Stringer) |
| Python | 構築子 `int(x)`, `str(x)`, `list(it)`, `dict.fromkeys` | `str(x)` / `repr(x)` |
| TS/JS | 構築子関数 `String(x)`, `Number(x)`, `Array.from(it)`, `Array.of(...)`, `Object.fromEntries` | `x.toString()` / `String(x)` |
| Gleam | 元モジュールの `to_X`: `int.to_string`, `int.to_float`, `list.to_...`; 先モジュールの `from_X`: `dict.from_list`, `set.from_list`, `string.from_utf_codepoints` | `int.to_string` (F) / `float.to_string` (F) / `string.inspect` (F) |
| Elixir | `Integer.to_string`, `String.to_integer`, `Map.new(list)`, `MapSet.new`, `Enum.into` | `to_string/1` (Kernel, String.Chars), `inspect/1` |
| OCaml | Stdlib `int_of_string`/`string_of_int`（`B_of_A`）と `Int.to_string`, `Float.of_string`（`of_`/`to_`）; Base は `of_X`/`to_X` に統一 | `string_of_int` / `Int.to_string` |
| Kotlin | `x.toX()`（`toInt`, `toList`, `toSet`, `toMap`）; 構築 `listOf`, `mapOf`, `setOf` | `x.toString()` |
| Swift | 先型の init `Int(x)`, `String(x)`, `Array(seq)`, `Set(xs)` | `String(x)` / `"\(x)"` / `description` |

- 多数派: **`to_X`（元側）方式 5/9**（Rust, Gleam, Elixir, OCaml 新, Kotlin）、構築子方式 3（Python, JS, Swift）。`from_X`（先側）を併用: Rust, Gleam, JS(`Array.from`)。
- to_string の綴り: `to_string`/`toString` 5（Rust, Gleam, Elixir, OCaml `Int.to_string`, Kotlin, JS）→ 6/9、`str()`/`String()` 構築子 2（Python, Swift）、Go `Itoa`/`String()` 1。**多数派 `to_string` 系 6/9**。

## 13. 述語の命名

| 言語 | 流儀 | 例 |
|---|---|---|
| Rust | `is_X` / `has_X` / `starts_with` | `is_empty`, `is_some`, `is_ascii_digit` |
| Go | `IsX` / `HasX` / `Contains` | `unicode.IsDigit`, `strings.HasPrefix`, `math.IsNaN` |
| Python | `isX`（**アンダースコアなし**）/ `startswith` | `str.isdigit`, `str.isupper`, `os.path.isfile`, `math.isnan` |
| TS/JS | `isX` / `has` / `includes` | `Array.isArray`, `Number.isInteger`, `Set.has`, `isSubsetOf` (F) |
| Gleam | `is_X` / `has_key` / `contains` | `list.is_empty`, `dict.has_key`, `set.is_subset` (F), `int.is_even` |
| Elixir | 末尾 `?`；ガード可能なものは `is_X`（両方の併用禁止）(F) | `Enum.empty?`, `String.contains?`, `Map.has_key?`, `is_list/1`, `Integer.is_even/1` |
| OCaml | `is_X` 一部、多くは `mem` / `exists` / `for_all` | `List.is_empty`, `String.starts_with` (F), `Float.is_nan` |
| Kotlin | `isX` / `hasX` | `isEmpty`, `isNotEmpty`, `isBlank`, `isDigit` |
| Swift | `isX`（主語に対する主張として読む）(F) | `isEmpty`, `isMultiple(of:)`, `contains`, `hasPrefix` |

- 多数派: **`is` 接頭辞 8/9**（綴りは `is_` 3 = Rust, Gleam, OCaml、`isX`/`IsX` 5 = Go, Python(区切りなし), JS, Kotlin, Swift）。例外 Elixir（`?` 接尾辞）。

## 14. 引数順（データ先頭か関数先頭か）とパイプ

| 言語 | 順序 | パイプ |
|---|---|---|
| Rust | 受け手が先（メソッド `xs.iter().map(f)`） | メソッドチェーンが代替 |
| Go | データ先頭 `slices.IndexFunc(xs, f)` | なし |
| Python | **関数先頭** `map(f, xs)`, `filter(f, xs)`, `sorted(xs, key=f)` は data 先頭 → 混在 | なし |
| TS/JS | 受け手が先（メソッド）; `Object.groupBy(items, fn)` は data 先頭 | なし（`\|>` は TC39 Stage 2 提案） |
| Gleam | **データ先頭** `list.map(xs, f)` (F シグネチャ確認) | `\|>` は**第 1 引数**へ差し込む |
| Elixir | **データ先頭** `Enum.map(xs, f)` | `\|>` は**第 1 引数**へ |
| OCaml Stdlib | **関数先頭** `List.map f xs`, `List.sort cmp xs` (F) | `\|>` は**最後の引数**へ（`xs \|> List.map f`） |
| OCaml Base | **データ先頭 + ラベル** `List.map xs ~f` (F) | ラベルのおかげで `xs \|> List.map ~f` も可 |
| Kotlin | 受け手が先（拡張関数 `xs.map { }`） | メソッドチェーン |
| Swift | 受け手が先（メソッド） | メソッドチェーン |
| (参考 F#) | 関数先頭 `List.map f xs` | `\|>` は最後の引数へ |

- 多数派: **データ（受け手）先頭 7/9**（Rust, Go, JS, Gleam, Elixir, Kotlin, Swift; + Base）。関数先頭: OCaml Stdlib, Python の map/filter。

## 15. 高階関数の派生名（_by / _with / _while / _map）

| 言語 | キー関数 `_by` | 比較/結合 `_with` | `_while` | `filter_map` | `flat_map` | `find_map` |
|---|---|---|---|---|---|---|
| Rust | `sort_by_key`, `max_by_key`, `min_by_key`, `dedup_by_key` | `sort_by`(比較), `max_by`(比較), `zip` + `map` | `take_while`, `skip_while`, `map_while` | `filter_map` | `flat_map` | `find_map` |
| Go | — | `SortFunc`, `IndexFunc`, `ContainsFunc`, `MaxFunc`（**`Func` 接尾辞**）(F) | — | — | — | — |
| Python | `key=` 引数 (`sorted`, `max`, `min`, `itertools.groupby`) | — | `itertools.takewhile`, `dropwhile` | （内包表記） | `itertools.chain.from_iterable` | — |
| TS/JS | — | — | — | — | `flatMap` | — (`find`) |
| Gleam | `list.chunk(xs, by: f)` (F), `group` | `sort(by:)`(比較), `dict.combine(with:)` (F) | `take_while`, `drop_while` (F) | `filter_map` (F) | `flat_map` (F) | `find_map` (F) |
| Elixir | `sort_by`, `max_by`, `min_by`, `uniq_by`, `group_by`, `chunk_by`, `split_with`(述語) | — | `take_while`, `drop_while`, `split_while` | (`filter_map` は廃止; `Enum.flat_map`/`for`) | `flat_map` | `find_value` |
| OCaml | — (Base `List.max_elt ~compare`) | `List.sort_uniq cmp` | `take_while`, `drop_while` (5.3〜, F) | `filter_map` (F) | `concat_map` (F) | `find_map` (F) |
| Kotlin | `sortBy`, `sortedBy`, `maxBy`, `minBy`, `distinctBy`, `groupBy`, `associateBy` | `sortWith`, `sortedWith`, `maxWith`, `minWith`, `zip(other){…}` | `takeWhile`, `dropWhile` | `mapNotNull` (F) | `flatMap` | `firstNotNullOf(OrNull)` (F) |
| Swift | — | `sorted(by:)`, `max(by:)`（比較, 引数ラベル） | `prefix(while:)`, `drop(while:)` | `compactMap` | `flatMap` | `first(where:)` + 手動 |

- `_by` = キー関数: Rust(`_by_key`), Elixir, Kotlin, Gleam `chunk(by:)` → Elixir/Kotlin が明確な体系。Rust と Swift と Gleam の `sort(by:)` では `by` = **比較関数**。**`_by` の意味に多数派なし**。
- `_with` = 比較関数: Kotlin のみが体系的（`sortWith`/`maxWith`）。Go は `Func` 接尾辞。
- `take_while`/`takeWhile` 系: **7/9**（Rust, Python, Gleam, Elixir, OCaml, Kotlin, Swift `prefix(while:)` を含めて）。
- `filter_map`: 綴りとして 3（Rust, Gleam, OCaml）、別名 `mapNotNull`, `compactMap`。
- `flat_map`/`flatMap`: **6/9**（Rust, JS, Gleam, Elixir, Kotlin, Swift）; OCaml `concat_map`。
- `find_map`: 3（Rust, Gleam, OCaml）、Elixir `find_value`、Kotlin `firstNotNullOf`。

## 16. スライス

| 言語 | 区間取り出し | take / drop | 負の index | 半開区間 |
|---|---|---|---|---|
| Rust | `&v[a..b]` (panic) / `v.get(a..b)` → Option (F) | `iter().take(n)`, `skip(n)` | 不可 | 半開 `a..b`; 閉 `a..=b` |
| Go | `xs[a:b]` (panic) | — | 不可 | 半開 |
| Python | `xs[a:b]` (範囲外は丸める) | — (`itertools.islice`) | 可 | 半開 |
| TS/JS | `a.slice(start, end)`, `s.slice`, `s.substring` | — | `slice` は可、`substring` は不可 | 半開 |
| Gleam | `string.slice(s, at_index:, length:)` = **(開始, 長さ)** 負可 (F); list は `take`/`drop` (F) | `list.take`/`drop` (F); `string.drop_start` (F) | `string.slice` は可 (F) | — |
| Elixir | `String.slice(s, start, len)` / `String.slice(s, a..b)`; `Enum.slice(xs, a..b)` | `Enum.take`/`drop`（負なら末尾から） | 可 | Range は**閉区間**（`a..b//1`） |
| OCaml | `String.sub s pos len` = **(開始, 長さ)**、範囲外は例外; `Array.sub` | `List.take`/`drop` (5.3〜, F) | 不可 | — |
| Kotlin | `subList(a, b)`, `slice(a..b)`（閉区間）, `substring(a, b)`（半開） | `take`/`drop`/`takeLast`/`dropLast` | 不可 | substring/subList は半開、slice(range) は閉 |
| Swift | `xs[a..<b]` / `xs[a...b]`（範囲外 trap）; String は `String.Index` 必須 | `prefix(n)`, `dropFirst(n)`, `suffix`, `dropLast` | 不可 | 演算子で両方 |

- 多数派: (start, end) 半開 **5/9**（Rust, Go, Python, JS, Kotlin substring）；(start, length) 2（Gleam string, OCaml）；Elixir は両方。take/drop の綴り: **`take`/`drop` 6/9**（Gleam, Elixir, OCaml, Kotlin, Rust の take + skip, …）。負 index 可: Python, JS(slice), Gleam, Elixir = 4。

## 17. 大文字小文字

| 言語 | 綴り |
|---|---|
| Rust | `to_uppercase` / `to_lowercase`；ASCII 版 `to_ascii_uppercase` (F) |
| Go | `strings.ToUpper` / `ToLower` |
| Python | `str.upper()` / `lower()`；`casefold` |
| TS/JS | `toUpperCase()` / `toLowerCase()` |
| Gleam | `string.uppercase` / `lowercase` (F) |
| Elixir | `String.upcase` / `downcase` |
| OCaml | `String.uppercase_ascii` / `lowercase_ascii`（Stdlib は ASCII のみ）; Base `String.uppercase` |
| Kotlin | `uppercase()` / `lowercase()`（1.5〜; `toUpperCase` は非推奨） |
| Swift | `uppercased()` / `lowercased()` |

- `to_upper` 系 (`to_uppercase`, `ToUpper`, `toUpperCase`) 3、`uppercase` 系（`uppercase`, `uppercase_ascii`, `uppercased`）4、`upper` 1、`upcase` 1。**相対多数 `uppercase` 4/9、過半数なし**。

## 18. trim 系

| 言語 | 両端 | 先頭 | 末尾 |
|---|---|---|---|
| Rust | `trim` | `trim_start`（`trim_left` 非推奨）(F) | `trim_end`（`trim_right` 非推奨）(F) |
| Go | `strings.TrimSpace`（`Trim(s, cutset)`） | `TrimLeft(s, cutset)` / `TrimPrefix` | `TrimRight` / `TrimSuffix` |
| Python | `strip` | `lstrip` | `rstrip` |
| TS/JS | `trim` | `trimStart`（`trimLeft` 別名） | `trimEnd`（`trimRight` 別名） |
| Gleam | `trim` (F) | `trim_start` (F) | `trim_end` (F) |
| Elixir | `String.trim` | `trim_leading` | `trim_trailing` |
| OCaml | `String.trim`（両端のみ） | —（Base `lstrip`） | —（Base `rstrip`, Base 両端は `strip`） |
| Kotlin | `trim` | `trimStart` | `trimEnd` |
| Swift | `trimmingCharacters(in: .whitespaces)`（Foundation） | — | — |

- 両端: **`trim` 6/9**。先頭/末尾: **`trim_start`/`trim_end`（`trimStart`/`trimEnd`）4/9**（Rust, JS, Gleam, Kotlin）が相対多数；`lstrip/rstrip` 2（Python, Base）、`trim_leading/trailing` 1、`TrimLeft/Right` 1。left/right 綴りは Rust・JS で非推奨化された。

## 19. index-of

| 言語 | 綴り | 見つからない時 |
|---|---|---|
| Rust | `s.find(pat)` (F) / `iter().position(f)` | `None` |
| Go | `strings.Index`, `slices.Index`, `IndexFunc` (F) | **-1** |
| Python | `s.find(sub)` / `s.index(sub)`, `xs.index(x)` | find は **-1**、index は ValueError |
| TS/JS | `indexOf`, `findIndex`, `lastIndexOf` | **-1** |
| Gleam | なし（string/list ともに index-of 関数なし, F） | — |
| Elixir | `Enum.find_index(xs, f)`; 文字列は `:binary.match` | `nil` / `:nomatch` |
| OCaml | `String.index_opt s c`（char）(F), `String.find_first ~sub s` (5.5〜) → option (F); `String.index` は例外 | `None` / 例外 |
| Kotlin | `indexOf`, `indexOfFirst`, `lastIndexOf` | **-1** |
| Swift | `firstIndex(of:)`, `firstIndex(where:)`, `lastIndex(of:)` | `nil` (Optional) |

- -1 を返す: Go, Python(find), JS, Kotlin = 4。Option/nil: Rust, Elixir, OCaml, Swift = 4。Gleam は関数自体なし。**拮抗、多数派なし**（綴りは `index`/`indexOf` 系 5）。

## 20. プロセス / argv / env

| 言語 | argv | env 取得 | env 設定 | exit | cwd |
|---|---|---|---|---|---|
| Rust | `std::env::args()` (iter) | `std::env::var(k)` → `Result` | `std::env::set_var`（2024 edition で `unsafe`） | `std::process::exit(code)` | `std::env::current_dir()` → Result |
| Go | `os.Args` ([]string) | `os.Getenv(k)` (無ければ "") / `os.LookupEnv` → (v, ok) | `os.Setenv` → error | `os.Exit(code)` | `os.Getwd()` → (string, error) |
| Python | `sys.argv` | `os.environ[k]` (KeyError) / `os.getenv(k, default)` / `os.environ.get` | `os.environ[k] = v` | `sys.exit(code)` | `os.getcwd()` |
| TS/JS | ECMAScript に無し（Node `process.argv`, Deno `Deno.args`） | 同（`process.env.K`） | 同 | 同（`process.exit`） | 同（`process.cwd()`） |
| Gleam | gleam_stdlib に無し（外部 `argv` パッケージ、`envoy`） | 無し | 無し | 無し | 無し |
| Elixir | `System.argv()` | `System.get_env(k)` → nil / `System.fetch_env` → {:ok,v}\|:error / `fetch_env!` | `System.put_env` | `System.halt(code)` / `System.stop` | `File.cwd()` → {:ok, p} / `File.cwd!` |
| OCaml | `Sys.argv` (array) | `Sys.getenv` (Not_found) / `Sys.getenv_opt` | `Unix.putenv`（unix ライブラリ） | `exit code` (Stdlib) | `Sys.getcwd ()` |
| Kotlin | `main(args: Array<String>)` | JVM `System.getenv(k)` → `String?` | （JVM 標準なし） | `kotlin.system.exitProcess(code)` | JVM `System.getProperty("user.dir")` |
| Swift | `CommandLine.arguments` (stdlib) | Foundation `ProcessInfo.processInfo.environment[k]` | `setenv` (libc) | `exit(code)` (libc / Foundation) | `FileManager.default.currentDirectoryPath` |

- 置き場所: **`env`/`os`/`sys`/`System` などプロセス専用モジュール**に置くのが 7/9（Rust `env`+`process`、Go `os`、Python `sys`+`os`、Elixir `System`、OCaml `Sys`、Kotlin `kotlin.system`、Swift `CommandLine`）。言語標準に無い: JS(ECMAScript), Gleam。
- 綴り: argv は `args`/`Args`/`argv`/`arguments` が拮抗（args 3: Rust, Go, Kotlin / argv 3: Python, Elixir, OCaml / arguments 1）。env 取得は `getenv`/`get_env`/`Getenv` 5（Go, Python, Elixir, OCaml, Kotlin）、Rust `var`。exit は **`exit` 6/9**（Rust, Go, Python, OCaml, Kotlin `exitProcess`, Swift）。cwd は `getcwd`/`Getwd` 4（Go, Python, OCaml, + Rust `current_dir`）。

---

## 要約（多数派が存在するもの / しないもの）

多数派あり: 空判定 `is_empty` (5/9) ・ list 所属 `contains` (5/9) ・ set 削除 `remove` (5/8) ・ set 和 `union` (8/8) / 積 `intersection` (6/8) / 差 `difference` (5/8) ・ `reverse` (8/9) ・ to_string 系 (6/9) ・ 変換は元側 `to_X` (5/9) ・ 述語 `is` 接頭辞 (8/9) ・ データ先頭の引数順 (7/9) ・ `flat_map` (6/9) ・ `take_while` 系 (7/9) ・ `trim` (6/9) ・ parse は値で失敗を返す (7/9) ・ exit (6/9)。

多数派なし: 長さの綴り（length 4 / len 3 / size / count）と文字列長の単位 ・ in-place 追加（append/push/add）・ pop / remove-at ・ map の挿入（insert/put/set/添字）と削除（remove 4 対 delete 4）・ upsert ・ sort の既定が in-place か copy か（6 対 3）・ `_by` の意味（キー関数 か 比較関数 か）・ 大文字化（uppercase 4 / to_upper 3）・ index-of（-1 が 4 / Option が 4）・ 数値変換 4 種（Rust と Swift のみ網羅）・ Option 版/例外版の無印がどちらか（OCaml Stdlib/Kotlin は無印=例外、Base/Rust/Elixir は無印=安全）。
