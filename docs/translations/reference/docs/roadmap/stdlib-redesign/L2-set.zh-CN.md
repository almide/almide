> 说明（原本索引元数据的非活动副本）：标准库重设计 L2/L3 — set：用 count 和 find 补齐高阶函数族，insert/remove 保持复制，拒绝 superset 和 partition
<a id="l2l3-set"></a>

# L2/L3: set

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-set.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-set.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-set.zh-CN.md)

> 非规范性历史重设计参照版，固定原本权威且不变。不自动同步、未独立重跑或核实现行实现。保留初始全部复制/无mut/无需clear规则与后段clear mut表行、partition拒绝与后段新增行、21行/19+2计数与扩展表的差异。带日期probe及多数派为原报告，不是新测量；E042仍以提案规则落地为条件。原代码日文注释意为：不报错、什么也没发生；赋值写法正确。 (F) 标签记录原作者当时的获取核实，不表示本次翻译重新获取了来源。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-set.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)・[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> 来源：Rust https://doc.rust-lang.org/std/collections/struct.HashSet.html (F) ・Go https://pkg.go.dev/maps (F)（没有 set 类型）・Python https://docs.python.org/3/library/stdtypes.html#set-types-set-frozenset ・TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Set (F) ・Gleam https://gleam-stdlib.hexdocs.pm/gleam/set.html (F) ・Elixir https://elixir.hexdocs.pm/MapSet.html (F) ・OCaml https://ocaml.org/manual/latest/api/Set.S.html (F) ・Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.collections/-set/ ・Swift https://developer.apple.com/documentation/swift/set (F)
> 共通判定：与 list・map・option・result・string 页面一样遵循 D1–D11（[L2-map 对照表](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-map.md#共通の判定との対応)）

阅读约定（同 L0）：
- `mut`（读作 mute）= 原地改写调用者 `var` 的参数
- `|>`（读作 pipe）= 将左侧值作为右侧函数第 1 个参数传入

当前公开函数有 19 个（interface JSON，带 `mut` 标记）。有 `mut` 参数的函数一个也没有，set 全部采用复制。

---

<a id="l2-役割の比較"></a>

## L2：角色比较

(F) 是本轮获取官方页面并核实的单元。Go 没有 set 类型，用 `map[T]struct{}` 代替（所有行 —）。

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 创建空集合／从列表构建 | `set.new`・`set.from_list` | `new`・`from_iter` | — | `set()`・`set(xs)` | `new Set(xs)` (F) | `new`・`from_list` (F) | `new/0`・`new/1` (F) | `empty`・`of_list` (F) | `setOf`・`toSet` | `Set(xs)` |
| 添加 | `set.insert`（copy） | `insert` (F, in-place) | — | `add`（in-place） | `add` (F, in-place) | `insert` (F, copy) | `put` (F, copy) | `add` (F, copy) | `add`（in-place）/ `s + x` | `insert` (F, mutating) |
| 移除 | `set.remove`（copy） | `remove` (F) | — | `remove`・`discard` | `delete` (F) | `delete` (F) | `delete` (F) | `remove` (F) | `remove` / `s - x` | `remove` (F) |
| 是否包含 | `set.contains` | `contains` (F) | — | `x in s` | `has` (F) | `contains` (F) | `member?` (F) | `mem` (F) | `contains` | `contains` (F) |
| 元素数／空判定 | `set.len`・`set.is_empty` | `len`・`is_empty` (F) | — | `len` | `size` (F) | `size`・`is_empty` (F) | `size` (F) | `cardinal`・`is_empty` (F) | `size`・`isEmpty` | `count`・`isEmpty` |
| 转为列表 | `set.to_list` | `iter().collect` | — | `list(s)` | `[...s]` | `to_list` (F) | `to_list` (F) | `to_list`・`elements` (F) | `toList` | `Array(s)` |
| 并／交／差／对称差 | `union`・`intersection`・`difference`・`symmetric_difference` | 相同 4 个 (F) | — | 相同 4 个 | `union`・`intersection`・`difference`・`symmetricDifference` (F) | 相同 4 个 (F) | 相同 4 个 (F) | `union`・`inter`・`diff` (F) | `union`・`intersect`・`subtract` | `union`・`intersection`・`subtracting`・`symmetricDifference` (F) |
| 是否子集 | `set.is_subset` | `is_subset` (F) | — | `issubset` | `isSubsetOf` (F) | `is_subset` (F) | `subset?` (F) | `subset` (F) | `b.containsAll(a)` | `isSubset(of:)` (F) |
| 是否超集 | — | `is_superset` (F) | — | `issuperset` | `isSupersetOf` (F) | — (F) | — (F) | — (F) | `containsAll` | `isSuperset(of:)` (F) |
| 是否不相交 | `set.is_disjoint` | `is_disjoint` (F) | — | `isdisjoint` | `isDisjointFrom` (F) | `is_disjoint` (F) | `disjoint?` (F) | `disjoint` (F) | — | `isDisjoint(with:)` (F) |
| 筛选／映射／折叠 | `set.filter`・`set.map`・`set.fold` | `retain`・`iter().map`・`fold` | — | 推导式 | — | `filter`・`map`・`fold` (F) | `filter` (F)・`Enum.map` | `filter`・`map`・`fold` (F) | `filter`・`map`・`fold` | `filter`・`map` (F)・`reduce` |
| any / all | `set.any`・`set.all` | `iter().any`・`all` | — | `any()`・`all()` | — | — (F) | `Enum.any?`・`all?` | `exists`・`for_all` (F) | `any`・`all` | `contains(where:)`・`allSatisfy` |
| 按谓词计数 | — | `iter().filter().count()` | — | `sum(...)` | — | — (F) | `Enum.count/2` | — (F) | `count {}` | `count(where:)` (F) |
| 按谓词查找 1 个 | — | `iter().find` | — | `next(...)` | — | — (F) | `Enum.find` | `find_first`（仅单调谓词）(F) | `find` | `first(where:)` (F) |
| 按谓词分为 2 份 | — | — | — | — | — | — (F) | `split_with` (F) | `partition` (F) | `partition` | — |

<a id="完全性の規則"></a>

### 完整性规则

1. **高阶函数族（list・map・set 共通）**：`map`、`filter`、`fold`、`any`、`all`、`count`、`find` 共 7 个，3 类集合全部提供（共通判定 D3、[L2-map R-map-06](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-map.md)）。set 缺 `count` 和 `find` → R-set-01。
2. **基本操作集合（list・map・set 共通）**：`new`、`from_list`、`len`、`is_empty`、`contains`。set 全部具备。
3. **集合运算族**：由 2 个 set 生成 set 的 4 个（`union`、`intersection`、`difference`、`symmetric_difference`），以及比较 2 个 set 并返回 Bool 的 2 个（`is_subset`、`is_disjoint`）。超集 `is_superset` 与 `is_subset` 交换参数后的判定相同，因此有意不提供（R-set-02）。
4. **修改方式**：set 所有公开函数都采用复制，没有带 `mut` 参数的函数。`insert`・`remove` 属于复制动词（R-L0-04）。set 没有需要原地动词（`push`・`pop`・`clear`）的操作。
5. **set.map 的性质**：转换结果重复时元素数减少（实测：把 `{1, 2, 3}` 用 `x % 2` 转换得到 `[1, 0]`）。这符合 set 定义，不改变。

<a id="l2-の判定"></a>

### L2 判定

- **函数族缺口**：没有 `set.count` 和 `set.find` → R-set-01。现在写 `set.count` 会得到 E002，提示“用 `set.len`”（实测）。它来自 `ALIASES` 表（`crates/almide-frontend/src/stdlib.rs:162`）的 `("set", "count", "set.len")` 行，将希望按谓词计数的作者引向错误函数。R-set-01 删除该行。
- **缺失角色（5/9 以上）**：
  - 是否超集：有专用名称的是 Rust・Python・JS・Swift，共 4/9（Kotlin `containsAll` 是集合通用函数）→ R-set-02（×）。
  - 按谓词分为 2 份：Elixir・OCaml・Kotlin，共 3/9 → R-set-03（×）。
- **拼写冲突**：没有。`insert` 为 Rust・Gleam・Swift 的 3/9，`add` 为 Python・JS・OCaml・Kotlin 的 4/9，无多数，故保留（R-L0-00）。`remove` 为 Rust・Python・OCaml・Kotlin・Swift 的 5/9，与多数一致。
- **被静默丢弃的调用（R-L0-05）**：正是 L0 正文中的实测示例。native 与 wasm 目前仍如下静默通过（2026-10-21，`almide 0.68.0 (dev, 437b8f127)`，`scratchpad/sr/probe-set/s.almd`）：

  ```almide
  var s = set.new()
  set.insert(s, 1)     // エラーにならない。何も起きない
  println(int.to_string(set.len(s)))   // 0
  s = set.insert(s, 1) // 正しい書き方
  ```

  set 所有函数均复制，因此把 `set.insert`・`set.remove`・`set.union`・`set.filter` 写成语句时，结果必定被丢弃。Python・JS・Rust・Kotlin・Swift 的 `add`/`insert` 原地修改，所以这是 LLM 最容易弄错的模块。R-L0-05 加入后会用 E042 阻止。不为 set 提出改名案（R-L0-04 已将其定为复制动词）。

---

<a id="案"></a>

## 提案

<a id="r-set-01-setcount-と-setfind-を足す---採用"></a>

### R-set-01 添加 `set.count` 和 `set.find` — ○ 采纳

- 内容：
  - `set.count(s: Set[A], f: (A) -> Bool) -> Int` — 满足 `f` 的元素数，空集为 0。
  - `set.find(s: Set[A], f: (A) -> Bool) -> Option[A]` — 按插入顺序第一个满足 `f` 的元素，没有则 none。找到后不再调用 `f`（与 `list.find` 相同）。
  - 两者均用纯 Almide 写入 `stdlib/set_core.almd`，注册到 `self_host_registry.rs`。
  - 删除 `ALIASES` 中的 `("set", "count", "set.len")`（`stdlib.rs:162`）。在同表加入 `("set", "add", "set.insert")`・`("set", "has", "set.contains")`（截至 2026-10-21 两者均无，与 R-map-03 相同机制）。
- 依据的 L0 规则：R-L0-01（相同角色用相同拼写；与 list 和 map 的 `count`・`find` 同名、同回调形式）、R-L0-10、CLAUDE.md 的“用矩阵扩展函数族”。
- (a) 具有专用名称的语言，计数为 Kotlin `count {}`・Elixir `Enum.count/2`・Swift `count(where:)` (F)，查找为 Kotlin `find`・Elixir `Enum.find`・Swift `first(where:)` (F)，两者均不到 5/9。但这次新增不是应用 L1“增加新角色”标准，而是补齐 Almide 内部函数族（完整性规则 1）。list・map 有而 set 没有，LLM 按规则推测书写就会出错。
- (b) 纯计算，不需 host 功能。native 与 wasm 都按相同顺序（插入顺序）运行。
- (c) 可按 `xs |> list.count(f)`、`m |> map.count(f)` 相同形式写 `s |> set.count(f)`。当前“用 `set.len`”的提示会诱导错误地丢弃谓词。
- (d) 新增，无兼容性问题。使用数 0（新拼写）。spec / tests / docs / stdlib 都没有写 `set.count` 而得到 E002 的调用。
- 拟议门禁：L2-map R-map-06 的“集合函数族矩阵”测试要求 set 的这 2 个单元。在 `spec/stdlib/set_test.almd` 中添加边界测试（空、无匹配、全部匹配），为 almide-interp 的 3-way oracle 增加桥接。
- issue: #3901

<a id="r-set-02-setis_superset-を足す---不採用"></a>

### R-set-02 添加 `set.is_superset` — × 不采纳

- 内容：不添加。`a` 是否为 `b` 的超集，用 `set.is_subset(b, a)`。
- 依据的 L0 规则：R-L0-01（仅交换参数的同一判定不提供 2 种拼写）、L1 新增标准（5/9 以上）。
- (a) 专用名称为 Rust `is_superset` (F)・Python `issuperset`・JS `isSupersetOf` (F)・Swift `isSuperset(of:)` (F)，共 4/9。Gleam・Elixir・OCaml 只有子集判定 (F)。
- (c) 为作者写出 `set.is_superset` 的情况，在 `ALIASES` 添加 `("set", "is_superset", "set.is_subset (arguments swapped)")`。
- (b)(d) 无变更（仅提示）。
- issue：在 R-set-01 同一 issue 中添加提示。

<a id="r-set-03-setpartition-を足す---不採用"></a>

### R-set-03 添加 `set.partition` — × 不采纳

- 内容：不添加。可写成 `(set.filter(s, f), set.filter(s, (x) => not f(x)))`。
- 依据的 L0 规则：L1 新增标准（5/9 以上）、完整性规则 1（函数族的 7 个中不含 `partition`）。
- (a) Elixir `split_with` (F)・OCaml `partition` (F)・Kotlin `partition`，共 3/9。
- (b)(d) 无变更。
- issue：无

<a id="r-set-04-setinsert-を-setadd-に改名する---不採用"></a>

### R-set-04 将 `set.insert` 改名为 `set.add` — × 不采纳

- 内容：保留。
- 依据的 L0 规则：R-L0-00（无多数则不改）、R-L0-04（`insert` 在所有模块中统一为复制动词）。
- (a) `add` 为 Python・JS・OCaml・Kotlin 的 4/9，`insert` 为 Rust・Gleam・Swift 的 3/9，`put` 为 Elixir，均不到 5/9。
- (c) 写 `add` 时通过 `ALIASES` 行引导至 `set.insert`（R-set-01 添加的行）。静默丢弃错误由 R-L0-05 阻止，而不是靠名字。
- (d) 无变更。
- issue：无

---

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（现行，mut 标注） | 其他语言的主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `set.new` | `set.new() -> Set[A]` | Rust `new` / Gleam `new` / Elixir `new/0` / OCaml `empty` | 保持 | — |
| `set.from_list` | `set.from_list(xs: List[A]) -> Set[A]` | Gleam `from_list` / OCaml `of_list` / Kotlin `toSet` / Rust `from_iter` | 保持 | — (R-L0-07) |
| `set.insert` | `set.insert(s: Set[A], value: A) -> Set[A]` | Rust・Gleam・Swift `insert` / Python・JS・OCaml・Kotlin `add` / Elixir `put` | 保持 | R-set-04 |
| `set.remove` | `set.remove(s: Set[A], value: A) -> Set[A]` | Rust・Python・OCaml・Kotlin・Swift `remove` / JS・Gleam・Elixir `delete` | 保持 | — |
| `set.contains` | `set.contains(s: Set[A], value: A) -> Bool` | Rust・Gleam・Kotlin・Swift `contains` / JS `has` / OCaml `mem` / Elixir `member?` | 保持 | — |
| `set.len` | `set.len(s: Set[A]) -> Int` | Rust `len` / Gleam・JS・Elixir `size` / OCaml `cardinal` / Swift `count` | 保持 | — (R-L0-11) |
| `set.is_empty` | `set.is_empty(s: Set[A]) -> Bool` | Rust・Gleam・OCaml `is_empty` / Kotlin `isEmpty` / Swift `isEmpty` | 保持 | — |
| `set.to_list` | `set.to_list(s: Set[A]) -> List[A]` | Gleam・Elixir・OCaml `to_list` / Kotlin `toList` | 保持 | — |
| `set.union` | `set.union(a: Set[A], b: Set[A]) -> Set[A]` | 9 种语言中的 8 种 `union`（Go —） | 保持 | — |
| `set.intersection` | `set.intersection(a: Set[A], b: Set[A]) -> Set[A]` | Rust・Python・JS・Gleam・Elixir・Swift `intersection` / OCaml `inter` / Kotlin `intersect` | 保持 | — |
| `set.difference` | `set.difference(a: Set[A], b: Set[A]) -> Set[A]` | Rust・Python・JS・Gleam・Elixir `difference` / Swift `subtracting` / OCaml `diff` / Kotlin `subtract` | 保持 | — |
| `set.symmetric_difference` | `set.symmetric_difference(a: Set[A], b: Set[A]) -> Set[A]` | Rust・Python・Gleam・Elixir `symmetric_difference` / JS・Swift `symmetricDifference` | 保持 | — |
| `set.is_subset` | `set.is_subset(a: Set[A], b: Set[A]) -> Bool` | Rust・Gleam `is_subset` / Python `issubset` / JS `isSubsetOf` / OCaml `subset` / Swift `isSubset(of:)` | 保持 | R-set-02 |
| `set.is_disjoint` | `set.is_disjoint(a: Set[A], b: Set[A]) -> Bool` | Rust・Gleam `is_disjoint` / Python `isdisjoint` / JS `isDisjointFrom` / OCaml `disjoint` / Swift `isDisjoint(with:)` | 保持 | — |
| `set.filter` | `set.filter(s: Set[A], f: (A) -> Bool) -> Set[A]` | Gleam・Elixir・OCaml・Kotlin・Swift `filter` / Rust `retain` | 保持 | R-map-06 |
| `set.map` | `set.map(s: Set[A], f: (A) -> B) -> Set[B]` | Gleam・OCaml `map` / Kotlin `map`（返回 List）/ Swift `map`（返回 Array） | 保持 | R-map-06 |
| `set.fold` | `set.fold(s: Set[A], init: B, f: (B, A) -> B) -> B` | Gleam・OCaml `fold` / Kotlin `fold` / Swift `reduce` | 保持 | R-map-06 |
| `set.any` | `set.any(s: Set[A], f: (A) -> Bool) -> Bool` | Kotlin `any` / OCaml `exists` / Elixir `Enum.any?` / Swift `contains(where:)` | 保持 | R-map-06 |
| `set.all` | `set.all(s: Set[A], f: (A) -> Bool) -> Bool` | Kotlin `all` / OCaml `for_all` / Elixir `Enum.all?` / Swift `allSatisfy` | 保持 | R-map-06 |
| `set.count` | `set.count(s: Set[A], f: (A) -> Bool) -> Int`（提案） | Kotlin `count {}` / Elixir `Enum.count/2` / Swift `count(where:)` / Gleam — | 新增 | R-set-01 |
| `set.find` | `set.find(s: Set[A], f: (A) -> Bool) -> Option[A]`（提案） | Kotlin `find` / Elixir `Enum.find` / Swift `first(where:)` / Rust `iter().find` | 新增 | R-set-01 |
| `set.clear` | `set.clear(mut s: Set[A]) -> Unit` | Rust `clear` / Python `clear` / JS `clear` / Kotlin `clear` | 新增 | R-L0-04（`clear` 的矩阵） |
| `set.enumerate` | （新增）`set.enumerate(s: Set[A]) -> List[(Int, A)]` | Swift `enumerated` / Rust `enumerate` | 新增 | R-L0-42 |
| `set.zip` | （新增）`set.zip(s: Set[A], ys: List[B]) -> List[(A, B)]` | Swift `zip` / Rust `zip` | 新增 | R-L0-42 |
| `set.zip_with` | （新增）`set.zip_with(s: Set[A], ys: List[B], f: (A, B) -> C) -> List[C]` | Kotlin `zip(other, f)` | 新增 | R-L0-42 |
| `set.filter_map` | （新增）`set.filter_map(s: Set[A], f: (A) -> B?) -> List[B]` | Swift `compactMap` / Rust `filter_map` | 新增 | R-L0-42 |
| `set.find_map` | （新增）`set.find_map(s: Set[A], f: (A) -> B?) -> B?` | Rust `find_map` | 新增 | R-L0-42 |
| `set.reduce` | （新增）`set.reduce(s: Set[A], f: (A, A) -> A) -> A?` | Swift `reduce` / Rust `reduce` | 新增 | R-L0-42 |
| `set.scan` | （新增）`set.scan(s: Set[A], init: B, f: (B, A) -> B) -> List[B]` | Rust `scan` / Kotlin `runningFold` | 新增 | R-L0-42 |
| `set.partition` | （新增）`set.partition(s: Set[A], f: (A) -> Bool) -> (List[A], List[A])` | Kotlin `partition` | 新增 | R-L0-42 |
| `set.group_by` | （新增）`set.group_by(s: Set[A], f: (A) -> B) -> Map[B, List[A]]` | Kotlin `groupBy` / Swift `Dictionary(grouping:)` | 新增 | R-L0-42 |
| `set.sort` | （新增）`set.sort(s: Set[A]) -> List[A]` | Swift `sorted()` | 新增 | R-L0-42 |
| `set.sort_by` | （新增）`set.sort_by(s: Set[A], f: (A) -> B) -> List[A]` | Kotlin `sortedBy` | 新增 | R-L0-42 |
| `set.sort_with` | （新增）`set.sort_with(s: Set[A], cmp: (A, A) -> Int) -> List[A]` | Swift `sorted(by:)` | 新增 | R-L0-42 |
| `set.min` | （新增）`set.min(s: Set[A]) -> A?` | Swift `min()` / Rust `min` | 新增 | R-L0-42 |
| `set.max` | （新增）`set.max(s: Set[A]) -> A?` | Swift `max()` / Rust `max` | 新增 | R-L0-42 |
| `set.min_by` | （新增）`set.min_by(s: Set[A], f: (A) -> K) -> A?` | Kotlin `minByOrNull` | 新增 | R-L0-42 |
| `set.max_by` | （新增）`set.max_by(s: Set[A], f: (A) -> K) -> A?` | Kotlin `maxByOrNull` | 新增 | R-L0-42 |
| `set.min_with` | （新增）`set.min_with(s: Set[A], cmp: (A, A) -> Int) -> A?` | Swift `min(by:)` | 新增 | R-L0-42 |
| `set.max_with` | （新增）`set.max_with(s: Set[A], cmp: (A, A) -> Int) -> A?` | Swift `max(by:)` | 新增 | R-L0-42 |
| `set.sum` | （新增）`set.sum[A: Numeric](s: Set[A]) -> A` | Rust `sum` / Kotlin `sum` | 新增 | R-L0-42 |
| `set.product` | （新增）`set.product[A: Numeric](s: Set[A]) -> A` | Rust `product` | 新增 | R-L0-42 |

L3 行数：21（现有 19 + 新增 2）。判定明细：保持 19、新增 2。

使用数（参考，`\bset\.<fn>\b` 的出现次数，包含定义及生成文档的上限值；spec / tests / docs / stdlib）：insert 32 / 26 / 24 / 6、remove 8 / 20 / 13 / 0、to_list 33 / 20 / 5 / 9、map 20 / 15 / 16 / 6。

---

<a id="l0-への差し戻し"></a>

## 退回 L0 的事项

无。
