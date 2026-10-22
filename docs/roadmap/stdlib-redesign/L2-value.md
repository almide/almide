<!-- description: Stdlib redesign L2/L3 — value: the dynamic tree owns building and reading (typed getters and paths move in from json, as_object fills the as_ family), and loses stringify, keys and the key-case converters -->
# L2/L3: value

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-27）・対になるページ [L2-json](L2-json.md)
> 出典: Rust —（標準に無い。serde_json の `Value` は外部 https://docs.rs/serde_json/latest/serde_json/enum.Value.html ）・ Go https://pkg.go.dev/encoding/json （`any` に解析）・ Python https://docs.python.org/3/library/json.html (F)（dict/list に解析）・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/JSON ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/dynamic.html ・ https://gleam-stdlib.hexdocs.pm/gleam/dynamic/decode.html ・ Elixir https://elixir.hexdocs.pm/Access.html (F) ・ OCaml —（標準に無い）・ Kotlin —（公式 pkg https://kotlinlang.org/api/kotlinx.serialization/kotlinx-serialization-json/kotlinx.serialization.json/-json-element/ ）・ Swift https://developer.apple.com/documentation/foundation/jsonserialization
> 実測: `almide 0.68.0 (dev, 437b8f127)`、native と `--target wasm`。プローブは scratchpad の `probe-json/`。

読み方: `?`（読み: はてな。`Result` を `Option` に変える）、`??`（読み: ナル合体）、`|>`（読み: パイプ）。

## L2: 役割の比較

| 役割 | Almide | Rust（serde_json, 外部） | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin（pkg） | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 値を作る | `null`、`bool`、`int`、`float`、`str`、`array`、`object` | `json!`、`Value::String` | `any` のリテラル | dict/list のリテラル | オブジェクトリテラル | pkg `json.string` など | 項そのもの | — | `JsonPrimitive`、`buildJsonObject` | `Any` |
| 種類を確かめて取り出す（失敗に理由） | `as_string`・`as_int`・`as_float`・`as_bool`・`as_array`（Result） | `as_str`・`as_i64`・`as_object`（Option） | 型アサーション `v.(string)` | `isinstance` | `typeof` | `decode.string` ほか（Result） | パターン一致 | — | `jsonPrimitive.content`、`jsonObject` | `as? String` |
| オブジェクトとして取り出す | **—**（`keys` + `field` で組み立てる） | `as_object` | `v.(map[string]any)` | dict | `Object.entries` | `decode.dict` | map | — | `jsonObject` | `as? [String: Any]` |
| キーで子を読む | `field`（Result） | `get`（Option） | `m["k"]` | `d.get("k")` | `o.k` | `decode.field` | `m["k"]` / `Access.key` (F) | — | `obj["k"]` | `d["k"]` |
| キーで型つきの値を読む | `json.get_int` ほか（json にある） | `v["k"].as_i64()` | — | — | — | `decode.field("k", decode.int)` | — | — | `obj["k"]?.jsonPrimitive?.intOrNull` | — |
| キーの一覧 | `keys`（オブジェクトでなければ `[]`） | `as_object()?.keys()` | `maps.Keys` | `d.keys()` | `Object.keys` | — | `Map.keys` | — | `obj.keys` | `d.keys` |
| 深い位置を道筋で読む・書く | `json.get_path` ほか（json にある） | `pointer` / `pointer_mut` | — | — | — | `decode.at` | `get_in`/`put_in`/`pop_in` + `Access.key`/`Access.at` (F) | — | — | — |
| 部分を選ぶ・除く・重ねる | `pick`、`omit`、`merge` | — | — | dict 内包 | スプレッド | — | `Map.take`/`drop`/`merge` | — | — | — |
| テキストにする | `stringify`（`json.stringify` と同じ） | `to_string` | `Marshal` | `dumps` (F) | `JSON.stringify` | pkg `to_string` | `JSON.encode!` | — | `toString` | `JSONSerialization.data` |
| キー名の書き方を変える | `to_camel_case`、`to_snake_case`（1 段目だけ） | serde の `rename_all`（型の属性） | — | — | — | — | — | — | `JsonNamingStrategy` | `keyDecodingStrategy` |

### 完全性の規則

1. **`value` は値の木の組み立てと読み取り**（R-L1-27）。テキストとの変換は `json` だけが持ちます。
2. **種類の族**: 中身を持つ種類 k ∈ {`bool`、`int`、`float`、`string`、`array`、`object`} のそれぞれに、次の 3 セルを置きます。
   - 構築子: `bool`、`int`、`float`、`str`、`array`、`object`（いまの綴り）
   - `as_<k>(v) -> Result[T, String]`: 種類が違えば err（理由つき）
   - `get_<k>(v, key) -> Option[T]`: キーが無い・オブジェクトでない・種類が違うなら none
   `object` の `T` は、構築子 `object` が受け取る `List[(String, Value)]` と同じ形です（順序と重複を保つ）。`null` は中身を持たないので `as_`・`get_` を持たず、`v == value.null()` で確かめます。
3. **道筋（`Path`）**: `root()`、`key(p, name)`、`index(p, i)` で作り、`get_path`（Option）、`set_path`（Result）、`remove_path`（無ければそのまま）で使います。負の添字は範囲外です（R-L0-12）。
4. **失敗を番兵で隠さない**（R-L0-06 の 4）: オブジェクトでない値に対する読みは Option か Result にします。`pick`・`omit`・`merge` は読みではなく変換なので、オブジェクトでなければそのまま返す今の形を保ちます。
5. **意図した抜け**: 構築子の `str` は `as_string`・`get_string` と綴りが違いますが、改名しません（R-L0-00: Almide の中で同じ役割の衝突ではなく、多数派もない）。

### L2 の判定

- 重複: `value.stringify` ≡ `json.stringify`（実測: 両 target で同じ文字列）→ R-value-01。
- 族の穴: `as_object` が無い。そのためオブジェクトを回すには `keys` と `field` を組み合わせることになり、`json.to_map` のような専用の関数が生まれています → R-value-02。
- 置き場所: `json.get_*` と道筋の操作は木の操作です → R-value-02・R-value-03（[L2-json](L2-json.md) の R-json-01）。
- 番兵: `value.keys(value.int(1))` は `[]` を返します（実測）。オブジェクトでないことと、空のオブジェクトの区別がつきません（R-L0-06 の 4）→ R-value-04。
- 一つの用途だけの関数: `to_camel_case`・`to_snake_case` は 1 段目のキーだけを変え、`userID` は `user_i_d` になります（実測）。CHEATSHEET の「wire の型は wire の名前を使う。名前を変える層は持たない」と食い違います → R-value-05（#3852）。

## 案

### R-value-01 `value.stringify` を撤去する — ○ 採用

- 内容: `value.stringify(v)` を撤去し、`json.stringify(v)` に寄せます。
- 根拠の L0 規則: R-L1-27（テキストへの変換は json）、R-L0-01（同じ出力の 2 つの綴り）、R-L0-14。
- (b) 同じ出力です（`1.0` → `1`、非有限 → `null`。両 target で実測）。
- (c) `value` は自動 import、`json` は import が要るので、置き換えでは `import json` を足します。
- (d) 撤去。`@deprecated(since=15, use="json.stringify")`、`removal_epoch = 16`。`almide fix` は 1 対 1 で書き換え、`import json` を足します。使用数: spec 50 / tests 2 / docs 27 / stdlib 5 / research 2。
- ゲート案: #3851 F の同義語ゲート規則 5（モジュールをまたぐ同じ本体）。
- issue: #3953

### R-value-02 種類の族を閉じる: `as_object` と `get_<種類>` を置く — ○ 採用

- 内容:
  - 追加: `as_object(v) -> Result[List[(String, Value)], String]`。
  - `json` から移す: `get_string`・`get_int`・`get_float`・`get_bool`・`get_array`（シグネチャは同じ。`json.get_int` と同じく Float は Int にしない）。
  - 追加: `get_object(v, key) -> Option[List[(String, Value)]]`（族の最後のセル）。
- 根拠の L0 規則: R-L1-27、R-L0-02、R-L0-06（キーでの読みは `get` で Option。理由つきの失敗は Result）、CLAUDE.md の「族は行列で広げる」。
- (a) オブジェクトとして取り出す関数は serde の `as_object`、Kotlin pkg の `jsonObject`、Gleam の `decode.dict` が持ちます。
- (b) 純粋な計算です。
- (c) `as_array` を知っている LLM は `as_object` を書きます。いまはそれが未定義エラーになります。
- (d) 追加 2・移動 5（json 側の撤去と組。json 側の `@deprecated(since=15, use="value.get_<種類>")` と `removal_epoch = 16` は [L2-json](L2-json.md) の R-json-01）。
- ゲート案: 族ゲート `tests/stdlib_value_family_test.rs` が、規則 2 の 6 種類 × 3 セルの名前と型を interface JSON と突き合わせます。
- issue: #3955

### R-value-03 道筋の操作を `json` から移す — ○ 採用

- 内容: `type value.Path`、`value.root() -> Path`、`value.key(p: Path, name: String) -> Path`、`value.index(p: Path, i: Int) -> Path`、`value.get_path(v, p) -> Option[Value]`、`value.set_path(v, p, x) -> Result[Value, String]`、`value.remove_path(v, p) -> Value`。
  - `json.field` は `value.key` にします。`value.field(v, key)`（値を読む）と同じ綴りにはできないためです。
  - `value.index` の負の添字は範囲外です（いまの `json.index` は末尾から数えます。実測: `index(root, -1)` で `some(3)`）。名前が新しくなるので、R-L0-14 の 4 には当たりません。
- 根拠の L0 規則: R-L1-27、R-L0-00 の条件 1、R-L0-12、R-L0-14。
- (a) 道筋の部品を `key` と `at`/`index` で書くのは Elixir（`Access.key`、`Access.at`）(F)。
- (b) 純粋な計算です。
- (d) 追加 6・型 1（json 側の撤去 6 と組。json 側は `@deprecated(since=15, use="value.root")` などを付け、`removal_epoch = 16` で撤去）。使用数（json の 6 関数）: spec 262 / tests 20 / docs 76。`almide fix` は 1 対 1 で書き換えます（`json.index` の負の添字だけはヒント）。
- ゲート案: R-L0-12 の境界行列に `value.index` を入れます。
- issue: #3953

### R-value-04 `keys` を撤去する — ○ 採用

- 内容: `value.keys(v) -> List[String]`（オブジェクトでなければ `[]`）を撤去します。置き換えは `value.as_object(v)` です。
- 根拠の L0 規則: R-L0-06 の 4（失敗しうる読みは番兵を返さない）、R-L0-01（同じ読みに綴りを 2 つ置かない）。R-L0-14 の 4 は `keys` の戻り型を `Result` に変えることを許しますが、そうすると族のセル `as_object` と同じ読みが 2 つの綴りになります。そこで `keys` は残さず、`as_object` に寄せます。
- (a) キーの一覧を取る前に「オブジェクトであること」を型で確かめるのは serde（`as_object()?.keys()`）と Kotlin pkg（`jsonObject.keys`）です。
- (c) いまの形では、オブジェクトでない値を渡したことに気づけません。また `keys` で回して `field` で読む書き方は、キーごとに探し直します。`as_object` なら組を 1 回で回せます。
- (d) 撤去。`@deprecated(since=15, use="value.as_object")`、`removal_epoch = 16`。`almide fix` の書き換え（意味が同じになる形）: `value.keys(v)` → `(value.as_object(v) ?? []) |> list.map(((k, _)) => k)`。ヒントには「`as_object` の組を直接回すほうがよい」と出します。使用数: spec 25 / tests 1 / docs 10 / stdlib 0 / research 1。
- issue: #3954

### R-value-05 `to_camel_case` と `to_snake_case` を撤去する — ○ 採用

- 内容: #3852 の案どおり、2 関数を撤去します。置き換えの関数は用意しません。
- 根拠の L0 規則: R-L0-01、R-L0-14。CHEATSHEET の Rule 1（wire の型は wire の名前を使う）。
- (a) 9 言語で、名前の書き方の変換を値の木の関数として持つものはありません（serde・Kotlin・Swift は型の直列化の設定）。
- (c) 1 段目だけ変える、`userID` → `user_i_d` という挙動は、名前から推測できません（実測）。
- (d) 撤去。`@deprecated(since=15, use="Codec")`（置き換えの関数はない）、`removal_epoch = 16`。機械的な書き換えはできないので、ヒントで「`Codec` の型で wire の名前を書く」と出します。使用数（2 関数）: spec 14 / tests 0 / docs 12 / stdlib 2。
- issue: #3852（既存）

### R-value-06 構築子 `str` を `string` に改名する — × 不採用

- 理由: `str`（作る）と `as_string`（読む）は役割が違うので、R-L0-00 の条件 1（同じ役割に 2 つの綴り）に当たりません。多数派の綴りもありません。使用数が多く（spec 111 / tests 20 / docs 29 / stdlib 4 / research 11）、改名で新旧が混ざる期間の害のほうが大きいと判定します。
- issue: なし

### R-value-07 `value.field` を `value.get`（Option）にして、`get_<種類>` の族の頭にする — ○ 採用

- いま: `fn value.field(v: Value, key: String) -> Result[Value, String]` と、`fn value.get_int(v: Value, key: String) -> Option[Int]` ほか 6 つ。`value.get` はありません。
- 案: `fn value.get(v: Value, key: String) -> Value?` を足し、`value.field` を撤去します。キーで読む族は `get` と `get_<種類>` の 7 つで、すべて Option です。理由が要るときは `as_<種類>`（Result）を使います。
- 根拠の L0 規則: R-L0-06 の 1（失敗しうる取り出しは無印の名前で Option。`list.get`・`map.get`・`bytes.get`）、R-L0-01（キーで読む役割に `field` と `get_` の 2 つの動詞）。
- (a) キーで子を読む関数は Rust の JSON ライブラリの `Value::get`（Option）、Python `dict.get`、Elixir `Map.get`。`field` は Gleam の `decode.field` だけです。
- (c) 実測: `value.get(v, "a")` は E002 で、ヒントは「もしかして `value.int`?」です。LLM は `map.get` と同じ綴りで書きます。
- (d) 追加 1・撤去 1。`@deprecated(since=15, use="value.get")`、epoch 16 で撤去。`almide fix` が `value.field(v, k)` を `value.get(v, k)` に書き換え、`!` で伝搬していた箇所にはヒント「理由が要るなら `value.get(v, k) ?? …` か `match`」を出します。使用数: spec 24 / tests 17 / docs 14 / stdlib 24。
- 完全性の規則: `get(v, key)` と `get_<k>(v, key)` はすべて Option、`as_<k>(v)` はすべて Result です。
- ゲート案: R-value-02 の族ゲートに `get` のセルを足します。
- issue: #3971

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `array` | `value.array(items: List[Value]) -> Value` | serde `Value::Array` / Kotlin pkg `JsonArray` / gleam_json `json.array` | 据え置き | R-value-02 |
| `as_array` | `value.as_array(v: Value) -> Result[List[Value], String]` | serde `as_array` / Gleam `decode.list` / Kotlin pkg `jsonArray` | 据え置き | R-value-02 |
| `as_bool` | `value.as_bool(v: Value) -> Result[Bool, String]` | serde `as_bool` / Gleam `decode.bool` | 据え置き | R-value-02 |
| `as_float` | `value.as_float(v: Value) -> Result[Float, String]` | serde `as_f64` / Gleam `decode.float` | 据え置き | R-value-02 |
| `as_int` | `value.as_int(v: Value) -> Result[Int, String]` | serde `as_i64` / Gleam `decode.int` | 据え置き | R-value-02 |
| `as_string` | `value.as_string(v: Value) -> Result[String, String]` | serde `as_str` / Gleam `decode.string` | 据え置き | R-value-02 |
| `bool` | `value.bool(b: Bool) -> Value` | serde `Value::Bool` / gleam_json `json.bool` | 据え置き | R-value-02 |
| `float` | `value.float(f: Float) -> Value` | gleam_json `json.float` / Kotlin pkg `JsonPrimitive` | 据え置き | R-value-02 |
| `int` | `value.int(n: Int) -> Value` | gleam_json `json.int` / Kotlin pkg `JsonPrimitive` | 据え置き | R-value-02 |
| `merge` | `value.merge(a: Value, b: Value) -> Value` | Elixir `Map.merge` / JS `{...a, ...b}` / Python `a \| b` | 据え置き | — |
| `null` | `value.null() -> Value` | serde `Value::Null` / gleam_json `json.null` / Kotlin pkg `JsonNull` | 据え置き | — |
| `object` | `value.object(pairs: List[(String, Value)]) -> Value` | gleam_json `json.object` / Kotlin pkg `buildJsonObject` | 据え置き | R-value-02 |
| `omit` | `value.omit(v: Value, keys: List[String]) -> Value` | Elixir `Map.drop` / lodash `omit` | 据え置き | — |
| `pick` | `value.pick(v: Value, keys: List[String]) -> Value` | Elixir `Map.take` / lodash `pick` | 据え置き | — |
| `str` | `value.str(s: String) -> Value` | gleam_json `json.string` / serde `Value::String` | 据え置き | R-value-06 |
| `stringify` | `value.stringify(v: Value) -> String` | JS `JSON.stringify` / Python `dumps` (F) | 撤去 → `json.stringify` | R-value-01 |
| `to_camel_case` | `value.to_camel_case(v: Value) -> Value` | Kotlin pkg `JsonNamingStrategy` / Swift `keyEncodingStrategy`（型の設定） | 撤去 | R-value-05 |
| `to_snake_case` | `value.to_snake_case(v: Value) -> Value` | 同上 | 撤去 | R-value-05 |
| `field` | `value.field(v: Value, key: String) -> Result[Value, String]` | serde `get`（Option） / Gleam `decode.field` / Elixir `Access.key` (F) | 撤去 → `value.get`（Option） | R-value-07 |
| `keys` | `value.keys(v: Value) -> List[String]` | serde `as_object()?.keys()` / JS `Object.keys` / Python `d.keys()` | 撤去 → `as_object` | R-value-04 |
| `as_object` | `value.as_object(v: Value) -> Result[List[(String, Value)], String]`（案） | serde `as_object` / Kotlin pkg `jsonObject` / Gleam `decode.dict` | 追加 | R-value-02 |
| `get_string` | `value.get_string(v: Value, key: String) -> Option[String]`（案） | serde `v["k"].as_str()` | 追加（`json.get_string` から移す） | R-value-02 |
| `get_int` | `value.get_int(v: Value, key: String) -> Option[Int]`（案） | serde `v["k"].as_i64()` | 追加（`json.get_int` から移す） | R-value-02 |
| `get_float` | `value.get_float(v: Value, key: String) -> Option[Float]`（案） | serde `v["k"].as_f64()` | 追加（`json.get_float` から移す） | R-value-02 |
| `get_bool` | `value.get_bool(v: Value, key: String) -> Option[Bool]`（案） | serde `v["k"].as_bool()` | 追加（`json.get_bool` から移す） | R-value-02 |
| `get_array` | `value.get_array(v: Value, key: String) -> Option[List[Value]]`（案） | serde `v["k"].as_array()` | 追加（`json.get_array` から移す） | R-value-02 |
| `get_object` | `value.get_object(v: Value, key: String) -> Option[List[(String, Value)]]`（案） | serde `v["k"].as_object()` | 追加 | R-value-02 |
| `root` | `value.root() -> Path`（案） | serde `pointer("")` | 追加（`json.root` から移す） | R-value-03 |
| `key` | `value.key(p: Path, name: String) -> Path`（案） | Elixir `Access.key` (F) | 追加（`json.field` から移す） | R-value-03 |
| `index` | `value.index(p: Path, i: Int) -> Path`（案） | Elixir `Access.at` (F) | 追加（`json.index` から移す。負は範囲外） | R-value-03 |
| `get_path` | `value.get_path(v: Value, p: Path) -> Option[Value]`（案） | Elixir `get_in` (F) / serde `pointer` | 追加（`json.get_path` から移す） | R-value-03 |
| `set_path` | `value.set_path(v: Value, p: Path, x: Value) -> Result[Value, String]`（案） | Elixir `put_in` (F) / serde `pointer_mut` | 追加（`json.set_path` から移す） | R-value-03 |
| `remove_path` | `value.remove_path(v: Value, p: Path) -> Value`（案） | Elixir `pop_in` (F) | 追加（`json.remove_path` から移す） | R-value-03 |
| `get` | `value.get(v: Value, key: String) -> Option[Value]`（案） | Rust JSON `Value::get` / Python `dict.get` / Elixir `Map.get` | 追加 | R-value-07 |
