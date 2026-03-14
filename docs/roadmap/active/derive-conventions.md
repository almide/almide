# Derive Conventions [ACTIVE]

## Summary
trait/typeclass を導入せず、固定 convention + `derive` 構文で polymorphism を実現する。
LLM の生成精度を最大化する設計判断。

## Design Rationale
- **LLM は固定パターンを最も正確に書ける**: convention が 5-6 個なら完全に覚えられる
- **型エラーの発生源を消す**: trait + impl + bound の組み合わせ爆発がない
- **「見慣れないパターン」問題を回避**: 全プロジェクトで同じ convention
- **人間の学習コストはエラーメッセージで導ける**: LLM にとって初見問題は存在しない

## Syntax

```almide
type Dog = { name: String, breed: String }
  derive Eq, Show

// カスタム実装
fn Dog.eq(self, other: Dog) -> Bool = self.name == other.name
fn Dog.show(self) -> String = "${self.name} (${self.breed})"

// auto derive（関数を書かなければコンパイラが自動生成）
type Point = { x: Int, y: Int }
  derive Eq, Show
```

## Fixed Conventions

| Convention | Required Function | Enables |
|---|---|---|
| `Eq` | `T.eq(self, other: T) -> Bool` | `==`, `!=` |
| `Show` | `T.show(self) -> String` | string interpolation, `println` |
| `Compare` | `T.compare(self, other: T) -> Int` | `sort()`, `<`, `>`, `<=`, `>=` |
| `Hash` | `T.hash(self) -> Int` | `Map` key, `Set` |
| `Encode` | `T.encode(self) -> String` | JSON/TOML serialize |
| `Decode` | `T.decode(s: String) -> Result[T, String]` | deserialize |

## Auto Derive Rules
- `Eq`: 全フィールドの `==` で比較
- `Show`: `TypeName { field1: value1, field2: value2 }` 形式
- `Compare`: フィールド順に辞書順比較
- `Hash`: 全フィールドの hash を combine
- `Encode`: `{ "field1": value1, "field2": value2 }` JSON 形式
- `Decode`: Encode の逆

カスタム関数が定義されていればそちらを優先。なければ auto derive。

## Variant Types

```almide
type Shape =
  | Circle(Float)
  | Rect(Float, Float)
  derive Eq, Show

// auto derive: tag + payload の比較/表示
```

## Structural Bound との関係
独自の polymorphism は structural bound で書く。derive convention は不要:
```almide
fn print_all[T: { display: () -> String }](items: List[T]) =
  for item in items { println(item.display()) }
```

## Implementation Phases

### Phase 1: Parser + Checker
- `derive` を type 宣言の構文に追加
- AST に `derive: Vec<String>` フィールド追加
- Checker が derive 宣言を検証（convention 名が有効か）

### Phase 2: Auto Derive (Eq, Show)
- `T.eq`/`T.show` が未定義の場合にコンパイラが自動生成
- IR に生成された関数を挿入

### Phase 3: Operator Dispatch
- `==` で `T.eq` を呼ぶ codegen
- string interpolation で `T.show` を呼ぶ codegen

### Phase 4: Compare, Hash, Encode, Decode
- 残りの convention を実装

## Files
```
src/ast.rs             — TypeDecl に derive フィールド追加
src/parser/declarations.rs — derive 構文パース
src/check/mod.rs       — derive 検証、convention 関数の存在チェック
src/lower.rs           — auto derive 関数の IR 生成
src/emit_rust/         — convention dispatch の codegen
src/emit_ts/           — 同上
```
