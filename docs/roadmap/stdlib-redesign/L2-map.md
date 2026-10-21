<!-- description: Stdlib redesign L2/L3 — map: retire the in-place insert/delete pair, fold get_or into ??, and gate the higher-order family shared with list and set -->
# L2/L3: map

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)
> 出典: Rust https://doc.rust-lang.org/std/collections/struct.HashMap.html ・ Go https://pkg.go.dev/maps (F) ・ Python https://docs.python.org/3/library/stdtypes.html#mapping-types-dict ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Map ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/dict.html (F) ・ Elixir https://elixir.hexdocs.pm/Map.html (F) ・ OCaml https://ocaml.org/manual/latest/api/Map.S.html (F) ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.collections/-map/ (F) ・ Swift https://developer.apple.com/documentation/swift/dictionary
> 共通の判定: [collections の共通判定 D1–D11](#共通の判定との対応)（list・set・option・result・string のページと同じ内容です）

読み方の約束（L0 と同じ）:
- `mut`（読み: ミュート）= 呼び出し元の `var` をその場で書き換える引数
- `??`（読み: ナル合体）= 左が none なら右を使う演算子
- `|>`（読み: パイプ）= 左の値を右の関数の第 1 引数に渡す
- `m[k] = v`（読み: 添字代入）= 変数 `m` のキー `k` を `v` にする文。`m = map.set(m, k, v)` と同じ意味で、`@reuse` によりコピーせずに動きます

いまの公開関数は 25 個です（interface JSON、`mut` 表示つき）。

---

## L2: 役割の比較

(F) は今回公式ページを取り寄せて確かめたセルです。in-place = その場書き換え、copy = 新しい値を返す。

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 空を作る | `map.new` | `HashMap::new` | `make(map[K]V)` | `{}` / `dict()` | `new Map()` | `dict.new` (F) | `Map.new` (F) | `M.empty` | `mapOf()` | `[:]` |
| 取り出し（無いと none） | `map.get` | `get` → Option | `v, ok := m[k]` | `d.get(k)` | `m.get(k)` | `dict.get` → Result (F) | `Map.fetch` (F) | `find_opt` (F) | `m[k]` → `V?` (F) | `d[k]` → Optional |
| 取り出し（既定値つき） | `map.get_or` | — (`unwrap_or`) | 零値 | `d.get(k, d0)` | `?? d` | — (`result.unwrap`) | `Map.get(m,k,d)` (F) | — | `getOrDefault` (F) | `d[k, default:]` |
| 書く（copy） | `map.set` | — | — | `d \| {k: v}` | — | `dict.insert` (F) | `Map.put` (F) | `add` (F) | `m + (k to v)` (F) | — |
| 書く（in-place） | `map.insert(mut)`・`m[k] = v` | `insert` | `m[k] = v` | `d[k] = v` | `m.set` | — | — | — | `m[k] = v` | `d[k] = v` |
| 消す（copy） | `map.remove` | — | — | — | — | `dict.delete` (F) | `Map.delete` (F) | `remove` (F) | `m - k` (F) | — |
| 消す（in-place） | `map.delete(mut)` | `remove` | `delete(m,k)` | `del d[k]` | `m.delete` | — | — | — | `remove` | `removeValue` |
| 全部消す（in-place） | `map.clear(mut)` | `clear` | `clear(m)` | `d.clear()` | `m.clear()` | — | — | — | `clear` | `removeAll` |
| キーがあるか | `map.contains` | `contains_key` | `_, ok := m[k]` | `k in d` | `m.has` | `dict.has_key` (F) | `Map.has_key?` (F) | `mem` (F) | `containsKey` (F) | `d[k] != nil` |
| 個数・空判定 | `map.len`・`map.is_empty` | `len`・`is_empty` | `len` | `len` | `size` | `size`・`is_empty` (F) | `map_size` | `cardinal`・`is_empty` (F) | `size`・`isEmpty` (F) | `count`・`isEmpty` |
| キー一覧・値一覧 | `map.keys`・`map.values` | `keys`・`values` | `maps.Keys` (F) | `keys()` | `keys()` | `dict.keys`・`values` (F) | `Map.keys`・`values` (F) | — | `keys`・`values` (F) | `keys`・`values` |
| 組の一覧 | `map.entries` | `iter()` | `maps.All` (F) | `items()` | `entries()` | `dict.to_list` (F) | `Map.to_list` (F) | `to_list`・`bindings` (F) | `entries`・`toList` (F) | 反復 |
| 組の一覧から作る | `map.from_list` | `from_iter` / `collect` | `maps.Collect` (F) | `dict(pairs)` | `new Map(pairs)` | `dict.from_list` (F) | `Map.new(list)` | `of_list` (F) | `toMap` | `Dictionary(uniqueKeysWithValues:)` |
| 2 つを合わせる（右が勝つ） | `map.merge` | `extend` | `maps.Copy` (F) | `a \| b` | — | `dict.merge` (F) | `Map.merge/2` (F) | — | `a + b` (F) | `merging` |
| 合わせ方を関数で決める | — | — | — | — | — | `dict.combine` (F) | `Map.merge/3` (F) | `union` (F) | — | `merging(_:uniquingKeysWith:)` |
| 値だけ変換 | `map.map`（`(V) -> B`） | — | — | 内包表記 | — | `dict.map_values`（`fn(k, v)`）(F) | — | `map`（値）・`mapi`（F） | `mapValues` (F) | `mapValues` |
| 絞り込み | `map.filter`（`(K, V)`） | `retain` | `maps.DeleteFunc` (F) | 内包表記 | — | `dict.filter` (F) | `Map.filter` (F) | `filter` (F) | `filter` (F) | `filter` |
| 畳み込み | `map.fold` | `iter().fold` | — | — | — | `dict.fold` (F) | `Enum.reduce` | `fold` (F) | `entries.fold` | `reduce` |
| any / all / count / find | `map.any`・`all`・`count`・`find` | `iter().any` など | — | `any(...)` | — | — | `Enum.*` | `exists`・`for_all` (F) | `any`・`all`・`count` (F) | `contains(where:)` など |
| あれば関数で更新（無ければ何もしない） | `map.update` | `entry().and_modify` | — | — | — | — | `Map.update!`（無ければ例外） | `update`（option→option）(F) | — | — |
| あれば更新・無ければ初期値 | `map.upsert` | `entry().and_modify(f).or_insert(v)` | — | `setdefault` | — | `dict.upsert`（`fn(Option(v)) -> v`）(F) | `Map.update/4`（初期値はそのまま入る）(F) | `update` (F) | `getOrPut` | `d[k, default:] += 1` |
| 取り出して消す | — | `remove` → Option | — | `d.pop(k)` | — | — | `Map.pop` → `{v, map}` (F) | — | `remove` → `V?` | `removeValue` → Optional |

### 完全性の規則

1. **高階関数の family（list・map・set で共通）**: `map`、`filter`、`fold`、`any`、`all`、`count`、`find` の 7 つは、3 つのコレクションすべてに置きます（共通判定 D3）。map はすでに 7 つとも持っています。set に足りない `count` と `find` は [L2-set](L2-set.md) で足します。
2. **基本の集合（list・map・set で共通）**: `new`、`from_list`、`len`、`is_empty`、`contains`。map はすべて持っています。
3. **map のコールバックの形**: キーと値の両方を見る関数（`filter`、`fold`、`any`、`all`、`count`、`find`）は `(K, V)` を受け取ります。値だけを変換する `map.map`、`map.update`、`map.upsert` は `(V)` を受け取ります。値だけを書き換える関数がキーを受け取らないのは意図した省略です（キーは変わらないので）。
4. **書き換えの様式**: map の公開関数は、`clear` を除いてすべてコピー版です（R-L0-04）。キー 1 つの書き込みのその場版は、関数ではなく言語の添字代入 `m[k] = v` が受け持ちます。
5. **意図した省略**: 取り出して消す（`pop`）、合わせ方を関数で決める `merge`、キーの変換 `map_keys` は置きません（R-map-05）。
6. **範囲外の書き込み**: L0 の「位置を指定した書き込みは範囲外で abort」は、添字（位置）で書く関数の規則です。map の書き込みはキーで行うので、位置の範囲外はありません。`map.update` のキーが無いときに何もしないのは、`map.get` が none を返すのと同じ「キーが無い」という通常の場合で、契約違反ではありません（Rust の `entry().and_modify`、OCaml の `update` と同じ）。据え置きます。

### L2 の判定

- **様式の衝突（R-L0-04）**: `map.insert(mut)` と `map.delete(mut)` だけが、コピー側の動詞 `insert`・`delete` でその場書き換えをしています。`set.insert` と `list.insert` はコピーです → R-map-01。
- **既定値版の重複（R-L0-06）**: `map.get_or` は `map.get(m, k) ?? d` と同じです → R-map-02。
- **抜けている役割（5/9 以上）**:
  - 組の一覧を `to_list` で取る（Gleam・Elixir・OCaml・Kotlin の 4/9）→ 多数派ではなく、`map.entries` が役割を果たしています → R-map-03（×）。
  - 取り出して消す（Rust・Python・Elixir・Kotlin・Swift の 5/9）→ 2 段（`get` と `remove`）で書けて、`pop` は「末尾から取る」その場の動詞として予約済みです → R-map-05（×）。
- **黙って捨てられる呼び出し（R-L0-05）**: 実測で、文として書いた `map.set(m, "z", 9)` と `map.remove(m, "b")` は、native と wasm のどちらでもエラーにならず、何も起きません（下の実測）。R-map-01 で `map.insert` を撤去すると、他言語の癖で `map.set(m, k, v)` を文で書く誤りが増えるので、R-L0-05 が先に入ることが R-map-01 の前提です。
- **family の穴**: map にはありません。family 全体のゲートは R-map-06 で置きます。

実測（2026-10-21、`almide 0.68.0 (dev, 437b8f127)`、native と `--target wasm` で同じ出力。`scratchpad/sr/probe-map/a.almd`）:

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

`map.to_list` は E002 になり、ヒントは「CHEATSHEET を見よ」だけで `map.entries` を示しません。

---

## 案

### R-map-01 `map.insert` と `map.delete`（その場書き換え）を撤去する — ○ 採用

- 内容:
  - `map.insert(mut m, k, v) -> Unit` と `map.delete(mut m, k) -> Unit` に `@deprecated(since=15, use=…)` を付け、epoch 16 で撤去します。
  - 置き換え先: `map.insert(m, k, v)` → `m[k] = v`（`m` が変数のとき）または `m = map.set(m, k, v)`。`map.delete(m, k)` → `m = map.remove(m, k)`。
  - `map.clear(mut m)` は残します（`clear` は R-L0-04 の動詞の集合に入っています）。
  - CHEATSHEET 193 行目の「in-place stdlib op」の列挙と、`llms.txt` の 2 か所（E032 の行など）から `map.insert` を外します。
- 根拠の L0 規則: R-L0-04（`insert`・`delete` はコピー側の動詞。L0 本文で名指しで決まっています）、R-L0-01、R-L0-05、R-L0-14
- (a) 9 言語では、名前で様式を区別しない言語（Rust、JS、Python）が `insert`・`set`・`delete` をその場書き換えに使い、書き換えのない言語（Gleam、Elixir、OCaml Map）が同じ動詞をコピーに使います。Almide はコピーが既定なので、後者に揃えます。その場で 1 キー書く操作は、Go・Python・Kotlin・Swift と同じく添字代入 `m[k] = v` で書けます。
- (b) native と wasm: 影響なし。添字代入と `map.set` + `@reuse` はすでに両方の経路で動きます（実測で同じ出力）。L0 の計測で、200 万件で `map.set` と `map.insert` と添字代入はどれも約 0.07 秒です。
- (c) いまは `set.insert` がコピーで `map.insert` がその場書き換えなので、動詞から様式が読めません。撤去後は「`insert` はいつもコピー」になります。ただし Rust の癖で `map.insert(m, k, v)` を書いた場合に、撤去後のヒントで `map.set` を文で書くと黙って捨てられるので、ヒントは添字代入を第一候補にします。
- (d) 撤去。互換: `@deprecated(since=15, use="m[k] = v")` と `@deprecated(since=15, use="map.remove")`、`proofs/dialect-epochs.toml` に `[[deprecation]]`（`removal_epoch = 16`）。`almide fix` の書き換え: 第 1 引数が変数の道（`m`、`r.m`）なら `m[k] = v` に、`map.delete(m, k)` は `m = map.remove(m, k)` にします。撤去後は `removed_spellings.rs` のヒントで同じ形を示します。
  - 使用数（`\bmap\.insert\b` / `\bmap\.delete\b` の出現数、定義と生成文書を含む上限値）: insert は spec 96 / tests 27 / docs 19 / stdlib 1（コメントだけ）、delete は spec 16 / tests 19 / docs 10 / stdlib 0。
  - 順序: R-L0-05（文で捨てた値をエラーにする）が同じ epoch 15 で先に入ることを条件にします。入らないと、移行で書かれた `map.set(m, k, v);` が黙って捨てられます。
- ゲート案: R-L0-G の `tests/stdlib_surface_conventions_test.rs` で、`mut` 引数を持つ map の公開関数が `clear` だけであることを検査します。`spec/stdlib/` に、添字代入と `map.remove` 再代入の native⇄wasm fixture を置きます。
- issue: #3928

### R-map-02 `map.get_or` を撤去して `??` に一本化する — ○ 採用

- 内容: `map.get_or(m, k, d)` に `@deprecated(since=15, use="??")`、epoch 16 で撤去。置き換えは `map.get(m, k) ?? d`。
- 根拠の L0 規則: R-L0-06（2）、R-L0-01、R-L0-14
- (a) 既定値版を持つのは Python `get(k, d)`、Elixir `Map.get(m, k, d)`、Kotlin `getOrDefault`、Swift `d[k, default:]` です。Rust・JS・Gleam は演算子か汎用の関数で書きます。多数派はなく、Almide は CHEATSHEET が教える `??` に寄せます。
- (b) 影響なし。`??` は ADR-0005 で `option.unwrap_or_else` に脱糖され、両 target で同じです。
- (c) `get_or` は既定値を必ず評価しますが、`??` は none のときだけ右辺を評価します。重い既定値を書く場合に `??` のほうが安全です。
- (d) 撤去。互換: `@deprecated(since=15, use="??")`、`removal_epoch = 16`、`#1107` の `option.unwrap_or` / `result.unwrap_or` と同じ窓です。`almide fix`: `map.get_or(m, k, d)` → `map.get(m, k) ?? d`（`|>` の形 `m |> map.get_or(k, d)` は `m |> map.get(k) ?? d`）。使用数: spec 161 / tests 29 / docs 21 / stdlib 0。spec の 161 件は `almide fix` で一括で書き換え、run/check/ast の台帳を再生成します。
- ゲート案: R-L0-G に「公開関数の名前が `_or` で終わらない」検査を足します（許容リストは減らす方向だけ）。
- issue: #3913

### R-map-03 `map.to_list` を足す — × 不採用

- 内容: 組の一覧を返す `map.to_list` は足しません。`map.entries` を唯一の綴りとして据え置きます。代わりに、E002 で `map.to_list` を書いたときのヒントを「`map.entries` を使う」に直します（いまは「CHEATSHEET を見よ」だけ、実測）。
- 根拠の L0 規則: R-L0-00（多数派なしは据え置き）、R-L0-01（同じ役割に綴りを 2 つ置かない）
- (a) `to_list` は Gleam・Elixir・OCaml・Kotlin の 4/9 です（OCaml は `bindings` も、Kotlin は `entries` も持ちます）。JS は `entries()`、Python は `items()`、Rust は `iter()` で、5/9 に届きません。
- (b) 影響なし。
- (c) `set.to_list` と `map.from_list` があるので、LLM は `map.to_list` を推測で書きます。足すと同義語になるので、ヒントで受け止めます。
- (d) 変更なし（診断のヒント文だけ）。
- 仕組み: E002 の「did you mean」表 `ALIASES`（`crates/almide-frontend/src/stdlib.rs:156-230`。未定義の名前のときだけ引かれます）に行を足します: `("map", "to_list", "map.entries")`、`("map", "put", "map.set")`、`("map", "items", "map.entries")`。R-map-01 の撤去後は `("map", "insert", "m[k] = v")`・`("map", "delete", "map.remove")` も足します（撤去した綴りのヒントは `removed_spellings.rs` が優先なので、どちらか一方に置きます）。
- 同じ表の古い行も直します:
  - `("map", "count", "map.len")`（164 行目）: `map.count` は述語を数える関数として公開されているので、この行は引かれません。しかし `count` = `len` という誤った対応を表に残しているので消します。
  - `("map", "new", "[:] (empty map literal)")`（223 行目）: `map.new` は公開中なので、この行も引かれない死んだ行です。消します。
- ゲート案: `tests/diagnostics/` に `map.to_list` の broken / fixed を置き、ヒントが `map.entries` を示すことを確かめます。`ALIASES` の各行について「左の名前が公開関数として存在しない」ことを検査するテストを足し、死んだ行が残らないようにします。
- issue: #3905（診断の改善として）

### R-map-04 `map.map` を `map_values` に改名する、またはコールバックを `(K, V)` にする — × 不採用

- 内容: `map.map(m, f: (V) -> B)` は名前もコールバックの形も据え置きます。
- 根拠の L0 規則: R-L0-00、R-L0-14（4. 意味の付け替えは禁止）
- (a) 値だけを変換する関数の綴りは、`map_values`（Gleam）・`mapValues`（Kotlin、Swift）が 3/9、`map`（OCaml）が 1/9 で、多数派はありません。コールバックがキーも受け取るのは Gleam `map_values` と Kotlin `mapValues`、OCaml `mapi` です。
- (c) `map.filter` などが `(K, V)` なのに `map.map` は `(V)` なので、`(k, v) =>` と書く誤りはあり得ます。ただしそのときは型エラー（引数の数の不一致）になり、黙って通りません。同じ名前で形を変えると、旧い形が学習データに残り続けます（R-L0-14 の 4）。
- (d) 変更なし。
- ゲート案: なし（型検査で止まります）。
- issue: なし

### R-map-05 取り出して消す `pop`、合わせ方つきの `merge_with`、キーを変換する `map_keys` を足す — × 不採用

- 内容: 3 つとも足しません。
- 根拠の L0 規則: R-L0-04（`pop` はその場で末尾から取る動詞）、L1 の追加基準（5/9 以上）、R-L0-00
- (a)
  - 取り出して消す: Rust・Python・Elixir・Kotlin・Swift の 5/9 が持ちます。ただし 4 言語はその場書き換えの戻り値として持つだけで、コピー版は Elixir `Map.pop` だけです。Almide では `map.get(m, k)` と `m = map.remove(m, k)` の 2 段で書けます。コピー版を `pop` と名付けると、R-L0-04 で「末尾から取るその場の動詞」と決めた `pop` の意味が割れます。
  - 合わせ方を関数で決める merge: Gleam `combine`・Elixir `merge/3`・OCaml `union`・Swift `merging` の 4/9 で、綴りも割れています。`map.fold(b, a, (acc, k, v) => map.upsert(acc, k, v, (old) => g(old, v)))` で書けます。
  - キーの変換: Kotlin `mapKeys` だけです。
- (b)(d) 変更なし。
- issue: なし

### R-map-06 高階関数の family を list・map・set の行列でゲートする — ○ 採用

- 内容: `map`、`filter`、`fold`、`any`、`all`、`count`、`find` の 7 つが list・map・set のすべてにあること、map では述語のコールバックが `(K, V)`、list と set では `(A)` であることを、interface JSON から検査します。map は変更なし。set に `count` と `find` を足すのは [L2-set R-set-01](L2-set.md) です。
- 根拠の L0 規則: R-L0-01、R-L0-10、CLAUDE.md の「API の family は行列で広げる」
- (a) 9 言語でも、コレクションの種類によって高階関数がそろわない言語（Gleam の set に `count`・`find` がない）は、書き手が試して外れる場所になっています。
- (b) 影響なし（検査だけ）。
- (c) 1 つのコレクションで覚えた形が、ほかのコレクションでもそのまま通ります。
- (d) 追加なし（ゲートだけ）。
- ゲート案: R-L0-G のテストに「コレクション family 行列」の節を足します。セルが欠けたら失敗し、意図した省略は許容リストに理由つきで書きます（減らす方向だけ）。
- issue: #3901

---

## L3: 関数表

使用数は 判定が据え置き以外の行だけに書いています（R-map-01、R-map-02 を参照）。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `map.new` | `map.new() -> Map[K, V]` | Rust `HashMap::new` / Gleam `dict.new` / Elixir `Map.new` / OCaml `empty` | 据え置き | — |
| `map.get` | `map.get(m: Map[K, V], key: K) -> Option[V]` | Rust `get` / Python `get` / JS `get` / Gleam `get`(Result) / OCaml `find_opt` / Kotlin `m[k]` | 据え置き | — |
| `map.get_or` | `map.get_or(m: Map[K, V], key: K, default: V) -> V` | Python `get(k, d)` / Elixir `get/3` / Kotlin `getOrDefault` / Rust・JS・Gleam — (演算子) | 撤去 | R-map-02 |
| `map.set` | `map.set(m: Map[K, V], key: K, value: V) -> Map[K, V]` | Gleam `insert`(copy) / Elixir `put` / OCaml `add` / Kotlin `m + (k to v)` | 据え置き | R-map-01（置き換え先） |
| `map.contains` | `map.contains(m: Map[K, V], key: K) -> Bool` | Rust `contains_key` / JS `has` / Gleam `has_key` / OCaml `mem` / Kotlin `containsKey` | 据え置き | — |
| `map.remove` | `map.remove(m: Map[K, V], key: K) -> Map[K, V]` | Rust `remove` / Gleam `delete` / Elixir `delete` / OCaml `remove` / Kotlin `m - k` | 据え置き | R-map-01（置き換え先） |
| `map.keys` | `map.keys(m: Map[K, V]) -> List[K]` | 全言語 `keys`（Go `maps.Keys`） | 据え置き | — |
| `map.values` | `map.values(m: Map[K, V]) -> List[V]` | 全言語 `values` | 据え置き | — |
| `map.len` | `map.len(m: Map[K, V]) -> Int` | Rust `len` / Gleam `size` / JS `size` / OCaml `cardinal` / Swift `count` | 据え置き | — (R-L0-11) |
| `map.entries` | `map.entries(m: Map[K, V]) -> List[(K, V)]` | JS `entries` / Kotlin `entries` / Gleam・Elixir・OCaml `to_list` / Python `items` | 据え置き | R-map-03 |
| `map.merge` | `map.merge(a: Map[K, V], b: Map[K, V]) -> Map[K, V]` | Gleam `merge` / Elixir `merge/2` / Python `a \| b` / Kotlin `a + b`（どれも右が勝つ） | 据え置き | — |
| `map.is_empty` | `map.is_empty(m: Map[K, V]) -> Bool` | Rust `is_empty` / Gleam `is_empty` / OCaml `is_empty` / Kotlin `isEmpty` | 据え置き | — |
| `map.from_list` | `map.from_list(pairs: List[(K, V)]) -> Map[K, V]` | Gleam `from_list` / OCaml `of_list` / Rust `from_iter` / Go `maps.Collect` | 据え置き | — (R-L0-07) |
| `map.map` | `map.map(m: Map[K, V], f: (V) -> B) -> Map[K, B]` | Gleam `map_values` / Kotlin・Swift `mapValues` / OCaml `map` | 据え置き | R-map-04 |
| `map.filter` | `map.filter(m: Map[K, V], f: (K, V) -> Bool) -> Map[K, V]` | Gleam `filter` / Elixir `filter` / OCaml `filter` / Kotlin `filter` / Rust `retain` | 据え置き | R-map-06 |
| `map.fold` | `map.fold(m: Map[K, V], init: A, f: (A, K, V) -> A) -> A` | Gleam `fold` / OCaml `fold` / Elixir `Enum.reduce` | 据え置き | R-map-06 |
| `map.any` | `map.any(m: Map[K, V], f: (K, V) -> Bool) -> Bool` | Kotlin `any` / OCaml `exists` / Rust `iter().any` | 据え置き | R-map-06 |
| `map.all` | `map.all(m: Map[K, V], f: (K, V) -> Bool) -> Bool` | Kotlin `all` / OCaml `for_all` / Rust `iter().all` | 据え置き | R-map-06 |
| `map.count` | `map.count(m: Map[K, V], f: (K, V) -> Bool) -> Int` | Kotlin `count` / Elixir `Enum.count/2` | 据え置き | R-map-06 |
| `map.find` | `map.find(m: Map[K, V], f: (K, V) -> Bool) -> Option[(K, V)]` | Rust `iter().find` / Elixir `Enum.find` / Swift `first(where:)` | 据え置き | R-map-06 |
| `map.update` | `map.update(m: Map[K, V], key: K, f: (V) -> V) -> Map[K, V]` | Rust `entry().and_modify` / OCaml `update` / Elixir `update!`（無ければ例外） | 据え置き | — |
| `map.upsert` | `map.upsert(m: Map[K, V], key: K, init: V, f: (V) -> V) -> Map[K, V]` | Elixir `update/4`（同じ意味）/ Rust `entry().and_modify(f).or_insert(v)` / Gleam `upsert`（`fn(Option(v))`） | 据え置き | — |
| `map.insert` | `map.insert(mut m: Map[K, V], key: K, value: V) -> Unit` | Rust `insert` / JS `set` / Python `d[k] = v`（どれも in-place） | 撤去 | R-map-01 |
| `map.delete` | `map.delete(mut m: Map[K, V], key: K) -> Unit` | Rust `remove` / JS `delete` / Go `delete`（in-place） | 撤去 | R-map-01 |
| `map.clear` | `map.clear(mut m: Map[K, V]) -> Unit` | Rust `clear` / Python `clear` / JS `clear` / Kotlin `clear` | 据え置き | — (R-L0-04 の動詞) |

L3 の行数: 25（追加なし）。判定の内訳: 据え置き 22、撤去 3（`get_or`、`insert`、`delete`）。

---

## 共通の判定との対応

list・set・option・result・string のページと揃えるための共通判定のうち、このページに関わるもの:

| 共通判定 | このページでの扱い |
|---|---|
| D1 `_or` の撤去 | R-map-02 |
| D2 `map.insert` / `map.delete` の撤去、`map.clear` は残す | R-map-01 |
| D3 高階 family と `map.to_list` × | R-map-06、R-map-03 |
| D8 互換の窓（since=15、removal 16） | R-map-01、R-map-02 |
| D9 R-L0-05 で止まる呼び出し | `map.set`・`map.remove`・`map.update`・`map.upsert`・`map.merge`・`map.filter` を文で書いたとき（実測で、いまは黙って捨てられます） |

## L0 への差し戻し

なし。
