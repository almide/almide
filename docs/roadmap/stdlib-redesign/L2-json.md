<!-- description: Stdlib redesign L2/L3 — json: only the text codec stays (parse, stringify, stringify_pretty); typed getters and JsonPath move to value; parse rejects trailing text -->
# L2/L3: json

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-27）・対になるページ [L2-value](L2-value.md)
> 出典: Rust —（標準に無い。serde_json は外部 https://docs.rs/serde_json ）・ Go https://pkg.go.dev/encoding/json ・ Python https://docs.python.org/3/library/json.html (F) ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/JSON ・ Gleam —（公式 pkg https://hexdocs.pm/gleam_json/ ）・ Elixir https://elixir.hexdocs.pm/JSON.html ・ OCaml —（標準に無い）・ Kotlin —（公式 pkg https://kotlinlang.org/api/kotlinx.serialization/kotlinx-serialization-json/ ）・ Swift https://developer.apple.com/documentation/foundation/jsonserialization
> 実測: `almide 0.68.0 (dev, 437b8f127)`、native と `--target wasm`。プローブは scratchpad の `probe-json/p1.almd`。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| テキスト → 値 | `json.parse` | pkg `from_str` | `Unmarshal` | `loads` (F) | `JSON.parse` | pkg `json.parse` | `JSON.decode` | — | pkg `Json.parseToJsonElement` | `JSONSerialization.jsonObject` |
| 値 → テキスト（詰める） | `json.stringify`、`value.stringify`（同じ出力） | pkg `to_string` | `Marshal` | `dumps` (F) | `JSON.stringify` | pkg `json.to_string` | `JSON.encode!` | — | pkg `toString()` | `JSONSerialization.data` |
| 値 → テキスト（字下げ） | `json.stringify_pretty` | pkg `to_string_pretty` | `MarshalIndent` | `dumps(indent=)` (F) | `JSON.stringify(v, null, 2)` | — | — | — | pkg `prettyPrint` | `.prettyPrinted` |
| 後ろに余計な文字があるテキスト | **受け取る**（`"1 garbage"` → `1`） | pkg err | err | err（`Extra data`）(F) | SyntaxError | pkg err | err | — | pkg err | err |
| キーで型つきの値を取り出す | `json.get_string` ほか 5 | pkg `v["k"].as_str()` | — | `d.get("k")` | `o.k` | — | — | — | pkg `jsonObject["k"]?.jsonPrimitive` | — |
| 深い位置を道筋で読む・書く | `json.root`/`field`/`index` + `get_path`/`set_path`/`remove_path` | pkg `pointer` | — | — | — | — | `get_in`/`put_in`/`pop_in` + `Access.key`/`Access.at` (F) | — | — | — |

### 完全性の規則

1. **`json` はテキストとの変換だけ**（R-L1-27）: `parse`（テキスト → `Value`）、`stringify`、`stringify_pretty`（`Value` → テキスト）。値の木を組み立てる・読む関数は、テキストが関わらない限り `value` に置きます。
2. `parse` は RFC 8259 の文法どおりで、文書の後ろの空白以外の文字は err にします。
3. `stringify` と `stringify_pretty` は同じ値の規則（非有限の Float は `null`、`1.0` は `1`）で書きます。

### L2 の判定

- 置き場所: `get_string`・`get_int`・`get_float`・`get_bool`・`get_array`、`root`・`field`・`index`・`get_path`・`set_path`・`remove_path`、`to_map` の 12 関数は、テキストに触れない「木の操作」です → R-json-01（R-L1-27）。
- 重複: `value.stringify` と `json.stringify` は同じ出力です（実測: 両 target で `eq true`）。`value` 側で撤去します（[L2-value](L2-value.md) の R-value-01）。
- 受理範囲: `json.parse("1 garbage")` → `ok(1)`、`json.parse("{\"a\":1} {")` → `ok({"a":1})`（両 target で実測）。9 言語のどれも受け取りません → R-json-02。
- `json.field(path, name)`（道筋を作る）と `value.field(v, key)`（値を読む）は、同じ綴りで意味が違います。移すときに名前を分けます（R-json-01）。

## 案

### R-json-01 型つきの取り出しと道筋の操作を `value` に移す — ○ 採用

- 内容（新しい綴りは [L2-value](L2-value.md) の R-value-02・R-value-03 で決めます）:

  | 撤去する綴り | 移す先 |
  |---|---|
  | `json.get_string`・`get_int`・`get_float`・`get_bool`・`get_array` | `value.get_string`・`get_int`・`get_float`・`get_bool`・`get_array`（同じシグネチャ） |
  | `json.root()` | `value.root()` |
  | `json.field(path, name)` | `value.key(path, name)`（`value.field` と名前を分ける） |
  | `json.index(path, i)` | `value.index(path, i)`（負の添字は範囲外。末尾から数えない） |
  | `json.get_path`・`set_path`・`remove_path` | `value.get_path`・`set_path`・`remove_path`（同じシグネチャ） |
  | `type json.JsonPath` | `type value.Path` |
  | `json.to_map(j)` → `Option[Map[String, String]]` | `value.as_object(v)` と `map.from_list`（値は `Value` のまま） |

- 根拠の L0 規則: R-L1-27、R-L0-02（主語は `Value` と道筋。テキストではない）、R-L0-00 の条件 1（`field` の意味が json と value で違う）、R-L0-12（負の添字は範囲外）、R-L0-14。
- (a) テキストの変換と値の木を分けるのは Kotlin（`Json` と `JsonElement`）、Gleam（`gleam_json` と `dynamic`）です。道筋の部品の名前は Elixir の `Access.key`・`Access.at`(F) に近い形です。
- (b) 純粋な計算です。本体は変えずに移します。
- (c) `value` は自動 import なので、`import json` を書かずに使えます。いまは `json.get_int` を使うためだけに `import json` が要ります。
- (d) 撤去 12・型の改名 1。各関数に移す先を指す `@deprecated(since=15, use="value.get_int")` など（`use` は上の表の移す先）を付け、`removal_epoch = 16` で撤去します。`almide fix` は 1 対 1 で書き換え、使われなくなった `import json` を消します。`json.index` に負の添字を渡している呼び出しと、`json.to_map` は形が変わるので、ヒントで示します。使用数: `get_*` 5 関数 spec 54 / tests 0 / docs 52 / stdlib 3 / research 19、道筋 6 関数 spec 262 / tests 20 / docs 76 / stdlib 0、`to_map` spec 4 / tests 0 / docs 15 / stdlib 3。
- ゲート案: R-L0-G の R-L0-02 の検査で、`json` の公開関数が「`String` を受け取るか返す」ことを確かめます。
- issue: #3953

### R-json-02 `parse` は文書の後ろの余計な文字を err にする — ○ 採用

- 内容: `json.parse` が、値の後ろに空白以外の文字があれば `err("unexpected trailing text at pos N")` を返します。名前と型は変えません。
- 根拠の L0 規則: R-L0-06 の 4（失敗しうる変換は、失敗を値で隠さない）。定義域を狭める変更で、正しい JSON に対する答えは変わらないので、R-L0-14 の 4 の「同じ名前で別の結果」には当たりません（L0 の「定義域を狭めても epoch にしない」扱い）。
- (a) 9 言語のうち標準・公式 pkg に JSON を持つ 7 言語すべてが拒みます（Python は `Extra data`(F)）。
- (b) 純粋な計算です。両 target の parser は同じ self-host 実装なので、同時に変わります。
- (c) いまは、2 つの JSON をつないだテキストや、途中で切れたログ行を、最初の値だけで黙って成功にします。
- (d) 意味変更（定義域を狭める）。epoch の `breaks` には入れず、dialect epoch 15 の `notes` に書きます。spec・tests・下流で、後ろに文字が残るテキストを `parse` している箇所を、変更の前に数えます（`json.parse` 自体の使用数: spec 203 / tests 41 / docs 77 / stdlib 3 / research 29）。
- ゲート案: `spec/stdlib/json_test.almd` に「後ろに空白だけ → ok」「後ろに文字 → err」の 2 件を両 target で置きます。JSONTestSuite の `n_structure_trailing_*` 系を differential fuzz の種に入れます。
- issue: #3921

### R-json-03 `stringify` と `stringify_pretty` を据え置く — ○ 採用（変更なし）

- 内容: 2 関数とも据え置きます。`stringify_pretty` の引数名 `j` は `v` に揃えます（シグネチャの意味は変わらない、文書だけの変更）。
- 根拠の L0 規則: R-L0-00（`stringify` は JS と同じ綴り。多数派はないが衝突もない）。
- issue: なし

### R-json-04 `parse` の入れ子の深さに上限を置く — ○ 採用

- いま: `json.parse` は再帰下降で、入れ子の深さに上限がありません（`runtime/rs/src/json.rs:47-223`、`stdlib/json_parse.almd`）。`[[[[…]]]]` を深く重ねた文字列は、スタックの溢れの abort（C-196）で終わります。
- 案: 配列と object の入れ子が 1000 段を超えたら `err("json: nesting deeper than 1000")` を返します。
- 根拠の L0 規則: R-L0-47 の 2。上限を変える関数は置きません（R-L0-47 の 3 の例外）。比べた言語のうち、深さを変えられるのは Rust の `serde_json` の `disable_recursion_limit` だけで、それも上限を外すだけです。
- (a) Go `encoding/json`（10000）、Rust `serde_json`（128）、Python `json`（再帰の上限、既定でおよそ 1000）。
- (d) 意味変更（1000 段を超える入れ子が abort から err になる）。いま 1000 段を超えて成功している入力は、native のスタックの大きさ次第で、すでに abort しうる範囲です。
- issue: #4020

### 関連: Codec で型に decode するとき（R-L0-48）

`json.parse` の値を型に読み込むのは、Codec が導出する `T.decode` と `T.decode_strict` です（json のモジュールの関数ではありません）。知らないキーを拒むのは `decode_strict`、入力から読ませないフィールドは `@skip_decode` です。規則は L0 の R-L0-48 にあります。

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `parse` | `json.parse(text: String) -> Result[Value, String]` | JS `JSON.parse` / Python `loads` (F) / Go `Unmarshal` / Elixir `JSON.decode` | 意味変更（後ろの余計な文字は err。入れ子が 1000 段を超えたら err） | R-json-02、R-json-04 |
| `stringify` | `json.stringify(v: Value) -> String` | JS `JSON.stringify` / Python `dumps` (F) / Go `Marshal` / Elixir `JSON.encode!` | 据え置き | R-json-03 |
| `stringify_pretty` | `json.stringify_pretty(j: Value) -> String` | Go `MarshalIndent` / Python `dumps(indent=2)` (F) / serde `to_string_pretty` | 据え置き | R-json-03 |
| `get_string` | `json.get_string(j: Value, key: String) -> Option[String]` | Kotlin pkg `jsonPrimitive.contentOrNull` / serde `as_str()` | 撤去 → `value.get_string` | R-json-01 |
| `get_int` | `json.get_int(j: Value, key: String) -> Option[Int]` | serde `as_i64()` / Kotlin pkg `intOrNull` | 撤去 → `value.get_int` | R-json-01 |
| `get_float` | `json.get_float(j: Value, key: String) -> Option[Float]` | serde `as_f64()` / Kotlin pkg `doubleOrNull` | 撤去 → `value.get_float` | R-json-01 |
| `get_bool` | `json.get_bool(j: Value, key: String) -> Option[Bool]` | serde `as_bool()` / Kotlin pkg `booleanOrNull` | 撤去 → `value.get_bool` | R-json-01 |
| `get_array` | `json.get_array(j: Value, key: String) -> Option[List[Value]]` | serde `as_array()` / Kotlin pkg `jsonArray` | 撤去 → `value.get_array` | R-json-01 |
| `root` | `json.root() -> JsonPath` | serde `pointer("")` | 撤去 → `value.root` | R-json-01 |
| `field` | `json.field(path: JsonPath, name: String) -> JsonPath` | Elixir `Access.key` (F) | 撤去 → `value.key` | R-json-01 |
| `index` | `json.index(path: JsonPath, i: Int) -> JsonPath` | Elixir `Access.at` (F) | 撤去 → `value.index`（負の添字は範囲外） | R-json-01 |
| `get_path` | `json.get_path(j: Value, path: JsonPath) -> Option[Value]` | Elixir `get_in` (F) / serde `pointer` | 撤去 → `value.get_path` | R-json-01 |
| `set_path` | `json.set_path(j: Value, path: JsonPath, value: Value) -> Result[Value, String]` | Elixir `put_in` (F) / serde `pointer_mut` | 撤去 → `value.set_path` | R-json-01 |
| `remove_path` | `json.remove_path(j: Value, path: JsonPath) -> Value` | Elixir `pop_in` (F) | 撤去 → `value.remove_path` | R-json-01 |
| `to_map` | `json.to_map(j: Value) -> Option[Map[String, String]]` | serde `as_object()` / Kotlin pkg `jsonObject` | 撤去 → `value.as_object(v) \|> result.map(map.from_list)` | R-json-01 |
