# Effect Fn Call Semantics — the type of a call expression

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](effect-fn-call-semantics.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](effect-fn-call-semantics.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](effect-fn-call-semantics.zh-CN.md)



> Authoritative source: [docs/specs/effect-fn-call-semantics.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/effect-fn-call-semantics.md), fixed at `35297a612fb29e3f78f6587aba253aa91d51f466`. This is a non-normative reader snapshot translation, not automatically synchronized. The original specifications and their stated authority remain unchanged. Model proofs, implementation claims, historical status and evidence bounds retain their source scope; this translation supplies no independent execution, measurement, conformance or safety proof. Code and comments inside fences remain verbatim.

> Last updated: 2026-08-20

## Rule

A call `foo()` to `effect fn foo() -> T` is a **value** of type `Result[T, String]` **in every context** (if declared `-> Result[T, E]`, it is that `Result[T, E]`, without double wrapping). The checker and codegen see the same type. Propagation uses explicit `!` only (ADR-0008); position-dependent implicit propagation (auto-`?`) was removed, mechanism and all, in 0.55. Current behavior as a whole is described by [result-option-effect.md §3](../../../../specs/result-option-effect.md) and [effect-system.md](../../../../specs/effect-system.md). This chapter focuses on call expressions.

## 1. Declaration type

```almide
effect fn foo(x: Int) -> String = "v${int.to_string(x)}"
```

This has the following type:

```text
foo : (Int) -> Result[String, String]
```

However, **inside the function body**, a `String` can be written directly. The tail expression is implicitly wrapped in `ok(...)`.

## 2. Call type — a Result value in every context

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

## 3. There is no implicit propagation (E041 / E042)

Even inside an effect fn body, an effect call on the right-hand side of an unannotated `let` is not unwrapped. Former auto-`?` positions are **hard errors**, with a hint to insert `!`:

```almide check-fail=E041
effect fn read_file(path: String) -> String = "contents of " + path

effect fn main() -> Unit = {
  let content = read_file("test.txt")   // E041: Result を暗黙に剥がす位置はもう無い — `!` を書く
  println(content)
}
```

Discarding a Result in statement position is likewise an error (must-use):

```almide check-fail=E042
effect fn touch() -> Result[Unit, String] = ok(())

effect fn main() -> Unit = {
  touch()           // E042: `touch()!` か `let _ = touch()` の 2 綴りのどちらか
  println("done")
}
```

An effect call in the **tail** of an `-> Unit` effect fn (the function body's tail, both branches of `if`, each `match` arm, or a `guard` else) also gets E042, but the meaning differs: the tail value is the function's value, so its Result goes straight out through that function's own error channel (it is not discarded — codegen emits `Ok(call?)`). The problem is that propagation happens without being written, and the message says so (#2277). Writing `expr!` makes the same behavior visible:

```almide check-fail=E042
effect fn say(s: String) -> Unit = println(s)

effect fn run(cmd: String) -> Unit =
  if cmd == "a" then say("x")   // E042: 末尾の Result が暗黙に伝搬 — `say("x")!` と綴る
  else say("y")!
```

Regression tests: `spec/lang/explicit_propagation_test.almd`, `tests/diagnostics/e042-tail-call-in-branches/`.

## 4. The `!` operator

`!` extracts the success value; failure handling follows the **current channel**. In a fallible fn / lambda, it propagates through that function's own failure channel, not across a closure boundary to an outer function. In a test block it unwraps, and a failure fails the test.

`none` from `Option[T]` propagates as `none` in an enclosing Option return channel. When conversion to a Result channel is permitted, it becomes `err("none")`. Do not interpret this as “always returns Err” while omitting error-type compatibility or pure/infallible contexts. For the complete context table and verification examples, see [Result / Option / Effect](../../../../specs/result-option-effect.md#2-演算子). Regression tests: `spec/lang/unwrap_operators_test.almd`, `spec/lang/fallible_lambda_test.almd`.

## 5. Test blocks

A test block is an effect context, but the result of an effect call is **still a Result value**: unwrap with `!` (failure panics and fails the test), or consume with `??` / match.

```almide
effect fn load_data() -> Result[String, String] = ok("data")

test "effect calls inside tests" {
  let data = load_data()!                  // 明示的 ! — test でも省略できない
  assert(string.len(data) > 0)
  assert_eq(load_data(), ok("data"))       // Result 値としてそのまま比較もできる
}
```

## 6. Inside a lambda

A lambda cannot propagate `!` to the enclosing fn (closure boundary — ADR-0006 D1). Effect calls inside a lambda are also Result values, consumed with `match` or the lambda's own `!` (which enters the lambda's failure channel; [result-option-effect.md §3](../../../../specs/result-option-effect.md)):

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

## History (completed)

The old approach, where the checker returned `T` and codegen lifted it to `Result`, caused `!` type errors and inconsistent cross-module calls. Checker lifting (unifying call types as Result throughout) was implemented through the ABI in v0.34.x (#840 / #841). The auto-`?` layered on top was removed by ADR-0008 (v0.55); ADR-0008 contains the design alternatives and their counterexamples.

## Verification tests

```text
spec/lang/effect_fn_test.almd
spec/lang/effect_assign_unwrap_test.almd
spec/lang/effect_if_branch_unwrap_test.almd
spec/lang/effect_result_arg_test.almd
spec/lang/explicit_propagation_test.almd
spec/integration/codegen_effect_fn_test.almd
```
