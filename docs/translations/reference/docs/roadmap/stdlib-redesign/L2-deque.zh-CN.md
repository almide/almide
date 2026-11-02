> 说明（原本索引元数据的非活动副本）：标准库重设计 L2/L3 — deque（新增，R-L1-01）：以纯 Almide 编写的双端队列，两端 push/pop 与 list.push/pop 一样采用原地修改的 mut 动词
<a id="l2l3-deque新しいモジュール"></a>

# L2/L3: deque（新模块）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-deque.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-deque.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-deque.zh-CN.md)

> 非规范性历史重设计参照版。固定原本权威且不变，不自动同步、未独立测量性能或核实现行实现。保留初始12函数与后来的函数族扩展及最终表。不擅自调和初期不声明为有序类型与后来Ordered层级提案。O(1)、target一致及速度为原设计/主张，不是新建立的保证。保留的伪代码中，端意为端点，同名意为相同名称。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-deque.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)・[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-01：添加双端队列 `deque`）
> 来源：Rust https://doc.rust-lang.org/std/collections/struct.VecDeque.html ・Go https://pkg.go.dev/container/list ・Python https://docs.python.org/3/library/collections.html#collections.deque ・TS/JS https://jsr.io/@std/data-structures （Deno std，ECMAScript 中没有）・Gleam —（没有）・Elixir https://www.erlang.org/doc/apps/stdlib/queue.html ・OCaml https://ocaml.org/manual/latest/api/Queue.html ・Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.collections/-array-deque/ ・Swift https://swiftpackageindex.com/apple/swift-collections/documentation/dequemodule

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide（提案） | Rust | Go | Python | TS/JS | Gleam | Elixir(:queue) | OCaml(Queue) | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 创建空队列／从列表构建 | `new`・`from_list` | `new`・`from` | `list.New` | `deque()`・`deque(xs)` | `new Deque` | — | `new`・`from_list` | `create`・`of_seq` | `ArrayDeque()` | `Deque()` |
| 末尾添加 | `push`（R-deque-03） | `push_back` | `PushBack` | `append` | `push` | — | `in` | `push` | `addLast` | `append` |
| 前端添加 | `push_front` | `push_front` | `PushFront` | `appendleft` | `unshift` | — | `in_r` | — | `addFirst` | `prepend` |
| 末尾取出 | `pop`（R-deque-03） | `pop_back` | `Remove(Back())` | `pop` | `pop` | — | `out_r` | — | `removeLast` | `popLast` |
| 前端取出 | `pop_front` | `pop_front` | `Remove(Front())` | `popleft` | `shift` | — | `out` | `pop`/`take_opt` | `removeFirst` | `popFirst` |
| 查看前端／末尾 | `first`・`last` | `front`・`back` | `Front`・`Back` | `d[0]`・`d[-1]` | `peekFront` | — | `peek`・`peek_r` | `peek_opt` | `first`・`last` | `first`・`last` |
| 第 i 个 | `get` | `get` | — | `d[i]` | — | — | — | — | `get` | `d[i]` |
| 长度／是否为空 | `len`・`is_empty` | `len`・`is_empty` | `Len` | `len` | `length` | — | `len`・`is_empty` | `length`・`is_empty` | `size`・`isEmpty` | `count`・`isEmpty` |
| 转为列表 | `to_list` | `iter().collect` | — | `list(d)` | `[...d]` | — | `to_list` | `to_seq` | `toList` | `Array(d)` |

<a id="完全性の規則"></a>

### 完整性规则

- 覆盖两端 × {添加、取出、查看} 的全部 6 个单元：`push_front`・`push`・`pop_front`・`pop`・`first`・`last`。末尾的 2 个与 `list` 一样是 `push`・`pop`（R-deque-03）。
- **添加与取出采用 `mut` 原地修改**。R-L0-04 的 2 将带前端名称的 `push_front`・`pop_front` 归入与 `push`・`pop` 相同的族，加入动词集合。依据 R-L0-04 的 3“同一个动词在所有模块中采用同一种方式”，与 `list.push(mut xs, x)`・`list.pop(mut xs) -> A?` 一样原地修改。L1 的 R-L1-01 也已改为相同形式。
- **不提供返回二元组的复制版 `pop`。** 若要从不能修改的值（如函数参数）取出，先用 `var q2 = q` 复制，再写 `pop_front(q2)`。若将来添加复制版，二元组顺序为 `(Option[A], Deque[A])`（取出的元素在前），使阅读顺序与 `list.pop` 返回元素的 `Option[A]` 一致。
- 无法取出时返回 none（R-L0-06）。`get` 越界也返回 none。
- 不声明为有序类型，因此不提供 `deque.compare`（R-L0-16 仅针对有序类型）。

<a id="l2-の判定"></a>

### L2 判定

- 新模块。具备 9 种语言中占多数的所有角色。

<a id="案"></a>

## 提案

<a id="r-deque-01-deque-モジュールを足す---採用"></a>

### R-deque-01 添加 `deque` 模块 — ○ 采纳

- 内容：下方 L3 的 12 个函数。类型为 `Deque[A]`。内部使用纯 Almide（前后各一条，共 2 条列表，或建立在 `list` 上的环形缓冲区；任一方式在两端添加/取出均摊为 O(1)）。
- 依据的 L0 规则：R-L0-04（原地修改仅用指定动词；`push_<端>`・`pop_<端>` 在 R-L0-04 的 2 中计为 `push`・`pop` 族）、R-L0-06、R-L0-02（构造函数属于返回类型的模块）、R-L0-07（`from_list`・`to_list`）。
- (a) 7 种语言的标准库提供（L1）。前端拼写与 Rust 的 `push_front`/`pop_front` 对齐。末尾与 `list` 一样为 `push`・`pop`（R-deque-03；Swift 的 `Deque` 也与 `Array` 一样采用 `append`・`popLast`）。
- (b) 纯计算，native 与 wasm 相同，不需 host 功能。
- (c) 广度优先搜索目前每次 `list.drop(xs, 1)` 都为 O(n)。LLM 按其他语言 deque 编写的代码可原样快速运行。
- (d) 新增（新模块；`import` 与 `list`・`map` 一样纳入自动导入，加入 stdlib_info 的 `AUTO_IMPORT_BUNDLED`）。
- 未采纳方案：像 Go 的 `s[1:]` 一样，从 `List` 前端取出时不复制，从而不添加 deque。这虽减少需记忆的类型，却会改动 `List` 表示并深入编译器内部，也不会加速前端添加。LLM 在其他语言中看到的也是“队列用 deque”（Python `collections.deque`、Rust `VecDeque`、Kotlin `ArrayDeque`），因此添加 deque。`List` 前端操作速度在重新评估 `List` 性能时另行测量。
- 拟议门禁：与 list 相同的边界输入矩阵测试（空队列 `pop`、`get` 越界）、native 与 wasm cross fixture，以及测量 n 次 `push` 后 n 次 `pop_front` 的时间是否线性（采用 `check-perf-ratio.sh` 的形式）。
- issue: #3932

<a id="r-deque-02-deque-に高階関数の族と-containsclear-を足す---採用"></a>

### R-deque-02 为 deque 添加高阶函数族与 `contains`・`clear` — ○ 采纳

- 现状：`deque` 有 12 个函数，没有 `contains` 和 D3 族（`map`・`filter`・`fold`・`any`・`all`・`count`・`find`）。也不能写 `for x in d`，因此每次检查内容都要经由 `deque.to_list`。
- 提案（从前向后运行）：
  - `fn deque.contains[A](d: Deque[A], x: A) -> Bool`
  - `fn deque.map[A, B](d: Deque[A], f: (A) -> B) -> Deque[B]`
  - `fn deque.filter[A](d: Deque[A], f: (A) -> Bool) -> Deque[A]`
  - `fn deque.fold[A, B](d: Deque[A], init: B, f: (B, A) -> B) -> B`
  - `fn deque.any[A](d: Deque[A], f: (A) -> Bool) -> Bool`、`all`（相同形式）、`fn deque.count[A](d: Deque[A], f: (A) -> Bool) -> Int`
  - `fn deque.find[A](d: Deque[A], f: (A) -> Bool) -> A?`（靠前的优先）
  - `fn deque.clear[A](mut d: Deque[A]) -> Unit`（R-L0-04 的 `clear` 矩阵）
- 依据的 L0 规则：L2-list 完整性规则 1・R-map-06（所有集合具备 D3 的 7 个函数与 `contains`）、R-L0-01（与 list 名称及回调形式相同）、R-L0-04（除 `clear` 外均为复制）。
- (a) 8/9 提供检查、转换 deque 的方式（Python 用迭代和 `in`；Erlang/Elixir 的 `:queue` 有 `all`・`any`・`filter`・`fold`・`member`；Rust `VecDeque` 有 `contains` 和 `iter()`；Kotlin・Swift 作为集合全部具备）。只有 Go 的 `container/list` 手动循环。
- (b) 纯 Almide 计算。顺序固定为从前向后，因此两个 target 相同。
- (c) list・map・set 能写 `xs |> X.any(f)`，唯独 deque 不行，LLM 就会按规则推测而得到 E002。广度优先搜索中用 `deque.contains` 判断“是否访问过”很常见。
- (d) 新增 9 个。新模块，无兼容性影响。
- 完整性规则：顺序确定的集合（list・deque）与集合类（map・set）均具备 D3 的 7 个函数和 `contains`。运行顺序与 `to_list` 相同。
- 拟议门禁：在 R-map-06 集合族矩阵中增加 deque 列。性质测试：每个单元均与 `deque.to_list(d) |> list.<同名>(f)` 结果相同。
- issue: #3968

<a id="r-deque-03-末尾の-push_backpop_back-を-pushpop-にする---採用"></a>

### R-deque-03 将末尾的 `push_back`・`pop_back` 改为 `push`・`pop` — ○ 采纳

- 内容：`deque.push(mut d, x)`・`deque.pop(mut d) -> A?` 用于末尾添加/取出，不提供 `push_back`・`pop_back`。`push_front`・`pop_front` 保留为 `deque` 特有函数。
- 理由：[protocol 层级](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md)中 `Growable` 的要求是 `push`・`pop`（末尾）。可用同一泛型函数处理 `list` 和 `deque`。Swift 的 `Deque`（swift-collections）也在与 `Array` 相同的 `append`・`popLast` 上添加 `prepend`・`popFirst`。
- (d) `deque` 是此次重设计新增模块，因此不是重命名，而是修改待新增名称。
- issue: #4000

<a id="r-deque-04-deque-に-orderedindexedgrowable-の要件をそろえる---採用"></a>

### R-deque-04 为 `deque` 配齐 `Ordered`・`Indexed`・`Growable` 要求 — ○ 采纳

- 内容：提供 `reverse`・`take`・`drop`・`take_end`・`drop_end`・`slice`・`set`・`insert`・`remove_at`。形式及范围处理与 `list` 同名函数一致。
- 理由：`deque` 与 `list` 一样是可按位置读写的序列（Swift 的 `Deque` 是 `RandomAccessCollection`・`MutableCollection`・`RangeReplaceableCollection`），因此属于层级中的 `Indexed`・`Growable`。
- (d) 新增 9 个。
- issue: #4000

<a id="r-deque-05--と-d-は要素を前から順に比べ書く---採用"></a>

### R-deque-05 `==` 与 `"${d}"` 从前向后比较、输出元素 — ○ 采纳

- 现状（#4033 的实现）：`Deque` 是含前后 2 条列表的记录。内置 `==` 比较记录字段，因此即便 2 个 deque 的元素及顺序相同，只要内部分割不同就不相等。`"${d}"` 也输出内部的 2 条列表。
- 提案：`==` 从前向后比较元素，`"${d}"` 像 `deque[1, 2, 3]` 一样从前向后输出元素。内部划分对外不可见。
- 依据的 L0 规则：R-L0-37（`Eq`・`Repr` 按值语义决定）、R-L0-01。
- (a) Swift 的 `Deque`（swift-collections）、Rust 的 `VecDeque`、Python 的 `collections.deque`、Kotlin 的 `ArrayDeque` 都按元素序列决定相等性。
- 实现：用 `Eq` 和 `Repr` 的惯例方法（`fn Deque.eq`・`fn Deque.to_string`，R-L0-38）编写。
- issue: #4033

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（提案） | 其他语言的主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `deque.new` | `fn new[A]() -> Deque[A]` | Rust `new` / Python `deque()` / Elixir `new` | 新增 | R-deque-01 |
| `deque.from_list` | `fn from_list[A](xs: List[A]) -> Deque[A]` | Rust `from` / Elixir `from_list` | 新增 | R-deque-01 |
| `deque.to_list` | `fn to_list[A](d: Deque[A]) -> List[A]` | Elixir `to_list` / Kotlin `toList` | 新增 | R-deque-01 |
| `deque.len` | `fn len[A](d: Deque[A]) -> Int` | Rust `len` / Python `len` | 新增 | R-deque-01 |
| `deque.is_empty` | `fn is_empty[A](d: Deque[A]) -> Bool` | Rust `is_empty` / Kotlin `isEmpty` | 新增 | R-deque-01 |
| `deque.push` | `fn push[A](mut d: Deque[A], x: A) -> Unit` | Rust `push_back` / Go `PushBack` / Python `append` | 新增 | R-deque-01、R-deque-03 |
| `deque.push_front` | `fn push_front[A](mut d: Deque[A], x: A) -> Unit` | Rust `push_front` / Go `PushFront` / Python `appendleft` | 新增 | R-deque-01 |
| `deque.pop` | `fn pop[A](mut d: Deque[A]) -> A?` | Rust `pop_back` / Python `pop` / Swift `popLast` | 新增 | R-deque-01、R-deque-03 |
| `deque.pop_front` | `fn pop_front[A](mut d: Deque[A]) -> A?` | Rust `pop_front` / Python `popleft` / Swift `popFirst` | 新增 | R-deque-01 |
| `deque.first` | `fn first[A](d: Deque[A]) -> A?` | Kotlin・Swift `first` / Rust `front` | 新增 | R-deque-01 |
| `deque.last` | `fn last[A](d: Deque[A]) -> A?` | Kotlin・Swift `last` / Rust `back` | 新增 | R-deque-01 |
| `deque.get` | `fn get[A](d: Deque[A], i: Int) -> A?` | Rust・Kotlin `get` / Python `d[i]` | 新增 | R-deque-01 |
| `deque.contains` | `fn contains[A](d: Deque[A], x: A) -> Bool` | Rust `contains` / Python `in` / Erlang `member` | 新增 | R-deque-02 |
| `deque.map` | `fn map[A, B](d: Deque[A], f: (A) -> B) -> Deque[B]` | Kotlin・Swift（集合） | 新增 | R-deque-02 |
| `deque.filter` | `fn filter[A](d: Deque[A], f: (A) -> Bool) -> Deque[A]` | Erlang `:queue.filter` | 新增 | R-deque-02 |
| `deque.fold` | `fn fold[A, B](d: Deque[A], init: B, f: (B, A) -> B) -> B` | Erlang `:queue.fold` / OCaml `Queue.fold` | 新增 | R-deque-02 |
| `deque.any` | `fn any[A](d: Deque[A], f: (A) -> Bool) -> Bool` | Erlang `:queue.any` | 新增 | R-deque-02 |
| `deque.all` | `fn all[A](d: Deque[A], f: (A) -> Bool) -> Bool` | Erlang `:queue.all` | 新增 | R-deque-02 |
| `deque.count` | `fn count[A](d: Deque[A], f: (A) -> Bool) -> Int` | Kotlin `count {}` | 新增 | R-deque-02 |
| `deque.find` | `fn find[A](d: Deque[A], f: (A) -> Bool) -> A?` | Kotlin `find` / Swift `first(where:)` | 新增 | R-deque-02 |
| `deque.clear` | `fn clear[A](mut d: Deque[A]) -> Unit` | Rust `clear` / Python `clear` | 新增 | R-deque-02、R-L0-04 |
| `deque.reverse` | `fn reverse[A](d: Deque[A]) -> Deque[A]` | Swift `reversed` / Kotlin `reversed` | 新增 | R-deque-04 |
| `deque.take` | `fn take[A](d: Deque[A], n: Int) -> Deque[A]` | Swift `prefix` | 新增 | R-deque-04 |
| `deque.drop` | `fn drop[A](d: Deque[A], n: Int) -> Deque[A]` | Swift `dropFirst` | 新增 | R-deque-04 |
| `deque.take_end` | `fn take_end[A](d: Deque[A], n: Int) -> Deque[A]` | Swift `suffix` | 新增 | R-deque-04 |
| `deque.drop_end` | `fn drop_end[A](d: Deque[A], n: Int) -> Deque[A]` | Swift `dropLast` | 新增 | R-deque-04 |
| `deque.slice` | `fn slice[A](d: Deque[A], start: Int, end: Int) -> Deque[A]` | Swift `d[a..<b]` / Rust `range` | 新增 | R-deque-04 |
| `deque.set` | `fn set[A](d: Deque[A], i: Int, x: A) -> Deque[A]` | Swift `d[i] = x` / Rust `d[i] = x` | 新增 | R-deque-04 |
| `deque.insert` | `fn insert[A](d: Deque[A], i: Int, x: A) -> Deque[A]` | Swift `insert(_:at:)` / Rust `insert` | 新增 | R-deque-04 |
| `deque.remove_at` | `fn remove_at[A](d: Deque[A], i: Int) -> Deque[A]` | Swift `remove(at:)` / Rust `remove` | 新增 | R-deque-04 |
| `deque.enumerate` | （新增）`deque.enumerate(d: Deque[A]) -> List[(Int, A)]` | Swift `enumerated` / Rust `enumerate` | 新增 | R-L0-42 |
| `deque.zip` | （新增）`deque.zip(d: Deque[A], ys: List[B]) -> List[(A, B)]` | Swift `zip` / Rust `zip` | 新增 | R-L0-42 |
| `deque.zip_with` | （新增）`deque.zip_with(d: Deque[A], ys: List[B], f: (A, B) -> C) -> List[C]` | Kotlin `zip(other, f)` | 新增 | R-L0-42 |
| `deque.filter_map` | （新增）`deque.filter_map(d: Deque[A], f: (A) -> B?) -> List[B]` | Swift `compactMap` / Rust `filter_map` | 新增 | R-L0-42 |
| `deque.find_map` | （新增）`deque.find_map(d: Deque[A], f: (A) -> B?) -> B?` | Rust `find_map` | 新增 | R-L0-42 |
| `deque.reduce` | （新增）`deque.reduce(d: Deque[A], f: (A, A) -> A) -> A?` | Swift `reduce` / Rust `reduce` | 新增 | R-L0-42 |
| `deque.scan` | （新增）`deque.scan(d: Deque[A], init: B, f: (B, A) -> B) -> List[B]` | Rust `scan` / Kotlin `runningFold` | 新增 | R-L0-42 |
| `deque.partition` | （新增）`deque.partition(d: Deque[A], f: (A) -> Bool) -> (List[A], List[A])` | Kotlin `partition` | 新增 | R-L0-42 |
| `deque.group_by` | （新增）`deque.group_by(d: Deque[A], f: (A) -> B) -> Map[B, List[A]]` | Kotlin `groupBy` / Swift `Dictionary(grouping:)` | 新增 | R-L0-42 |
| `deque.sort` | （新增）`deque.sort(d: Deque[A]) -> List[A]` | Swift `sorted()` | 新增 | R-L0-42 |
| `deque.sort_by` | （新增）`deque.sort_by(d: Deque[A], f: (A) -> B) -> List[A]` | Kotlin `sortedBy` | 新增 | R-L0-42 |
| `deque.sort_with` | （新增）`deque.sort_with(d: Deque[A], cmp: (A, A) -> Int) -> List[A]` | Swift `sorted(by:)` | 新增 | R-L0-42 |
| `deque.min` | （新增）`deque.min(d: Deque[A]) -> A?` | Swift `min()` / Rust `min` | 新增 | R-L0-42 |
| `deque.max` | （新增）`deque.max(d: Deque[A]) -> A?` | Swift `max()` / Rust `max` | 新增 | R-L0-42 |
| `deque.min_by` | （新增）`deque.min_by(d: Deque[A], f: (A) -> K) -> A?` | Kotlin `minByOrNull` | 新增 | R-L0-42 |
| `deque.max_by` | （新增）`deque.max_by(d: Deque[A], f: (A) -> K) -> A?` | Kotlin `maxByOrNull` | 新增 | R-L0-42 |
| `deque.min_with` | （新增）`deque.min_with(d: Deque[A], cmp: (A, A) -> Int) -> A?` | Swift `min(by:)` | 新增 | R-L0-42 |
| `deque.max_with` | （新增）`deque.max_with(d: Deque[A], cmp: (A, A) -> Int) -> A?` | Swift `max(by:)` | 新增 | R-L0-42 |
| `deque.sum` | （新增）`deque.sum[A: Numeric](d: Deque[A]) -> A` | Rust `sum` / Kotlin `sum` | 新增 | R-L0-42 |
| `deque.product` | （新增）`deque.product[A: Numeric](d: Deque[A]) -> A` | Rust `product` | 新增 | R-L0-42 |
| `deque.find_index` | （新增）`deque.find_index(d: Deque[A], f: (A) -> Bool) -> Int?` | Swift `firstIndex(where:)` | 新增 | R-L0-42 |
| `deque.take_while` | （新增）`deque.take_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]` | Swift `prefix(while:)` | 新增 | R-L0-42 |
| `deque.drop_while` | （新增）`deque.drop_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]` | Swift `drop(while:)` | 新增 | R-L0-42 |
| `deque.chunk` | （新增）`deque.chunk(d: Deque[A], n: Int) -> List[Deque[A]]` | Kotlin `chunked` | 新增 | R-L0-42 |
| `deque.window` | （新增）`deque.window(d: Deque[A], n: Int) -> List[Deque[A]]` | Kotlin `windowed` | 新增 | R-L0-42 |
| `deque.split_at` | （新增）`deque.split_at(d: Deque[A], n: Int) -> (Deque[A], Deque[A])` | Rust `split_at` | 新增 | R-L0-42 |
| `deque.dedup` | （新增）`deque.dedup(d: Deque[A]) -> Deque[A]` | Rust `dedup` | 新增 | R-L0-42 |
| `deque.intersperse` | （新增）`deque.intersperse(d: Deque[A], sep: A) -> Deque[A]` | Rust `intersperse` | 新增 | R-L0-42 |
| `deque.unique` | （新增）`deque.unique(d: Deque[A]) -> Deque[A]` | Kotlin `distinct` | 新增 | R-L0-42 |
| `deque.unique_by` | （新增）`deque.unique_by(d: Deque[A], f: (A) -> K) -> Deque[A]` | Kotlin `distinctBy` | 新增 | R-L0-42 |

<a id="l0-への差し戻し処理済み"></a>

## 退回 L0 的事项（已处理）

1. 带前端限定的动词（`push_front`・`pop_front`）：已在 R-L0-04 的 2 中纳入与 `push`・`pop` 相同的族。R-L0-G 门禁将这 2 个视为 `mut` 动词集合成员。
2. L1 R-L1-01 中的“`pop_*` 是返回二元组的复制版”：已根据 R-L0-04 的 3，将 L1 正文改为 `mut` 原地修改。本页与 L1 一致。
