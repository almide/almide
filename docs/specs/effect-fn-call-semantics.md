> Last updated: 2026-07-25

# Effect Fn Call Semantics — 型の解釈

## 問題

`effect fn` の呼び出しが、checker と codegen で異なる型を持つ。

```
checker:  auth.authenticate() → Profile
codegen:  auth.authenticate() → Result[Profile, String]
```

この不一致が `!` 演算子の型エラーや cross-module 呼び出しの不整合を引き起こす。

## 設計方針

### 選択肢

| # | 方式 | 概要 | 問題点 |
|---|------|------|--------|
| A | **Codegen lift** (現状) | checker は `T` を返す。codegen の ResultPropagation が `Result[T, String]` に持ち上げ | checker と codegen の型が不一致。`!` が checker で通らない |
| B | **Checker lift** | checker が `Result[T, String]` を返す。codegen はそのまま | match arm の型統一が壊れる。`let x = foo()` で x が Result になり、`.field` でエラー |
| C | **Contextual erasure** | effect fn call は文脈に応じて型が変わる。effect body 内では `T`、lambda/test では `Result[T, String]` | 同じ式が文脈で型が変わるのは型システムとして不健全 |
| D | **Checker lift + auto-`?`** | checker が `Result[T, String]` を返し、effect body 内の式文に auto-`?` を挿入 | 正しいが、全 effect fn call site の型が変わる大規模リファクタ |

### 採用: D — Checker lift + auto-`?`

**理由**: 型システムが一貫する唯一の選択肢。effect fn の呼び出し型は常に `Result[T, String]`。呼び出し元が effect fn なら auto-`?` で透過的に `T` を取り出す。

## 正式な意味論

### 1. `effect fn` の宣言型

```almide
effect fn foo(x: Int) -> String = ...
```

これは以下の糖衣構文:

```
foo : (Int) -> Result[String, String]
```

ただし、**関数本体内では** `String` を直接書ける。末尾式は暗黙に `ok(...)` で包まれる。

### 2. 呼び出し型

`effect fn foo() -> T` の呼び出し `foo()` は、**あらゆるコンテキストで** `Result[T, String]` 型を持つ。

```almide
effect fn main() -> Unit = {
  let r = foo()        // r: Result[T, String]  ← これが正式な型
  let v = foo()!       // v: T                  ← ! で unwrap
  let w = foo() ?? d   // w: T                  ← ?? で fallback
}
```

### 3. Auto-`?` 挿入 (ergonomic sugar)

effect fn body 内の **式文** (let 束縛の右辺、代入の右辺) で、effect fn call が `Result[T, String]` を返す場合、checker が自動的に `?` (Try) を挿入する。

```almide
effect fn main() -> Unit = {
  let content = read_file("test.txt")   // ← auto-? 挿入
  //            ↓ 脱糖
  // let content = read_file("test.txt")?
  println(content)   // content: String
}
```

これにより、ユーザーは `!` なしで effect fn を呼び出せる。auto-`?` は以下の条件で挿入:

- 呼び出し元が `effect fn` body 内
- 呼び出し先が user-defined `effect fn` (stdlib bundled は除外)
- 呼び出し結果が `Result[T, E]` で、束縛先の期待型が `T`

auto-`?` は **match の subject** や **関数引数** では挿入しない。これらの文脈では明示的に `!` が必要。

### 4. `!` 演算子

`!` は `Result[T, E] → T` (error propagation) と `Option[T] → T` (None → panic/propagation)。

effect fn call が `Result[T, String]` を返すので、`foo()!` は自然に `T` を返す。checker が特別扱いする必要はない。

### 5. test ブロック

test ブロックは effect context。auto-`?` が同様に適用される。

```almide
test "reads a file" {
  let content = fs.read_text("test.txt")!   // 明示的 ! (stdlib)
  let data = load_data()                     // auto-? (user effect fn)
  assert(string.len(data) > 0)
}
```

### 6. lambda 内

lambda は `?` を enclosing fn に propagate できない。auto-`?` は挿入されない。

```almide
effect fn main() -> Unit = {
  let items = [1, 2, 3]
  // lambda 内では明示的に ! or match が必要
  items |> list.map((n) => {
    match parse(n) {
      ok(v) => v
      err(_) => 0
    }
  })
}
```

## 移行の記録（完了済み）

上記の checker lift（方式 D）は**実装済み**。lifted effect fn の ABI は
v0.34.x で完全統一された: never-err な lifted effect fn は raw `T` を返し
呼び出し網が rewrap、can-err は常時 `Result` wrap（#840 / #841）。
現行挙動は [effect-system.md](./effect-system.md) と
[../HIDDEN_OPERATIONS.md](../HIDDEN_OPERATIONS.md) §2 が記述する。

## 検証テスト

```
spec/lang/effect_fn_test.almd
spec/lang/effect_assign_unwrap_test.almd
spec/lang/effect_if_branch_unwrap_test.almd
spec/lang/effect_result_arg_test.almd
spec/integration/codegen_effect_fn_test.almd
```
