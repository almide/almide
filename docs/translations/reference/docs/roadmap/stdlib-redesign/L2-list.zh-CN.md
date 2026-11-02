读者说明：标准库重设计L2/L3——list：每种角色一种拼写（window/windows、get_or、shuffle）、比较器排序和min_by/max_by、unzip、Float求和，以及slice/chunk/window的统一边界规则。
<a id="l2l3-list"></a>

# L2/L3: list

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-list.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-list.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-list.zh-CN.md)

> 非规范性固定原文历史重设计读者版。固定正本不变，不自动同步，未新增探针、改实现或执行门禁。采纳案为当时决策，不是独立确认已落地。保留改订层次：初期有意省略比较函数min/max与后文R-list-19新增；原负slice/个数截边与第六轮abort/L3 R-L0-29；Int/Float及String/Bytes扩展与后续R-L0-42 Numeric/Text泛型化。后续新增后仍保留原末尾67+5=72行统计。F标记和测量属于原文10月21日调查/dev环境，不是本次核验。不统一角色数量及表述分歧。原文越界guard示例仅检查上界，不作为处理负索引的推荐。位相等和LLM行为断言仍是原文有限范围主张，不是新增独立证明。保留代码占位符中的型意为类型。命令及修复提案是历史文本，不是执行指令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-list.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)、[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> 来源：Rust https://doc.rust-lang.org/std/primitive.slice.html (F)、https://doc.rust-lang.org/std/iter/trait.Iterator.html ・ Go https://pkg.go.dev/slices (F) ・ Python https://docs.python.org/3/library/functions.html (F)、https://docs.python.org/3/library/stdtypes.html ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array (F) ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/list.html (F) ・ Elixir https://elixir.hexdocs.pm/Enum.html (F) ・ OCaml https://ocaml.org/manual/latest/api/List.html (F) ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.collections/ （`minByOrNull`见 https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.collections/min-by-or-null.html (F)） ・ Swift https://developer.apple.com/documentation/swift/array (F)

阅读约定（与L0相同）：
- `mut`（读作mute）=原地改写调用方`var`的参数
- `??`（读作null coalescing，空合并）=左侧为none时使用右侧的运算符
- `|>`（读作pipe，管道）=将左侧值插入右侧函数第1个参数的运算符
- (F)=此次获取官方文档核验的单元格。未标记项来自facts/naming-conventions.md或众所周知的拼写

实测环境：`almide 0.68.0 (dev, 437b8f127)`。探针为`scratchpad/sr/probe-list/p1.almd`至`p6.almd`。
使用数为`/usr/bin/grep -rEo '\blist\.<fn>\b'`出现次数，按spec / tests / docs / stdlib顺序列出（包含定义行及生成文档行，因此是上界）。

---

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 长度 | `len`（`length`已弃用） | `len` | `len()` | `len()` | `length` | `length` | `length`/`count` | `length` | `size` | `count` |
| 是否为空 | `is_empty` | `is_empty` | — | — | — | `is_empty` | `empty?` | `is_empty` (F) | `isEmpty` | `isEmpty` |
| 按位置取值（Option） | `get` | `get` | — | — | `at` (F) | — | `at`/`fetch` (F) | `nth_opt` (F) | `getOrNull` | — |
| 带默认值取值 | `get_or` | — | — | — | — | — | `at(xs,i,d)` | — | `getOrElse` | — |
| 首/尾 | `first`/`last` | `first`/`last` (F) | — | — | `at(0)`/`at(-1)` | `first`/`last` (F) | `at`/`List.first` | — | `firstOrNull`/`lastOrNull` | `first`/`last` |
| 按位置写入（复制） | `set`/`update` | — | — | — | `with` (F) | — | `List.replace_at` | — | — | — |
| 按位置插入/删除（复制） | `insert`/`remove_at` | — | `Insert`/`Delete` (F) | — | `toSpliced` (F) | — | `List.insert_at`/`delete_at` | — | — | — |
| 尾部添加/取出（原地） | `push`/`pop`（`mut`） | `push`/`pop` | `append` | `append`/`pop` | `push`/`pop` | — | — | — | `add`/`removeLastOrNull` | `append`/`popLast` |
| 全部清除（原地） | `clear`（`mut`） | `clear` | `clear()` | `clear` | `length = 0` | — | — | — | `clear` | `removeAll` |
| 拼接 | `+`运算符 | `concat` (F) | `Concat` (F) | `+` | `concat` | `append` (F) | `++` | `@`/`append` | `plus` | `+` |
| 是否包含 | `contains` | `contains` (F) | `Contains` (F) | `in` | `includes` (F) | `contains` | `member?` | `mem` (F) | `contains` | `contains` |
| 值的位置 | `index_of` | `iter().position` | `Index` (F) | `index` | `indexOf` (F) | — | `find_index` (F) | `find_index` (F) | `indexOf` | `firstIndex(of:)` (F) |
| 谓词对应位置 | `find_index` | `position` | `IndexFunc` (F) | — | `findIndex` | — | `find_index` (F) | `find_index` (F) | `indexOfFirst` | `firstIndex(where:)` |
| 按谓词查找 | `find` | `find` | — | — | `find` | `find` | `find` | `find_opt` | `find` | `first(where:)` |
| 转换/筛选 | `map`/`filter` | `map`/`filter` | — | 推导式 | `map`/`filter` | `map`/`filter` | `map`/`filter` | `map`/`filter` | `map`/`filter` | `map`/`filter` |
| 转换并排除none | `filter_map` | `filter_map` | — | — | — | `filter_map` | — | `filter_map` (F) | `mapNotNull` | `compactMap` (F) |
| 首个some | `find_map` | `find_map` | — | — | — | `find_map` (F) | `find_value` (F) | `find_map` (F) | `firstNotNullOfOrNull` | — |
| 展平 | `flatten`/`flat_map` | `flatten`/`flat_map` | — | — | `flat` (F)/`flatMap` (F) | `flatten` (F)/`flat_map` (F) | `concat`/`flat_map` (F) | `concat`/`concat_map` (F) | `flatten`/`flatMap` | `joined`/`flatMap` (F) |
| 折叠 | `fold`/`reduce` | `fold`/`reduce` | — | `functools.reduce` | `reduce` (F) | `fold`/`reduce` | `reduce` | `fold_left` (F) | `fold`/`reduce` | `reduce` (F) |
| 带中间结果的折叠 | `scan` | `scan` | — | `itertools.accumulate` | — | `scan` | `scan` (F) | — | `runningFold`/`scan` | — |
| 任一/全部 | `any`/`all` | `any`/`all` | `ContainsFunc` (F) | `any`/`all` | `some`/`every` | `any`/`all` | `any?`/`all?` | `exists`/`for_all` (F) | `any`/`all` | `contains(where:)` (F)/`allSatisfy` (F) |
| 按谓词计数 | `count` | `filter().count()` | — | — | — | `count` (F) | `count/2` (F) | — | `count{}` | `count(where:)` |
| 排序（自然顺序） | `sort` | `sort` | `Sort` | `sorted` (F) | `toSorted` (F) | `sort(by:)` | `sort` | `sort` (F) | `sorted` | `sorted()` (F) |
| 按键排序 | `sort_by` | `sort_by_key` (F) | — | `sorted(key=)` (F) | — | — | `sort_by` (F) | — | `sortedBy` | — |
| 按比较函数排序 | **没有** | `sort_by` (F) | `SortFunc` (F) | `cmp_to_key` | `toSorted(cmp)` (F) | `sort(by:)` | `sort/2` | `sort cmp` (F) | `sortedWith` | `sorted(by:)` (F) |
| 反转 | `reverse` | `reverse` | `Reverse` (F) | `reversed` | `toReversed` (F) | `reverse` | `reverse` | `rev` | `reversed` | `reversed()` (F) |
| 最小/最大 | `min`/`max` | `min`/`max` | `Min`/`Max` (F) | `min`/`max` (F) | — | `max(with:)` (F) | `min`/`max` | — | `minOrNull` | `min()`/`max()` |
| 按键最小/最大 | **没有** | `min_by_key` | `MinFunc`（F，比较） | `min(key=)` (F) | — | — (F) | `min_by`/`max_by` (F) | — | `minByOrNull` (F) | `min(by:)`（F，比较） |
| 总和/乘积（Int） | `sum`/`product` | `sum`/`product` | — | `sum` (F) | — | `int.sum` | `sum`/`product` (F) | — | `sum` | — |
| 总和（Float） | **没有**（仅`List[Int]`） | `sum` | — | `sum` (F) | — | `float.sum` | `sum` (F) | — | `sum` | — |
| 去重 | `unique`/`unique_by` | `dedup`（相邻） | `Compact`（相邻，F） | `dict.fromkeys` | `new Set` | `unique` (F) | `uniq`/`uniq_by` (F) | `sort_uniq` | `distinct`/`distinctBy` | — |
| 合并相邻重复项 | `dedup` | `dedup` | `Compact` (F) | — | — | — | `dedup` (F) | — | — | — |
| 按键分组 | `group_by` | — | — | `itertools.groupby` | `Object.groupBy` | — | `group_by` | — | `groupBy` | `Dictionary(grouping:)` |
| 按谓词分成2组 | `partition` | `partition` | — | — | — | `partition` | `split_with` | `partition` (F) | `partition` | — |
| 首/尾取出或丢弃 | `take`/`drop`/`take_end`/`drop_end`/`tail` | `take`/`skip` | — | `xs[:n]` | `slice` | `take`/`drop` | `take`/`drop` | `take`/`drop` (F) | `take`/`drop`/`takeLast`/`dropLast` | `prefix`/`dropFirst`/`suffix`/`dropLast` |
| 条件成立期间 | `take_while`/`drop_while` | `take_while`/`skip_while` | — | `itertools.takewhile` | — | `take_while` (F) | `take_while` (F) | `take_while` (F) | `takeWhile` | `prefix(while:)` (F) |
| 切片 | `slice` | `&v[a..b]`/`get` (F) | `xs[a:b]` | `xs[a:b]` | `slice` (F) | — | `slice` (F) | — | `subList`/`slice` | `xs[a..<b]` |
| 按位置分成2组 | `split_at` | `split_at` (F) | — | — | — | `split` | `split` | — | — | — |
| 每n个分组 | `chunk` | `chunks` (F) | `Chunk` (F) | `itertools.batched` | — | `sized_chunk` (F) | `chunk_every` (F) | — | `chunked` (F) | — |
| 长度n的窗口 | `window`与`windows`（相同） | `windows` (F) | — | — | — | `window` (F) | `chunk_every(n,1,:discard)` | — | `windowed` | — |
| 配对/用函数配对 | `zip`/`zip_with` | `zip` | — | `zip` | — | `zip`/`map2` | `zip`/`zip_with` (F) | `combine` (F) | `zip` | `zip` (F) |
| 解对 | **没有** | `unzip` | — | `zip(*xs)` | — | `unzip` (F) | `unzip` (F) | `split` (F) | `unzip` | — |
| 添加索引 | `enumerate` | `enumerate` | — | `enumerate` | `entries` | `index_map` | `with_index` | `mapi` | `withIndex` | `enumerated()` (F) |
| 范围 | `range` | `a..b` | — | `range` | — | `range` | `a..b` | `init` (F) | `until`/`..<` | `a..<b` |
| 带步长的范围 | **没有** | `step_by` | — | `range(a,b,s)` | — | — | `a..b//s` | — | `step` | `stride(from:to:by:)` |
| 用重复/迭代构造 | `repeat`/`iterate` | `repeat` | `Repeat` (F) | `[x]*n` | `Array(n).fill` | `repeat` | `List.duplicate` | `init` (F) | `List(n){}` | `Array(repeating:count:)` |
| 间隔插入/连接 | `intersperse`/`join` | `join` (F) | — | `sep.join` | `join` (F) | `intersperse` | `intersperse`/`join` | — | `joinToString` | `joined(separator:)` (F) |
| 二分查找 | `binary_search`（仅`List[Int]`） | `binary_search` (F) | `BinarySearch` (F) | `bisect` | — | — | — | — | `binarySearch` | — |
| 随机排列 | `shuffle`（纯`fn`）与`random.shuffle`（effect） | 外部`rand` | — | `random.shuffle` | — | `shuffle` | `shuffle` (F) | — | `shuffled` | `shuffled()` (F) |
| 从后查找 | — | `rposition`/`rfind` | — | — | `lastIndexOf`/`findLast` (F) | — | — | — | `lastIndexOf`/`findLast` | `lastIndex(of:)` (F)/`last(where:)` (F) |
| 按相邻条件分组 | — | `chunk_by` (F) | — | `itertools.groupby` | — | `chunk(by:)` (F) | `chunk_by` (F) | — | — | — |
| 是否已排序 | — | `is_sorted` (F) | `IsSorted` (F) | — | — | — | — | — | — | — |
| 用相同值填满（原地） | — | `fill` (F) | — | — | `fill` (F) | — | — | — | `fill` | — |

<a id="完全性の規則"></a>

### 完整性规则

1. **集合高阶函数族**（D3）：`map`、`filter`、`fold`、`any`、`all`、`count`、`find`共7个，list/map/set全部提供。list已有全部7个。
2. **排序及最小/最大函数族**（R-L0-10）：在`sort`、`min`、`max`全部3行提供“自然顺序”“键函数`_by`”2列。比较函数`_with`列仅有`sort_with`。不提供`min_with`/`max_with`是有意省略（可写`xs |> list.sort_with(cmp) |> list.first`，9种语言中仅Go、Swift、Gleam、Kotlin提供比较函数版最小/最大，且拼写不统一）。
3. **配对函数族**：有`zip`就提供逆向`unzip`（`zip` 6/9、`unzip` 5/9）。
4. **数值折叠族**：让`sum`、`product`、`min`、`max`同时支持`Int`与`Float`元素类型。`min`/`max`/`sort`已按类型分派，因此仅`sum`/`product`有缺口。
5. **按大小分组族**（`chunk`、`window`）：名称统一为单数动词，`n <= 0`按契约违反停止（R-L0-06第3项、R-L0-12）。
6. **原地改写**（R-L0-04）：list的`mut`函数仅`push`、`pop`、`clear`共3个。`set`、`insert`、`update`、`remove_at`、`swap`为复制。`with_capacity`是无主体构造器，不是`mut`。不再增加`mut`函数（`extend`、`fill`、`sort_in_place`）。
7. **有意省略**：拼接由`+`运算符承担，不设`concat`/`append`函数。降序可写`sort_with`或`sort |> reverse`，不设`sort_desc`。

<a id="l2-の判定"></a>

### L2判定

- **重复**：`window` ≡ `windows`（独立intrinsic，函数体一个字符也不差：`runtime/rs/src/list.rs:52`和`:257`）→R-list-03。`shuffle` ≡ `random.shuffle`（仅一侧是纯`fn`）→R-list-05。`get_or` ≡ `get ?? d`→R-list-01。`length` ≡ `len`→R-list-02。
- **不该出现在表中的项目**：`bundled_probe`→R-list-04（#3850）。
- **缺少的角色（5/9以上）**：比较函数排序（8/9）、按键最小/最大（5/9：Rust/Python/Elixir/Kotlin为键函数，Go/Swift为比较函数）→R-list-06。解对（5/9）→R-list-07。Float求和（5/9）→R-list-08。带步长范围（5/9）→R-list-09。
- **函数族缺口/边界不一致**：仅`slice`在负开始位置返回空；`chunk`的负n返回全体→R-list-10。
- **越界写入**：`set`、`update`、`swap`、`remove_at`越界时不做任何事，`insert`越界时追加到尾部。索引赋值`xs[i] = v`会abort→R-list-13。
- **旧提示表**：E002的“是否想写”表指向偏离现存函数的方向→R-list-11。
- **不添加**：从后查找（4/9）、`chunk_by`（4/9）、`is_sorted`（2/9）、`fill`（3/9）、`concat`函数→R-list-12（×）。
- **不暴露到公开面的重复**：#1460的`_str`双生函数（`list_contains_str`等120个）是`stdlib/list_*.almd`内部实现名称，公开函数表（interface JSON的67个函数）一个也没有。作为拼写问题不属L2范围。#1460的protocol化裁定继续由#1460处理。

---

<a id="案"></a>

## 提案

<a id="r-list-00-その場書き換えとコピーの様式を据え置きr-l0-05-の保険をかける---採用"></a>

### R-list-00 保持原地改写/复制模式，并加上R-L0-05保险——○采纳

- 内容：不变。`push`、`pop`、`clear`为`mut`（原地），其他函数均复制。`list.set`、`list.insert`、`list.update`、`list.remove_at`、`list.swap`中的动词（`set`、`insert`、`update`、`remove`）按R-L0-04属复制侧动词，与map/set模式相同。保留`with_capacity`。传入越界位置的行为由另案R-list-13统一为abort。
- 依据L0规则：R-L0-04、R-L0-05。
- (a) 与Gleam/Elixir/OCaml List相同，默认复制。仅保留Rust/Python/JS/Kotlin/Swift的`push`/`append`/`pop`式原地改写。
- (b) native与wasm均已有`mut`写回路径。
- (c) 危险在于沿用其他语言习惯，将`list.sort(xs)`或`list.set(xs, 0, 9)`写成语句。实测（`p1.almd`）：对`var xs = [3, 1, 2]`写语句`list.sort(xs)`、`list.reverse(xs)`、`list.set(xs, 0, 9)`，不会报错，仍显示`[3, 1, 2]`。R-L0-05加入后，3行均以E042停止。list特别危险的有`sort`、`sort_by`、`reverse`、`set`、`insert`、`update`、`remove_at`、`swap`（与Python/JS/Rust/Kotlin原地改写同名）。
- (d) 不变。
- 门禁提案：R-L0-G的`mut`动词检查固定list的`mut`函数仅`push`、`pop`、`clear`。在R-L0-05 fixture的broken侧加一行`list.sort(xs)`。
- issue：包含在R-L0-05新issue中。
- issue：#3928。

<a id="r-list-01-listget_or-を撤去し--に一本化する---採用"></a>

### R-list-01 移除`list.get_or`，统一为`??`——○采纳

- 内容：移除`list.get_or(xs, i, d)`，替换为`list.get(xs, i) ?? d`。
- 依据L0规则：R-L0-06、R-L0-14（D1）。
- (a) 仅Elixir `Enum.at(xs, i, d)`和Kotlin `getOrElse`以函数提供默认值版。
- (b) 无影响。ADR-0005将`??`脱糖为`option.unwrap_or_else`，两目标相同。实测（`p1.almd`）`list.get_or([1,2], 5, 0)`与`list.get([1,2], 5) ?? 0`均为`0`。
- (c) `??`是CHEATSHEET教的唯一写法。`get_or`必先求值默认值，去掉后也无需记忆其与`??`惰性求值的细微差异。
- (d) 移除。`@deprecated(since=15, use="??")`，`[[deprecation]]`中`removal_epoch = 16`。`almide fix`将`list.get_or(a, b, c)`→`list.get(a, b) ?? c`。`removed_spellings.rs`提示：“`list.get(xs, i) ?? d`”。内部双生`list_get_or_str.almd`一同删除。使用数99 / 9 / 21 / 7（research 9）。
- 门禁提案：R-L0-G增加“名称以`_or`结尾的公开函数为0”（与`option.unwrap_or`、`result.unwrap_or`、`map.get_or`、`bytes.get_or`、`args.option_or`在同一窗口归零）。
- issue：#3913。

<a id="r-list-02-listlength-を-epoch-16-で撤去する---採用"></a>

### R-list-02 在epoch 16移除`list.length`——○采纳

- 内容：为已`@deprecated`（dialect 5起）的`list.length`增加移除日期。
- 依据L0规则：R-L0-11、R-L0-14（D7）。
- (a) 长度拼写分散（length 4、len 3），依R-L0-00保留`len`。
- (b) 无影响。
- (c) 实测（`p3.almd`）：当前给E052警告及`almide fix`改写。移除后变为带`removed_spellings.rs`提示的错误。
- (d) 移除。现有`@deprecated`始于dialect 5，因此`[[deprecation]]`保留`since_epoch = 5`，补`use="list.len"`和`removal_epoch = 16`（不新开since=15窗口，已有窗口开放）。`almide fix`一对一改写`list.length(xs)`→`list.len(xs)`。移除后提示：“`list.len(xs)`”。使用数4 / 7 / 9 / 1。
- 门禁提案：现有同义词门禁的`len`/`length`/`size`词汇规则。
- issue：#3851 A（已有）。

<a id="r-list-03-listwindows-を撤去しlistwindow-を残す---採用"></a>

### R-list-03 移除`list.windows`，保留`list.window`——○采纳

- 内容：`list.window`与`list.windows`相同（`runtime/rs/src/list.rs:52`与`:257`函数体一致，实测`p1.almd`的`[[1, 2], [2, 3], [3, 4]]`一致）。绑定不同intrinsic（`almide_rt_list_windows`和`almide_rt_list_window`），现有同义词门禁无法检测。保留单数`window`，移除`windows`。
- 依据L0规则：R-L0-01、R-L0-00、R-L0-14。
- 选择单数的理由：
  - Almide内部一致性：同属“按大小n分组”族的`list.chunk`为单数动词。统一`window`后，list内部该族只有一种形式。
  - 贯彻一致性后的改动量：统一单数，移除`list.windows`（18 / 3 / 10 / 1 = 32），bytes侧`bytes.chunks`→`bytes.chunk`（6 / 0 / 5 / 1 = 12），共44处。统一复数，则`list.window`（6 / 4 / 7 / 1 = 18）及`list.chunk`→`list.chunks`（31 / 3 / 16 / 2 = 52），共70处。
  - (a) 其他语言拼写分散（Rust `windows` (F)、Gleam `window` (F)、Kotlin `windowed`）。没有多数，按消解Almide内部冲突（R-L0-00第1项）决定。
- (b) 无影响，函数体相同。
- (c) 两者并存时，LLM会因Rust习惯写`windows`，也会与`chunk`配对写`window`。只有一种时，从`chunk`类推必定命中。
- (d) 移除。`@deprecated(since=15, use="list.window")`、`removal_epoch = 16`，`almide fix`一对一改写。`windows`使用数18 / 3 / 10 / 1。
- 门禁提案：fixture确认同义词门禁规则6（同模块不同intrinsic具有相同函数体，#3851 F）能检测此对。
- issue：#3851（已有，在C节加入list这一对）。

<a id="r-list-04-listbundled_probe-を公開面から外す---採用"></a>

### R-list-04 将`list.bundled_probe`移出公开面——○采纳

- 内容：`list.bundled_probe(n) = n + 1`用于验证打包调用路径。移出公开面。路径回归测试交给同路径的`list.split_at`与`list.iterate`（`crates/almide-mir/src/pipeline.rs:79`并列3者）。若必须有专用函数，改名为检查器不公开的`__bundled_probe`。
- 依据L0规则：R-L0-01（表中不放无意义拼写）、R-L0-14（D10）。
- (a) 各语言标准库均不公开此类函数。
- (b) 无影响。实测（`p1.almd`）`list.bundled_probe(4)`为`5`。
- (c) 函数表是LLM判断“list有什么”的材料（#1483）。仅多一个无意义函数，就多一个似乎合理的拼写。
- (d) 移除。`@deprecated(since=15, use="n + 1")`、`removal_epoch = 16`。替代是表达式而非函数，`almide fix`不改写，仅提示“写`n + 1`”。移除后`removed_spellings.rs`给相同提示。使用数5 / 2 / 5 / 0（均自身测试和生成文档行）。同次改动重生成`docs/stdlib/list.md`、`since.toml`、semantics-manifest行、检验台账行、IDE outline snapshot。
- 门禁提案：无（移除即完成）。
- issue：#3850（已有）。

<a id="r-list-05-listshuffle-を撤去しrandomshuffle-に一本化する---採用"></a>

### R-list-05 移除`list.shuffle`，统一为`random.shuffle`——○采纳

- 内容：`list.shuffle`虽为纯`fn`，每次运行结果不同（实测`p4.almd`：纯`fn mix(xs) = list.shuffle(xs)`可编译，两次运行分别为`[5, 4, 8, 6, 1, 2, 3, 7]`和`[1, 5, 6, 8, 2, 4, 3, 7]`）。相同处理的`random.shuffle`为`effect fn`，从纯`fn`调用会以E006停止（`p5.almd`）。移除`list.shuffle`，保留`random.shuffle`。
- 依据L0规则：R-L0-01（每角色一种拼写）、R-L0-13（读取环境的函数为effect，ADR-0022）、R-L0-02（随机数归`random`，R-L1-04）。
- (a) Python为`random.shuffle`，Rust为`rand` crate，很多语言放在随机模块。Elixir/Kotlin/Swift放集合侧，但不以类型区分纯度。
- (b) wasm从`wasi:random`获取随机数（与`random.shuffle`同路径）。ADR-0027将`list.shuffle`作为“第141个依赖宿主函数”单独检验。移除可消去此例外。
- (c) 纯函数对相同输入返回不同值，会破坏测试/缓存/fusion前提（同表达式同值）。LLM发现`list.shuffle`会在无effect标记函数中使用。
- (d) 移除。`@deprecated(since=15, use="random.shuffle")`、`removal_epoch = 16`。`almide fix`改写拼写，若调用方为纯`fn`则出现E006并引导改为`effect fn`（提示中说明仅改写可能仍不通过）。使用数2 / 2 / 9 / 0（`random.shuffle`为12 / 0 / 7 / 1）。
- 门禁提案：R-L0-G增加“ADR-0027宿主依赖函数列表中的函数为`effect fn`”。
- issue：#3943。

<a id="r-list-06-比較関数の-sort_with-とキー関数の-min_bymax_by-を足す---採用"></a>

### R-list-06 添加比较函数`sort_with`及键函数`min_by`/`max_by`——○采纳

- 内容：增加以下3个函数。
  - `list.sort_with(xs: List[A], cmp: (A, A) -> Int) -> List[A]`——`cmp(a, b)`为负则a在前，0则保持原序（稳定），正则b在前。
  - `list.min_by(xs: List[A], f: (A) -> K) -> Option[A]`——键`f(x)`最小的元素；同键取首个；空为none。每元素仅调用一次`f`（同`sort_by`）。
  - `list.max_by(xs: List[A], f: (A) -> K) -> Option[A]`——同理，同键取首个。
- 依据L0规则：R-L0-10（`_by`为键函数，`_with`为比较函数）、R-L0-06（可失败提取无后缀返回Option）、D4/D5。
- (a) 8/9提供比较函数排序（Rust `sort_by` (F)、Go `SortFunc` (F)、JS `toSorted(cmp)` (F)、Gleam `sort(by:)`、Elixir `sort/2`、OCaml `sort cmp` (F)、Kotlin `sortedWith`、Swift `sorted(by:)` (F)）。以整数符号作比较返回的有Go (F)、JS、OCaml (F)、Kotlin、Python `cmp_to_key`共5种，也与Almide现有`bytes.cmp`（-1/0/1）相同。按键取最小/最大中，Rust `min_by_key`、Python `min(key=)` (F)、Elixir `min_by` (F)、Kotlin `minByOrNull` (F)共4种用键函数；加Go `MinFunc` (F)、Swift `min(by:)` (F)，该角色为6/9。Kotlin `minByOrNull` (F)在最小值相同时取首元素，与此约定相同。
- (b) 纯计算。排序用现有稳定归并排序，native与wasm顺序相同。
- (c) 降序或复合键排序时LLM会写比较函数。当前只能向`sort_by`传键元组或反转符号。`min_by`已出现在rainbow-bridge设计笔记（`docs/roadmap/on-hold/rainbow-bridge.md:178`）。
- (d) 添加。使用数均0（`min_by`在docs有1）。
- 传给比较器的`compare`函数（`int.compare`、`float.compare`、`string.compare`、`bytes.compare`等）按R-L0-16放各类型模块。
- 门禁提案：R-L0-G的`_by`/`_with`类型检查（`_by`最后参数是一元函数，`_with`为二元）。矩阵测试：`sort`/`min`/`max` × {自然顺序，`_by`}的6格均存在，且自然顺序下`sort_with(xs, cmp) |> list.first`与`min`一致。
- issue：#3924。

<a id="r-list-07-listunzip-を足す---採用"></a>

### R-list-07 添加`list.unzip`——○采纳

- 内容：添加`list.unzip(xs: List[(A, B)]) -> (List[A], List[B])`。
- 依据L0规则：L2完整性规则3（有`zip`即提供逆向）、R-L0-00（`unzip`拼写最多，4/9：Rust、Gleam (F)、Elixir (F)、Kotlin；OCaml为`split` (F)）。
- (a) 角色为5/9（以上4种加OCaml）。
- (b) 纯计算。
- (c) 当前只能用`list.fold`构造两个列表，或写两次`list.map`。`unzip`是LLM自然作为`zip`配对写出的名称。
- (d) 添加，使用数0。
- 门禁提案：`zip`与`unzip`往返性质测试（长度相同时恢复原值）。
- issue：#3925。

<a id="r-list-08-listsum-と-listproduct-を-listfloat-でも使えるようにする---採用"></a>

### R-list-08 让`list.sum`与`list.product`也支持`List[Float]`——○采纳

- 内容：`list.sum`、`list.product`按元素类型分派，如`list.sort`、`list.min`、`list.max`。`List[Int]`保持当前行为（回绕）；`List[Float]`从左到右做IEEE加/乘（空为`0.0`/`1.0`）。当前`list.sum([1.5, 2.5])`以E005“expects List[Int] but got List[Float]”停止（实测`p2.almd`）。
- 依据L0规则：L2完整性规则4、R-L0-02（主体为`List`，放`list`，不建`float.sum`）、R-L0-14第4项（不改变现有调用意义）。
- (a) 5/9有浮点求和（Rust `sum`、Python `sum` (F)、Gleam `float.sum`、Elixir `sum` (F)、Kotlin `sum`）。
- (b) 固定从左到右相加，native与wasm结果一位不差（ADR-0016确定性）。不做Kahan等补偿（不添加Python `math.fsum`对应物）。
- (c) LLM不看元素类型便写`xs |> list.sum`。当前仅Float需改为`list.fold(xs, 0.0, (a, x) => a + x)`。
- (d) 语义变化（仅扩展定义域，`List[Int]`调用结果一个不变，不是重赋名称意义）。使用数92 / 51 / 18 / 1（sum）、14 / 5 / 13 / 1（product）。
- 门禁提案：元素类型 × {sum, product, min, max}矩阵（Int/Float共8格）在native/wasm运行，增加一个`spec/wasm_cross/` fixture和契约（新增可观察行为，先把契约写入als）。
- issue：#3922。

<a id="r-list-09-刻み幅つきの-listrange_step-を足す---採用"></a>

### R-list-09 添加带步长的`list.range_step`——○采纳

- 内容：添加`list.range_step(start: Int, end: Int, step: Int) -> List[Int]`。`step > 0`从`start`数到小于`end`；`step < 0`从`start`向下数，保持大于`end`。`step == 0`按契约违反停止（R-L0-06第3项）。与`list.range`同为半开区间（R-L0-12）。
- 依据L0规则：R-L0-02（无主体构造器归返回类型模块）、R-L0-10（`_by`仅键函数，不用`range_by`）、R-L0-12。
- (a) 5/9有带步长范围（Rust `step_by`、Python `range(a, b, s)`、Elixir `a..b//s`、Kotlin `step`、Swift `stride(from:to:by:)`）。拼写分散，因此在Almide现有`range`后加参数含义。
- (b) 纯计算。
- (c) 当前写`list.range(0, n) |> list.map((i) => i * 2)`或`list.filter`，容易算错边界。LLM受Python `range(a, b, s)`习惯影响，容易写3参数`list.range`，因此E002“是否想写”引导到`list.range_step`（R-list-11）。
- (d) 添加，使用数0。
- 门禁提案：`range_step(a, b, 1) == range(a, b)`及负步长边界（`range_step(5, 0, -2) == [5, 3, 1]`）fixture。
- issue：#3923。

<a id="r-list-10-境界の扱いをそろえる-slice-の負の開始位置chunkwindow-の-n--0---採用"></a>

### R-list-10 统一边界处理：`slice`负开始位置，`chunk`/`window`的`n <= 0`——○采纳

- 第六轮注：将`slice`负开始位置截到0的部分已由R-L0-29改为abort。`chunk`/`window`的`n <= 0`处理维持本案。
- 内容：
  1. 将`list.slice(xs, start, end)`负`start`截到0。实测（`p6.almd`）同样`(-1, 2)`，`string.slice("abc", -1, 2)`返回`"ab"`，`list.slice([1, 2, 3], -1, 2)`为`[]`，`bytes.slice(..., -1, 2)`长度0。符合R-L0-12（越界截到端点，负索引越界故截到0）和文档“clamped”的是string。list/bytes向其对齐，使3种类型的`slice(-1, 2)`都返回前2个元素（bytes见L2-bytes `slice`行，string不变）。
  2. `list.chunk(xs, n)`和`list.window(xs, n)`在`n <= 0`时按契约违反停止。当前仅`n = 0`停止，`chunk(xs, -1)`为`[[1, 2, 3]]`，`window(xs, -1)`为`[]`（实测`p6.almd`）。
- 依据L0规则：R-L0-12、R-L0-06第3项。
- (a) 半开区间和截边同Python/JS/Gleam/Elixir。大小非正时停止同Rust `chunks`/`windows`（panic）和Kotlin `chunked`（异常）。
- (b) 两目标统一行为，不产生wasm/native差异（当前两者同样存在不一致）。
- (c) 同参数按类型产生不同结果，会使从string学到的边界行为在list失效。
- (d) 语义变化（仅边界输入结果，是使实现符合文档的修正）。`chunk`/`window`负n为缩小定义域，不提升dialect epoch（仅缩小定义域，拼写/接受性不变）。在spec/tests计数传负参数调用，将影响数写入PR。
- 门禁提案：将本页2处不一致放入R-L0-12矩阵的首批单元格（list/string/bytes × `slice`/`take`/`drop`/`take_end`/`drop_end` × 边界{-1, 0, len, len+1}）。负个数（`take(xs, -1)`为全部、`drop(xs, -1)`为空）在list/string已一致，将当前答案固定为单元格（L2-string完整性规则1）。
- issue：#3944。

<a id="r-list-11-e002-のもしかして表を生きている関数に向ける---採用"></a>

### R-list-11 将E002“是否想写”表指向现存函数——○采纳

- 内容：修正`crates/almide-frontend/src/stdlib.rs:156-230`的`ALIASES`（调用未定义名称时的建议表）。现有list行有些指向偏离现存函数的方向。
  - `("list", "push", "list.concat ...")`与`("list", "append", "list.concat ...")`：`list.concat`不存在，`push`现存。→删除`push`行，`append`改为“`xs + ys`（原地则`list.push`）”。
  - `("list", "count", "list.len")`：`list.count`现存且意义不同（按谓词计数）。→删除此行。
  - `("list", "find_index", "list.index_of")`、`("list", "reduce", "list.fold")`、`("list", "tail", "list.drop(xs, 1)")`：均现存函数。→删除这些行。
  - 新增行（他语言拼写→Almide拼写）：`sorted`/`toSorted`→`list.sort`、`sorted_by`/`sortWith`→`list.sort_with`、`distinct`/`uniq`→`list.unique`、`windows`/`windowed`→`list.window`（移除后）、`chunks`/`chunked`→`list.chunk`、`concat`/`extend`→`xs + ys`、`get_or`/`getOrElse`→`list.get(xs, i) ?? d`（移除后）、`min_by_key`/`minBy`→`list.min_by`、3参数`range`→`list.range_step`。与string页面在此表增加`split_whitespace`等提示机制相同。
- 依据L0规则：R-L0-01（建议不得教第二种拼写）、R-L0-14第3项（给已移除拼写提示）。
- (a) Rust/Kotlin编译器也提供相似名称候选。
- (b) 仅改检查器。
- (c) LLM写他语言拼写后，能否从首个错误到达正确拼写决定修改存活率。指向不存在的`list.concat`会让第二次仍报错。
- (d) 仅诊断变化，无兼容问题。
- 门禁提案：测试“`ALIASES`全部键为未定义名称，目标函数均现存”（新增`tests/stdlib_alias_table_test.rs`，对照interface JSON）。已移除拼写转入`removed_spellings.rs`。
- issue：#3905。

<a id="r-list-13-範囲外の位置への書き込みを-abort-にする---採用"></a>

### R-list-13 越界位置写入改为abort——○采纳

- 内容：指定位置写入的复制函数，将越界视为调用方契约违反而停止（abort）。
  - `list.set(xs, i, v)`、`list.update(xs, i, f)`、`list.remove_at(xs, i)`：当前`i`越界无操作，返回原列表（`stdlib/list.almd`说明“no-op if i out of range”）。`0 <= i < len`之外改为abort。
  - `list.swap(xs, i, j)`：当前`i`或`j`越界无操作，改为任一越界即abort。
  - `list.insert(xs, i, v)`：接受`0..=len`位置（`i == len`为尾部追加）。当前负`i`及大于`len`的`i`也追加到尾部。改为范围外abort。
- 依据L0规则：L0“指定位置写入”（R-L0-12后、R-L0-13前）、R-L0-06第3项（abort仅契约违反）。
- (a) Rust/Go/Kotlin/Swift用panic/trap停止越界写入，Python/OCaml用异常。仅Elixir `List.replace_at`默默无操作。
- (b) 两目标使用同一abort路径（索引赋值`xs[i] = v`已使用），不产生差异。
- (c) 当前`list.set(xs, 5, v)`无操作，而`xs[5] = v`会abort。相同写入有两种行为，无操作写入掩盖错误。
- (d) 语义变化（缩小定义域）。正确位置结果不变，因此不计入dialect epoch的`breaks`（现有处理：缩小定义域不触发epoch）。名称/类型不变，无需`@deprecated`或`almide fix`。依赖越界无操作的调用写为`if i < list.len(xs) then list.set(xs, i, v) else xs`。计数spec/tests越界位置调用，将影响数写PR。
- 门禁提案：list/bytes位置写入 × 边界{-1, 0, len-1, len, len+1}矩阵在native/wasm运行，确认abort位置相同（与bytes `put_<型>`同表）。可观察行为改变，先将契约台账写入als。
- issue：#3937。

<a id="r-list-14-listtail-を撤去しlistdropxs-1-に一本化する---採用"></a>

### R-list-14 移除`list.tail`，统一为`list.drop(xs, 1)`——○采纳

- 当前：`list.tail(xs: List[A]) -> List[A]`返回去首元素的列表，与`list.drop(xs, 1)`相同（空仍为空）。
- 依据L0规则：R-L0-01（仅调用一次其他公开函数的函数，也算同角色第二种拼写）。L3表无理由保留，按规则修正。
- (a) 有`tail`的是OCaml（`List.tl`，空则异常）、Elixir（`tl`，空则异常）、Haskell（`tail`，空则异常）。Kotlin/Swift/Rust/Python/JS写`drop(1)`、`dropFirst()`、`xs[1:]`、`slice(1)`。Gleam为`list.rest`（Result）。拼写/空处理都不一致。
- (c) 英语`tail`也可理解为“末尾元素”，易混淆`list.last`。`drop(xs, 1)`名称可读出含义。
- (d) 移除。按R-L0-14，epoch 15加`@deprecated(since=15, use="list.drop(xs, 1)")`，epoch 16移除。`almide fix`将`list.tail(xs)`改为`list.drop(xs, 1)`。E002表（R-list-11）加`tail`→`list.drop(xs, 1)`。
- 完整性规则：R-L0-01转发检测（#3851门禁）检出`list.tail`，不留在允许列表。
- issue：#3969。

<a id="r-list-12-多数派に届かない役割は足さない---不採用"></a>

### R-list-12 不增加未达多数的角色——×不采纳

- 内容：不添加以下项目。
  - 从后查找（`last_index_of`、`find_last`）：4/9（Rust、JS (F)、Kotlin、Swift (F)），可用`list.reverse`与`find_index`。
  - 按相邻条件分组（`chunk_by`）：4/9（Rust (F)、Python、Gleam (F)、Elixir (F)）。
  - 是否已排序（`is_sorted`）：2/9（Rust (F)、Go (F)）。
  - 相同值填满（`fill`，原地）：3/9（Rust (F)、JS (F)、Kotlin），可写`list.repeat(v, list.len(xs))`。
  - 拼接函数（`concat`、`append`）及原地追加多个元素`extend`：角色9/9，但Almide由`+`运算符（`xs = xs + ys`）承担。添加函数会使同处理有两种写法（R-L0-01）。
  - 降序排序（`sort_desc`）、`zip3`、`frequencies`：均不足4/9。
- 依据L0规则：与L1“添加类别标准”同样5/9门槛、R-L0-01。
- (c) 不添加，而在R-list-11为他语言拼写给“是否想写”提示。
- issue：无。

---

<a id="r-list-15-listbinary_search-を順序のある要素型すべてで使えるようにする---採用"></a>

### R-list-15 让`list.binary_search`适用于所有有序元素类型——○采纳

- 当前：`fn list.binary_search(xs: List[Int], target: Int) -> Option[Int]`。`list.sort`、`min`、`max`支持所有有序类型，仅查找侧专用`Int`。
- 提案：`fn list.binary_search[A](xs: List[A], target: A) -> Option[Int]`。顺序同`list.sort`（Float为IEEE totalOrder），`List[Int]`结果不变。
- 依据L0规则：R-L0-16、L2完整性规则4（与R-list-08相同扩展定义域）。
- (a) Rust `binary_search`（`T: Ord`）、Go `slices.BinarySearch`（`cmp.Ordered`）、Python `bisect`、Kotlin `binarySearch`（`Comparable`）均不限于单一元素类型。
- (b) 纯计算，顺序复用`list.sort`实现。
- (c) 实测：`list.binary_search(["a","b"], "b")`报E005，提示“用`int.parse(s)`转Int”，导向无意义修复。
- (d) 语义变化（仅扩定义域，现有调用不变，不设epoch）。
- 完整性规则：list使用顺序的函数（`sort`、`min`、`max`、`binary_search`）接受相同元素类型。
- 门禁提案：元素类型（Int/Float/String/元组）× {`sort`, `min`, `max`, `binary_search`}矩阵在两目标运行。新增可观察行为，先写als契约。
- issue：#3969。

<a id="r-list-16-listjoin-を-listbytes-にも使えるようにするbytesjoin-は置かない---採用"></a>

### R-list-16 让`list.join`支持`List[Bytes]`（不设`bytes.join`）——○采纳

- 当前：`fn list.join(xs: List[String], sep: String) -> String`。无将`List[Bytes]`带分隔符合一的函数（`bytes.join`报E002）。
- 提案：`list.join`接受具有拼接`+`的序列类型（`String`、`Bytes`）。`list.join(parts: List[Bytes], sep: Bytes) -> Bytes`。与R-list-08将`list.sum`在`Int`/`Float`分派的机制相同。
- 依据L0规则：R-string-01（join放list）、R-L0-01（加`bytes.join`会使同角色位置按类型分裂）、R-bytes-16（`Bytes`也以`+`拼接）。
- (a) 至少5种语言有带分隔符拼接：Go `bytes.Join`、Python `bytes.join`、OCaml `Bytes.concat`、Rust `<[V]>::join`、Swift `joined(separator:)`。
- (b) 纯计算。
- (c) 记住`list.join`，字符串/字节均可同形写出。
- (d) 语义变化（仅扩定义域），无兼容影响。
- 完整性规则：所有可`+`拼接的序列类型（String/Bytes）均可`list.join`（List间拼接用`list.flatten`）。
- issue：#3969。

<a id="r-list-17-binary_search_bybinary_search_with-を足す---採用"></a>

### R-list-17 添加`binary_search_by`/`binary_search_with`——○采纳

- 当前：排序有`sort`、`sort_by`、`sort_with`3种，查找仅`binary_search`。没有按同一键查找经`sort_by(xs, key)`排序列表的函数。
- 提案：`fn list.binary_search_by[A, K](xs: List[A], target: K, key: (A) -> K) -> Int?`、`fn list.binary_search_with[A](xs: List[A], target: A, cmp: (A, A) -> Int) -> Int?`。排序/查找形成同样`_by`/`_with`对。
- 依据L0规则：R-L0-12（函数族按矩阵对齐）、L2完整性规则4（顺序函数有相同元素类型）。
- (a) Rust `binary_search_by`/`binary_search_by_key`、Go `slices.BinarySearchFunc`、Python `bisect(key=)`、Kotlin `binarySearch(comparator)`/`binarySearchBy`。
- (b) 纯计算。
- (c) 用`sort_by`排则用`binary_search_by`找，名称成对即可命中。
- (d) 添加2个。
- 完整性规则：`sort`的3形态（无后缀、`_by`、`_with`）在`binary_search`也齐备。
- issue：#3980。

<a id="r-list-18-listrepeat-を列全体の繰り返しにする---不採用"></a>

### R-list-18 让`list.repeat`重复整个序列——×不采纳

- 理由：`string.repeat`、`bytes.repeat`重复整个序列，`list.repeat(v, n)`则排列n个单值。统一会使`list.repeat([1, 2], 3)`由`[[1, 2], [1, 2], [1, 2]]`无类型错误地变为`[1, 2, 1, 2, 1, 2]`（R-L0-14第4项：同名不得默默改变结果）。重复元素同Gleam `repeat`、Elixir `List.duplicate`、Kotlin `List(n) { v }`、Swift `Array(repeating:count:)`，也未偏离多数。L3表“Go slices.Repeat”重复整个序列，故从比较栏去除。
- issue：无。

<a id="r-list-19-min_withmax_with-を足す---採用"></a>

### R-list-19 添加`min_with`/`max_with`——○采纳

- 当前：排序/查找有无后缀、`_by`、`_with`3种（R-list-17），但`min`/`max`仅无后缀与`_by`。
- 提案：`fn list.min_with[A](xs: List[A], cmp: (A, A) -> Int) -> A?`、`fn list.max_with[A](xs: List[A], cmp: (A, A) -> Int) -> A?`。
- 依据L0规则：R-L0-10（`_with`为比较函数）、R-L0-12（矩阵对齐函数族）。
- (a) Rust `min_by`/`max_by`（比较函数）、Kotlin `minWithOrNull`/`maxWithOrNull`、Swift `min(by:)`。
- (d) 添加2个。
- 完整性规则（扩展R-list-17）：顺序函数{`sort`, `min`, `max`, `binary_search`}均有无后缀、`_by`、`_with`3形态。
- issue：#3985。

<a id="r-list-20-listnewlistfrom_listlistto_list-を置く---採用"></a>

### R-list-20 设置`list.new`、`list.from_list`、`list.to_list`——○采纳

- 内容：`fn list.new() -> List[A]`（`[]`）、`fn list.from_list(xs: List[A]) -> List[A]`/`fn list.to_list(xs: List[A]) -> List[A]`（原样返回）。
- 理由：[protocol层级](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md)中`Container`的要求。在泛型函数写`T.new()`/`T.from_list(xs)`时，`List`需与其他容器同样工作。Swift `Array`也有`init()`及`init(_ s: some Sequence)`。
- (c) 直接使用仍写`[]`，引导（R-L0-19）指向字面量。
- (d) 添加3个。
- issue：#4000。

<a id="l3-関数表"></a>

## L3：函数表

判定图例：保留/改名/语义变化/移除/添加。其他语言栏仅列主要拼写。

| 函数 | 签名（当前，标示mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `len` | `list.len(xs: List[A]) -> Int` | Rust len / Gleam length / Kotlin size / Swift count | 保留 | R-L0-11 |
| `length` | `list.length(xs: List[A]) -> Int`（已弃用） | Gleam length / JS length | 移除（→ `list.len`） | R-list-02 |
| `get` | `list.get(xs: List[A], i: Int) -> Option[A]` | Rust get / Kotlin getOrNull / OCaml nth_opt | 保留 | — |
| `get_or` | `list.get_or(xs: List[A], i: Int, default: A) -> A` | Kotlin getOrElse / Elixir at(xs,i,d) | 移除（→ `list.get(xs, i) ?? d`） | R-list-01 |
| `set` | `list.set(xs: List[A], i: Int, val: A) -> List[A]` | JS with / Elixir List.replace_at | 语义变化（越界abort） | R-list-00、R-list-13 |
| `swap` | `list.swap(xs: List[A], i: Int, j: Int) -> List[A]` | Rust swap（原地）/ Swift swapAt | 语义变化（越界abort） | R-list-00、R-list-13 |
| `sort` | `list.sort(xs: List[A]) -> List[A]` | Kotlin sorted / Swift sorted() / Python sorted | 保留 | R-list-00 |
| `reverse` | `list.reverse(xs: List[A]) -> List[A]` | Gleam reverse / Kotlin reversed / JS toReversed | 保留 | R-list-00 |
| `contains` | `list.contains(xs: List[A], x: A) -> Bool` | Rust contains / JS includes / Gleam contains | 保留 | — |
| `enumerate` | `list.enumerate(xs: List[A]) -> List[(Int, A)]` | Rust enumerate / Python enumerate / Swift enumerated | 保留 | — |
| `zip` | `list.zip(xs: List[A], ys: List[B]) -> List[(A, B)]` | Rust zip / Gleam zip / OCaml combine | 保留 | — |
| `flatten` | `list.flatten(xss: List[List[T]]) -> List[T]` | Rust flatten / Gleam flatten / JS flat | 保留 | — |
| `take` | `list.take(xs: List[A], n: Int) -> List[A]` | Gleam take / Kotlin take / Swift prefix | 语义变化（负个数abort；R-L0-29） | R-L0-12、R-L0-29 |
| `drop` | `list.drop(xs: List[A], n: Int) -> List[A]` | Gleam drop / Rust skip / Swift dropFirst | 语义变化（负个数abort；R-L0-29） | R-L0-12、R-L0-29 |
| `tail` | `list.tail(xs: List[A]) -> List[A]` | OCaml tl / Elixir tl / Kotlin drop(1) | 移除（→ `list.drop(xs, 1)`） | R-list-14 |
| `unique` | `list.unique(xs: List[A]) -> List[A]` | Gleam unique / Elixir uniq / Kotlin distinct | 保留 | — |
| `index_of` | `list.index_of(xs: List[A], x: A) -> Option[Int]` | JS indexOf / Go Index / Swift firstIndex(of:) | 保留 | — |
| `last` | `list.last(xs: List[A]) -> Option[A]` | Rust last / Gleam last / Swift last | 保留 | — |
| `chunk` | `list.chunk(xs: List[A], n: Int) -> List[List[A]]` | Rust chunks / Kotlin chunked / Gleam sized_chunk | 语义变化（`n <= 0`则abort） | R-list-10 |
| `sum` | `list.sum[A: Numeric](xs: List[A]) -> A` | Rust sum / Python sum / Elixir sum | 语义变化（将`Int`/`Float`的2种形态合为接受全部数值类型的1个泛型函数） | R-L0-42 |
| `product` | `list.product[A: Numeric](xs: List[A]) -> A` | Rust product / Elixir product | 语义变化（将`Int`/`Float`的2种形态合为接受全部数值类型的1个泛型函数） | R-L0-42 |
| `first` | `list.first(xs: List[A]) -> Option[A]` | Rust first / Gleam first / Kotlin firstOrNull | 保留 | — |
| `is_empty` | `list.is_empty(xs: List[A]) -> Bool` | Rust is_empty / Gleam is_empty / Swift isEmpty | 保留 | — |
| `min` | `list.min(xs: List[A]) -> Option[A]` | Rust min / Go Min / Kotlin minOrNull | 保留 | — |
| `max` | `list.max(xs: List[A]) -> Option[A]` | Rust max / Go Max / Kotlin maxOrNull | 保留 | — |
| `join` | `list.join[T: Text](xs: List[T], sep: T) -> T` | Rust join / JS join / Kotlin joinToString / Go strings.Join | 语义变化（将`String`/`Bytes`的2种形态合为1个`Text`泛型函数） | R-L0-42 |
| `map` | `list.map(xs: List[A], f: (A) -> B) -> List[B]` | 所有语言map | 保留 | — |
| `filter` | `list.filter(xs: List[A], f: (A) -> Bool) -> List[A]` | 所有语言filter | 保留 | — |
| `find` | `list.find(xs: List[A], f: (A) -> Bool) -> Option[A]` | Rust find / Kotlin find / Swift first(where:) | 保留 | — |
| `any` | `list.any(xs: List[A], f: (A) -> Bool) -> Bool` | Rust any / JS some / OCaml exists | 保留 | — |
| `all` | `list.all(xs: List[A], f: (A) -> Bool) -> Bool` | Rust all / JS every / Swift allSatisfy | 保留 | — |
| `count` | `list.count(xs: List[A], f: (A) -> Bool) -> Int` | Kotlin count{} / Gleam count / Elixir count/2 | 保留 | — |
| `flat_map` | `list.flat_map(xs: List[A], f: (A) -> List[B]) -> List[B]` | Rust flat_map / JS flatMap / OCaml concat_map | 保留 | — |
| `filter_map` | `list.filter_map(xs: List[A], f: (A) -> Option[B]) -> List[B]` | Rust filter_map / Kotlin mapNotNull / Swift compactMap | 保留 | — |
| `fold` | `list.fold(xs: List[A], init: B, f: (B, A) -> B) -> B` | Rust fold / Gleam fold / OCaml fold_left | 保留 | — |
| `sort_by` | `list.sort_by(xs: List[A], f: (A) -> B) -> List[A]` | Rust sort_by_key / Kotlin sortedBy / Elixir sort_by | 保留 | R-list-06 |
| `take_while` | `list.take_while(xs: List[A], f: (A) -> Bool) -> List[A]` | Rust take_while / Kotlin takeWhile / Swift prefix(while:) | 保留 | — |
| `drop_while` | `list.drop_while(xs: List[A], f: (A) -> Bool) -> List[A]` | Rust skip_while / Kotlin dropWhile / Swift drop(while:) | 保留 | — |
| `partition` | `list.partition(xs: List[A], f: (A) -> Bool) -> (List[A], List[A])` | Rust partition / Gleam partition / Elixir split_with | 保留 | — |
| `reduce` | `list.reduce(xs: List[A], f: (A, A) -> A) -> Option[A]` | Rust reduce / Gleam reduce / Kotlin reduceOrNull | 保留 | — |
| `group_by` | `list.group_by(xs: List[A], f: (A) -> B) -> Map[B, List[A]]` | Kotlin groupBy / Elixir group_by / JS Object.groupBy | 保留 | — |
| `find_index` | `list.find_index(xs: List[A], f: (A) -> Bool) -> Option[Int]` | JS findIndex / Go IndexFunc / OCaml find_index | 保留 | — |
| `update` | `list.update(xs: List[A], i: Int, f: (A) -> A) -> List[A]` | Elixir List.update_at | 语义变化（越界abort） | R-list-00、R-list-13 |
| `scan` | `list.scan(xs: List[A], init: B, f: (B, A) -> B) -> List[B]` | Rust scan / Kotlin runningFold / Elixir scan | 保留 | — |
| `zip_with` | `list.zip_with(xs: List[A], ys: List[B], f: (A, B) -> C) -> List[C]` | Elixir zip_with / Gleam map2 / Kotlin zip(other){} | 保留 | — |
| `unique_by` | `list.unique_by(xs: List[A], f: (A) -> K) -> List[A]` | Elixir uniq_by / Kotlin distinctBy | 保留 | — |
| `range` | `list.range(start: Int, end: Int) -> List[Int]` | Python range / Gleam range / Rust a..b | 保留 | — |
| `slice` | `list.slice(xs: List[A], start: Int, end: Int) -> List[A]` | JS slice / Python xs[a:b] / Elixir slice | 语义变化（负位置abort；R-L0-29） | R-list-10、R-L0-29 |
| `insert` | `list.insert(xs: List[A], i: Int, val: A) -> List[A]` | Go slices.Insert / Elixir List.insert_at / JS toSpliced | 语义变化（`0..=len`外abort） | R-list-00、R-list-13 |
| `remove_at` | `list.remove_at(xs: List[A], i: Int) -> List[A]` | Elixir List.delete_at / Kotlin removeAt / Swift remove(at:) | 语义变化（越界abort） | R-list-00、R-list-13 |
| `repeat` | `list.repeat(val: A, n: Int) -> List[A]` | Gleam repeat / Elixir List.duplicate / Kotlin `List(n) { v }` | 保留 | — |
| `intersperse` | `list.intersperse(xs: List[A], sep: A) -> List[A]` | Gleam intersperse / Elixir intersperse | 保留 | — |
| `windows` | `list.windows(xs: List[A], n: Int) -> List[List[A]]` | Rust windows | 移除（→ `list.window`） | R-list-03 |
| `dedup` | `list.dedup(xs: List[A]) -> List[A]` | Rust dedup / Elixir dedup / Go Compact | 保留 | — |
| `take_end` | `list.take_end(xs: List[A], n: Int) -> List[A]` | Kotlin takeLast / Swift suffix | 语义变化（负个数abort；R-L0-29） | R-L0-12、R-L0-29 |
| `drop_end` | `list.drop_end(xs: List[A], n: Int) -> List[A]` | Kotlin dropLast / Swift dropLast | 语义变化（负个数abort；R-L0-29） | R-L0-12、R-L0-29 |
| `shuffle` | `list.shuffle(xs: List[A]) -> List[A]` | Python random.shuffle / Kotlin shuffled / Elixir shuffle | 移除（→ `random.shuffle`） | R-list-05 |
| `window` | `list.window(xs: List[A], n: Int) -> List[List[A]]` | Gleam window / Rust windows / Kotlin windowed | 语义变化（`n <= 0`则abort；`windows`的保留侧） | R-list-03、R-list-10 |
| `binary_search` | `list.binary_search(xs: List[A], target: A) -> Option[Int]` | Rust binary_search / Go BinarySearch / Kotlin binarySearch | 语义变化（元素类型扩至全部有序类型） | R-list-15 |
| `push` | `list.push(mut xs: List[A], x: A) -> Unit` | Rust push / JS push / Python append | 保留 | R-list-00 |
| `with_capacity` | `list.with_capacity(cap: Int) -> List[A]` | Rust Vec::with_capacity / Go make(.., 0, cap) | 保留 | R-list-00 |
| `pop` | `list.pop(mut xs: List[A]) -> Option[A]` | Rust pop / Swift popLast / Kotlin removeLastOrNull | 保留 | R-list-00 |
| `clear` | `list.clear(mut xs: List[A]) -> Unit` | Rust clear / Python clear / Kotlin clear | 保留 | R-list-00 |
| `bundled_probe` | `list.bundled_probe(n: Int) -> Int` | —（所有语言均无） | 移除（→ `n + 1`） | R-list-04 |
| `split_at` | `list.split_at(xs: List[T], n: Int) -> (List[T], List[T])` | Rust split_at / Gleam split / Elixir split | 保留 | — |
| `iterate` | `list.iterate(seed: T, f: (T) -> T, n: Int) -> List[T]` | Kotlin generateSequence / Elixir Stream.iterate / Rust successors | 保留 | — |
| `find_map` | `list.find_map(xs: List[A], f: (A) -> Option[B]) -> Option[B]` | Rust find_map / Gleam find_map / Elixir find_value | 保留 | — |
| `sort_with` | `list.sort_with(xs: List[A], cmp: (A, A) -> Int) -> List[A]`（提案） | Kotlin sortedWith / Go SortFunc / Swift sorted(by:) | 添加 | R-list-06 |
| `min_by` | `list.min_by(xs: List[A], f: (A) -> K) -> Option[A]`（提案） | Rust min_by_key / Kotlin minByOrNull / Elixir min_by | 添加 | R-list-06 |
| `max_by` | `list.max_by(xs: List[A], f: (A) -> K) -> Option[A]`（提案） | Rust max_by_key / Kotlin maxByOrNull / Elixir max_by | 添加 | R-list-06 |
| `unzip` | `list.unzip(xs: List[(A, B)]) -> (List[A], List[B])`（提案） | Rust unzip / Gleam unzip / OCaml split | 添加 | R-list-07 |
| `range_step` | `list.range_step(start: Int, end: Int, step: Int) -> List[Int]`（提案） | Python range(a,b,s) / Rust step_by / Kotlin step | 添加 | R-list-09 |
| `binary_search_by` | （新增）`list.binary_search_by(xs: List[A], target: K, key: (A) -> K) -> Option[Int]` | Rust binary_search_by_key / Kotlin binarySearchBy | 添加 | R-list-17 |
| `binary_search_with` | （新增）`list.binary_search_with(xs: List[A], target: A, cmp: (A, A) -> Int) -> Option[Int]` | Rust binary_search_by / Go BinarySearchFunc | 添加 | R-list-17 |
| `min_with` | （新增）`list.min_with(xs: List[A], cmp: (A, A) -> Int) -> Option[A]` | Kotlin minWithOrNull / Swift min(by:) | 添加 | R-list-19 |
| `max_with` | （新增）`list.max_with(xs: List[A], cmp: (A, A) -> Int) -> Option[A]` | Kotlin maxWithOrNull / Swift max(by:) | 添加 | R-list-19 |
| `new` | （新增）`list.new() -> List[A]` | Swift `Array()` / Rust `Vec::new` | 添加 | R-list-20 |
| `from_list` | （新增）`list.from_list(xs: List[A]) -> List[A]` | Swift `Array(seq)` | 添加 | R-list-20 |
| `to_list` | （新增）`list.to_list(xs: List[A]) -> List[A]` | Swift `Array(seq)` / Kotlin `toList` | 添加 | R-list-20 |

行数：当前67个函数+新增5个函数=72行。

---

<a id="l0-への差し戻し処理済み"></a>

## 退回L0的事项（已处理）

1. **比较函数拼写与类型。** L0已加入R-L0-16（有序类型设置`compare(a, b) -> Int`）。`list.sort_with`接受R-L0-16形态函数（R-list-06）。`bytes.cmp`改名为`bytes.compare`（L2-bytes R-bytes-08）。`int.compare`、`float.compare`、`string.compare`各页添加。
2. **“按大小n分组”族拼写。** R-L0-01加入“每n个分组的函数统一为单数动词`chunk`”。保留`list.window`（R-list-03），`bytes.chunks`改名`bytes.chunk`（L2-bytes R-bytes-08）。
3. **`string.join`与`list.join`重复。** 按R-L0-02（放主体类型模块）保留`list.join`，移除`string.join`（L2-string R-string-01）。
