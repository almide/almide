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

## Default Arguments [HIGH PRIORITY]

### Problem

```almide
// 今: status code を毎回指定
web.json(200, data)
web.text_status(200, "Hello")

// 欲しい: common case はデフォルト
web.json(data)             // status = 200
web.json(data, 201)        // explicit
```

### Proposed

```almide
fn greet(name: String, greeting: String = "Hello") -> String =
  "${greeting}, ${name}!"

greet("Alice")              // "Hello, Alice!"
greet("Alice", "Hi")        // "Hi, Alice!"
```

### ルール

- デフォルト引数は末尾に寄せる
- field default values と同じ仕組み（既に AST に default expr の概念がある）
- codegen: Rust は複数の関数を生成（`rust_min` パターンの一般化）
- TS はそのまま default params にマッピング

### 影響

Web framework:

```almide
fn json(body: Json, status: Int = 200) -> Response = ...
fn text(body: String, status: Int = 200) -> Response = ...

// 使う側
web.json(data)        // 200
web.json(data, 201)   // 201
```

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

## Raw String Literals

```almide
let regex_pattern = r"^\d{3}-\d{4}$"
let path = r"C:\Users\test"
```

## Block Comments

```almide
/*
  multi-line comment
  currently only // is supported
*/
```

---

## Priority

~~Range literals~~ ✅ > ~~exhaustiveness checking~~ ✅ > **lambda short syntax** > **default arguments** > block comments > list comprehensions > named arguments > raw strings

Lambda short syntax と default arguments は web framework DX の前提条件として優先度を上げた。
