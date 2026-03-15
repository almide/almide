# Codec Implementation Plan [ACTIVE]

## 3層モデル

```
Layer 1: Codec (コンパイラ)     T ←→ Value
Layer 2: Format (ライブラリ)    Value ←→ String/Bytes
Layer 3: User code              T ←→ String (パイプで合成)
```

```
encode: T ──.encode()──▶ Value ──json.stringify──▶ String
                               ──yaml.stringify──▶ String
                               ──toml.stringify──▶ String

decode: String ──json.parse──▶ Value ──T.decode()──▶ Result[T, E]
        String ──yaml.parse──▶ Value ──T.decode()──▶ Result[T, E]

transform: Value ──rename_keys──▶ Value  (naming strategy)
           Value ──set_path──▶ Value     (局所操作)
           Value ──json→yaml──▶ Value    (フォーマット変換、型不要)
```

**型はフォーマットを知らない。フォーマットは型を知らない。Value が唯一の接点。**

## Value 型 (universal data model)

```almide
// value module で定義
type Value =
  | Null
  | Bool(Bool)
  | Int(Int)
  | Float(Float)
  | Str(String)
  | Array(List[Value])
  | Object(List[(String, Value)])
```

variant constructor は `value` module で qualified:
```almide
value.Null
value.Str("hello")
value.Object([("name", value.Str("Alice"))])
```

型名衝突 (`String`, `Int` 等) は module スコープで解決。`V` prefix は不要。

JSON / YAML / TOML / msgpack の data model は全てこれに写像できる。
TOML の datetime は `Str("2024-01-15T10:30:00Z")` として格納、toml.stringify が ISO 8601 を検出して TOML datetime に変換。

### Value アクセサ関数 (stdlib: value module)

```almide
// フィールドアクセス (Result を返す — ? で連鎖可能)
fn value.field(v: Value, key: String) -> Result[Value, String]
fn value.index(v: Value, i: Int) -> Result[Value, String]

// 型変換 (Result を返す)
fn value.as_string(v: Value) -> Result[String, String]
fn value.as_int(v: Value) -> Result[Int, String]
fn value.as_float(v: Value) -> Result[Float, String]
fn value.as_bool(v: Value) -> Result[Bool, String]
fn value.as_array(v: Value) -> Result[List[Value], String]
fn value.as_object(v: Value) -> Result[List[(String, Value)], String]

// 構築ショートカット
fn value.str(s: String) -> Value = Str(s)
fn value.int(n: Int) -> Value = Int(n)
fn value.object(pairs: List[(String, Value)]) -> Value = Object(pairs)
fn value.array(items: List[Value]) -> Value = Array(items)
```

`value.field` は key が存在しなければ `err("missing field 'name'")` を返す。
`value.as_string` は型が一致しなければ `err("expected String but got Int")` を返す。
全て `Result` なので `?` で自然に連鎖する。

### Obj の内部表現

`Obj(List[(String, Value)])` は挿入順を保持する。decode 時のフィールド検索は線形探索 O(n)。

- 小さい struct (≤20 fields) — 気にしない。実用上の JSON object は大半がこのサイズ
- 大きい struct — decode 関数内で一度 `Map[String, Value]` に変換してから lookup
- manual codec — 性能が必要なら手書き

## Codec convention

```almide
type Person: Codec = { name: String, age: Int, active: Bool = true }
```

`: Codec` は「`T.encode` と `T.decode` が存在する」という宣言。コンパイラが auto-derive する:

```almide
// auto-generated:
fn Person.encode(p: Person) -> Value =
  Object([("name", Str(p.name)), ("age", Int(p.age)), ("active", Bool(p.active))])

fn Person.decode(v: Value) -> Result[Person, String] = ...
```

### Nested types — encode

```almide
type Address: Codec = { city: String, zip: String }
type Person: Codec = { name: String, address: Address }

// Person.encode は Address.encode を呼ぶ:
fn Person.encode(p: Person) -> Value =
  Object([("name", Str(p.name)), ("address", Address.encode(p.address))])
```

### Nested types — decode (型ディスパッチ)

auto-derive はフィールドの型を見て適切な decode 関数を選ぶ:

```
フィールド型       → 生成する decode コード
──────────────    ──────────────────────
String            value.as_string(v)?
Int               value.as_int(v)?
Float             value.as_float(v)?
Bool              value.as_bool(v)?
Named("Address")  Address.decode(v)?       ← Codec 保証チェック
List[T]           value.as_arr(v)? |> list.map((x) => T.decode(x)?)
Option[T]         field missing → none, Null → none, other → some(T.decode(v)?)
```

```almide
type Team: Codec = { name: String, leader: Person, members: List[Person] }

// auto-generated decode — 全て value.field + ? で連鎖:
fn Team.decode(v: Value) -> Result[Team, String] = {
  let name = value.field(v, "name")? |> value.as_string?
  let leader = value.field(v, "leader")? |> Person.decode?
  let members = value.field(v, "members")? |> value.as_array?
    |> list.map((x) => Person.decode(x))?
  ok(Team { name: name, leader: leader, members: members })
}
```

decode 生成パターン:
- `value.field(v, "key")?` — フィールド取得 (missing → `err("missing field 'key'"`)
- `|> value.as_string?` — プリミティブ型変換 (型不一致 → `err("expected String but got Int")`)
- `|> Person.decode?` — nested Codec 型の再帰 decode
- `|> value.as_array? |> list.map(...)` — List の各要素を decode
- `Option[T]` フィールド — `value.field` が err なら `none`、成功なら `some(decode?)`
- field default — `value.field` が err なら default 値を使用

### Codec 制約の検証

`Team: Codec` を auto-derive するとき、フィールド型が Named(Person) なら `Person` も Codec を持つ必要がある。

**検証タイミング**: `generate_auto_derives` (lowerer) で `type_conventions` を参照。

```
error: field 'leader' has type Person which does not derive Codec
  --> app.almd:3
  hint: Add `: Codec` to the type declaration: type Person: Codec = ...
```

trait も protocol も不要。auto-derive 生成時の静的チェックで保証。

### Variant types

```almide
type Shape: Codec = Circle(radius: Float) | Rect(w: Float, h: Float)

// Tagged (default):
// Circle(3.0) → Object([("Circle", Object([("radius", Float(3.0))]))])
```

## Format modules (ライブラリ)

各フォーマットは **3層 API** を提供する:

```
Layer 1 (内部):  stringify / parse       — Value ↔ text
Layer 2 (主役):  encode / decode         — T ↔ text (convenience, LLM はこれを使う)
Layer 3 (上級):  パイプ合成              — encode() |> stringify (カスタマイズ用)
```

### JSON (stdlib)

```almide
// Layer 1: Value ↔ text (フォーマット提供者が実装)
fn json.stringify(v: Value) -> String
fn json.stringify_pretty(v: Value) -> String
fn json.parse(text: String) -> Result[Value, String]

// Layer 2: T ↔ text (convenience — LLM はこれを書く)
fn json.encode[T](value: T) -> String =
  T.encode(value) |> json.stringify

fn json.decode[T](text: String) -> Result[T, String] =
  json.parse(text)? |> T.decode

// 使い方 (LLM が書く形):
let text = json.encode(person)
let p = json.decode[Person](input)?
```

### YAML (stdlib or package)

```almide
fn yaml.stringify(v: Value) -> String
fn yaml.parse(text: String) -> Result[Value, String]

// 同じ convenience パターン:
fn yaml.encode[T](value: T) -> String =
  T.encode(value) |> yaml.stringify

fn yaml.decode[T](text: String) -> Result[T, String] =
  yaml.parse(text)? |> T.decode

// 使い方 (Person の定義は一切変えない):
let yaml_text = yaml.encode(person)
let p = yaml.decode[Person](yaml_input)?
```

### ユーザー定義フォーマット

```almide
// 実装者は Layer 1 だけ書く:
fn csv.stringify(v: Value) -> String = ...
fn csv.parse(text: String) -> Result[Value, String] = ...

// Layer 2 の convenience は同じパターンをコピー:
fn csv.encode[T](value: T) -> String =
  T.encode(value) |> csv.stringify

fn csv.decode[T](text: String) -> Result[T, String] =
  csv.parse(text)? |> T.decode
```

**設計判断: convenience のボイラープレートは意図的。** 全フォーマットで `T.encode(value) |> XXX.stringify` の4行が重複する。共通化するには higher-order module か trait が必要だが、Almide にはどちらもない。4行のコピペは trait 導入のコストに見合わないと判断。

### LLM が書くコードの完成形

```almide
type Config: Codec = {
  host: String = "localhost",
  port: Int = 8080,
  debug: Bool = false
}

// JSON で保存・読み込み
let text = json.encode(config)
let loaded = json.decode[Config](text)?

// YAML でも同じ型で動く
let yaml_text = yaml.encode(config)
let from_yaml = yaml.decode[Config](yaml_text)?
```

## Format 提供者の実装ガイド

フォーマットを追加するには `Value ↔ 外部表現` の2関数を書くだけ。型の知識は不要。

### 最小実装: stringify

```almide
// my_format/mod.almd
fn stringify(v: Value) -> String =
  match v {
    Null          => "null"
    Bool(b)      => if b then "true" else "false"
    Int(n)       => int.to_string(n)
    Float(f)     => float.to_string(f)
    Str(s)       => "\"" ++ escape(s) ++ "\""
    Array(items)  => "[" ++ items |> list.map(stringify) |> string.join(", ") ++ "]"
    Object(pairs) => "{" ++ pairs |> list.map((k, v) => "\"" ++ k ++ "\": " ++ stringify(v)) |> string.join(", ") ++ "}"
  }
```

これだけで `person.encode() |> my_format.stringify` が動く。Person の型定義を知る必要はない。

### 最小実装: parse

```almide
fn parse(text: String) -> Result[Value, String] = {
  // テキストをトークン化 → Value を再帰的に構築
  // 各フォーマット固有のパーサーロジック
  // 出力は常に Value 型
}
```

### フォーマット固有オプション

```almide
type MyFormatOptions = { pretty: Bool = false, indent: Int = 2 }

fn stringify_with(v: Value, opts: MyFormatOptions) -> String = ...

// 使う側:
person.encode() |> my_format.stringify_with(MyFormatOptions { pretty: true })
```

オプションはフォーマット側の引数。Codec 側には影響しない。

### binary フォーマット

```almide
// Value ↔ Bytes (String の代わり)
fn msgpack.to_bytes(v: Value) -> List[Int] = ...
fn msgpack.from_bytes(b: List[Int]) -> Result[Value, String] = ...

// 使う側:
let bytes = person.encode() |> msgpack.to_bytes
let p2 = msgpack.from_bytes(bytes)? |> Person.decode
```

### フォーマット提供者が知るべきこと

1. **入力は常に Value** — 7 variant のパターンマッチで全ケースを処理
2. **出力も常に Value** — parse は必ず Value を返す
3. **型の知識は不要** — Person か Config かは関係ない
4. **エラーは Result** — parse 失敗は `err("message")` で返す
5. **ネストは再帰** — Arr と Obj の中身を再帰的に処理するだけ
6. **テスト**: `json.parse(text)? |> my_format.stringify |> my_format.parse` が roundtrip すれば正しい

## 利用側ユースケース

### JSON encode/decode

```almide
type Person: Codec = { name: String, age: Int }

let alice = Person { name: "Alice", age: 30 }

// encode
let json_text = alice.encode() |> json.stringify
// → '{"name":"Alice","age":30}'

// decode
let bob = json.parse(input)? |> Person.decode
```

### 同じ型で YAML

```almide
// Person の定義は一切変えない

let yaml_text = alice.encode() |> yaml.stringify
// → "name: Alice\nage: 30\n"

let carol = yaml.parse(yaml_input)? |> Person.decode
```

### フォーマット変換 (型不要)

```almide
// JSON → YAML を型を経由せずに変換
let value = json.parse(json_text)?
let yaml_text = yaml.stringify(value)
```

### naming strategy

```almide
type ApiResponse: Codec = { userId: String, createdAt: String }

// encode はフィールド名そのまま
let v = response.encode()  // Object([("userId", ...), ("createdAt", ...)])

// snake_case が欲しい場合は Value 変換関数を挟む
let v_snake = v |> value.rename_keys(to_snake_case)
let text = v_snake |> json.stringify
// → '{"user_id":"...","created_at":"..."}'
```

## Generic 制約と Codec

```almide
// mono 時に T.encode の存在をチェック
fn json.encode_typed[T](value: T) -> String =
  T.encode(value) |> json.stringify
```

Almide には trait がないので、mono 時の関数存在チェックで制約を保証。
エラーメッセージは `: Codec` メタデータを使って改善:
- ❌ `T.encode が見つからない`
- ✅ `型 Foo は Codec ではありません。type Foo: Codec = ... で宣言してください`

## 実装順序

```
Phase 0: Value 型を stdlib に追加
  └─ type Value = Null | Bool(...) | Int(...) | Float(...) | Str(...) | Array(...) | Object(...)
  └─ 構築 API: value.str, value.int, value.obj, ...

Phase 1: Codec auto-derive
  └─ generate_auto_derives に "Codec" ケース追加
  └─ Record encode (フィールド → Obj)
  └─ Record decode (Obj → フィールド)
  └─ Nested (再帰 encode/decode)
  └─ Variant encode/decode (Tagged)

Phase 2: json module を Value ベースに移行
  └─ json.stringify(Value) -> String
  └─ json.parse(String) -> Result[Value, String]
  └─ 既存 Json 型 → Value 型にリネーム

Phase 3: yaml/toml module
  └─ yaml.stringify / yaml.parse
  └─ toml.stringify / toml.parse

Phase 4: DecodeError + repair + validate
  └─ 構造化エラー、json.repair[T], json.describe[T]

Phase 5: value 変換ユーティリティ
  └─ value.rename_keys, value.set_path, value.get_path
```

## 設計判断の根拠

- **trait なしで拡張可能** — Value が具体型として接点になる。抽象じゃなく具体。
- **convention ベース** — `: Codec` は「.encode と .decode が存在する」の宣言
- **関数の合成** — `encode() |> json.stringify` がパイプで繋がる
- **フォーマットは言語の外** — json, yaml はただの module。言語に組み込まない
- **JSON ファーストではない** — Value は universal data model。JSON はその serialization の1つ

## 関連ロードマップ

| ロードマップ | 関係 |
|------------|------|
| codec-and-json.md | 元の設計仕様 (Json → Value にリネーム予定) |
| derive-conventions (done) | convention 宣言の基盤 |
| operator-protocol (done) | auto-derive の仕組み |
| web-framework | Phase 1 完了後に Codec 統合 |
| monomorphization (done) | generic Codec の基盤 |
