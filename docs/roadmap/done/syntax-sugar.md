<!-- description: Syntax sugar (ranges, exhaustiveness check, lambda shorthand) -->
<!-- done: 2026-03-15 -->
# Syntax Sugar

## Range Literals ✅

```almide
let xs = 0..10        // [0, 1, 2, ..., 9]
let ys = 0..=10       // [0, 1, 2, ..., 10]
let zs = 10..0..-1    // [10, 9, 8, ..., 1]
```

## Exhaustiveness Checking for Pattern Match ✅

Detects at compile time when a match on a variant type does not cover all cases. Implemented in `src/check/mod.rs`.

---

## Lambda Syntax: `fn` を廃止、パレンスタイルに統一 ✅

`fn(params) => expr` を廃止し、`(params) => expr` に統一完了。
- パーサーから `fn(` lambda パスを削除
- 全 `.almd` ファイル (44ファイル) を一括置換
- Rust テスト、ドキュメント、ヒントメッセージを更新
- `fn` keyword は関数宣言 (`fn name(...) -> Type = ...`) 専用に

---

## Default Arguments ✅

Call-site expansion 方式で実装。Lowering 時に不足引数にデフォルト値を IR に挿入。codegen 変更不要。

```almide
fn greet(name: String, greeting: String = "Hello") -> String =
  "${greeting}, ${name}!"

greet("Alice")              // → greet("Alice", "Hello") at IR level
greet("Alice", "Hi")        // → greet("Alice", "Hi")
```

- Parser: `name: Type = expr` を Param.default に格納
- Checker: fn_min_params で引数数を許容
- Lowerer: call-site で defaults を展開（ターゲット非依存）

---

## List Comprehensions — Won't Do

Canonicity 違反。`xs |> list.filter(...) |> list.map(...)` で同じことが書ける。

## Named Arguments

```almide
// Optional, positional args の後ろでのみ使用可能
create_user("Alice", admin: true)
http.response(status: 200, body: "OK")

// 位置引数のみも OK
create_user("Alice", 30, false)
```

Swift 参考だが external/internal name 分離なし。Almide は Vocabulary Economy 重視。

## Raw String Literals ✅

```almide
let regex_pattern = r"^\d{3}-\d{4}$"
let path = r"C:\Users\test"
```

## Block Comments ✅

```almide
/* inline comment */
/* nestable /* nested */ comment */
```

---

## Priority

Range ✅ > Exhaustiveness ✅ > Lambda ✅ > Default args ✅ > Block comments ✅ > Raw strings ✅ > ~~List comprehensions~~ (Won't do) > **Named arguments**
