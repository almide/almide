<!-- description: Stdlib redesign L2/L3 — set: complete the higher-order family with count and find, keep insert/remove as copies, and decline superset and partition -->
# L2/L3: set

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)
> 出典: Rust https://doc.rust-lang.org/std/collections/struct.HashSet.html (F) ・ Go https://pkg.go.dev/maps (F)（set 型なし） ・ Python https://docs.python.org/3/library/stdtypes.html#set-types-set-frozenset ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Set (F) ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/set.html (F) ・ Elixir https://elixir.hexdocs.pm/MapSet.html (F) ・ OCaml https://ocaml.org/manual/latest/api/Set.S.html (F) ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.collections/-set/ ・ Swift https://developer.apple.com/documentation/swift/set (F)
> 共通の判定: list・map・option・result・string のページと同じ D1–D11 に従います（[L2-map の対応表](L2-map.md#共通の判定との対応)）

読み方の約束（L0 と同じ）:
- `mut`（読み: ミュート）= 呼び出し元の `var` をその場で書き換える引数
- `|>`（読み: パイプ）= 左の値を右の関数の第 1 引数に渡す

いまの公開関数は 19 個です（interface JSON、`mut` 表示つき）。`mut` 引数を持つ関数は 1 つもありません。set は全部コピーです。

---

## L2: 役割の比較

(F) は今回公式ページを取り寄せて確かめたセルです。Go は set 型を持たず、`map[T]struct{}` で代用します（全行 —）。

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 空・リストから作る | `set.new`・`set.from_list` | `new`・`from_iter` | — | `set()`・`set(xs)` | `new Set(xs)` (F) | `new`・`from_list` (F) | `new/0`・`new/1` (F) | `empty`・`of_list` (F) | `setOf`・`toSet` | `Set(xs)` |
| 足す | `set.insert`（copy） | `insert` (F, in-place) | — | `add`（in-place） | `add` (F, in-place) | `insert` (F, copy) | `put` (F, copy) | `add` (F, copy) | `add`（in-place）/ `s + x` | `insert` (F, mutating) |
| 除く | `set.remove`（copy） | `remove` (F) | — | `remove`・`discard` | `delete` (F) | `delete` (F) | `delete` (F) | `remove` (F) | `remove` / `s - x` | `remove` (F) |
| 含むか | `set.contains` | `contains` (F) | — | `x in s` | `has` (F) | `contains` (F) | `member?` (F) | `mem` (F) | `contains` | `contains` (F) |
| 個数・空判定 | `set.len`・`set.is_empty` | `len`・`is_empty` (F) | — | `len` | `size` (F) | `size`・`is_empty` (F) | `size` (F) | `cardinal`・`is_empty` (F) | `size`・`isEmpty` | `count`・`isEmpty` |
| リストにする | `set.to_list` | `iter().collect` | — | `list(s)` | `[...s]` | `to_list` (F) | `to_list` (F) | `to_list`・`elements` (F) | `toList` | `Array(s)` |
| 和・積・差・対称差 | `union`・`intersection`・`difference`・`symmetric_difference` | 同じ 4 つ (F) | — | 同じ 4 つ | `union`・`intersection`・`difference`・`symmetricDifference` (F) | 同じ 4 つ (F) | 同じ 4 つ (F) | `union`・`inter`・`diff` (F) | `union`・`intersect`・`subtract` | `union`・`intersection`・`subtracting`・`symmetricDifference` (F) |
| 部分集合か | `set.is_subset` | `is_subset` (F) | — | `issubset` | `isSubsetOf` (F) | `is_subset` (F) | `subset?` (F) | `subset` (F) | `b.containsAll(a)` | `isSubset(of:)` (F) |
| 上位集合か | — | `is_superset` (F) | — | `issuperset` | `isSupersetOf` (F) | — (F) | — (F) | — (F) | `containsAll` | `isSuperset(of:)` (F) |
| 共通がないか | `set.is_disjoint` | `is_disjoint` (F) | — | `isdisjoint` | `isDisjointFrom` (F) | `is_disjoint` (F) | `disjoint?` (F) | `disjoint` (F) | — | `isDisjoint(with:)` (F) |
| 絞り込み・変換・畳み込み | `set.filter`・`set.map`・`set.fold` | `retain`・`iter().map`・`fold` | — | 内包表記 | — | `filter`・`map`・`fold` (F) | `filter` (F)・`Enum.map` | `filter`・`map`・`fold` (F) | `filter`・`map`・`fold` | `filter`・`map` (F)・`reduce` |
| any / all | `set.any`・`set.all` | `iter().any`・`all` | — | `any()`・`all()` | — | — (F) | `Enum.any?`・`all?` | `exists`・`for_all` (F) | `any`・`all` | `contains(where:)`・`allSatisfy` |
| 述語で数える | — | `iter().filter().count()` | — | `sum(...)` | — | — (F) | `Enum.count/2` | — (F) | `count {}` | `count(where:)` (F) |
| 述語で 1 つ探す | — | `iter().find` | — | `next(...)` | — | — (F) | `Enum.find` | `find_first`（単調な述語だけ）(F) | `find` | `first(where:)` (F) |
| 述語で 2 つに分ける | — | — | — | — | — | — (F) | `split_with` (F) | `partition` (F) | `partition` | — |

### 完全性の規則

1. **高階関数の family（list・map・set で共通）**: `map`、`filter`、`fold`、`any`、`all`、`count`、`find` の 7 つを、3 つのコレクションすべてに置きます（共通判定 D3、[L2-map R-map-06](L2-map.md)）。set は `count` と `find` が欠けています → R-set-01。
2. **基本の集合（list・map・set で共通）**: `new`、`from_list`、`len`、`is_empty`、`contains`。set はすべて持っています。
3. **集合演算の family**: 2 つの set から set を作る 4 つ（`union`、`intersection`、`difference`、`symmetric_difference`）と、2 つの set を比べる Bool の 2 つ（`is_subset`、`is_disjoint`）。上位集合 `is_superset` は、`is_subset` の引数を入れ替えた同じ判定なので、意図して置きません（R-set-02）。
4. **書き換えの様式**: set の公開関数はすべてコピーで、`mut` 引数を持つ関数はありません。`insert`・`remove` はコピー側の動詞です（R-L0-04）。その場の動詞（`push`・`pop`・`clear`）が要る操作は set にはありません。
5. **set.map の性質**: 変換の結果が重なると要素数が減ります（実測: `{1, 2, 3}` を `x % 2` で変換すると `[1, 0]`）。これは set の定義どおりで、変えません。

### L2 の判定

- **family の穴**: `set.count` と `set.find` がありません → R-set-01。いま `set.count` を書くと E002 になり、ヒントは「`set.len` を使え」と出ます（実測）。これは `ALIASES` 表（`crates/almide-frontend/src/stdlib.rs:162`）の `("set", "count", "set.len")` の行から来ていて、述語を数えたい書き手を誤った関数に誘導しています。R-set-01 でこの行を消します。
- **抜けている役割（5/9 以上）**:
  - 上位集合か: 専用の名前を持つのは Rust・Python・JS・Swift の 4/9 です（Kotlin の `containsAll` はコレクション一般の関数）→ R-set-02（×）。
  - 述語で 2 つに分ける: Elixir・OCaml・Kotlin の 3/9 → R-set-03（×）。
- **綴りの衝突**: ありません。`insert` は Rust・Gleam・Swift の 3/9、`add` は Python・JS・OCaml・Kotlin の 4/9 で、多数派はないので据え置きます（R-L0-00）。`remove` は Rust・Python・OCaml・Kotlin・Swift の 5/9 で多数派と一致しています。
- **黙って捨てられる呼び出し（R-L0-05）**: L0 本文の実測例そのものです。いまも native と wasm の両方で、次のとおり黙って通ります（2026-10-21、`almide 0.68.0 (dev, 437b8f127)`、`scratchpad/sr/probe-set/s.almd`）:

  ```almide
  var s = set.new()
  set.insert(s, 1)     // エラーにならない。何も起きない
  println(int.to_string(set.len(s)))   // 0
  s = set.insert(s, 1) // 正しい書き方
  ```

  set は全関数がコピーなので、`set.insert`・`set.remove`・`set.union`・`set.filter` を文で書くと必ず捨てられます。Python・JS・Rust・Kotlin・Swift の `add`/`insert` はその場書き換えなので、LLM が最も間違えやすいモジュールです。R-L0-05 が入れば E042 で止まります。set 側で名前を変える案は立てません（R-L0-04 でコピーの動詞と決まっています）。

---

## 案

### R-set-01 `set.count` と `set.find` を足す — ○ 採用

- 内容:
  - `set.count(s: Set[A], f: (A) -> Bool) -> Int` — `f` が成り立つ要素の数。空なら 0。
  - `set.find(s: Set[A], f: (A) -> Bool) -> Option[A]` — 挿入順で最初に `f` が成り立つ要素。なければ none。見つけた後は `f` を呼びません（`list.find` と同じ）。
  - どちらも純粋な Almide で `stdlib/set_core.almd` に書き、`self_host_registry.rs` に登録します。
  - `ALIASES` の `("set", "count", "set.len")`（`stdlib.rs:162`）を消します。同じ表に `("set", "add", "set.insert")`・`("set", "has", "set.contains")` を足します（2026-10-21 の時点でどちらもありません。R-map-03 と同じ仕組み）。
- 根拠の L0 規則: R-L0-01（同じ役割に同じ綴り。list と map の `count`・`find` と同じ名前、同じコールバックの形）、R-L0-10、CLAUDE.md の「family は行列で広げる」
- (a) 専用の名前で持つ言語は、数えるほうが Kotlin `count {}`・Elixir `Enum.count/2`・Swift `count(where:)` (F)、探すほうが Kotlin `find`・Elixir `Enum.find`・Swift `first(where:)` (F) で、どちらも 5/9 に届きません。それでもこの追加は L1 の「新しい役割を足す」基準ではなく、Almide の中の family を埋めるもの（完全性の規則 1）です。list・map にあって set にないと、LLM は規則から推測して書いて外れます。
- (b) 純粋な計算です。host の機能は要りません。native と wasm の両方で同じ順序（挿入順）で動きます。
- (c) `xs |> list.count(f)`、`m |> map.count(f)` と同じ形で `s |> set.count(f)` が書けます。いまの「`set.len` を使え」というヒントは、述語を捨てる誤りに誘導しています。
- (d) 追加。互換の問題はありません。使用数: 0（新しい綴り）。`set.count` を書いて E002 になっている呼び出しは spec / tests / docs / stdlib のどれにもありません。
- ゲート案: L2-map R-map-06 の「コレクション family 行列」テストが、set の 2 セルを要求します。`spec/stdlib/set_test.almd` に境界（空、該当なし、全部該当）のテストを置き、almide-interp の 3-way oracle に橋を足します。
- issue: #3901

### R-set-02 `set.is_superset` を足す — × 不採用

- 内容: 足しません。`a` が `b` の上位集合かは `set.is_subset(b, a)` で書きます。
- 根拠の L0 規則: R-L0-01（引数を入れ替えただけの同じ判定に綴りを 2 つ置かない）、L1 の追加基準（5/9 以上）
- (a) 専用の名前は Rust `is_superset` (F)・Python `issuperset`・JS `isSupersetOf` (F)・Swift `isSuperset(of:)` (F) の 4/9 です。Gleam・Elixir・OCaml は部分集合の判定だけを持ちます (F)。
- (c) 書き手が `set.is_superset` を書いたときのために、`ALIASES` に `("set", "is_superset", "set.is_subset (arguments swapped)")` を足します。
- (b)(d) 変更なし（ヒントだけ）。
- issue: R-set-01 と同じ issue でヒントを足します。

### R-set-03 `set.partition` を足す — × 不採用

- 内容: 足しません。`(set.filter(s, f), set.filter(s, (x) => not f(x)))` で書けます。
- 根拠の L0 規則: L1 の追加基準（5/9 以上）、完全性の規則 1（family の 7 つに `partition` は入っていません）
- (a) Elixir `split_with` (F)・OCaml `partition` (F)・Kotlin `partition` の 3/9 です。
- (b)(d) 変更なし。
- issue: なし

### R-set-04 `set.insert` を `set.add` に改名する — × 不採用

- 内容: 据え置きます。
- 根拠の L0 規則: R-L0-00（多数派がなければ動かさない）、R-L0-04（`insert` はコピー側の動詞として全モジュールで揃えます）
- (a) `add` は Python・JS・OCaml・Kotlin の 4/9、`insert` は Rust・Gleam・Swift の 3/9、`put` は Elixir で、5/9 に届きません。
- (c) `add` と書いた場合は `ALIASES` の行で `set.insert` に誘導します（R-set-01 で足す行）。黙って捨てられる誤りは、名前ではなく R-L0-05 で止めます。
- (d) 変更なし。
- issue: なし

---

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `set.new` | `set.new() -> Set[A]` | Rust `new` / Gleam `new` / Elixir `new/0` / OCaml `empty` | 据え置き | — |
| `set.from_list` | `set.from_list(xs: List[A]) -> Set[A]` | Gleam `from_list` / OCaml `of_list` / Kotlin `toSet` / Rust `from_iter` | 据え置き | — (R-L0-07) |
| `set.insert` | `set.insert(s: Set[A], value: A) -> Set[A]` | Rust・Gleam・Swift `insert` / Python・JS・OCaml・Kotlin `add` / Elixir `put` | 据え置き | R-set-04 |
| `set.remove` | `set.remove(s: Set[A], value: A) -> Set[A]` | Rust・Python・OCaml・Kotlin・Swift `remove` / JS・Gleam・Elixir `delete` | 据え置き | — |
| `set.contains` | `set.contains(s: Set[A], value: A) -> Bool` | Rust・Gleam・Kotlin・Swift `contains` / JS `has` / OCaml `mem` / Elixir `member?` | 据え置き | — |
| `set.len` | `set.len(s: Set[A]) -> Int` | Rust `len` / Gleam・JS・Elixir `size` / OCaml `cardinal` / Swift `count` | 据え置き | — (R-L0-11) |
| `set.is_empty` | `set.is_empty(s: Set[A]) -> Bool` | Rust・Gleam・OCaml `is_empty` / Kotlin `isEmpty` / Swift `isEmpty` | 据え置き | — |
| `set.to_list` | `set.to_list(s: Set[A]) -> List[A]` | Gleam・Elixir・OCaml `to_list` / Kotlin `toList` | 据え置き | — |
| `set.union` | `set.union(a: Set[A], b: Set[A]) -> Set[A]` | 9 言語中 8 言語 `union`（Go —） | 据え置き | — |
| `set.intersection` | `set.intersection(a: Set[A], b: Set[A]) -> Set[A]` | Rust・Python・JS・Gleam・Elixir・Swift `intersection` / OCaml `inter` / Kotlin `intersect` | 据え置き | — |
| `set.difference` | `set.difference(a: Set[A], b: Set[A]) -> Set[A]` | Rust・Python・JS・Gleam・Elixir `difference` / Swift `subtracting` / OCaml `diff` / Kotlin `subtract` | 据え置き | — |
| `set.symmetric_difference` | `set.symmetric_difference(a: Set[A], b: Set[A]) -> Set[A]` | Rust・Python・Gleam・Elixir `symmetric_difference` / JS・Swift `symmetricDifference` | 据え置き | — |
| `set.is_subset` | `set.is_subset(a: Set[A], b: Set[A]) -> Bool` | Rust・Gleam `is_subset` / Python `issubset` / JS `isSubsetOf` / OCaml `subset` / Swift `isSubset(of:)` | 据え置き | R-set-02 |
| `set.is_disjoint` | `set.is_disjoint(a: Set[A], b: Set[A]) -> Bool` | Rust・Gleam `is_disjoint` / Python `isdisjoint` / JS `isDisjointFrom` / OCaml `disjoint` / Swift `isDisjoint(with:)` | 据え置き | — |
| `set.filter` | `set.filter(s: Set[A], f: (A) -> Bool) -> Set[A]` | Gleam・Elixir・OCaml・Kotlin・Swift `filter` / Rust `retain` | 据え置き | R-map-06 |
| `set.map` | `set.map(s: Set[A], f: (A) -> B) -> Set[B]` | Gleam・OCaml `map` / Kotlin `map`（List を返す）/ Swift `map`（Array を返す） | 据え置き | R-map-06 |
| `set.fold` | `set.fold(s: Set[A], init: B, f: (B, A) -> B) -> B` | Gleam・OCaml `fold` / Kotlin `fold` / Swift `reduce` | 据え置き | R-map-06 |
| `set.any` | `set.any(s: Set[A], f: (A) -> Bool) -> Bool` | Kotlin `any` / OCaml `exists` / Elixir `Enum.any?` / Swift `contains(where:)` | 据え置き | R-map-06 |
| `set.all` | `set.all(s: Set[A], f: (A) -> Bool) -> Bool` | Kotlin `all` / OCaml `for_all` / Elixir `Enum.all?` / Swift `allSatisfy` | 据え置き | R-map-06 |
| `set.count` | `set.count(s: Set[A], f: (A) -> Bool) -> Int`（案） | Kotlin `count {}` / Elixir `Enum.count/2` / Swift `count(where:)` / Gleam — | 追加 | R-set-01 |
| `set.find` | `set.find(s: Set[A], f: (A) -> Bool) -> Option[A]`（案） | Kotlin `find` / Elixir `Enum.find` / Swift `first(where:)` / Rust `iter().find` | 追加 | R-set-01 |

L3 の行数: 21（現行 19 + 追加 2）。判定の内訳: 据え置き 19、追加 2。

使用数（参考。`\bset\.<fn>\b` の出現数、定義と生成文書を含む上限値。spec / tests / docs / stdlib）: insert 32 / 26 / 24 / 6、remove 8 / 20 / 13 / 0、to_list 33 / 20 / 5 / 9、map 20 / 15 / 16 / 6。

---

## L0 への差し戻し

なし。
