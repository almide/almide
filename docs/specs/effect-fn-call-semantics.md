> Last updated: 2026-08-20

# Effect Fn Call Semantics — 呼び出し式の型

## 規則

`effect fn foo() -> T` の呼び出し `foo()` は、**あらゆるコンテキストで**
`Result[T, String]` 型の**値**である(宣言が `-> Result[T, E]` ならその
`Result[T, E]`、二重包装はしない)。checker と codegen は同じ型を見る。
伝搬は明示の `!` だけ(ADR-0008)— 位置依存の暗黙伝搬(auto-`?`)は 0.55 で
機構ごと削除された。現行挙動の全体は
[result-option-effect.md §3](./result-option-effect.md) と
[effect-system.md](./effect-system.md) が記述する。本章は呼び出し式に絞る。

## 1. 宣言型

```almide
effect fn foo(x: Int) -> String = "v${int.to_string(x)}"
```

これは以下の型を持つ:

```text
foo : (Int) -> Result[String, String]
```

ただし、**関数本体内では** `String` を直接書ける。末尾式は暗黙に `ok(...)` で包まれる。

## 2. 呼び出し型 — どの文脈でも Result 値

```almide
effect fn foo() -> String = "x"

effect fn main() -> Unit = {
  let r: Result[String, String] = foo()   // r: Result[String, String] ← これが正式な型
  let v = foo()!                          // v: String                ← ! で unwrap(err は伝搬)
  let w = foo() ?? "d"                    // w: String                ← ?? で fallback
  println(v + w + (r ?? ""))
}

test "the call is a Result value" {
  assert_eq(foo(), ok("x"))
}
```

## 3. 暗黙伝搬は存在しない(E041 / E042)

effect fn body 内であっても、un-annotated な `let` の右辺の effect call は
剥がされない。旧 auto-`?` の位置は**ハードエラー**で、`!` 挿入の hint を伴う:

```almide check-fail=E041
effect fn read_file(path: String) -> String = "contents of " + path

effect fn main() -> Unit = {
  let content = read_file("test.txt")   // E041: Result を暗黙に剥がす位置はもう無い — `!` を書く
  println(content)
}
```

文の位置で Result を捨てるのも同様にエラー(must-use):

```almide check-fail=E042
effect fn touch() -> Result[Unit, String] = ok(())

effect fn main() -> Unit = {
  touch()           // E042: `touch()!` か `let _ = touch()` の 2 綴りのどちらか
  println("done")
}
```

回帰テスト: `spec/lang/explicit_propagation_test.almd`

## 4. `!` 演算子

`!` は `Result[T, E] → T`(err は囲む関数の失敗チャネルへ伝搬)と
`Option[T] → T`(none は `err("none")` へ)。effect fn call が
`Result[T, String]` を返すので、`foo()!` は自然に `T` を返す — checker が
特別扱いする必要はない。合法な文脈は effect fn 本体・test ブロック・戻り値型が
Result / Option / `T!` に解決される pure fn(C-211)。

## 5. test ブロック

test ブロックは effect context だが、effect call の結果は**そのまま Result 値**:
`!` で unwrap する(失敗は panic でテスト失敗)か、`??` / match で消費する。

```almide
effect fn load_data() -> Result[String, String] = ok("data")

test "effect calls inside tests" {
  let data = load_data()!                  // 明示的 ! — test でも省略できない
  assert(string.len(data) > 0)
  assert_eq(load_data(), ok("data"))       // Result 値としてそのまま比較もできる
}
```

## 6. lambda 内

lambda は `!` を enclosing fn に propagate できない(クロージャ境界 — ADR-0006 D1)。
lambda 内の effect call も Result 値であり、`match` か lambda 自身の `!`
(lambda の失敗チャネルに落ちる、[result-option-effect.md §3](./result-option-effect.md))で消費する:

```almide
effect fn parse(n: Int) -> Result[Int, String] = if n > 0 then ok(n) else err("neg")

effect fn main() -> Unit = {
  let items = [1, -2, 3]
  let parsed = items |> list.map((n) => {
    match parse(n) {          // lambda 内では match か ! が必要 — 暗黙伝搬はない
      ok(v) => v,
      err(_) => 0,
    }
  })
  println(int.to_string(list.len(parsed)))
}

test "effect calls in lambdas are Result values" {
  assert_eq(parse(-1), err("neg"))
}
```

## 経緯(完了済み)

checker が `T` を返し codegen が `Result` に持ち上げる旧方式は、`!` の型エラーと
cross-module 呼び出しの不整合を生んだ。checker lift(呼び出し型を常に Result に
統一)は v0.34.x で ABI まで含めて実装済み(#840 / #841)、その上に乗っていた
auto-`?` は ADR-0008(v0.55)で削除された — 設計の選択肢比較と反証は ADR-0008 に
ある。

## 検証テスト

```text
spec/lang/effect_fn_test.almd
spec/lang/effect_assign_unwrap_test.almd
spec/lang/effect_if_branch_unwrap_test.almd
spec/lang/effect_result_arg_test.almd
spec/lang/explicit_propagation_test.almd
spec/integration/codegen_effect_fn_test.almd
```
