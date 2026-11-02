> **已移除（ADR-0007）**：`result.collect` / `collect_map` 已不存在，调用现在会产生普通的 [E002](../../../../diagnostics/E002.md)。收集全部错误使用 `result.partition`。

# result

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](result.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](result.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](result.zh-CN.md)



> 原文：[result.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/result.md)。这是面向读者的非规范性固定快照。API 索引对应原文固定时点，本译文不参与自动生成或自动同步。签名、类型、`@since` 及代码示例（包括注释）保持原样，API 说明译文附在末尾。本次翻译未独立运行代码示例。

> 已移除、弃用和删除计划均为原文固定时点的说明。区分 collect 系列已移除与 unwrap_or 的删除计划，并保留立即求值与延迟求值的区别。

Result 类型操作。自动导入。

### `result.map(r: Result[A, E], f: (A) -> B) -> Result[B, E]`

使用函数转换 ok 值。若为 err，则原样传递。

```almd run
fn show(r: Result[Int, String]) -> String = match r {
  ok(n) => "ok(${n})",
  err(e) => "err(\"${e}\")",
}

fn main() -> Unit = {
  println(show(result.map(ok(2), (x) => x * 10)))
  println(show(result.map(err("fail"), (x) => x * 10)))
}
```
```output
ok(20)
err("fail")
```

### `result.map_err(r: Result[A, E], f: (E) -> F) -> Result[A, F]`

使用函数转换 err 值。若为 ok，则原样传递。

```almd run
fn show(r: Result[Int, String]) -> String = match r {
  ok(n) => "ok(${n})",
  err(e) => "err(\"${e}\")",
}

fn main() -> Unit = {
  println(show(result.map_err(err("fail"), (e) => "wrapped: " + e)))
  println(show(result.map_err(ok(1): Result[Int, String], (e) => "wrapped: " + e)))
}
```
```output
err("wrapped: fail")
ok(1)
```

### `result.flat_map(r: Result[A, E], f: (A) -> Result[B, E]) -> Result[B, E]`

在 ok 值上链接返回 Result 的函数。将嵌套的 Result 展平。

```almd run
fn show(r: Result[Int, String]) -> String = match r {
  ok(n) => "ok(${n})",
  err(e) => "err(\"${e}\")",
}

fn main() -> Unit = {
  println(show(result.flat_map(ok(5), (x) => if x > 0 then ok(x) else err("negative"))))
  println(show(result.flat_map(ok(-5), (x) => if x > 0 then ok(x) else err("negative"))))
}
```
```output
ok(5)
err("negative")
```

### `result.unwrap_or(r: Result[A, E], default: A) -> A`

**已弃用（E052、ADR-0005 D4）** — 立即求值的回退是第三种写法，与 `??`（`≡ unwrap_or_else`，默认值仅在回退路径求值）没有定义关系。请写 `value ?? default`；`almide fix` 会改写调用形式和管道阶段形式。在下一个方言迁移窗口删除。


获取 ok 值；若为 err，则返回默认值。

```almd run
fn main() -> Unit = {
  let r: Result[Int, String] = err("fail")
  println("${r ?? 0}")
  println("${(ok(7): Result[Int, String]) ?? 0}")
}
```
```output
0
7
```

### `result.unwrap_or_else(r: Result[A, E], f: (E) -> A) -> A`

获取 ok 值，或用函数根据错误计算默认值。

```almd run
fn main() -> Unit = {
  println("${result.unwrap_or_else(err("fail"), (e) => string.len(e))}")
}
```
```output
4
```

### `result.is_ok(r: Result[A, E]) -> Bool`

检查 Result 是否为 ok。

```almd run
fn main() -> Unit = {
  println("${result.is_ok(ok(42): Result[Int, String])}")
}
```
```output
true
```

### `result.is_err(r: Result[A, E]) -> Bool`

检查 Result 是否为 err。

```almd run
fn main() -> Unit = {
  println("${result.is_err(err("fail"): Result[Int, String])}")
}
```
```output
true
```

### `result.to_option(r: Result[A, E]) -> Option[A]`

将 ok 转为 some，将 err 转为 none。丢弃错误值。

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(result.to_option(ok(42): Result[Int, String])))
  println(show(result.to_option(err("fail"): Result[Int, String])))
}
```
```output
some(42)
none
```

### `result.to_err_option(r: Result[A, E]) -> Option[E]`

将 err 转为 some，将 ok 转为 none。丢弃 ok 值。

```almd run
fn show(o: Option[String]) -> String = match o {
  some(e) => "some(\"${e}\")",
  none => "none",
}

fn main() -> Unit = {
  println(show(result.to_err_option(err("fail"): Result[Int, String])))
  println(show(result.to_err_option(ok(42): Result[Int, String])))
}
```
```output
some("fail")
none
```

### `result.partition(rs: List[Result[T, E]]) -> (List[T], List[E])`

将 Result 列表分为 ok 值和 err 值。这是收集全部错误的基本操作，已移除的 `result.collect` / `result.collect_map` 用它表示：

```almd run
fn main() -> Unit = {
  println("${result.partition([ok(1), err("x"), ok(2)])}")
}

// result.collect(rs)          → let (oks, errs) = result.partition(rs)
// result.collect_map(xs, f)   → let (oks, errs) = result.partition(list.map(xs, f))
```
```output
([1, 2], ["x"])
```


## 原文固定快照 API 索引（15个函数）

```
// ok(f(v)) for ok(v); err passes through.
// @since 0.5.13 or earlier
result.map(r: Result[A, E], f: (A) -> B) -> Result[B, E]

// err(f(e)) for err(e); ok passes through.
// @since 0.5.13 or earlier
result.map_err(r: Result[A, E], f: (E) -> F) -> Result[A, F]

// f(v) for ok(v); err passes through.
// @since 0.6.1 or earlier
result.flat_map(r: Result[A, E], f: (A) -> Result[B, E]) -> Result[B, E]

// v for ok(v), else default (always evaluated).
// @since 0.5.13 or earlier
result.unwrap_or(r: Result[A, E], default: A) -> A   (deprecated — use ??)

// v for ok(v), else f(e); f runs only on err.
// @since 0.5.13 or earlier
result.unwrap_or_else(r: Result[A, E], f: (E) -> A) -> A

// true for ok(_), false for err(_).
// @since 0.6.0 or earlier
result.is_ok(r: Result[A, E]) -> Bool

// true for err(_), false for ok(_).
// @since 0.6.0 or earlier
result.is_err(r: Result[A, E]) -> Bool

// some(v) for ok(v); none for err, e dropped.
// @since 0.5.13 or earlier
result.to_option(r: Result[A, E]) -> Option[A]

// some(e) for err(e); none for ok.
// @since 0.5.13 or earlier
result.to_err_option(r: Result[A, E]) -> Option[E]

// (ok values, err values), each in list order.
// @since 0.9.1 or earlier
result.partition(xs: List[Result[A, E]]) -> (List[A], List[E])

// Inner result of ok(inner); outer err kept.
// @since 0.15.0 or earlier
result.flatten(r: Result[Result[A, E], E]) -> Result[A, E]

// [v] for ok(v), [] for err.
// @since 0.15.0 or earlier
result.to_list(r: Result[A, E]) -> List[A]

// ok((x, y)) if both ok, else the first err.
// @since 0.15.0 or earlier
result.zip(a: Result[A, E], b: Result[B, E]) -> Result[(A, B), E]

// r when ok, else f(e); f runs only on err.
// @since 0.15.0 or earlier
result.or_else(r: Result[A, E], f: (E) -> Result[A, F]) -> Result[A, F]

// err(err_val) if f fails on ok; err kept.
// @since 0.15.0 or earlier
result.filter(r: Result[A, E], f: (A) -> Bool, err_val: E) -> Result[A, E]
```


## API 说明译文（按上述签名顺序）

- `result.map(r: Result[A, E], f: (A) -> B) -> Result[B, E]`：ok(v) 得到 ok(f(v))；err 原样传递。
- `result.map_err(r: Result[A, E], f: (E) -> F) -> Result[A, F]`：err(e) 得到 err(f(e))；ok 原样传递。
- `result.flat_map(r: Result[A, E], f: (A) -> Result[B, E]) -> Result[B, E]`：ok(v) 得到 f(v)；err 原样传递。
- `result.unwrap_or(r: Result[A, E], default: A) -> A   (deprecated — use ??)`：ok(v) 得到 v，否则为 default（总是求值）。
- `result.unwrap_or_else(r: Result[A, E], f: (E) -> A) -> A`：ok(v) 得到 v，否则为 f(e)；仅在 err 时运行 f。
- `result.is_ok(r: Result[A, E]) -> Bool`：ok(_) 为 true，err(_) 为 false。
- `result.is_err(r: Result[A, E]) -> Bool`：err(_) 为 true，ok(_) 为 false。
- `result.to_option(r: Result[A, E]) -> Option[A]`：ok(v) 得到 some(v)，err 得到 none，丢弃 e。
- `result.to_err_option(r: Result[A, E]) -> Option[E]`：err(e) 得到 some(e)，ok 得到 none。
- `result.partition(xs: List[Result[A, E]]) -> (List[A], List[E])`：(ok 值，err 值)，各自保持列表顺序。
- `result.flatten(r: Result[Result[A, E], E]) -> Result[A, E]`：ok(inner) 的内部 result；保留外层 err。
- `result.to_list(r: Result[A, E]) -> List[A]`：ok(v) 得到 [v]，err 得到 []。
- `result.zip(a: Result[A, E], b: Result[B, E]) -> Result[(A, B), E]`：两者均为 ok 时得到 ok((x, y))，否则为第一个 err。
- `result.or_else(r: Result[A, E], f: (E) -> Result[A, F]) -> Result[A, F]`：为 ok 时返回 r，否则为 f(e)；仅在 err 时运行 f。
- `result.filter(r: Result[A, E], f: (A) -> Bool, err_val: E) -> Result[A, E]`：若对 ok 的 f 判定失败，则 err(err_val)；保留 err。
