# Codec Protocol & JSON [ACTIVE]

## Vision

JSON 専用 API ではなく、フォーマット非依存の **Codec protocol** を設計し、JSON をその最初の実装にする。

```almide
type Person = { name: String, age: Int } deriving Codec

let text = json.encode_to_string(alice)        // Person -> String
let bob = json.decode_from_string[Person](text)?  // String -> Person

// 将来: 同じ deriving Codec で別フォーマット
let bytes = msgpack.encode(alice)
```

## Design Principles

1. **derive が主経路** — 日常は `deriving Codec` で完結。manual は例外用
2. **field annotation なし** — 型レベル設定 + manual 実装で対応。annotation DSL の爆発を避ける
3. **JsonValue を第一級に** — syntax (parse/stringify) と semantic (encode/decode) を分離
4. **missing/null/default を明確に** — Option + field default で自然に解決
5. **エラーは構造化** — path + kind で位置と原因を明示

## Architecture

```
Text ──parse──▶ JsonValue ──decode[T]──▶ T
T ──encode[T]──▶ JsonValue ──stringify──▶ Text

便利API:
T ──encode_to_string──▶ Text
Text ──decode_from_string[T]──▶ T
```

### Codec Protocol (内部的に分離)

```almide
// 概念上の定義 (言語にtraitが入ったら明示化)
// Encodable: T -> JsonValue (または汎用 Encoder)
// Decodable: JsonValue (または汎用 Decoder) -> T
// Codec = Encodable + Decodable
```

encode だけ / decode だけが必要なケースに対応するため、内部的には分離。
ユーザー表面は `deriving Codec` を推奨。

---

## Phase 1: JSON Builder API (stdlib 追加のみ)

### 手動 JsonValue 構築

```almide
let person = json.object([
  ("name",    json.s("Alice")),
  ("age",     json.i(30)),
  ("active",  json.b(true)),
  ("address", json.object([
    ("city",  json.s("Tokyo"))
  ])),
  ("tags",    json.array([json.s("dev")])),
  ("notes",   json.null())
])
```

### API

```
// 構築
json.object     : (List[(String, Json)]) -> Json
json.array      : (List[Json]) -> Json
json.s          : (String) -> Json
json.i          : (Int) -> Json
json.f          : (Float) -> Json
json.b          : (Bool) -> Json
json.null       : () -> Json

// パース / 文字列化
json.parse      : (String) -> Result[Json, String]
json.stringify   : (Json) -> String
json.stringify_pretty : (Json) -> String

// アクセス
json.get        : (Json, String) -> Option[Json]
json.get_string : (Json) -> Option[String]
json.get_int    : (Json) -> Option[Int]
json.get_float  : (Json) -> Option[Float]
json.get_bool   : (Json) -> Option[Bool]
json.get_array  : (Json) -> Option[List[Json]]
json.keys       : (Json) -> List[String]
```

### 設計判断

- `json.s/i/f/b` で `from_*` を置き換え。7トークンで覚える
- `json.object(List[(String, Json)])` でネスト構造 = コード構造
- 既存の `from_string`, `from_int` 等は deprecate

---

## Phase 2: deriving Codec

### 基本

```almide
type Person = { name: String, age: Int } deriving Codec

let j = json.encode(alice)                    // Person -> Json
let text = json.encode_to_string(alice)       // Person -> String

let bob = json.decode[Person](j)?             // Json -> Person
let bob2 = json.decode_from_string[Person](text)?  // String -> Person
```

### field default との統合

```almide
type Config = {
  host: String = "localhost",
  port: Int = 8080,
  debug: Bool = false
} deriving Codec

// {"host": "example.com"} → Config { host: "example.com", port: 8080, debug: false }
// missing field は default 値で埋める
```

### naming strategy

```almide
type ApiResponse = { userId: String, createdAt: String }
  deriving Codec(field_names: snake_case)

// encode: {"user_id": "...", "created_at": "..."}
// decode: {"user_id": "..."} → ApiResponse { userId: "..." }
```

対応する戦略:
- `identity` (default) — フィールド名そのまま
- `snake_case` — camelCase → snake_case
- `camel_case` — snake_case → camelCase

### ADT (variant type) の表現

```almide
type Shape =
  | Circle(radius: Float)
  | Rect(w: Float, h: Float)
  deriving Codec

// デフォルト: Tagged (安全、明確)
// {"Circle": {"radius": 5.0}}

// opt-in で Adjacent
// deriving Codec(variant: adjacent("type", "data"))
// {"type": "Circle", "data": {"radius": 5.0}}
```

デフォルトは **Tagged** (externally tagged)。理由:
- decode が明確 (先頭キーで variant 確定)
- Untagged は曖昧性が出やすい → opt-in のみ、制約付き

### manual 実装 (残り5%のケース)

field annotation なしの設計なので、個別フィールドの rename や custom 変換は manual で書く:

```almide
type Weird = { name: String, kind: String }

fn weird_to_json(w: Weird) -> Json =
  json.object([("name", json.s(w.name)), ("type", json.s(w.kind))])

fn weird_from_json(j: Json) -> Result[Weird, DecodeError] = {
  let name = json.get(j, "name") |> json.as_string
  let kind = json.get(j, "type") |> json.as_string
  ok(Weird { name: name?, kind: kind? })
}
```

---

## Phase 3: JsonOptions

```almide
type JsonOptions = {
  unknown_fields: UnknownFieldPolicy = Reject,
  trailing_commas: Bool = false,
  comments: Bool = false,
  pretty: Bool = false
}

type UnknownFieldPolicy = Reject | Ignore

// 使用
let config = json.decode_from_string[Config](text, JsonOptions { unknown_fields: Ignore })?
```

デフォルトは `Reject` (unknown fields を拒否)。理由:
- 安全側に倒す
- タイポやスキーマ不一致を早期発見
- 緩くしたい場合は明示的に `Ignore`

---

## Phase 4: DecodeError 構造化

```almide
type DecodeError = { path: List[PathItem], kind: DecodeErrorKind }

type PathItem =
  | Field(name: String)
  | Index(i: Int)
  | Variant(name: String)

type DecodeErrorKind =
  | SyntaxError(msg: String)
  | TypeMismatch(expected: String, got: String)
  | MissingField(name: String)
  | UnknownField(name: String)
  | InvalidValue(msg: String)
  | OutOfRange(msg: String)
  | DuplicateKey(key: String)
  | Custom(msg: String)
```

表示例: `error at .users[3].name: expected String but got Int`

---

## Phase 5: 他フォーマット

同じ `deriving Codec` で JSON 以外にも対応:

```almide
let bytes = msgpack.encode(alice)
let config = yaml.decode_from_string[Config](text)?
let data = cbor.decode[Packet](bytes)?
```

Codec protocol がフォーマット非依存なので、各フォーマットは Encoder/Decoder を実装するだけ。

---

## Key Design Decisions

### missing vs null

- `Option[T]` フィールド: missing も null も `none` として扱う (merge policy)
- 非 Option フィールド: missing → default 値があれば使う、なければ `MissingField` エラー
- null on 非 Option → `TypeMismatch` エラー
- missing/null の厳密区別が必要な場合: 将来的に `Presence[T] = Missing | Null | Value(T)` 型を検討

### 数値ポリシー

- `Int` フィールドに `1.0` → `TypeMismatch` (strict)
- `Float` フィールドに `1` → 許容 (Int は Float に昇格)
- overflow → `OutOfRange` エラー
- NaN / Infinity → encode 禁止 (JSON 仕様準拠)

### encode 時の Option

- `none` → フィールドごと省略 (omit)
- `some(x)` → 値を出力
- 将来: `deriving Codec(null_for_none: true)` で `none` → `null` 出力に切り替え可能

### 循環参照

- サポートしない。encode 時にスタックオーバーフロー → エラー
- Almide は immutable-first なので循環構造は稀

### generic deriving

```almide
type Box[T] = { value: T } deriving Codec
// T が Codec を満たす場合のみ有効
// codegen 時に T の encode/decode を呼び出すコードを生成
```

monomorphization ベース (Rust codegen と同じ方針)。

### field name collision

`deriving Codec(field_names: snake_case)` 使用時、変換後のキーが衝突する場合はコンパイルエラー:

```almide
type Bad = { userId: String, user_id: String }
  deriving Codec(field_names: snake_case)
// compile error: field name collision after snake_case conversion: "user_id"
```

---

## Implementation Order

1. **Phase 1** — `json.object`, `json.s/i/f/b` を stdlib に追加
2. **Phase 2** — `deriving Codec` の codegen 実装 (record → JSON, JSON → record)
3. **Phase 3** — `JsonOptions` (unknown_fields, trailing_commas)
4. **Phase 4** — `DecodeError` 構造化 (path + kind)
5. **Phase 5** — msgpack/yaml 等のフォーマット追加

## Supersedes

This roadmap replaces [JSON Builder API](json-builder-api.md) (Phase 1 に統合).
