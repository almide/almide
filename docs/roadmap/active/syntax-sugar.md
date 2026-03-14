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

## Lambda Syntax: `fn` を廃止、パレンスタイルに統一 [HIGH PRIORITY]

### Problem

現在の lambda 構文 `fn(params) => expr` は冗長。`fn` は関数宣言のキーワードであり、lambda に使うのは役割が混在する。

### Decision

Grammar Lab の実験結果により、`fn(x) => expr` と `(x) => expr` の modification survival rate に差がないことが確認された (fn 86% = paren 86%, p=1.0, Haiku N=30)。

**`fn(params) => expr` を廃止し、`(params) => expr` に統一する。**

### After

```almide
|> list.map((x) => x * 2)
|> list.filter((x) => x > 0)
|> list.fold(0, (acc, x) => acc + x)

// 型注釈付き
(x: Int) => x * 2

// 引数なし
() => 42
```

`fn` keyword は関数宣言 (`fn name(...) -> Type = ...`) 専用になる。

### Breaking Change

全 `.almd` ファイルの `fn(params) => expr` を `(params) => expr` に書き換える必要がある。

影響範囲:
- `spec/` 以下の全テスト
- `stdlib/` の例示コード
- `research/grammar-lab/` のタスクファイル
- `CLAUDE.md`, `docs/` のドキュメント
- Grammar Lab の Layer 2 prompt テンプレート

### Parser 変更

- `fn` + `(` の組み合わせを lambda として parse するパスを削除
- `(` の後に `)` `=>` パターンが来たら lambda として parse
- tuple/paren 式との曖昧性は `=>` の有無で解消
- 型注釈: `(x: Int) => expr` — `(` の中に `:` があれば型付き lambda

### Implementation Order

1. Parser に `(params) => expr` を追加（新構文）
2. 全 `.almd` ファイルを一括置換（`fn(` → `(`、lambda 文脈のみ）
3. `fn(params) => expr` を parse error にする（旧構文を削除）
4. ドキュメント更新

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
