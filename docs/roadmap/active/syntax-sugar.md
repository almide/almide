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

## Lambda Short Syntax [HIGH PRIORITY]

### Problem

現在の lambda 構文は `fn(params) => expr` で固定。web framework のような callback-heavy な API で冗長。

```almide
// 今
|> web.get("/", fn(req) => web.text("Hello"))
|> list.map(fn(x) => x * 2)
|> list.filter(fn(x) => x > 0)
```

### Proposed: パレンスタイル

```almide
// 短縮形
|> web.get("/", (req) => web.text("Hello"))
|> list.map((x) => x * 2)
|> list.filter((x) => x > 0)

// 単一引数はパレン省略可
|> list.map(x => x * 2)
|> list.filter(x => x > 0)

// 複数引数
|> list.fold(0, (acc, x) => acc + x)

// 型注釈付き（既存の fn 形式）
fn(x: Int) => x * 2
```

### ルール

- `(params) => expr` は `fn(params) => expr` の短縮形
- 単一引数 + 型注釈なしのとき `x => expr` も可
- 型注釈が必要なときは `fn(x: Type) => expr` を使う（推論が効かない場面）
- `fn(params) => expr` は引き続き使える（breaking change なし）

### Parser 変更

`(` の後に `)` `=>` が続くか、`Ident` `)` `=>` / `Ident` `=>` が続くかで lambda と判定。既存の tuple/paren 式との曖昧性は `=>` の有無で解消。

### 影響

Web framework の書き味に直結:

```almide
// before
|> web.get("/users/:id", fn(req) => { ... })
|> web.use(fn(next) => fn(req) => { ... })

// after
|> web.get("/users/:id", (req) => { ... })
|> web.use((next) => (req) => { ... })
```

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
