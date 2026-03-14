# Derive Conventions [ACTIVE]

## Summary
trait/typeclass を導入せず、固定 convention + コロン構文で polymorphism を実現する。
LLM の生成精度を最大化する設計判断。

## Design Rationale
- **LLM は固定パターンを最も正確に書ける**: convention が 6 個なら完全に覚えられる
- **型エラーの発生源を消す**: trait + impl + bound の組み合わせ爆発がない
- **「見慣れないパターン」問題を回避**: 全プロジェクトで同じ convention
- **人間の学習コストはエラーメッセージで導ける**: LLM にとって初見問題は存在しない

## Almide の多相性モデル

2パターンだけ:
1. **組み込み convention** — コロンで宣言、演算子・言語機能と連動
2. **structural bound** — メソッドを書けば使える、bound で制約できる

```almide
// 1. 組み込み convention
type Dog: Eq, Show = { name: String, breed: String }

fn Dog.eq(self, other: Dog) -> Bool = self.name == other.name
fn Dog.show(self) -> String = "${self.name} (${self.breed})"

// 2. structural bound (convention 定義不要、メソッド + bound)
fn print_all[T: { display: () -> String }](items: List[T]) =
  for item in items { println(item.display()) }
```

## Syntax

```almide
type Dog: Eq, Show = { name: String, breed: String }
type Color: Eq, Show = Red | Green | Blue | Rgb(Int, Int, Int)
type UserId = Int  // alias — convention なし
```

## Fixed Conventions (6個、これ以上増えない)

| Convention | Required Function | Enables |
|---|---|---|
| `Eq` | `T.eq(self, other: T) -> Bool` | `==`, `!=` |
| `Show` | `T.show(self) -> String` | string interpolation, `println` |
| `Compare` | `T.compare(self, other: T) -> Int` | `sort()`, `<`, `>`, `<=`, `>=` |
| `Hash` | `T.hash(self) -> Int` | `Map` key, `Set` |
| `Encode` | `T.encode(self) -> String` | JSON/TOML serialize |
| `Decode` | `T.decode(s: String) -> Result[T, String]` | deserialize (静的メソッド) |

Auto derive: カスタム関数が未定義ならコンパイラが自動生成。

## Implementation Phases

### Phase 1: Parser + Checker + Codegen mapping ✅ DONE
- `type Dog: Eq, Show = { ... }` コロン構文パース
- `fn Dog.eq(self, ...)` メソッド定義構文パース
- Checker が convention 名を検証 (6種固定)
- Rust codegen が `#[derive(PartialEq, Eq, Ord, Hash)]` にマッピング
- Formatter が `: Eq, Show` 出力

### Phase 2: Method Resolution
- `fn Dog.show(self, ...)` を checker が型の関連関数として登録
- `dog.show()` → UFCS で `Dog.show(dog)` に解決
- lower が `Dog.show` を IR の `CallTarget` に変換

### Phase 3: Operator Dispatch
- `a == b` on Dog → `Dog.eq(a, b)` にディスパッチ (`Eq` 宣言時)
- `"${dog}"` → `Dog.show(dog)` にディスパッチ (`Show` 宣言時)
- `dogs.sort()` → `Dog.compare` を使用 (`Compare` 宣言時)

### Phase 4: Auto Derive
- convention 関数が未定義の場合、IR に自動生成
- `Eq`: 全フィールドの `==` で比較
- `Show`: `TypeName { field1: value1, ... }` 形式
- `Compare`: フィールド順に辞書順比較
- `Hash`: 全フィールドの hash を combine

### Phase 5: Static Methods + Encode/Decode
- `Config.decode(json)` — 型名を namespace とした静的メソッド呼び出し
- `Encode`: JSON 形式出力
- `Decode`: JSON パース

## Files
```
src/parser/mod.rs          — fn Dog.eq() パース (expect_any_fn_name)
src/parser/declarations.rs — type Dog: Eq, Show パース
src/check/mod.rs           — convention 名検証、関連関数登録
src/lower.rs               — convention メソッド解決、auto derive 生成
src/emit_rust/lower_rust.rs — Rust derive マッピング
src/emit_ts/lower_ts.rs    — TS convention dispatch
src/fmt.rs                 — コロン構文出力
```
