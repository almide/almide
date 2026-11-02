# option

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](option.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](option.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](option.zh-CN.md)



> 原文：[option.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/option.md)。这是面向读者的非规范性固定快照。API 索引对应原文固定时点，本译文不参与自动生成或自动同步。签名、类型、`@since` 及代码示例（包括注释）保持原样，API 说明译文附在末尾。本次翻译未独立运行代码示例。

> 弃用状态和删除计划均为原文固定时点的说明。保留了立即求值的 unwrap_or 与仅回退时求值的 ?? / unwrap_or_else 之间的条件差异。

Option 类型操作。自动导入。

### `option.map(o: Option[A], f: (A) -> B) -> Option[B]`

使用函数转换内部值。如果为 none，则返回 none。

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(option.map(some(2), (x) => x * 10)))
}
```
```output
some(20)
```

### `option.flat_map(o: Option[A], f: (A) -> Option[B]) -> Option[B]`

在内部值上链接返回 Option 的函数。将嵌套的 Option 展平。

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(option.flat_map(some(5), (x) => if x > 0 then some(x) else none)))
  println(show(option.flat_map(some(-5), (x) => if x > 0 then some(x) else none)))
}
```
```output
some(5)
none
```

### `option.flatten(o: Option[Option[A]]) -> Option[A]`

展平嵌套的 Option。some(some(x)) 变为 some(x)，some(none) 变为 none。

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(option.flatten(some(some(42)))))
  println(show(option.flatten(some(none))))
}
```
```output
some(42)
none
```

### `option.unwrap_or(o: Option[A], default: A) -> A`

获取内部值；若为 none，则返回默认值。

**已弃用（E052、ADR-0005 D4）** — 立即求值的回退是第三种写法，与 `??`（`≡ unwrap_or_else`，默认值仅在回退路径求值）没有定义关系。请写 `value ?? default`；`almide fix` 会改写调用形式和管道阶段形式。在下一个方言迁移窗口删除。


```almd run
fn main() -> Unit = {
  let n: Int? = none
  println("${n ?? 0}")
  println("${some(7) ?? 0}")
}
```
```output
0
7
```

### `option.unwrap_or_else(o: Option[A], f: () -> A) -> A`

获取内部值，或用函数计算默认值。

```almd run
fn main() -> Unit = {
  println("${option.unwrap_or_else(none, () => 42)}")
}
```
```output
42
```

### `option.is_some(o: Option[A]) -> Bool`

检查 Option 是否包含值。

```almd run
fn main() -> Unit = {
  println("${option.is_some(some(42))}")
}
```
```output
true
```

### `option.is_none(o: Option[A]) -> Bool`

检查 Option 是否为 none。

```almd run
fn main() -> Unit = {
  println("${option.is_none(none: Option[Int])}")
}
```
```output
true
```

### `option.to_result(o: Option[A], err: E) -> Result[A, E]`

将 some 转为 ok，将 none 转为包含给定 E 类型错误值的 err。

```almd run
fn show(r: Result[Int, String]) -> String = match r {
  ok(n) => "ok(${n})",
  err(e) => "err(\"${e}\")",
}

fn main() -> Unit = {
  println(show(option.to_result(some(42), "missing")))
  println(show(option.to_result(none, "missing")))
}
```
```output
ok(42)
err("missing")
```

### `option.filter(o: Option[A], f: (A) -> Bool) -> Option[A]`

若值满足谓词则保留，否则返回 none。

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(option.filter(some(5), (x) => x > 3)))
  println(show(option.filter(some(2), (x) => x > 3)))
}
```
```output
some(5)
none
```

### `option.zip(a: Option[A], b: Option[B]) -> Option[(A, B)]`

将两个 Option 合并为元组的 Option。任一为 none 时结果为 none。

```almd run
fn show(o: Option[(Int, Int)]) -> String = match o {
  some((a, b)) => "some((${a}, ${b}))",
  none => "none",
}

fn main() -> Unit = {
  println(show(option.zip(some(1), some(2))))
  println(show(option.zip(some(1), none)))
}
```
```output
some((1, 2))
none
```

### `option.or_else(o: Option[A], f: () -> Option[A]) -> Option[A]`

若为 some 则返回该 Option，否则调用函数生成替代值。

```almd run
fn show(o: Option[Int]) -> String = match o {
  some(n) => "some(${n})",
  none => "none",
}

fn main() -> Unit = {
  println(show(option.or_else(none, () => some(42))))
  println(show(option.or_else(some(1), () => some(42))))
}
```
```output
some(42)
some(1)
```

### `option.to_list(o: Option[A]) -> List[A]`

将 some(x) 转为 [x]，将 none 转为 []。

```almd run
fn main() -> Unit = {
  println("${option.to_list(some(42))}")
  println("${option.to_list(none: Option[Int])}")
}
```
```output
[42]
[]
```


## 原文固定快照 API 索引（14个函数）

```
// some(f(v)) for some(v); none stays none.
// @since 0.7.0 or earlier
option.map(o: Option[A], f: (A) -> B) -> Option[B]

// f(v) for some(v); none stays none.
// @since 0.7.0 or earlier
option.flat_map(o: Option[A], f: (A) -> Option[B]) -> Option[B]

// Inner option of some(inner); none for none.
// @since 0.7.0 or earlier
option.flatten(o: Option[Option[A]]) -> Option[A]

// v for some(v), else default (always evaluated).
// @since 0.7.0 or earlier
option.unwrap_or(o: Option[A], default: A) -> A   (deprecated — use ??)

// v for some(v), else f(); f runs only for none.
// @since 0.7.0 or earlier
option.unwrap_or_else(o: Option[A], f: () -> A) -> A

// true for some(_), false for none.
// @since 0.7.0 or earlier
option.is_some(o: Option[A]) -> Bool

// true for none, false for some(_).
// @since 0.7.0 or earlier
option.is_none(o: Option[A]) -> Bool

// ok(v) for some(v), err(e) for none.
// @since 0.7.0 or earlier
option.to_result(o: Option[A], e: E) -> Result[A, E]

// o when some(v) and f(v) holds, else none.
// @since 0.7.0 or earlier
option.filter(o: Option[A], f: (A) -> Bool) -> Option[A]

// some((x, y)) when both are some, else none.
// @since 0.7.0 or earlier
option.zip(a: Option[A], b: Option[B]) -> Option[(A, B)]

// o when some, else f(); f runs only for none.
// @since 0.7.0 or earlier
option.or_else(o: Option[A], f: () -> Option[A]) -> Option[A]

// [v] for some(v), [] for none.
// @since 0.7.0 or earlier
option.to_list(o: Option[A]) -> List[A]

// some(values) if no element is none, else none.
// @since 0.15.0 or earlier
option.collect(xs: List[Option[A]]) -> Option[List[A]]

// some of all f(x), or none; f runs on every x.
// @since 0.15.0 or earlier
option.collect_map(xs: List[A], f: (A) -> Option[B]) -> Option[List[B]]
```


## API 说明译文（按上述签名顺序）

- `option.map(o: Option[A], f: (A) -> B) -> Option[B]`：some(v) 得到 some(f(v))；none 保持 none。
- `option.flat_map(o: Option[A], f: (A) -> Option[B]) -> Option[B]`：some(v) 得到 f(v)；none 保持 none。
- `option.flatten(o: Option[Option[A]]) -> Option[A]`：some(inner) 的内部 option；none 得到 none。
- `option.unwrap_or(o: Option[A], default: A) -> A   (deprecated — use ??)`：some(v) 得到 v，否则为 default（总是求值）。
- `option.unwrap_or_else(o: Option[A], f: () -> A) -> A`：some(v) 得到 v，否则为 f()；仅在 none 时运行 f。
- `option.is_some(o: Option[A]) -> Bool`：some(_) 为 true，none 为 false。
- `option.is_none(o: Option[A]) -> Bool`：none 为 true，some(_) 为 false。
- `option.to_result(o: Option[A], e: E) -> Result[A, E]`：some(v) 得到 ok(v)，none 得到 err(e)。
- `option.filter(o: Option[A], f: (A) -> Bool) -> Option[A]`：为 some(v) 且 f(v) 成立时返回 o，否则为 none。
- `option.zip(a: Option[A], b: Option[B]) -> Option[(A, B)]`：两者均为 some 时得到 some((x, y))，否则为 none。
- `option.or_else(o: Option[A], f: () -> Option[A]) -> Option[A]`：为 some 时返回 o，否则为 f()；仅在 none 时运行 f。
- `option.to_list(o: Option[A]) -> List[A]`：some(v) 得到 [v]，none 得到 []。
- `option.collect(xs: List[Option[A]]) -> Option[List[A]]`：没有元素为 none 时得到 some(values)，否则为 none。
- `option.collect_map(xs: List[A], f: (A) -> Option[B]) -> Option[List[B]]`：包含所有 f(x) 的 some，或 none；对每个 x 都运行 f。
