<a id="effect-fn-call-semantics--呼び出し式の型"></a>

# Effect Fn 调用语义——调用表达式的类型

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](effect-fn-call-semantics.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](effect-fn-call-semantics.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](effect-fn-call-semantics.zh-CN.md)



> 权威原文：[docs/specs/effect-fn-call-semantics.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/effect-fn-call-semantics.md)，固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`。这是非规范性的读者快照译文，不自动同步。原规格及其声明的规范地位保持不变。模型证明、实现主张、历史状态和证据边界均保留原文范围；本译文不提供独立执行、测量、符合性或安全性证明。代码块内代码及注释逐字保留。

> 最后更新：2026-08-20

<a id="規則"></a>

## 规则

`effect fn foo() -> T` 的调用 `foo()` 在**所有上下文中**都是 `Result[T, String]` 类型的**值**（若声明为 `-> Result[T, E]`，则是该 `Result[T, E]`，不双重包装）。checker 和 codegen 看到相同类型。传播只有显式 `!`（ADR-0008）；依赖位置的隐式传播（auto-`?`）已在 0.55 连同机制整体删除。完整当前行为见 [result-option-effect.md §3](../../../../specs/result-option-effect.md) 及 [effect-system.md](../../../../specs/effect-system.md)。本章仅讨论调用表达式。

<a id="1-宣言型"></a>

## 1. 声明类型

```almide
effect fn foo(x: Int) -> String = "v${int.to_string(x)}"
```

其类型如下：

```text
foo : (Int) -> Result[String, String]
```

不过，**函数体内**可以直接写 `String`。尾表达式隐式包装为 `ok(...)`。

<a id="2-呼び出し型--どの文脈でも-result-値"></a>

## 2. 调用类型——任何上下文中都是 Result 值

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

<a id="3-暗黙伝搬は存在しないe041--e042"></a>

## 3. 不存在隐式传播（E041 / E042）

即使在 effect fn 体内，未注解 `let` 右侧的 effect call 也不会被解包。原先 auto-`?` 的位置是**硬错误**，并提示插入 `!`：

```almide check-fail=E041
effect fn read_file(path: String) -> String = "contents of " + path

effect fn main() -> Unit = {
  let content = read_file("test.txt")   // E041: Result を暗黙に剥がす位置はもう無い — `!` を書く
  println(content)
}
```

在语句位置丢弃 Result 同样报错（must-use）：

```almide check-fail=E042
effect fn touch() -> Result[Unit, String] = ok(())

effect fn main() -> Unit = {
  touch()           // E042: `touch()!` か `let _ = touch()` の 2 綴りのどちらか
  println("done")
}
```

置于 `-> Unit` effect fn **尾部**（函数体末尾、`if` 两个分支、`match` 每个 arm、`guard` 的 else）的 effect call 也是 E042，但含义不同：尾值就是函数值，因此其 Result 直接经函数自身的错误通道返回（不是丢弃——codegen 发出 `Ok(call?)`）。问题在于发生了传播却没有写明，消息也这样说明（#2277）。写 `expr!` 即可让相同行为显式可见：

```almide check-fail=E042
effect fn say(s: String) -> Unit = println(s)

effect fn run(cmd: String) -> Unit =
  if cmd == "a" then say("x")   // E042: 末尾の Result が暗黙に伝搬 — `say("x")!` と綴る
  else say("y")!
```

回归测试：`spec/lang/explicit_propagation_test.almd`、`tests/diagnostics/e042-tail-call-in-branches/`。

<a id="4--演算子"></a>

## 4. `!` 运算符

`!` 提取成功值，失败处理遵循**当前通道**。在可失败 fn / lambda 中，沿自身失败通道传播，不跨越闭包边界传播到外层函数。在 test 块中解包，失败即测试失败。

`Option[T]` 的 `none` 在包围它的 Option 返回通道中以 `none` 传播。允许转换到 Result 通道时，变为 `err("none")`。不要忽略错误类型兼容性或 pure/infallible 上下文的处理，将其解释为“总是返回 Err”。完整上下文表及验证示例见 [Result / Option / Effect](../../../../specs/result-option-effect.md#2-演算子)。回归测试：`spec/lang/unwrap_operators_test.almd`、`spec/lang/fallible_lambda_test.almd`。

<a id="5-test-ブロック"></a>

## 5. test 块

test 块是 effect 上下文，但 effect call 的结果**仍是 Result 值**：用 `!` 解包（失败通过 panic 令测试失败），或用 `??` / match 消耗。

```almide
effect fn load_data() -> Result[String, String] = ok("data")

test "effect calls inside tests" {
  let data = load_data()!                  // 明示的 ! — test でも省略できない
  assert(string.len(data) > 0)
  assert_eq(load_data(), ok("data"))       // Result 値としてそのまま比較もできる
}
```

## 6. lambda 内

lambda 不能将 `!` 传播到 enclosing fn（闭包边界——ADR-0006 D1）。lambda 中的 effect call 也是 Result 值，需通过 `match` 或 lambda 自己的 `!` 消耗（进入 lambda 的失败通道，见 [result-option-effect.md §3](../../../../specs/result-option-effect.md)）：

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

<a id="経緯完了済み"></a>

## 历史（已完成）

旧方式由 checker 返回 `T`、codegen 提升到 `Result`，造成 `!` 类型错误及跨模块调用不一致。checker lift（将调用类型统一为 Result）已在 v0.34.x 连同 ABI 实现（#840 / #841）；其上的 auto-`?` 已由 ADR-0008（v0.55）删除。设计选项比较及反例见 ADR-0008。

<a id="検証テスト"></a>

## 验证测试

```text
spec/lang/effect_fn_test.almd
spec/lang/effect_assign_unwrap_test.almd
spec/lang/effect_if_branch_unwrap_test.almd
spec/lang/effect_result_arg_test.almd
spec/lang/explicit_propagation_test.almd
spec/integration/codegen_effect_fn_test.almd
```
