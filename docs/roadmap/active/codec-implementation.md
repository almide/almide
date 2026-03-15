# Codec Implementation Plan [ACTIVE]

## 背景

codec-and-json.md に設計が完成している。この文書は **実装の依存関係と実現パス** を整理する。

今日完成した基盤:
- **Derive Conventions** — `type T: Eq, Repr = ...` + `fn T.method(t: T)` + auto-derive
- **Operator Protocol** — `==` → `T.eq`, `"${t}"` → `T.repr`, IR 関数自動生成
- **Call-site expansion** — default args, named args を lowerer で展開

これらの仕組みを **Encode/Decode に拡張する** のが Codec 実装の核心。

## 依存関係グラフ

```
[Done] auto-derive (Eq/Repr)
   │
   ▼
Phase 1: json stdlib 拡充
   │  json.object, json.s/i/f/b, path API
   │  (stdlib TOML 定義 + runtime 追加のみ)
   │
   ▼
Phase 2: deriving Codec ← 本丸
   │  auto-derive を Encode/Decode に拡張
   │  Record: フィールド順に json.object を生成
   │  Variant: Tagged/Adjacent 表現を生成
   │  Nested: 再帰的に encode/decode を呼ぶ
   │
   ├──▶ Web Framework (Codec 統合で JSON request/response 型安全化)
   ├──▶ Template (並行する boundary 機構)
   │
   ▼
Phase 3: JsonOptions
   │  unknown_fields, naming strategy
   │  (Phase 2 の encode/decode に options パラメータ追加)
   │
   ▼
Phase 4: DecodeError + repair + validate + schema
   │  構造化エラー, json.repair[T], json.describe[T]
   │  (LLM 差別化ポイント)
   │
   ▼
Phase 5: 他フォーマット (msgpack, yaml, cbor)
```

## Phase 2 の実装パス (本丸)

### 2a: Encode (T → Json)

**仕組み**: auto-derive と同じパターン。`deriving Codec` を持つ型に `T.encode` 関数を IR で自動生成。

```almide
type Person = { name: String, age: Int } deriving Codec

// auto-generate:
fn Person.encode(p: Person) -> Json =
  json.object([("name", json.s(p.name)), ("age", json.i(p.age))])
```

実装:
1. `lower.rs` の `generate_auto_derives` に `"Codec"` ケースを追加
2. Record のフィールドを走査して `json.object([...])` を構築する IR を生成
3. フィールド型に応じて `json.s` / `json.i` / `json.f` / `json.b` を選択
4. Nested record (フィールド型が Named で Codec を持つ) → 再帰的に `FieldType.encode(val)` を呼ぶ
5. Option[T] → `match val { some(v) => json.s(v), none => json.null() }`
6. List[T] → `json.array(list.map(val, (x) => x.encode()))` ← UFCS 解決必要

### 2b: Decode (Json → T)

```almide
// auto-generate:
fn Person.decode(j: Json) -> Result[Person, String] = {
  let name = json.get(j, "name") |> json.as_string
  let age = json.get(j, "age") |> json.as_int
  match (name, age) {
    (some(n), some(a)) => ok(Person { name: n, age: a })
    _ => err("decode failed")
  }
}
```

実装:
1. フィールドごとに `json.get(j, "field_name") |> json.as_TYPE` の IR を生成
2. 全フィールドが Some なら Record 構築、1つでも None なら err
3. field default がある場合 → `json.get(...).unwrap_or(default)` を使用
4. Option[T] フィールド → missing/null を none として許容
5. Nested record → 再帰的に `FieldType.decode(sub_json)` を呼ぶ

### 2c: 便利 API

```almide
// encode_to_string: T → String
fn json.encode_to_string[T](value: T) -> String =
  json.stringify(T.encode(value))

// decode_from_string: String → Result[T, String]
fn json.decode_from_string[T](text: String) -> Result[T, String] = do {
  let j = json.parse(text)
  T.decode(j)
}
```

これらは monomorphization で T を具体型に解決。

### 2d: Variant (ADT) の encode/decode

```almide
type Shape = Circle(radius: Float) | Rect(w: Float, h: Float) deriving Codec

// Tagged (default):
// Circle(3.0) → {"Circle": {"radius": 3.0}}
// Rect(1.0, 2.0) → {"Rect": {"w": 1.0, "h": 2.0}}
```

実装:
1. variant の各 case を match で分岐
2. 各 case の payload を encode (Unit → null, Tuple → array, Record → object)
3. 外側を `json.object([("CaseName", payload_json)])` で wrap

## 多層の課題

### Nested types

```almide
type Address = { city: String, zip: String } deriving Codec
type Person = { name: String, address: Address } deriving Codec

// Person.encode は Address.encode を呼ぶ必要がある
// → auto-derive 生成時に、フィールド型が Named で Codec を持つか確認
// → 持っていれば FieldType.encode(val) を IR に挿入
```

**解決**: `find_convention_fn` の仕組みを流用。`type_conventions` マップで "Codec" を持つ型を判定。

### Generic types

```almide
type Box[T] = { value: T } deriving Codec
// T が Codec を満たす場合のみ有効
// → monomorphization で T を具体化した後に encode/decode を生成
```

**解決**: `mono.rs` の既存基盤。`Box[Person]` → `Box__Person` に特殊化後、`Person.encode` を呼ぶコードを生成。

### 循環参照

サポートしない。Almide は immutable-first で循環構造は稀。encode 時にスタックオーバーフロー → ランタイムエラー。

## 優先順位

1. **Phase 1 + 2a (encode)** — 最小限の価値。`json.encode_to_string(person)` が動く
2. **Phase 2b (decode)** — 双方向。`json.decode_from_string[Person](text)` が動く
3. **Phase 2d (variant)** — ADT 対応
4. **Phase 3 (options)** — naming strategy, unknown fields
5. **Phase 4 (repair)** — LLM 差別化

Phase 1 は stdlib TOML 追加のみ (コンパイラ変更なし)。
Phase 2 はコンパイラ変更 (auto-derive 拡張) が必要。

## 関連ロードマップ

| ロードマップ | Codec との関係 |
|------------|--------------|
| codec-and-json.md | 設計ドキュメント (全 Phase の詳細仕様) |
| derive-conventions (done) | Encode/Decode convention 名を定義 |
| operator-protocol (done) | auto-derive の仕組みが基盤 |
| web-framework | Phase 2 完了後に Codec 統合 |
| template | 並行する typed boundary 機構 |
| stdlib-strategy | json module 36 関数が Phase 1 の基盤 |
