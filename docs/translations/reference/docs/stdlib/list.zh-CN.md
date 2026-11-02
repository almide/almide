# list

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](list.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](list.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](list.zh-CN.md)



> 原文：[list.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/list.md)。这是面向读者、固定源版本的非规范性快照，以英文原文为准。API 索引保留源版本固定时的内容，本译文不会自动生成或自动同步。签名、类型、版本注记和代码示例（包括注释）保持原样，文末按顺序附上全部 74 项 API 说明的译文。本次翻译未独立执行或重新验证代码示例、行为或主张。
>
> 原文注意：正文关于检查脚本和机器验证的描述针对固定版本的英文原文，不表示此译文文件已自动纳入相同检查范围。

列表操作。自动导入。

### `list.len(xs: List[A]) -> Int`

返回列表中的元素个数。

```almd run
fn main() -> Unit = {
  println("${list.len([1, 2, 3])}")
}
```
```output
3
```

### `list.get(xs: List[A], i: Int) -> Option[A]`

获取索引 i 处的元素；越界时返回 none。

```almd run
fn main() -> Unit = {
  println("${list.get([10, 20, 30], 1)}")
}
```
```output
some(20)
```

### `list.set(xs: List[A], i: Int, x: A) -> List[A]`

返回一个新列表，其中索引 i 处的元素已被替换。

```almd run
fn main() -> Unit = {
  println("${list.set([1, 2, 3], 1, 99)}")
}
```
```output
[1, 99, 3]
```

`list.set` 会复制整个列表，每次调用为 O(n)。在 `var` 上，将结果赋回来源列表，即 `xs = list.set(xs, i, v)`（或 `xs = xs.set(i, v)`），会被编译为对该单个槽位的原地写入（#2244）：值语义不变，之前取得的别名保留旧列表，越界索引不改变任何内容。未共享的缓冲区可以不经复制就更新；写时复制仍会保护存活的别名，因此反复创建别名可能增加复制成本。语句形式 `xs[i] = v` 是同一写入的直接写法，但索引越界时它会中止执行，而 `list.set` 不做任何操作。

### `list.swap(xs: List[A], i: Int, j: Int) -> List[A]`

返回一个新列表，其中索引 i 与 j 处的元素已交换。

```almd run
fn main() -> Unit = {
  println("${list.swap([1, 2, 3], 0, 2)}")
}
```
```output
[3, 2, 1]
```

### `list.sort(xs: List[A]) -> List[A]`

按升序对列表排序。

```almd run
fn main() -> Unit = {
  println("${list.sort([3, 1, 2])}")
}
```
```output
[1, 2, 3]
```

### `list.reverse(xs: List[A]) -> List[A]`

反转元素顺序。

```almd run
fn main() -> Unit = {
  println("${list.reverse([1, 2, 3])}")
}
```
```output
[3, 2, 1]
```

### `list.contains(xs: List[A], x: A) -> Bool`

检查列表是否包含某个元素。

```almd run
fn main() -> Unit = {
  println("${list.contains([1, 2, 3], 2)}")
}
```
```output
true
```

### `list.enumerate(xs: List[A]) -> List[(Int, A)]`

将每个元素与其索引配对。

```almd run
fn main() -> Unit = {
  println("${list.enumerate(["a", "b"])}")
}
```
```output
[(0, "a"), (1, "b")]
```

### `list.zip(xs: List[A], ys: List[B]) -> List[(A, B)]`

将两个列表合并为一个由二元组组成的列表。

```almd run
fn main() -> Unit = {
  println("${list.zip([1, 2], ["a", "b"])}")
}
```
```output
[(1, "a"), (2, "b")]
```

### `list.flatten(xs: List[List[A]]) -> List[A]`

将列表的列表展平为单个列表。

```almd run
fn main() -> Unit = {
  println("${list.flatten([[1, 2], [3]])}")
}
```
```output
[1, 2, 3]
```

### `list.take(xs: List[A], n: Int) -> List[A]`

取前 n 个元素。

```almd run
fn main() -> Unit = {
  println("${list.take([1, 2, 3, 4], 2)}")
}
```
```output
[1, 2]
```

### `list.drop(xs: List[A], n: Int) -> List[A]`

丢弃前 n 个元素。

```almd run
fn main() -> Unit = {
  println("${list.drop([1, 2, 3, 4], 2)}")
}
```
```output
[3, 4]
```

### `list.unique(xs: List[A]) -> List[A]`

去除重复元素，保留首次出现的元素。

```almd run
fn main() -> Unit = {
  println("${list.unique([1, 2, 1, 3])}")
}
```
```output
[1, 2, 3]
```

### `list.index_of(xs: List[A], x: A) -> Option[Int]`

查找某元素首次出现的索引；没有则返回 none。

```almd run
fn main() -> Unit = {
  println("${list.index_of([10, 20, 30], 20)}")
}
```
```output
some(1)
```

### `list.last(xs: List[A]) -> Option[A]`

获取最后一个元素；为空时返回 none。

```almd run
fn main() -> Unit = {
  println("${list.last([1, 2, 3])}")
}
```
```output
some(3)
```

### `list.chunk(xs: List[A], n: Int) -> List[List[A]]`

将列表分成大小为 n 的块。

```almd run
fn main() -> Unit = {
  println("${list.chunk([1, 2, 3, 4, 5], 2)}")
}
```
```output
[[1, 2], [3, 4], [5]]
```

### `list.sum(xs: List[A]) -> A`

对列表中的数值元素求和；空列表返回零。整数溢出时回绕。

```almd run
fn main() -> Unit = {
  println("${list.sum([1, 2, 3])}")
}
```
```output
6
```

### `list.product(xs: List[A]) -> A`

对列表中的数值元素求积；空列表返回一。整数溢出时回绕。

```almd run
fn main() -> Unit = {
  println("${list.product([2, 3, 4])}")
}
```
```output
24
```

### `list.first(xs: List[A]) -> Option[A]`

获取第一个元素；为空时返回 none。

```almd run
fn main() -> Unit = {
  println("${list.first([1, 2, 3])}")
}
```
```output
some(1)
```

### `list.is_empty(xs: List[A]) -> Bool`

检查列表是否为空。

```almd run
fn main() -> Unit = {
  println("${list.is_empty([]: List[Int])}")
}
```
```output
true
```

### `list.min(xs: List[A]) -> Option[A]`

查找最小元素；为空时返回 none。

```almd run
fn main() -> Unit = {
  println("${list.min([3, 1, 2])}")
}
```
```output
some(1)
```

### `list.max(xs: List[A]) -> Option[A]`

查找最大元素；为空时返回 none。

```almd run
fn main() -> Unit = {
  println("${list.max([3, 1, 2])}")
}
```
```output
some(3)
```

### `list.join(xs: List[T], sep: T) -> T`

用同类型的分隔符连接 String 或 Bytes 列表。

```almd run
fn main() -> Unit = {
  println("${list.join(["a", "b", "c"], "-")}")
}
```
```output
a-b-c
```

### `list.map(xs: List[A], f: (A) -> B) -> List[B]`

对每个元素应用函数，返回一个新列表。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3].map((x) => x * 2)}")
}
```
```output
[2, 4, 6]
```

### `list.filter(xs: List[A], f: (A) -> Bool) -> List[A]`

保留满足谓词的元素。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3, 4].filter((x) => x > 2)}")
}
```
```output
[3, 4]
```

### `list.find(xs: List[A], f: (A) -> Bool) -> Option[A]`

查找第一个匹配谓词的元素。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3].find((x) => x > 1)}")
}
```
```output
some(2)
```

### `list.any(xs: List[A], f: (A) -> Bool) -> Bool`

检查是否有任一元素满足谓词。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3].any((x) => x > 2)}")
}
```
```output
true
```

### `list.all(xs: List[A], f: (A) -> Bool) -> Bool`

检查是否所有元素都满足谓词。

```almd run
fn main() -> Unit = {
  println("${[2, 4, 6].all((x) => x % 2 == 0)}")
}
```
```output
true
```

### `list.sort_by(xs: List[A], f: (A) -> B) -> List[A]`

按键提取函数（而非比较器）排序。f 从每个元素提取排序键。

```almd run
fn main() -> Unit = {
  println("${["bb", "a", "ccc"].sort_by((s) => string.len(s))}")
}
```
```output
["a", "bb", "ccc"]
```

### `list.flat_map(xs: List[A], f: (A) -> List[B]) -> List[B]`

将每个元素映射为列表，再将结果展平。

```almd run
fn main() -> Unit = {
  println("${[1, 2].flat_map((x) => [x, x * 10])}")
}
```
```output
[1, 10, 2, 20]
```

### `list.filter_map(xs: List[A], f: (A) -> Option[B]) -> List[B]`

一次遍历中完成映射和过滤：只保留 some 值。

```almd run
fn main() -> Unit = {
  println("${["1", "x", "3"].filter_map((s) => int.parse(s)?)}")
}
```
```output
[1, 3]
```

### `list.find_map(xs: List[A], f: (A) -> Option[B]) -> Option[B]`

返回 f 给出的第一个 some 值；没有则返回 none。命中后不再调用 f，即短路版本的 `filter_map(xs, f) |> list.first`。与其他列表高阶函数一样，使用 `!` 传播的回调会使整个调用变为可失败调用。

```almd run
fn main() -> Unit = {
  println("${["x", "2", "3"].find_map((s) => int.parse(s)?)}")
}
```
```output
some(2)
```

### `list.take_while(xs: List[A], f: (A) -> Bool) -> List[A]`

从头部取元素，直到谓词不成立。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3, 1].take_while((x) => x < 3)}")
}
```
```output
[1, 2]
```

### `list.drop_while(xs: List[A], f: (A) -> Bool) -> List[A]`

从头部丢弃元素，直到谓词不成立。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3, 1].drop_while((x) => x < 3)}")
}
```
```output
[3, 1]
```

### `list.count(xs: List[A], f: (A) -> Bool) -> Int`

统计满足谓词的元素数量。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3, 4].count((x) => x > 2)}")
}
```
```output
2
```

### `list.partition(xs: List[A], f: (A) -> Bool) -> (List[A], List[A])`

将列表分成两个：匹配谓词的元素和不匹配的元素。

```almd check
fn main() -> Unit = {
  let (evens, odds) = [1, 2, 3, 4].partition((x) => x % 2 == 0)
  println("${evens}")
  println("${odds}")
}
```

### `list.reduce(xs: List[A], f: (A, A) -> A) -> Option[A]`

通过两两结合元素来归约列表。为空时返回 none。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3].reduce((a, b) => a + b)}")
}
```
```output
some(6)
```

### `list.group_by(xs: List[A], f: (A) -> B) -> Map[B, List[A]]`

按键函数将元素分组到一个 map 中。

```almd run
fn main() -> Unit = {
  let groups = ["hi", "hey", "bye"].group_by((s) => string.take(s, 1))
  println("${map.get(groups, "h")}")
  println("${map.get(groups, "b")}")
}
```
```output
some(["hi", "hey"])
some(["bye"])
```

### `list.range(start: Int, end: Int) -> List[Int]`

创建从 start（含）到 end（不含）的整数列表。

```almd run
fn main() -> Unit = {
  println("${list.range(1, 5)}")
}
```
```output
[1, 2, 3, 4]
```

### `list.slice(xs: List[A], start: Int, end: Int) -> List[A]`

提取从 start 到 end 索引的子列表。

```almd run
fn main() -> Unit = {
  println("${list.slice([1, 2, 3, 4, 5], 1, 4)}")
}
```
```output
[2, 3, 4]
```

### `list.insert(xs: List[A], i: Int, x: A) -> List[A]`

在索引 i 处插入元素，将元素右移。

```almd run
fn main() -> Unit = {
  println("${list.insert([1, 3], 1, 2)}")
}
```
```output
[1, 2, 3]
```

### `list.remove_at(xs: List[A], i: Int) -> List[A]`

删除索引 i 处的元素。

```almd run
fn main() -> Unit = {
  println("${list.remove_at([1, 2, 3], 1)}")
}
```
```output
[1, 3]
```

### `list.find_index(xs: List[A], f: (A) -> Bool) -> Option[Int]`

查找谓词成立的第一个索引。

```almd run
fn main() -> Unit = {
  println("${[10, 20, 30].find_index((x) => x > 15)}")
}
```
```output
some(1)
```

### `list.update(xs: List[A], i: Int, f: (A) -> A) -> List[A]`

返回一个新列表，其中索引 i 处的元素经 f 变换。

```almd run
fn main() -> Unit = {
  println("${list.update([1, 2, 3], 1, (x) => x * 10)}")
}
```
```output
[1, 20, 3]
```

### `list.repeat(x: A, n: Int) -> List[A]`

创建一个将某个值重复 n 次的列表。

```almd run
fn main() -> Unit = {
  println("${list.repeat(0, 3)}")
}
```
```output
[0, 0, 0]
```

### `list.scan(xs: List[A], init: B, f: (B, A) -> B) -> List[B]`

类似 fold，但返回所有中间累加器值。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3].scan(0, (acc, x) => acc + x)}")
}
```
```output
[1, 3, 6]
```

### `list.intersperse(xs: List[A], sep: A) -> List[A]`

在各元素之间插入分隔符。

```almd run
fn main() -> Unit = {
  println("${list.intersperse([1, 2, 3], 0)}")
}
```
```output
[1, 0, 2, 0, 3]
```

### `list.dedup(xs: List[A]) -> List[A]`

去除连续重复项。

```almd
list.dedup([1, 1, 2, 2, 1]) // => [1, 2, 1]
```

这是固定相邻重复规则的可执行示例（dedup 并非全局去重，末尾的 `1` 会保留），由 `scripts/check-doc-fences.sh` 设置门禁检查：

```almd run
fn main() -> Unit = {
  let xs = list.dedup([1, 1, 2, 2, 1])
  println(xs |> list.map((x) => int.to_string(x)) |> list.join(","))
  println(int.to_string(list.len(xs)))
}
```
```output
1,2,1
3
```

### `list.zip_with(xs: List[A], ys: List[B], f: (A, B) -> C) -> List[C]`

使用函数逐元素组合两个列表。

```almd run
fn main() -> Unit = {
  println("${list.zip_with([1, 2], [10, 20], (a, b) => a + b)}")
}
```
```output
[11, 22]
```

### `list.fold(xs: List[A], init: B, f: (B, A) -> B) -> B`

从初始累加器出发，从左侧归约列表。

```almd run
fn main() -> Unit = {
  println("${[1, 2, 3].fold(0, (acc, x) => acc + x)}")
}
```
```output
6
```

### `list.take_end(xs: List[A], n: Int) -> List[A]`

取最后 N 个元素。

```almd run
fn main() -> Unit = {
  println("${list.take_end([1, 2, 3, 4], 2)}")
}
```
```output
[3, 4]
```

### `list.drop_end(xs: List[A], n: Int) -> List[A]`

丢弃最后 N 个元素。

```almd run
fn main() -> Unit = {
  println("${list.drop_end([1, 2, 3, 4], 2)}")
}
```
```output
[1, 2]
```

### `list.unique_by(xs: List[A], f: (A) -> K) -> List[A]`

按键函数去重，保留首次出现的元素。

```almd check
fn main() -> Unit = {
  let firsts = list.unique_by(["aa", "ab", "ba"], (s) => string.get(s, 0))
  println("${firsts}")
}
```

### `list.window(xs: List[A], n: Int) -> List[List[A]]`

在列表上使用大小为 N 的滑动窗口。

```almd run
fn main() -> Unit = {
  println("${list.window([1, 2, 3, 4], 2)}")
}
```
```output
[[1, 2], [2, 3], [3, 4]]
```

### `list.push(mut xs: List[A], x: A) -> Unit`

原地追加一个元素。需要 var 绑定。

```almd run
fn main() -> Unit = {
  var xs = [1, 2, 3]
  list.push(xs, 42)
  println("${xs}")
}
```
```output
[1, 2, 3, 42]
```

### `list.pop(mut xs: List[A]) -> Option[A]`

原地删除最后一个元素并返回它（`some(x)`；列表为空时为 `none`）。需要 var 绑定。

```almd run
fn main() -> Unit = {
  var xs = [1, 2, 3]
  let popped = list.pop(xs)
  println("${popped}")
  println("${xs}")
}
```
```output
some(3)
[1, 2]
```

### `list.clear(mut xs: List[A]) -> Unit`

原地删除所有元素。需要 var 绑定。

```almd run
fn main() -> Unit = {
  var xs = [1, 2, 3]
  list.clear(xs)
  println("${xs}")
  println("${list.is_empty(xs)}")
}
```
```output
[]
true
```


<a id="fallible-pipelines--the-polymorphic-core-adr-0006"></a>

## 可失败流水线 — 多态核心（ADR-0006）

使用 `!` 传播的回调（或具名的 `-> T!E` 函数）会实例化核心组合子的可失败形式：调用产生带有回调错误类型的 `Result[_, E]`（普通 `T!` 的错误类型为 `String`），并在**第一个 err 处短路**（之后的元素不会被求值）。每个组合子只有一个名字，策略由标记承载：

```almd
effect fn read_all(files: List[String]) -> List[Entry] =
  files |> list.map((f) => read_meta(f)!)!
```

覆盖 `map` / `filter` / `flat_map` / `filter_map` / `fold` / `find` / `each`。已移除的对应 `list.try_*`（v0.55.0 有一个弃用窗口，自 v0.56.0 起移除）可机械式改写，E043 给出确切形式：

```almd
list.try_map(xs, f)      →  list.map(xs, (x) => f(x)!)!
list.try_fold(xs, z, f)  →  list.fold(xs, z, (a, x) => f(a, x)!)!
```

每个接收回调的列表函数都接受可失败回调（#3163）：不仅是 `map`，还包括谓词查询（`any` / `all` / `count` / `find_index` / `partition` / `take_while` / `drop_while`）、累加器（`reduce` / `scan` / `zip_with` / `update` / `iterate`）和键提取器（`sort_by` / `group_by` / `unique_by`；它们在任何重排前按列表顺序将每个键计算一次，因此第一个 err 有明确定义）。同一规则也覆盖 map、set 和 option 的高阶函数。对应矩阵为 `almide_types::fallible_hofs`；最终状态（每个单元都有无法从源码中直接命名的 `__fallible_*` 承载实现，且公开 try_ 接口为空）由 `tests/list_fallible_family_gate_test.rs` 进行机器检查。

若从不报错的回调使 `E` 不受约束，请标注结果类型：`let evens: Result[List[Int], String] = list.filter(xs, (n) => ok(n % 2 == 0)!)`。


<a id="signature-index-74-functions"></a>

## 固定快照 API 索引（74 个函数）

```
// Element count; 0 for an empty list.
// @since 0.5.0 or earlier
list.len(xs: List[A]) -> Int

// Alias of list.len.
// @since 0.19.2 or earlier
list.length(xs: List[A]) -> Int   (deprecated — use list.len)

// Element at index i, or none when out of range.
// @since 0.5.0 or earlier
list.get(xs: List[A], i: Int) -> Option[A]

// Copy with slot i = x; no-op if i out of range.
// @since 0.5.0 or earlier
list.set(xs: List[A], i: Int, x: A) -> List[A]

// Copy with i and j swapped; no-op if either OOB.
// @since 0.5.0 or earlier
list.swap(xs: List[A], i: Int, j: Int) -> List[A]

// Ascending copy; Floats in IEEE totalOrder.
// @since 0.5.0 or earlier
list.sort(xs: List[A]) -> List[A]

// Elements in reverse order.
// @since 0.5.0 or earlier
list.reverse(xs: List[A]) -> List[A]

// true if some element == x; NaN never matches.
// @since 0.5.0 or earlier
list.contains(xs: List[A], x: A) -> Bool

// (index, element) pairs, index from 0.
// @since 0.5.0 or earlier
list.enumerate(xs: List[A]) -> List[(Int, A)]

// Pairs by position; stops at the shorter list.
// @since 0.5.0 or earlier
list.zip(xs: List[A], ys: List[B]) -> List[(A, B)]

// Inner lists concatenated in order.
// @since 0.5.0 or earlier
list.flatten(xs: List[List[A]]) -> List[A]

// First n elements; all if n >= len, [] if n < 0.
// @since 0.5.0 or earlier
list.take(xs: List[A], n: Int) -> List[A]

// All but the first n; [] if n >= len, all if n < 0.
// @since 0.5.0 or earlier
list.drop(xs: List[A], n: Int) -> List[A]

// Duplicates removed; first occurrences kept.
// @since 0.5.0 or earlier
list.unique(xs: List[A]) -> List[A]

// Index of the first x, or none.
// @since 0.5.0 or earlier
list.index_of(xs: List[A], x: A) -> Option[Int]

// Final element, or none when empty.
// @since 0.5.0 or earlier
list.last(xs: List[A]) -> Option[A]

// Runs of n, last may be short; n <= 0 aborts.
// @since 0.5.0 or earlier
list.chunk(xs: List[A], n: Int) -> List[List[A]]

// Sum of any numbers (R-L0-42); 0 when empty; integers wrap on overflow.
// @since 0.5.0 or earlier
list.sum(xs: List[A]) -> A

// Product of any numbers (R-L0-42); 1 when empty; integers wrap on overflow.
// @since 0.5.0 or earlier
list.product(xs: List[A]) -> A

// Head element, or none when empty.
// @since 0.5.0 or earlier
list.first(xs: List[A]) -> Option[A]

// true iff xs has no elements.
// @since 0.6.0 or earlier
list.is_empty(xs: List[A]) -> Bool

// Least element, or none when empty.
// @since 0.5.0 or earlier
list.min(xs: List[A]) -> Option[A]

// Greatest element, or none when empty.
// @since 0.5.0 or earlier
list.max(xs: List[A]) -> Option[A]

// The parts joined by sep (R-L0-42): Strings or Bytes; empty for [].
// @since 0.5.0 or earlier
list.join(xs: List[T], sep: T) -> T

// f applied to each element, in order.
// @since 0.5.0 or earlier
list.map(xs: List[A], f: (A) -> B) -> List[B]

// Elements where f is true, order kept.
// @since 0.5.0 or earlier
list.filter(xs: List[A], f: (A) -> Bool) -> List[A]

// First element where f holds, or none.
// @since 0.5.0 or earlier
list.find(xs: List[A], f: (A) -> Bool) -> Option[A]

// true if f holds for some x; false when empty.
// @since 0.5.0 or earlier
list.any(xs: List[A], f: (A) -> Bool) -> Bool

// true if f holds for every x; true when empty.
// @since 0.5.0 or earlier
list.all(xs: List[A], f: (A) -> Bool) -> Bool

// Number of elements where f holds.
// @since 0.5.0 or earlier
list.count(xs: List[A], f: (A) -> Bool) -> Int

// f(x) lists concatenated in order.
// @since 0.5.0 or earlier
list.flat_map(xs: List[A], f: (A) -> List[B]) -> List[B]

// The some values of f(x), in order.
// @since 0.5.0 or earlier
list.filter_map(xs: List[A], f: (A) -> Option[B]) -> List[B]

// Left fold from init; init when empty.
// @since 0.5.0 or earlier
list.fold(xs: List[A], init: B, f: (B, A) -> B) -> B

// Stable sort by key f(x); f runs once per x.
// @since 0.5.0 or earlier
list.sort_by(xs: List[A], f: (A) -> B) -> List[A]

// Longest prefix where f holds.
// @since 0.5.0 or earlier
list.take_while(xs: List[A], f: (A) -> Bool) -> List[A]

// Rest after the longest prefix where f holds.
// @since 0.5.0 or earlier
list.drop_while(xs: List[A], f: (A) -> Bool) -> List[A]

// (where f holds, the rest), order kept.
// @since 0.5.0 or earlier
list.partition(xs: List[A], f: (A) -> Bool) -> (List[A], List[A])

// Left fold seeded by xs[0]; none when empty.
// @since 0.5.0 or earlier
list.reduce(xs: List[A], f: (A, A) -> A) -> Option[A]

// Key f(x) -> elements; keys in first-seen order.
// @since 0.5.0 or earlier
list.group_by(xs: List[A], f: (A) -> B) -> Map[B, List[A]]

// Index of the first x where f holds, or none.
// @since 0.5.0 or earlier
list.find_index(xs: List[A], f: (A) -> Bool) -> Option[Int]

// Copy with slot i = f(old); no-op if i out of range.
// @since 0.5.0 or earlier
list.update(xs: List[A], i: Int, f: (A) -> A) -> List[A]

// Running fold values, init excluded.
// @since 0.5.0 or earlier
list.scan(xs: List[A], init: B, f: (B, A) -> B) -> List[B]

// f over pairs; stops at the shorter list.
// @since 0.5.0 or earlier
list.zip_with(xs: List[A], ys: List[B], f: (A, B) -> C) -> List[C]

// First element per distinct key f(x).
// @since 0.7.0 or earlier
list.unique_by(xs: List[A], f: (A) -> K) -> List[A]

// start up to end, exclusive; [] if end <= start.
// @since 0.5.0 or earlier
list.range(start: Int, end: Int) -> List[Int]

// Indices start..<end, each clamped to 0..len; [] if start >= end.
// @since 0.5.0 or earlier
list.slice(xs: List[A], start: Int, end: Int) -> List[A]

// Copy with x at i; i >= len or i < 0 appends.
// @since 0.5.0 or earlier
list.insert(xs: List[A], i: Int, x: A) -> List[A]

// Copy without slot i; no-op if i out of range.
// @since 0.5.0 or earlier
list.remove_at(xs: List[A], i: Int) -> List[A]

// n copies of x; [] for n <= 0.
// @since 0.5.0 or earlier
list.repeat(x: A, n: Int) -> List[A]

// sep between adjacent elements.
// @since 0.5.0 or earlier
list.intersperse(xs: List[A], sep: A) -> List[A]

// Adjacent equal runs collapsed; not global.
// @since 0.5.0 or earlier
list.dedup(xs: List[A]) -> List[A]

// Last n elements; all if n >= len, [] if n < 0.
// @since 0.7.0 or earlier
list.take_end(xs: List[A], n: Int) -> List[A]

// All but the last n; [] if n >= len, all if n < 0.
// @since 0.7.0 or earlier
list.drop_end(xs: List[A], n: Int) -> List[A]

// All n-long runs; [] if n > len; n <= 0 aborts.
// @since 0.7.0 or earlier
list.window(xs: List[A], n: Int) -> List[List[A]]

// Index of target in xs sorted ascending (list.sort's order), or none.
// @since 0.14.6 or earlier
list.binary_search(xs: List[A], target: A) -> Option[Int]

// Appends x to xs in place.
// @since 0.9.3 or earlier
list.push(mut xs: List[A], x: A) -> Unit

// Empty list; cap is only a preallocation hint.
// @since 0.15.1 or earlier
list.with_capacity(cap: Int) -> List[A]

// Removes and yields the last element; none if empty.
// @since 0.9.3 or earlier
list.pop(mut xs: List[A]) -> Option[A]

// Removes every element of xs in place.
// @since 0.9.3 or earlier
list.clear(mut xs: List[A]) -> Unit

// (take n, drop n); n past the end gives (xs, []), n < 0 gives ([], xs).
// @since 0.14.6 or earlier
list.split_at(xs: List[A], n: Int) -> (List[A], List[A])

// [seed, f(seed), ...] of length n; [] if n <= 0.
// @since 0.14.6 or earlier
list.iterate(seed: A, f: (A) -> A, n: Int) -> List[A]

// The first some that f gives, or none; f is not called after the hit.
// @since unreleased
list.find_map(xs: List[A], f: (A) -> Option[B]) -> Option[B]

// An empty list; the same value as [].
// @since unreleased
list.new() -> List[A]

// xs itself; the Container constructor from a list.
// @since unreleased
list.from_list(xs: List[A]) -> List[A]

// xs itself; the Container view as a list.
// @since unreleased
list.to_list(xs: List[A]) -> List[A]

// Stable sort by cmp: negative puts a first, 0 keeps input order.
// @since unreleased
list.sort_with(xs: List[A], cmp: (A, A) -> Int) -> List[A]

// Element with the least key f(x); the first on ties; f runs once per x.
// @since unreleased
list.min_by(xs: List[A], f: (A) -> K) -> Option[A]

// Element with the greatest key f(x); the first on ties; f runs once per x.
// @since unreleased
list.max_by(xs: List[A], f: (A) -> K) -> Option[A]

// Least element by cmp; the first on ties; none when empty.
// @since unreleased
list.min_with(xs: List[A], cmp: (A, A) -> Int) -> Option[A]

// Greatest element by cmp; the first on ties; none when empty.
// @since unreleased
list.max_with(xs: List[A], cmp: (A, A) -> Int) -> Option[A]

// Index of target among keys f(x) of xs sorted by key, or none.
// @since unreleased
list.binary_search_by(xs: List[A], target: K, f: (A) -> K) -> Option[Int]

// Index of target in xs sorted by cmp, or none; cmp(x, target) is asked.
// @since unreleased
list.binary_search_with(xs: List[A], target: A, cmp: (A, A) -> Int) -> Option[Int]

// (firsts, seconds) of a list of pairs; the inverse of zip.
// @since unreleased
list.unzip(xs: List[(A, B)]) -> (List[A], List[B])

// start, start + step, ... before end; counts down for step < 0; step = 0 aborts.
// @since unreleased
list.range_step(start: Int, end: Int, step: Int) -> List[Int]
```



## API 说明译文（按上述签名顺序）

- `list.len(xs: List[A]) -> Int` — 元素数量；空列表为 0。
- `list.length(xs: List[A]) -> Int   (deprecated — use list.len)` — list.len 的别名。
- `list.get(xs: List[A], i: Int) -> Option[A]` — 索引 i 处的元素；越界时为 none。
- `list.set(xs: List[A], i: Int, x: A) -> List[A]` — 将槽位 i = x 的副本；i 越界时不做任何操作。
- `list.swap(xs: List[A], i: Int, j: Int) -> List[A]` — 交换 i 和 j 的副本；任一索引越界时不做任何操作。
- `list.sort(xs: List[A]) -> List[A]` — 升序副本；Float 遵循 IEEE totalOrder。
- `list.reverse(xs: List[A]) -> List[A]` — 按逆序排列的元素。
- `list.contains(xs: List[A], x: A) -> Bool` — 若有元素 == x 则为 true；NaN 永不匹配。
- `list.enumerate(xs: List[A]) -> List[(Int, A)]` — （索引、元素）对，索引从 0 开始。
- `list.zip(xs: List[A], ys: List[B]) -> List[(A, B)]` — 按位置配对；到较短列表的末尾停止。
- `list.flatten(xs: List[List[A]]) -> List[A]` — 按顺序连接内部列表。
- `list.take(xs: List[A], n: Int) -> List[A]` — 前 n 个元素；n >= len 时取全部，n < 0 时为 []。
- `list.drop(xs: List[A], n: Int) -> List[A]` — 除前 n 个元素以外的全部；n >= len 时为 []，n < 0 时取全部。
- `list.unique(xs: List[A]) -> List[A]` — 移除重复项；保留首次出现的元素。
- `list.index_of(xs: List[A], x: A) -> Option[Int]` — 第一个 x 的索引；没有则为 none。
- `list.last(xs: List[A]) -> Option[A]` — 最后一个元素；为空时为 none。
- `list.chunk(xs: List[A], n: Int) -> List[List[A]]` — 每段 n 个元素，最后一段可能较短；n <= 0 时中止执行。
- `list.sum(xs: List[A]) -> A` — 任意数值的和（R-L0-42）；为空时为 0；整数溢出时回绕。
- `list.product(xs: List[A]) -> A` — 任意数值的积（R-L0-42）；为空时为 1；整数溢出时回绕。
- `list.first(xs: List[A]) -> Option[A]` — 头部元素；为空时为 none。
- `list.is_empty(xs: List[A]) -> Bool` — 当且仅当 xs 没有元素时为 true。
- `list.min(xs: List[A]) -> Option[A]` — 最小元素；为空时为 none。
- `list.max(xs: List[A]) -> Option[A]` — 最大元素；为空时为 none。
- `list.join(xs: List[T], sep: T) -> T` — 以 sep 连接各部分（R-L0-42）：String 或 Bytes；[] 时为空。
- `list.map(xs: List[A], f: (A) -> B) -> List[B]` — 按顺序对每个元素应用 f。
- `list.filter(xs: List[A], f: (A) -> Bool) -> List[A]` — f 为 true 的元素，保持顺序。
- `list.find(xs: List[A], f: (A) -> Bool) -> Option[A]` — f 成立的第一个元素；没有则为 none。
- `list.any(xs: List[A], f: (A) -> Bool) -> Bool` — 若 f 对某个 x 成立则为 true；为空时为 false。
- `list.all(xs: List[A], f: (A) -> Bool) -> Bool` — 若 f 对每个 x 成立则为 true；为空时为 true。
- `list.count(xs: List[A], f: (A) -> Bool) -> Int` — f 成立的元素个数。
- `list.flat_map(xs: List[A], f: (A) -> List[B]) -> List[B]` — 按顺序连接 f(x) 列表。
- `list.filter_map(xs: List[A], f: (A) -> Option[B]) -> List[B]` — 按顺序取 f(x) 的 some 值。
- `list.fold(xs: List[A], init: B, f: (B, A) -> B) -> B` — 从 init 开始左折叠；为空时为 init。
- `list.sort_by(xs: List[A], f: (A) -> B) -> List[A]` — 按键 f(x) 稳定排序；每个 x 执行一次 f。
- `list.take_while(xs: List[A], f: (A) -> Bool) -> List[A]` — f 成立的最长前缀。
- `list.drop_while(xs: List[A], f: (A) -> Bool) -> List[A]` — 去掉 f 成立的最长前缀后的剩余部分。
- `list.partition(xs: List[A], f: (A) -> Bool) -> (List[A], List[A])` — （f 成立的元素，其余元素），保持顺序。
- `list.reduce(xs: List[A], f: (A, A) -> A) -> Option[A]` — 以 xs[0] 为初值左折叠；为空时为 none。
- `list.group_by(xs: List[A], f: (A) -> B) -> Map[B, List[A]]` — 键 f(x) → 元素；键按首次出现顺序排列。
- `list.find_index(xs: List[A], f: (A) -> Bool) -> Option[Int]` — f 成立的第一个 x 的索引；没有则为 none。
- `list.update(xs: List[A], i: Int, f: (A) -> A) -> List[A]` — 将槽位 i = f(old) 的副本；i 越界时不做任何操作。
- `list.scan(xs: List[A], init: B, f: (B, A) -> B) -> List[B]` — 折叠过程中的值，不含 init。
- `list.zip_with(xs: List[A], ys: List[B], f: (A, B) -> C) -> List[C]` — 对二元组应用 f；到较短列表的末尾停止。
- `list.unique_by(xs: List[A], f: (A) -> K) -> List[A]` — 每个不同的键 f(x) 对应的第一个元素。
- `list.range(start: Int, end: Int) -> List[Int]` — 从 start 到 end 之前，不含 end；end <= start 时为 []。
- `list.slice(xs: List[A], start: Int, end: Int) -> List[A]` — 索引 start..<end，分别限制在 0..len 内；start >= end 时为 []。
- `list.insert(xs: List[A], i: Int, x: A) -> List[A]` — 在 i 处放入 x 的副本；i >= len 或 i < 0 时追加到末尾。
- `list.remove_at(xs: List[A], i: Int) -> List[A]` — 移除槽位 i 的副本；i 越界时不做任何操作。
- `list.repeat(x: A, n: Int) -> List[A]` — n 个 x；n <= 0 时为 []。
- `list.intersperse(xs: List[A], sep: A) -> List[A]` — 在相邻元素之间插入 sep。
- `list.dedup(xs: List[A]) -> List[A]` — 合并相邻相等的连续段；并非全局去重。
- `list.take_end(xs: List[A], n: Int) -> List[A]` — 最后 n 个元素；n >= len 时取全部，n < 0 时为 []。
- `list.drop_end(xs: List[A], n: Int) -> List[A]` — 除最后 n 个元素以外的全部；n >= len 时为 []，n < 0 时取全部。
- `list.window(xs: List[A], n: Int) -> List[List[A]]` — 所有长度为 n 的连续段；n > len 时为 []；n <= 0 时中止执行。
- `list.binary_search(xs: List[A], target: A) -> Option[Int]` — target 在升序排列的 xs 中的索引（采用 list.sort 的顺序）；没有则为 none。
- `list.push(mut xs: List[A], x: A) -> Unit` — 原地将 x 追加到 xs。
- `list.with_capacity(cap: Int) -> List[A]` — 空列表；cap 只是预分配提示。
- `list.pop(mut xs: List[A]) -> Option[A]` — 移除并返回最后一个元素；为空时为 none。
- `list.clear(mut xs: List[A]) -> Unit` — 原地移除 xs 的每个元素。
- `list.split_at(xs: List[A], n: Int) -> (List[A], List[A])` — （取前 n 个，丢弃前 n 个）；n 超过末尾时为 (xs, [])，n < 0 时为 ([], xs)。
- `list.iterate(seed: A, f: (A) -> A, n: Int) -> List[A]` — 长度为 n 的 [seed, f(seed), ...]；n <= 0 时为 []。
- `list.find_map(xs: List[A], f: (A) -> Option[B]) -> Option[B]` — f 给出的第一个 some；没有则为 none；命中后不再调用 f。
- `list.new() -> List[A]` — 空列表；与 [] 是相同的值。
- `list.from_list(xs: List[A]) -> List[A]` — xs 本身；从列表构造 Container 的构造器。
- `list.to_list(xs: List[A]) -> List[A]` — xs 本身；Container 的列表视图。
- `list.sort_with(xs: List[A], cmp: (A, A) -> Int) -> List[A]` — 按 cmp 稳定排序：负数让 a 在前，0 保持输入顺序。
- `list.min_by(xs: List[A], f: (A) -> K) -> Option[A]` — 键 f(x) 最小的元素；相等时取第一个；每个 x 执行一次 f。
- `list.max_by(xs: List[A], f: (A) -> K) -> Option[A]` — 键 f(x) 最大的元素；相等时取第一个；每个 x 执行一次 f。
- `list.min_with(xs: List[A], cmp: (A, A) -> Int) -> Option[A]` — 按 cmp 比较的最小元素；相等时取第一个；为空时为 none。
- `list.max_with(xs: List[A], cmp: (A, A) -> Int) -> Option[A]` — 按 cmp 比较的最大元素；相等时取第一个；为空时为 none。
- `list.binary_search_by(xs: List[A], target: K, f: (A) -> K) -> Option[Int]` — 在按键排序的 xs 的键 f(x) 中查找 target 的索引；没有则为 none。
- `list.binary_search_with(xs: List[A], target: A, cmp: (A, A) -> Int) -> Option[Int]` — target 在按 cmp 排序的 xs 中的索引；没有则为 none；会调用 cmp(x, target)。
- `list.unzip(xs: List[(A, B)]) -> (List[A], List[B])` — 将二元组列表转为（第一项列表，第二项列表）；是 zip 的逆操作。
- `list.range_step(start: Int, end: Int, step: Int) -> List[Int]` — start, start + step, ...，止于 end 之前；step < 0 时递减；step = 0 时中止执行。
