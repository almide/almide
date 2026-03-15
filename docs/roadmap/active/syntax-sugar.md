# Syntax Sugar [ACTIVE]

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

## List Comprehensions

```almide
let evens = [x for x in 0..100 if x % 2 == 0]
let pairs = [(x, y) for x in xs for y in ys]
```

## Named Arguments

```almide
http.response(status: 200, body: "OK")
```

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

~~Range literals~~ ✅ > ~~exhaustiveness checking~~ ✅ > **lambda short syntax** > **default arguments** > block comments > list comprehensions > named arguments > raw strings

Lambda short syntax と default arguments は web framework DX の前提条件として優先度を上げた。
