# deque

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](deque.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](deque.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](deque.zh-CN.md)



> 原文：[deque.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/deque.md)。这是面向读者的非规范性固定快照，不参与自动生成或自动同步。签名、类型、`@since` 及代码（包括注释）保持原样，API 说明译文附在末尾。本次翻译未独立运行或验证示例、性能、安全性或跨目标一致性。

> 均摊 O(1) 不表示每次操作的最坏时间总是常数。API 索引中并列时优先靠前元素、空队列的返回值及整数溢出回绕均按原文保留。

双端队列 `Deque[A]`：两端添加和取出的均摊复杂度为 O(1)。自动导入。纯操作，两个目标一致。

`push` / `pop` 与 `list.push` / `list.pop` 一样操作尾部，`push_front` / `pop_front` 操作头部。这四者和 `clear` 都原地修改队列（`mut`）；其他操作不修改队列。从空队列取出得到 `none`。位置和范围函数（`get`、`take`、`slice`、`set` 等）与同名 `list` 函数接受相同参数并给出相同结果；序列算法（`map`、`fold`、`sort_by`、`min_by` 等）从头到尾遍历。

```almd run
fn main() -> Unit = {
  var q: Deque[Int] = deque.new()
  deque.push(q, 2)
  deque.push(q, 3)
  deque.push_front(q, 1)
  println("${deque.to_list(q)}")
  println("${deque.pop_front(q)} ${deque.pop(q)} ${deque.len(q)}")
}
```
```output
[1, 2, 3]
some(1) some(3) 1
```

广度优先搜索将待扩展的边界保存在双端队列中：

```almd run
fn reachable(edges: Map[Int, List[Int]], start: Int) -> List[Int] = {
  var seen = [start]
  var queue = deque.from_list([start])
  for _ in 0..<100 {
    match deque.pop_front(queue) {
      some(n) => for m in map.get(edges, n) ?? [] {
        if list.contains(seen, m) then ()
        else {
          seen = seen + [m]
          deque.push(queue, m)
        }
      },
      none => (),
    }
  }
  seen
}

fn main() -> Unit = {
  let edges = map.from_list([(1, [2, 3]), (2, [4]), (3, [4]), (4, [])])
  println("${reachable(edges, 1)}")
}
```
```output
[1, 2, 3, 4]
```


## 原文固定快照 API 索引（60个函数）

```
// An empty deque.
// @since unreleased
deque.new() -> Deque[A]

// A deque of xs, front to back.
// @since unreleased
deque.from_list(xs: List[A]) -> Deque[A]

// The elements, front to back.
// @since unreleased
deque.to_list(d: Deque[A]) -> List[A]

// Element count; 0 for an empty deque.
// @since unreleased
deque.len(d: Deque[A]) -> Int

// true iff d has no elements.
// @since unreleased
deque.is_empty(d: Deque[A]) -> Bool

// Appends x at the back in place.
// @since unreleased
deque.push(mut d: Deque[A], x: A) -> Unit

// Adds x at the front in place.
// @since unreleased
deque.push_front(mut d: Deque[A], x: A) -> Unit

// Removes and yields the last element; none when empty.
// @since unreleased
deque.pop(mut d: Deque[A]) -> Option[A]

// Removes and yields the first element; none when empty.
// @since unreleased
deque.pop_front(mut d: Deque[A]) -> Option[A]

// The first element, or none when empty.
// @since unreleased
deque.first(d: Deque[A]) -> Option[A]

// The last element, or none when empty.
// @since unreleased
deque.last(d: Deque[A]) -> Option[A]

// Element at index i from the front, or none when out of range.
// @since unreleased
deque.get(d: Deque[A], i: Int) -> Option[A]

// true if some element == x.
// @since unreleased
deque.contains(d: Deque[A], x: A) -> Bool

// Removes every element of d in place.
// @since unreleased
deque.clear(mut d: Deque[A]) -> Unit

// Deque of f(x), front to back; f runs front to back.
// @since unreleased
deque.map(d: Deque[A], f: (A) -> B) -> Deque[B]

// Elements where f holds, order kept.
// @since unreleased
deque.filter(d: Deque[A], f: (A) -> Bool) -> Deque[A]

// Folds f(acc, x) front to back.
// @since unreleased
deque.fold(d: Deque[A], init: B, f: (B, A) -> B) -> B

// true if f holds for some element; false when empty.
// @since unreleased
deque.any(d: Deque[A], f: (A) -> Bool) -> Bool

// true if f holds for every element; true when empty.
// @since unreleased
deque.all(d: Deque[A], f: (A) -> Bool) -> Bool

// Number of elements where f holds.
// @since unreleased
deque.count(d: Deque[A], f: (A) -> Bool) -> Int

// The element nearest the front where f holds, or none.
// @since unreleased
deque.find(d: Deque[A], f: (A) -> Bool) -> Option[A]

// Elements back to front.
// @since unreleased
deque.reverse(d: Deque[A]) -> Deque[A]

// First n elements, as list.take.
// @since unreleased
deque.take(d: Deque[A], n: Int) -> Deque[A]

// All but the first n, as list.drop.
// @since unreleased
deque.drop(d: Deque[A], n: Int) -> Deque[A]

// Last n elements, as list.take_end.
// @since unreleased
deque.take_end(d: Deque[A], n: Int) -> Deque[A]

// All but the last n, as list.drop_end.
// @since unreleased
deque.drop_end(d: Deque[A], n: Int) -> Deque[A]

// Indices start..<end, as list.slice.
// @since unreleased
deque.slice(d: Deque[A], start: Int, end: Int) -> Deque[A]

// Copy with slot i = x, as list.set.
// @since unreleased
deque.set(d: Deque[A], i: Int, x: A) -> Deque[A]

// Copy with x at i, as list.insert.
// @since unreleased
deque.insert(d: Deque[A], i: Int, x: A) -> Deque[A]

// Copy without slot i, as list.remove_at.
// @since unreleased
deque.remove_at(d: Deque[A], i: Int) -> Deque[A]

// (index, element) pairs, index from 0 at the front.
// @since unreleased
deque.enumerate(d: Deque[A]) -> List[(Int, A)]

// Elements paired with ys by position; stops at the shorter.
// @since unreleased
deque.zip(d: Deque[A], ys: List[B]) -> List[(A, B)]

// f over (element, ys element) pairs; stops at the shorter.
// @since unreleased
deque.zip_with(d: Deque[A], ys: List[B], f: (A, B) -> C) -> List[C]

// The somes f gives, front to back.
// @since unreleased
deque.filter_map(d: Deque[A], f: (A) -> Option[B]) -> List[B]

// The first some f gives, or none; f is not called after the hit.
// @since unreleased
deque.find_map(d: Deque[A], f: (A) -> Option[B]) -> Option[B]

// Left fold seeded by the first element; none when empty.
// @since unreleased
deque.reduce(d: Deque[A], f: (A, A) -> A) -> Option[A]

// Running fold values front to back, init excluded.
// @since unreleased
deque.scan(d: Deque[A], init: B, f: (B, A) -> B) -> List[B]

// (elements where f holds, the rest), order kept.
// @since unreleased
deque.partition(d: Deque[A], f: (A) -> Bool) -> (List[A], List[A])

// Key f(x) -> elements; keys in first-seen order.
// @since unreleased
deque.group_by(d: Deque[A], f: (A) -> B) -> Map[B, List[A]]

// Elements in ascending order; Floats in IEEE totalOrder.
// @since unreleased
deque.sort(d: Deque[A]) -> List[A]

// Elements sorted by key f(x), stable; f runs once per element.
// @since unreleased
deque.sort_by(d: Deque[A], f: (A) -> B) -> List[A]

// Elements sorted by cmp, stable: negative puts a first.
// @since unreleased
deque.sort_with(d: Deque[A], cmp: (A, A) -> Int) -> List[A]

// Least element, or none when empty.
// @since unreleased
deque.min(d: Deque[A]) -> Option[A]

// Greatest element, or none when empty.
// @since unreleased
deque.max(d: Deque[A]) -> Option[A]

// Element with the least key f(x); the nearest the front on ties.
// @since unreleased
deque.min_by(d: Deque[A], f: (A) -> K) -> Option[A]

// Element with the greatest key f(x); the nearest the front on ties.
// @since unreleased
deque.max_by(d: Deque[A], f: (A) -> K) -> Option[A]

// Least element by cmp; the nearest the front on ties.
// @since unreleased
deque.min_with(d: Deque[A], cmp: (A, A) -> Int) -> Option[A]

// Greatest element by cmp; the nearest the front on ties.
// @since unreleased
deque.max_with(d: Deque[A], cmp: (A, A) -> Int) -> Option[A]

// Index from the front of the first x where f holds, or none.
// @since unreleased
deque.find_index(d: Deque[A], f: (A) -> Bool) -> Option[Int]

// Longest front run where f holds.
// @since unreleased
deque.take_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]

// The rest after the longest front run where f holds.
// @since unreleased
deque.drop_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]

// Runs of n from the front, as list.chunk.
// @since unreleased
deque.chunk(d: Deque[A], n: Int) -> List[Deque[A]]

// All n-long runs, as list.window.
// @since unreleased
deque.window(d: Deque[A], n: Int) -> List[Deque[A]]

// (first n, the rest), as list.split_at.
// @since unreleased
deque.split_at(d: Deque[A], n: Int) -> (Deque[A], Deque[A])

// Adjacent equal runs collapsed, as list.dedup.
// @since unreleased
deque.dedup(d: Deque[A]) -> Deque[A]

// sep between adjacent elements.
// @since unreleased
deque.intersperse(d: Deque[A], sep: A) -> Deque[A]

// Duplicates removed; the occurrence nearest the front kept.
// @since unreleased
deque.unique(d: Deque[A]) -> Deque[A]

// The first element per distinct key f(x).
// @since unreleased
deque.unique_by(d: Deque[A], f: (A) -> K) -> Deque[A]

// Sum of the elements; 0 when empty; integers wrap on overflow.
// @since unreleased
deque.sum(d: Deque[A]) -> A

// Product of the elements; 1 when empty; integers wrap on overflow.
// @since unreleased
deque.product(d: Deque[A]) -> A
```

## API 说明译文（按上述签名顺序）

- `deque.new() -> Deque[A]`：空双端队列。
- `deque.from_list(xs: List[A]) -> Deque[A]`：由 xs 构成的队列，从头到尾。
- `deque.to_list(d: Deque[A]) -> List[A]`：从头到尾的元素。
- `deque.len(d: Deque[A]) -> Int`：元素数量；空时为0。
- `deque.is_empty(d: Deque[A]) -> Bool`：当且仅当 d 无元素时为 true。
- `deque.push(mut d: Deque[A], x: A) -> Unit`：在尾部原地追加 x。
- `deque.push_front(mut d: Deque[A], x: A) -> Unit`：在头部原地添加 x。
- `deque.pop(mut d: Deque[A]) -> Option[A]`：移除并返回末尾元素；空时为 none。
- `deque.pop_front(mut d: Deque[A]) -> Option[A]`：移除并返回头部元素；空时为 none。
- `deque.first(d: Deque[A]) -> Option[A]`：首个元素，空时为 none。
- `deque.last(d: Deque[A]) -> Option[A]`：末尾元素，空时为 none。
- `deque.get(d: Deque[A], i: Int) -> Option[A]`：从头部起索引 i 处的元素；越界为 none。
- `deque.contains(d: Deque[A], x: A) -> Bool`：某个元素 == x 时为 true。
- `deque.clear(mut d: Deque[A]) -> Unit`：原地移除 d 的全部元素。
- `deque.map(d: Deque[A], f: (A) -> B) -> Deque[B]`：由 f(x) 构成的队列，从头到尾；f 也按从头到尾的顺序运行。
- `deque.filter(d: Deque[A], f: (A) -> Bool) -> Deque[A]`：满足 f 的元素，保持顺序。
- `deque.fold(d: Deque[A], init: B, f: (B, A) -> B) -> B`：从头到尾折叠 f(acc, x)。
- `deque.any(d: Deque[A], f: (A) -> Bool) -> Bool`：某个元素满足 f 时为 true；空时为 false。
- `deque.all(d: Deque[A], f: (A) -> Bool) -> Bool`：所有元素满足 f 时为 true；空时为 true。
- `deque.count(d: Deque[A], f: (A) -> Bool) -> Int`：满足 f 的元素数量。
- `deque.find(d: Deque[A], f: (A) -> Bool) -> Option[A]`：最靠近头部且满足 f 的元素，或 none。
- `deque.reverse(d: Deque[A]) -> Deque[A]`：从尾到头的元素。
- `deque.take(d: Deque[A], n: Int) -> Deque[A]`：前 n 个元素，同 list.take。
- `deque.drop(d: Deque[A], n: Int) -> Deque[A]`：除前 n 个以外的全部元素，同 list.drop。
- `deque.take_end(d: Deque[A], n: Int) -> Deque[A]`：末尾 n 个元素，同 list.take_end。
- `deque.drop_end(d: Deque[A], n: Int) -> Deque[A]`：除末尾 n 个以外的全部元素，同 list.drop_end。
- `deque.slice(d: Deque[A], start: Int, end: Int) -> Deque[A]`：索引 start..<end，同 list.slice。
- `deque.set(d: Deque[A], i: Int, x: A) -> Deque[A]`：将位置 i 设为 x 的副本，同 list.set。
- `deque.insert(d: Deque[A], i: Int, x: A) -> Deque[A]`：在 i 插入 x 的副本，同 list.insert。
- `deque.remove_at(d: Deque[A], i: Int) -> Deque[A]`：去掉位置 i 的副本，同 list.remove_at。
- `deque.enumerate(d: Deque[A]) -> List[(Int, A)]`：(索引, 元素) 对，索引从头部的0开始。
- `deque.zip(d: Deque[A], ys: List[B]) -> List[(A, B)]`：按位置与 ys 配对，在较短者末尾停止。
- `deque.zip_with(d: Deque[A], ys: List[B], f: (A, B) -> C) -> List[C]`：对(元素, ys 元素)对应用 f，在较短者末尾停止。
- `deque.filter_map(d: Deque[A], f: (A) -> Option[B]) -> List[B]`：f 给出的 some，从头到尾排列。
- `deque.find_map(d: Deque[A], f: (A) -> Option[B]) -> Option[B]`：f 给出的第一个 some，或 none；命中后不再调用 f。
- `deque.reduce(d: Deque[A], f: (A, A) -> A) -> Option[A]`：以首元素为初值的左折叠；空时为 none。
- `deque.scan(d: Deque[A], init: B, f: (B, A) -> B) -> List[B]`：从头到尾的逐步折叠值，不包含 init。
- `deque.partition(d: Deque[A], f: (A) -> Bool) -> (List[A], List[A])`：(满足 f 的元素, 其余元素)，保持顺序。
- `deque.group_by(d: Deque[A], f: (A) -> B) -> Map[B, List[A]]`：键 f(x) → 元素；键按首次出现顺序排列。
- `deque.sort(d: Deque[A]) -> List[A]`：元素按升序排列；Float 按 IEEE totalOrder。
- `deque.sort_by(d: Deque[A], f: (A) -> B) -> List[A]`：按键 f(x) 稳定排序；每个元素运行 f 一次。
- `deque.sort_with(d: Deque[A], cmp: (A, A) -> Int) -> List[A]`：按 cmp 稳定排序；负数将 a 放在前面。
- `deque.min(d: Deque[A]) -> Option[A]`：最小元素，空时为 none。
- `deque.max(d: Deque[A]) -> Option[A]`：最大元素，空时为 none。
- `deque.min_by(d: Deque[A], f: (A) -> K) -> Option[A]`：键 f(x) 最小的元素；并列时取最靠前者。
- `deque.max_by(d: Deque[A], f: (A) -> K) -> Option[A]`：键 f(x) 最大的元素；并列时取最靠前者。
- `deque.min_with(d: Deque[A], cmp: (A, A) -> Int) -> Option[A]`：按 cmp 的最小元素；并列时取最靠前者。
- `deque.max_with(d: Deque[A], cmp: (A, A) -> Int) -> Option[A]`：按 cmp 的最大元素；并列时取最靠前者。
- `deque.find_index(d: Deque[A], f: (A) -> Bool) -> Option[Int]`：从头部起第一个满足 f 的 x 的索引，或 none。
- `deque.take_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]`：从头部起满足 f 的最长连续段。
- `deque.drop_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]`：去掉头部满足 f 的最长连续段后剩余的部分。
- `deque.chunk(d: Deque[A], n: Int) -> List[Deque[A]]`：从头部起每 n 个一段，同 list.chunk。
- `deque.window(d: Deque[A], n: Int) -> List[Deque[A]]`：所有长为 n 的连续段，同 list.window。
- `deque.split_at(d: Deque[A], n: Int) -> (Deque[A], Deque[A])`：(前 n 个, 其余)，同 list.split_at。
- `deque.dedup(d: Deque[A]) -> Deque[A]`：合并相邻相等的连续元素，同 list.dedup。
- `deque.intersperse(d: Deque[A], sep: A) -> Deque[A]`：相邻元素间插入 sep。
- `deque.unique(d: Deque[A]) -> Deque[A]`：去重，保留最靠近头部的出现。
- `deque.unique_by(d: Deque[A], f: (A) -> K) -> Deque[A]`：每个不同键 f(x) 的第一个元素。
- `deque.sum(d: Deque[A]) -> A`：元素之和；空时为0；整数溢出时回绕。
- `deque.product(d: Deque[A]) -> A`：元素之积；空时为1；整数溢出时回绕。
