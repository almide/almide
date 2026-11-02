# testing

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](testing.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](testing.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](testing.zh-CN.md)



> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/testing.md)的非规范读者译文，不自动生成或同步。签名、代码及代码注释保持原文，API 说明译文另附在代码外。翻译时没有独立执行示例或验证实现。


测试断言。import testing。

### `testing.assert_aborts[T](expected: String, f: () -> T) -> Unit`

断言 `f` 以包含 `expected` 的文本中止。仅 `f` 在重新启动程序的子进程（native）或新实例（wasm）中运行，因此结构化的 Almide 中止都计入——`panic`、下标越界、除零、断言失败——并且调用之后测试继续。`f` 必须是在调用处编写且不捕获测试中任何值的 lambda（E098），调用必须直接位于 `test` 块中（E099）。仅在断言失败时显示子进程的输出。原始的宿主 panic、非结构化的 Wasm trap 或非中止退出，不被接受为 Almide 中止。

```almd check
import testing

fn nth(xs: List[Int], i: Int) -> Int = xs[i]

test "reading past the end aborts" {
  testing.assert_aborts("index out of bounds", () => nth([1, 2, 3], 5))
  testing.assert_aborts("PANIC: oh no", () => panic("oh no"))
  assert_eq(nth([1, 2, 3], 0), 1)
}
```

### `testing.assert_contains(haystack: String, needle: String) -> Unit`

断言字符串包含子字符串。

```almd check
import testing

test "substring present" {
  testing.assert_contains("hello world", "world")
}
```

### `testing.assert_approx(a: Float, b: Float, tolerance: Float) -> Unit`

断言两个浮点数在容差内近似相等。

```almd check
import testing

test "close enough" {
  testing.assert_approx(3.14, 3.14159, 0.01)
}
```

### `testing.assert_gt(a: A, b: A) -> Unit`

断言 a 大于 b。

```almd check
import testing

test "greater" {
  testing.assert_gt(10, 5)
}
```

### `testing.assert_lt(a: A, b: A) -> Unit`

断言 a 小于 b。

```almd check
import testing

test "less" {
  testing.assert_lt(3, 7)
}
```

### `testing.assert_some(opt: Option[A]) -> Unit`

断言 Option 是 some（而不是 none）。

```almd check
import testing

test "option is some" {
  testing.assert_some(some("value"))
}
```

### `testing.assert_ok(result: Result[A, E]) -> Unit`

断言 Result 是 ok（而不是 err）。

```almd check
import testing

test "result is ok" {
  let result: Result[String, String] = ok("success")
  testing.assert_ok(result)
}
```

### `testing.assert_snapshot(actual: String, expected: String) -> Unit`

断言 `actual` 等于写在调用处的快照。预期结果就是字面量本身，没有附属文件；接受步骤原地重写它：从 `""` 开始，运行 `almide test --update-snapshots <file>`，得到的值写回第二个参数（跨行时使用 heredoc）。之后的变化会让普通的 `almide test` 运行失败，并显示差异和接受提示；`--update-snapshots` 会再次重写。在 CI 模式（`--ci` 或 `CI=true`）中绝不写入；新增或变化的快照在那里失败，因此快照需要像代码一样提交和评审。

```almd check
import testing

fn render(xs: List[Int]) -> String = xs |> list.map((x) => "item ${x}") |> list.join("\n")

test "single line" {
  testing.assert_snapshot("hello", "hello")
}

test "multi line, written back as a heredoc" {
  testing.assert_snapshot(render([1, 2]), """
    item 1
    item 2
    """)
}
```

第二个参数必须是普通字符串字面量（无插值），接受步骤才能重写它。不匹配会在每个目标上以结构化块 `Error: snapshot mismatch` / `at:` / `expected:` / `found:` 和退出码 1 中止运行（契约 C-336）。



<a id="signature-index-10-functions"></a>

## 签名索引 (10 个函数)

```
// Fails unless f, run alone in a child, aborts with text containing expected.
// @since 0.68.0
testing.assert_aborts(expected: String, f: () -> A) -> Unit

// Fails unless needle is in haystack; "" passes.
// @since 0.6.0 or earlier
testing.assert_contains(haystack: String, needle: String) -> Unit

// Fails unless abs(a - b) < tolerance (strict).
// @since 0.6.0 or earlier
testing.assert_approx(a: Float, b: Float, tolerance: Float) -> Unit

// Fails unless a > b; equal fails.
// @since 0.6.0 or earlier
testing.assert_gt(a: A, b: A) -> Unit

// Fails unless a < b; equal fails.
// @since 0.6.0 or earlier
testing.assert_lt(a: A, b: A) -> Unit

// Fails when o is none.
// @since 0.6.0 or earlier
testing.assert_some(o: Option[A]) -> Unit

// Fails when r is an err.
// @since 0.6.0 or earlier
testing.assert_ok(r: Result[A, B]) -> Unit

// Fails when o is some.
// @since 0.54.0
testing.assert_none(o: Option[A]) -> Unit

// Fails when r is ok.
// @since 0.54.0
testing.assert_err(r: Result[A, B]) -> Unit

// Fails unless actual == expected; --update-snapshots rewrites.
// @since 0.62.0
testing.assert_snapshot(actual: String, expected: String) -> Unit
```



## API 说明参考译文

- `testing.assert_aborts(expected: String, f: () -> A) -> Unit` — 若单独在子执行环境中运行的 f 未以包含 expected 的文本中止，则失败。
- `testing.assert_contains(haystack: String, needle: String) -> Unit` — 若 haystack 不包含 needle，则失败；空字符串 "" 通过。
- `testing.assert_approx(a: Float, b: Float, tolerance: Float) -> Unit` — 若不满足 abs(a - b) < tolerance（严格小于），则失败。
- `testing.assert_gt(a: A, b: A) -> Unit` — 若不满足 a > b，则失败；相等也失败。
- `testing.assert_lt(a: A, b: A) -> Unit` — 若不满足 a < b，则失败；相等也失败。
- `testing.assert_some(o: Option[A]) -> Unit` — o 为 none 时失败。
- `testing.assert_ok(r: Result[A, B]) -> Unit` — r 为 err 时失败。
- `testing.assert_none(o: Option[A]) -> Unit` — o 为 some 时失败。
- `testing.assert_err(r: Result[A, B]) -> Unit` — r 为 ok 时失败。
- `testing.assert_snapshot(actual: String, expected: String) -> Unit` — 若 actual != expected 则失败；--update-snapshots 会重写。
