<!-- description: Stdlib重设计L2/L3 — heap新模块R-L1-02：最小优先队列，key(_by)/comparator(_with)构造，像list原地push/pop，确定同优先级顺序 -->
<a id="l2l3-heap新しいモジュール"></a>

# L2/L3: heap（新模块）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-heap.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-heap.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-heap.zh-CN.md)

> 非规范性带日期重设计快照，固定原文为权威且不变，不自动同步，无新测量或实现验证。F标记属于原调查而非本次。保留初始十函数及后续六项新增。R-heap-03现状例已是int.compare却仍提修正，照录而不捏造旧语法。内部顺序无关及跨target相同是原设计主张，不是新普遍证明；不增加未写明callback可观测性保证。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-heap.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)、[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-02：新增优先队列`heap`，默认最小堆，key版本`new_by`）。
> 来源：Rust https://doc.rust-lang.org/std/collections/struct.BinaryHeap.html ；Go https://pkg.go.dev/container/heap ；Python https://docs.python.org/3/library/heapq.html ；TS/JS https://jsr.io/@std/data-structures （Deno std `BinaryHeap`，ECMAScript无）；Gleam — 无；Elixir — 无，以`:gb_sets`替代；OCaml https://ocaml.org/manual/latest/api/Pqueue.html ；Kotlin https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/util/PriorityQueue.html （JVM）；Swift https://swiftpackageindex.com/apple/swift-collections/documentation/heapmodule
> (F)表示原调查中获取并核实的单元格。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide（提案） | Rust | Go | Python | TS/JS(Deno) | Gleam | Elixir | OCaml | Kotlin(JVM) | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 默认方向 | 最小优先 | 最大优先 | `Less` 决定 | 最小(F)（从3.14起也有 `_max` 版本(F)） | 比较函数决定 | — | — | 最小（`pop_min`） | 最小 | 两者（`popMin`・`popMax`） |
| 创建空堆 | `new` | `new` | `heap.Init` | `[]` | `new BinaryHeap()` | — | — | `create` | `PriorityQueue()` | `Heap()` |
| 按key排序 | `new_by(key)` | `Reverse`/元组 | `Less` 需实现 | `(key, x)` 元组 | 比较函数 | — | — | `Make(Ord)` | `compareBy` | `Comparable` |
| 比较函数排序 | `new_with(cmp)` | 实现`Ord` | `Less` | — | `new BinaryHeap(cmp)` | — | — | `Make(Ord)` | `PriorityQueue(cmp)` | — |
| 从list | `from_list` | `from` | `heap.Init` | `heapify`(F) | `BinaryHeap.from` | — | — | `of_list` | `PriorityQueue(c)` | `Heap(xs)` |
| 添加 | `push` | `push` | `heap.Push` | `heappush`(F) | `push` | — | — | `add` | `add` | `insert` |
| 取出首项 | `pop` | `pop` | `heap.Pop` | `heappop`(F) | `pop` | — | — | `pop_min` | `poll` | `popMin` |
| 查看首项 | `peek` | `peek` | `h[0]` | `h[0]` | `peek` | — | — | `min_elt` | `peek` | `min` |
| 长度／是否空 | `len`・`is_empty` | `len`・`is_empty` | `Len` | `len` | `length` | — | — | `length`・`is_empty` | `size`・`isEmpty` | `count`・`isEmpty` |
| 有序list | `to_list` | `into_sorted_vec` | — | `sorted(h)` | `drain` | — | — | — | — | `unordered` |

<a id="完全性の規則"></a>

### 完整性规则

- 三种排序方式：默认`A`自然顺序，最小优先；key `new_by(key: (A) -> K)`；comparator `new_with(cmp: (A, A) -> Int)`。后缀遵循R-L0-10：`_by`是key函数，`_with`是代替值而接收的函数；R-L0-16规定比较函数为`(A, A) -> Int`。最大优先可写`new_with((a, b) => int.compare(b, a))`。
- 仅空构造器可指定顺序。`from_list`只用默认顺序。将list放进有key/comparator的heap时，先`new_by`后`push`；刻意不加`from_list_by`等，避免构造器变成3 × 2种。
- **添加／取出用`mut`原地修改**，R-L0-04，与`list.push` / `list.pop`同动词。不提供返回pair的copy版`pop`。将来若增加，pair顺序为`(Option[A], Heap[A])`，取出元素在前，与`list.pop`返回值同阅读顺序，和L2-deque同判定。
- 无法取出时为none，R-L0-06。
- 同优先级元素按插入顺序取出，稳定；native/wasm确定为同一顺序，ADR-0016。`to_list`与重复`pop`同序。
- `Heap`不声明为有序类型，故无`heap.compare`；R-L0-16仅针对有序类型。

<a id="l2-の判定"></a>

### L2判断

- 新模块，拥有六种语言多数派的全部角色。

<a id="案"></a>

## 提案

<a id="r-heap-01-heap-モジュールを足す---採用"></a>

### R-heap-01 新增`heap` — ○ 采纳

- 内容：下方L3十个函数，类型`Heap[A]`；纯Almide二叉堆，每元素带插入序号，使同优先级稳定。
- L0依据：R-L0-04、R-L0-06、R-L0-10、R-L0-02、R-L0-07（`from_list` / `to_list`）。
- 写法：`push` / `pop` / `peek`随Rust、Deno、Kotlin(JVM)；Python为`heappush`/`heappop` (F)，OCaml为`add`/`pop_min`。`peek`是Almide新动词，但`first`已由`list`/`deque`用于序列开头；heap内部并未排序，故另用词。
- (a) 六语言具备，见L1。最小优先与Python(F)、Go、OCaml、Kotlin相同。
- (b) 纯计算，native/wasm结果相同，包括同优先级顺序。
- (c) LLM写Dijkstra或top-k题会假定优先队列存在。同优先级顺序确定，测试期望不随target变化。
- (d) 新增模块，自动import。
- Gate提案：性质测试，重复`pop`与`list.sort`同序，同优先级按插入顺序；native/wasm cross fixture；测量n次push/pop为O(n log n)。
- Issue：#3933。

<a id="r-heap-02-heap-には中の並びに依存しないセルだけを置くcontainsanyallcountfilterclear---採用"></a>

### R-heap-02 仅提供不依赖内部排列的操作（`contains`、`any`、`all`、`count`、`filter`、`clear`）— ○ 采纳

- 当时：`heap`十函数，无D3族或`contains`。L2-heap定义`to_list`为重复`pop`顺序，但不规定内部array排列。
- 提案新增六函数：
  - `fn heap.contains[A](h: Heap[A], x: A) -> Bool`
  - `fn heap.any[A](h: Heap[A], f: (A) -> Bool) -> Bool`、同形的`all`、`fn heap.count[A](h: Heap[A], f: (A) -> Bool) -> Int`
  - `fn heap.filter[A](h: Heap[A], f: (A) -> Bool) -> Heap[A]`，保留默认/`_by`/`_with`排序定义及同优先级插入顺序。
  - `fn heap.clear[A](mut h: Heap[A]) -> Unit`，保留排序定义，R-L0-04 `clear`矩阵。
  - 刻意不提供`map`、`fold`、`find`。若需排序遍历，写`heap.to_list(h) |> list.<動詞>`。
- L0依据：R-map-06的D3族，但heap不提供可从外部观察遍历顺序的操作。R-L0-04，除`clear`外均copy。
- (a) Rust `BinaryHeap`的`iter()`按“arbitrary order”遍历，无`contains` (F)，https://doc.rust-lang.org/std/collections/struct.BinaryHeap.html 。JVM `PriorityQueue`也不保证迭代顺序，无语言承诺heap内部排列。
- (b) 仅顺序无关操作，未来改变内部array排列也不改变两target及用户代码结果。
- (c) `map`无法把`new_by(key)`/`new_with(cmp)`排序定义继承到`B`，会静默回默认；`fold`/`find`若按内部顺序走就泄露排列，按`pop`顺序走又隐藏名称未提示的排序。
- (d) 新增六个，新模块无兼容影响。
- 完整性：只从D3提供返回值不依赖遍历顺序的操作，返回Bool、Int或保持相同排序定义的Heap。将`map`/`fold`/`find`及理由放入R-map-06允许清单。
- Gate提案：R-map-06矩阵加入heap列，允许清单记录三项省略。性质测试：`heap.to_list(heap.filter(h, f)) == list.filter(heap.to_list(h), f)`。
- Issue：#3968。

<a id="r-heap-03-heapnew_with-の例を引き算にしない---採用文書だけ"></a>

### R-heap-03 `heap.new_with`示例不用减法 — ○ 采纳，仅文档

- 当时：完整性规则教最大优先用`new_with((a, b) => int.compare(b, a))`。减法在`Int`边界回绕、符号反转。
- 提案：示例改为`heap.new_with((a, b) => int.compare(b, a))`，即R-L0-16 `int.compare`；CHEATSHEET与docs/stdlib也用同例，放入```` ```almide check ```` fence。
- (c) LLM直接复制文档示例。
- (d) 仅文档，待`int.compare`落地再替换。
- Issue：#3968。

<a id="r-heap-04-new_minnew_max-に分ける---不採用"></a>

### R-heap-04 分成`new_min` / `new_max` — × 不采纳

- 理由：默认最小优先，与Python/Go/OCaml/Kotlin一致，R-L1-02。最大优先用`heap.new_with((a, b) => int.compare(b, a))`，不用差值`b - a`，会在`Int`边界溢出；或用`new_by`。默认方向已记在索引的共同约定。
- Issue：无。

<a id="r-heap-05-heapfoldheapfind-を足す---不採用"></a>

### R-heap-05 新增`heap.fold` / `heap.find` — × 不采纳

- 理由：按内部array顺序遍历会泄露排列，实现改变则结果变；按取出顺序、小到大遍历则隐藏名称未提示的排序。需顺序时写`heap.to_list(h) |> list.fold(…)`可显式展示排序。与L2完整性规则相同：只提供返回值不依赖遍历顺序的操作。
- Issue：无。

<a id="r-heap-06--は取り出す順の要素の並びで比べh-もその順で書く---採用"></a>

### R-heap-06 `==`按取出顺序的元素序列比较，`"${h}"`也按该顺序输出 — ○ 采纳

- 当时#4033实现：`Heap`是array二叉堆。内建`==`比较内部array，因此两heap即使元素相同，插入顺序不同也可能不相等。
- 提案：`==`比较取出顺序、重复`pop`的元素序列。`"${h}"`也按该序，如`heap[1, 2, 5]`。两heap的排序定义`new`/`new_by`/`new_with`即使不同，只要序列相同就相等。
- L0依据：R-L0-37、R-heap-02，不暴露内部排列。
- (a) Rust `BinaryHeap`和Swift `Heap`无相等操作；Almide除`Fn`外所有值类型均有`==`，无法选择省略。内部array比较违反R-heap-02，故比较外部可见序列。
- Issue：#4033。

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（提案） | 其他语言主要写法 | 判断 | 提案 |
|---|---|---|---|---|
| `heap.new` | `fn new[A]() -> Heap[A]` | Rust `new` / Kotlin `PriorityQueue()` / Swift `Heap()` | 新增 | R-heap-01 |
| `heap.new_by` | `fn new_by[A, K](key: (A) -> K) -> Heap[A]` | Kotlin `compareBy` / Python `(key, x)` | 新增 | R-heap-01 |
| `heap.new_with` | `fn new_with[A](cmp: (A, A) -> Int) -> Heap[A]` | Kotlin `PriorityQueue(cmp)` / Deno `new BinaryHeap(cmp)` | 新增 | R-heap-01 |
| `heap.from_list` | `fn from_list[A](xs: List[A]) -> Heap[A]` | Python `heapify`(F) / Rust `from` / OCaml `of_list` | 新增 | R-heap-01 |
| `heap.push` | `fn push[A](mut h: Heap[A], x: A) -> Unit` | Rust・Deno `push` / Python `heappush`(F) | 新增 | R-heap-01 |
| `heap.pop` | `fn pop[A](mut h: Heap[A]) -> A?` | Rust・Deno `pop` / Python `heappop`(F) / Swift `popMin` | 新增 | R-heap-01 |
| `heap.peek` | `fn peek[A](h: Heap[A]) -> A?` | Rust・Kotlin・Deno `peek` / Swift `min` | 新增 | R-heap-01 |
| `heap.len` | `fn len[A](h: Heap[A]) -> Int` | Rust `len` / Python `len` | 新增 | R-heap-01 |
| `heap.is_empty` | `fn is_empty[A](h: Heap[A]) -> Bool` | Rust `is_empty` / Kotlin `isEmpty` | 新增 | R-heap-01 |
| `heap.to_list` | `fn to_list[A](h: Heap[A]) -> List[A]` | Rust `into_sorted_vec` / Python `sorted(h)` | 新增 | R-heap-01 |
| `heap.contains` | `fn contains[A](h: Heap[A], x: A) -> Bool` | Python `in`（作为list） | 新增 | R-heap-02 |
| `heap.any` | `fn any[A](h: Heap[A], f: (A) -> Bool) -> Bool` | Kotlin `any`（PriorityQueue） | 新增 | R-heap-02 |
| `heap.all` | `fn all[A](h: Heap[A], f: (A) -> Bool) -> Bool` | Kotlin `all` | 新增 | R-heap-02 |
| `heap.count` | `fn count[A](h: Heap[A], f: (A) -> Bool) -> Int` | Kotlin `count {}` | 新增 | R-heap-02 |
| `heap.filter` | `fn filter[A](h: Heap[A], f: (A) -> Bool) -> Heap[A]` | Rust `retain`（原地） | 新增 | R-heap-02 |
| `heap.clear` | `fn clear[A](mut h: Heap[A]) -> Unit` | Rust `clear` / Kotlin `clear` | 新增 | R-heap-02、R-L0-04 |
