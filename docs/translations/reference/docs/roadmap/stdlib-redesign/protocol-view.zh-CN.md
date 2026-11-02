> 说明（固定历史）：标准库重设计——protocol 金字塔：以 Swift 为参考的标准库 protocol 分层、各类型的符合关系、暴露的缺口及所需决定（R-L0-37）。
<a id="標準ライブラリの-protocol-の階層"></a>

# 标准库 protocol 层级

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](protocol-view.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](protocol-view.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](protocol-view.zh-CN.md)

> 非规范性历史设计参照版。固定原本权威且不变。不自动同步、重新机械统计、执行Dojo或独立验证实现/性能。数量、零缺口、符合表和@reuse同速主张属于原预期设计，不证明全部protocol/默认实现/条件符合当前存在。保留已落地builtin与其余设计清单及tuple/UFCS限制的区别。语言功能例中从compare生成数值默认实现与正文数值要求分配并存，类型表/数量差不擅自协调。源图与API清单忠实保留。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md)

> 最后更新：2026-10-22
> 原记录：[R-L0-37](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)・[target-surface.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/target-surface.md)

按照 R-L0-37，标准具体类型用“所符合 protocol 的组合”及“该类型专属函数”表达。层级以 Swift 标准库为参考，Swift 没有的层会说明理由再添加。符合关系与缺口从 [target-surface.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/target-surface.md)（重设计后函数）机械生成。

该层级是**预期设计**（R-L0-37“定位”）。目前实现须遵守清单中的名称、形状，内部可使用每种类型的具体函数。随着语言引入 protocol 功能（继承、关联类型、默认实现、条件符合，#4000），内部逐步靠近该层级。无接收者的要求调用（`M.empty()`）已落地。`Numeric`、`Integer`、`Text` 已作为内置 protocol 实现，由 `from`、`list.sum`、`list.join` 及 `BigInt` 运算符使用。

- 37 种值类型的 1,089 个函数中，796 个（73%）是 protocol 要求，其余 293 个为类型专属函数。
- 确定符合关系后，仍缺少的要求函数为 0 个（缺口）。默认实现可补齐的部分不计入。
- 作为参考的 Swift protocol 列于表的“Swift”列。

<a id="階層"></a>

## 层级

按系列（基础、数、容器、输入输出、领域）绘制树。下层 protocol 包含上层 protocol（继承）；包含其他系列的 protocol 时写在“＋”之后。

基础：

```
Eq
├─ Hash
└─ Ord
Repr
└─ Parse
Codec
```

数：

```
Additive  ＋ Eq
├─ Numeric
│  ├─ Signed
│  │  └─ FloatingPoint  ＋ Ord・Parse
│  │     └─ BinaryFloatingPoint
│  └─ Integer  ＋ Ord・Hash・Parse
│     ├─ FixedWidthInteger
│     ├─ SignedInteger  ＋ Signed
│     └─ UnsignedInteger
└─ TimeAmount  ＋ Ord
```

容器：

```
Sized
├─ Container
│  ├─ Membership
│  │  └─ SetAlgebra
│  ├─ Sequence
│  │  └─ Ordered
│  │     ├─ Indexed
│  │     └─ Text
│  ├─ Growable
│  └─ PriorityQueue
└─ KeyValue
```

输入输出：

```
Read
Write
Close
```

领域：

```
HttpMessage
```

<a id="protocol"></a>

## protocol

| 层 | protocol | 包含 | 要求（函数） | 默认实现 | Swift | 符合类型 |
|---|---|---|---|---|---|---|
| 基础 | `Eq` | — | — | — | Equatable | Bool・String・Bytes・List[A]・Set[A]・BigInt・Float・Float32・Float64・Duration・Compute・Uuid・Url・Option[A]・Result[A, E]・Value・Matrix・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 基础 | `Hash` | `Eq` | — | — | Hashable | Bool・String・Bytes・List[A]・Set[A]・BigInt・Duration・Compute・Uuid・Url・Option[A]・Result[A, E]・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 基础 | `Repr` | — | `to_string` | `to_string` | CustomStringConvertible | Bool・String・BigInt・Float・Float32・Float64・Uuid・Url・Value・Matrix・Regex・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 基础 | `Parse` | `Repr` | `parse` | — | LosslessStringConvertible | Bool・BigInt・Float・Float32・Float64・Uuid・Url・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 基础 | `Ord` | `Eq` | `compare` | — | Comparable | Bool・String・Bytes・BigInt・Float・Float32・Float64・Duration・Compute・Uuid・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 基础 | `Codec` | — | — | — | Codable | Bool・String・Bytes・List[A]・Deque[A]・Set[A]・Map[K, V]・BigInt・Float・Float32・Float64・Duration・Uuid・Url・Option[A]・Result[A, E]・Value・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 数 | `Additive` | `Eq` | — | — | AdditiveArithmetic | BigInt・Float・Float32・Float64・Duration・Compute・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 数 | `Numeric` | `Additive` | `from`・`from_checked` | — | Numeric | BigInt・Float・Float32・Float64・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 数 | `Signed` | `Numeric` | `abs` | — | SignedNumeric | BigInt・Float・Float32・Float64・Int・Int8・Int16・Int32・Int64 |
| 数 | `Integer` | `Numeric`・`Ord`・`Hash`・`Parse` | `min`・`max`・`clamp`・`floor_div`・`modulo`・`pow`・`is_even`・`is_odd`・`parse_radix`・`to_string_radix`・`bit_length`・`band`・`bor`・`bxor`・`bnot`・`bshl`・`bshr` | `is_even`・`is_odd` | BinaryInteger | BigInt・Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 数 | `FixedWidthInteger` | `Integer` | `min_value`・`max_value`・`count_leading_zeros`・`count_trailing_zeros`・`count_ones`・`byte_swap`・`wrap_add`・`wrap_sub`・`wrap_mul`・`rotate_left`・`rotate_right`・`from_saturating`・`from_wrapping` | — | FixedWidthInteger | Int・Int8・Int16・Int32・Int64・UInt8・UInt16・UInt32・UInt64 |
| 数 | `SignedInteger` | `Integer`・`Signed` | `sign` | — | SignedInteger | BigInt・Int・Int8・Int16・Int32・Int64 |
| 数 | `UnsignedInteger` | `Integer` | — | — | UnsignedInteger | UInt8・UInt16・UInt32・UInt64 |
| 数 | `FloatingPoint` | `Signed`・`Ord`・`Parse` | `min`・`max`・`clamp`・`nan`・`infinity`・`epsilon`・`min_value`・`max_value`・`is_nan`・`is_finite`・`is_infinite`・`floor`・`ceil`・`round`・`trunc`・`signum` | — | FloatingPoint | Float・Float32・Float64 |
| 数 | `BinaryFloatingPoint` | `FloatingPoint` | `to_bits`・`from_bits` | — | BinaryFloatingPoint | Float・Float32・Float64 |
| 数 | `TimeAmount` | `Additive`・`Ord` | `ns`・`us`・`ms`・`s`・`min`・`h` | — | DurationProtocol | Duration・Compute |
| 容器 | `Sized` | — | `len`・`is_empty` | `is_empty` | Collection（count・isEmpty） | String・Bytes・List[A]・Deque[A]・Set[A]・Heap[A]・Map[K, V] |
| 容器 | `Container` | `Sized` | `new`・`from_list`・`to_list`・`any`・`all`・`count`・`filter`・`clear` | `any`・`all`・`count` | — | String・Bytes・List[A]・Deque[A]・Set[A]・Heap[A]・Map[K, V] |
| 容器 | `Membership` | `Container` | `contains` | — | Sequence.contains（Element: Equatable） | List[A]・Deque[A]・Set[A]・Heap[A] |
| 容器 | `Sequence` | `Container` | `fold`・`find`・`enumerate`・`zip`・`zip_with`・`filter_map`・`find_map`・`reduce`・`scan`・`partition`・`group_by`・`sort`・`sort_by`・`sort_with`・`min`・`max`・`min_by`・`max_by`・`min_with`・`max_with`・`sum`・`product` | `find`・`enumerate`・`zip`・`zip_with`・`filter_map`・`find_map`・`reduce`・`scan`・`partition`・`group_by`・`sort`・`sort_by`・`sort_with`・`min`・`max`・`min_by`・`max_by`・`min_with`・`max_with`・`sum`・`product` | Sequence | String・Bytes・List[A]・Deque[A]・Set[A]・Map[K, V] |
| 容器 | `Ordered` | `Sequence` | `first`・`last`・`reverse`・`take`・`drop`・`take_end`・`drop_end`・`slice`・`find_index`・`take_while`・`drop_while`・`chunk`・`window`・`split_at`・`dedup`・`intersperse`・`unique`・`unique_by` | `find_index`・`take_while`・`drop_while`・`chunk`・`window`・`split_at`・`dedup`・`intersperse`・`unique`・`unique_by` | BidirectionalCollection | String・Bytes・List[A]・Deque[A] |
| 容器 | `Indexed` | `Ordered` | `get`・`set` | — | RandomAccessCollection・MutableCollection | Bytes・List[A]・Deque[A] |
| 容器 | `Growable` | `Container` | `push`・`pop`・`insert`・`remove_at` | — | RangeReplaceableCollection | String・Bytes・List[A]・Deque[A] |
| 容器 | `SetAlgebra` | `Membership` | `insert`・`remove`・`union`・`intersection`・`difference`・`symmetric_difference`・`is_subset`・`is_disjoint` | — | SetAlgebra | Set[A] |
| 容器 | `KeyValue` | `Sized` | `new`・`get`・`set`・`remove`・`contains`・`keys`・`values`・`update`・`upsert`・`merge` | — | Dictionary（具体类型） | Map[K, V] |
| 容器 | `PriorityQueue` | `Container` | `push`・`pop`・`peek` | — | Heap（swift-collections） | Heap[A] |
| 容器 | `Text` | `Ordered` | `contains`・`starts_with`・`ends_with`・`index_of`・`last_index_of`・`count_matches`・`split`・`replace`・`replace_first`・`strip_prefix`・`strip_suffix`・`repeat`・`pad_start`・`pad_end`・`is_equal_constant_time` | — | StringProtocol | String・Bytes |
| 输入输出 | `Read` | — | `read`・`read_exact`・`read_to_end` | `read_exact`・`read_to_end` | — | TcpStream・Fd・Stdin |
| 输入输出 | `Write` | — | `write` | — | — | TcpStream・Fd・Stdout・Stderr |
| 输入输出 | `Close` | — | `close` | — | — | TcpStream・TcpListener・UdpSocket・Fd |
| 领域 | `HttpMessage` | — | `header`・`header_values`・`headers`・`body`・`body_bytes`・`set_header`・`set_body`・`set_body_bytes` | — | — | HttpRequest・HttpResponse |

注：

- `Eq`：`==`、`!=`。是运算符而非函数。除 `Fn` 外所有值类型自动具有。
- `Hash`：可作为 Map 键、Set 元素。浮点数不具有。
- `Repr`：`"${x}"` 的内容。函数形式为 `<mod>.to_string`。
- `Parse`：仅满足 `parse(to_string(x)) == ok(x)` 的类型具有。
- `Ord`：`<` 等。从两个值选择的 `min`、`max`、`clamp` 属于数值 protocol 要求（`TimeAmount` 的 `min` 表示分钟，序列的 `min` 表示最小元素，因此不放在 `Ord`）。
- `Codec`：`encode`、`decode`。由派生赋予。
- `Additive`：`+`、`-` 与 0。
- `Numeric`：`*`。`from`、`from_checked` 接受任意数值类型的转换（R-L0-40；Swift `init(_:)`、`init(exactly:)`）。
- `Signed`：一元 `-`。
- `Integer`：`/`、`%`。位运算是函数（如 `band`，R-int-10），不复用已移除的符号（R-L0-43）。`bigint` 也属于此处（Swift `BinaryInteger` 设计上也包含任意精度整数）。数值类型间转换（`to_<T>`、`_checked`、`_saturating`、`_wrapping`）也在此处（Swift `init(_:)`、`init(exactly:)`、`init(clamping:)`、`init(truncatingIfNeeded:)`）。
- `FixedWidthInteger`：固定宽度整数。`wrap_*`、`rotate_*` 按类型宽度工作（Swift `&+`、`&-`、`&*`）。`from_saturating`、`from_wrapping` 对应 Swift `init(clamping:)`、`init(truncatingIfNeeded:)`。`count_trailing_zeros` 在 0 时返回类型宽度，因此不放在无宽度的 `bigint`。
- `FloatingPoint`：`/`。顺序采用 IEEE totalOrder（R-L0-16）。
- `TimeAmount`：`Duration`、`Compute`（R-duration-03）。
- `Sized`：从 `len` 提供 `is_empty` 默认实现。
- `Container`：不依赖顺序的查询。`filter` 返回同类型（Swift `RangeReplaceableCollection.filter`）。Swift 无此层，为 `heap` 等不向外暴露内部顺序的容器设置（R-heap-05）。
- `Membership`：元素是否存在，仅在元素为 `Eq` 时。
- `Sequence`：按确定顺序逐个遍历。序列算法只写一次基于 `fold` 的默认实现，可用于所有序列（同 Swift `Sequence` 扩展）。改变元素类型的结果返回 `List`。`sum`、`product` 仅在元素为 `Numeric` 时，`sort`、`min`、`max` 仅在元素为 `Ord` 时提供（R-L0-42）。
- `Ordered`：可从两端计数。截取函数（`take_while`、`chunk`、`split_at` 等）返回相同类型（Swift `SubSequence`）。
- `Indexed`：按位置读写。
- `Growable`：可增减。`push`、`pop`、`clear` 原地修改（`mut`），`insert`、`remove_at` 返回副本。方式由动词决定，各类型相同（R-L0-04；副本形式通过 `@reuse` 达到原地同速，因此原地修改仅限扩缩缓冲区的动词）。Swift 全部采用 `mutating`，Almide 则以值语义采用函数式风格。
- `KeyValue`：`contains` 接收键。
- `PriorityQueue`：Swift `Heap` 也不是 `Sequence`。
- `Text`：按子序列查找。`contains` 接收子序列，因此不能在同一类型同时具有 `Membership`。
- `Read`：R-L0-44。泛型函数只在 `io` 放一次（`io.read[H: Read]`）。`read` 最多返回 `max` 字节，对端关闭则为空。
- `Write`：R-L0-44。全部写入（Rust `write_all`）。
- `Close`：R-L0-44。第二次调用也为 ok。
- `HttpMessage`：R-http-12。

<a id="型ごとの準拠と穴"></a>

## 各类型符合关系与缺口

“符合”指声明的 protocol（省略其包含的 protocol）。“缺口”指要求中仍没有的函数，括号表示提出该要求的 protocol。

| 类型 | 符合 | 函数 | 类型专属 | 缺口 |
|---|---|---|---|---|
| `Bool` | Parse・Ord・Hash・Codec | 3 | 0 | 无 |
| `String` | Text・Growable・Ord・Repr・Hash・Codec | 85 | 21 | 无 |
| `Bytes` | Text・Indexed・Growable・Ord・Hash・Codec | 177 | 106 | 无 |
| `List[A]` | Indexed・Growable・Membership・Hash・Codec | 73 | 16 | 无 |
| `Deque[A]` | Indexed・Growable・Membership・Codec | 60 | 3 | 无 |
| `Set[A]` | SetAlgebra・Sequence・Hash・Codec | 42 | 1 | 无 |
| `Heap[A]` | PriorityQueue・Membership | 16 | 2 | 无 |
| `Map[K, V]` | KeyValue・Sequence・Codec | 37 | 1 | 无 |
| `BigInt` | SignedInteger・Codec | 24 | 0 | 无 |
| `Float` | BinaryFloatingPoint・Codec | 26 | 2 | 无 |
| `Float32` | BinaryFloatingPoint・Codec | 24 | 0 | 无 |
| `Float64` | BinaryFloatingPoint・Codec | 24 | 0 | 无 |
| `Duration` | TimeAmount・Hash・Codec | 7 | 0 | 无 |
| `Compute` | TimeAmount・Hash | 7 | 0 | 无 |
| `Uuid` | Parse・Ord・Hash・Codec | 7 | 4 | 无 |
| `Url` | Parse・Hash・Codec | 9 | 7 | 无 |
| `Option[A]` | Hash・Codec | 13 | 13 | 无 |
| `Result[A, E]` | Hash・Codec | 14 | 14 | 无 |
| `Value` | Repr・Eq・Codec | 29 | 29 | 无 |
| `Matrix` | Repr・Eq | 25 | 25 | 无 |
| `Regex` | Repr | 15 | 15 | 无 |
| `HttpRequest・HttpResponse` | HttpMessage | 34 | 26 | 无 |
| `TcpStream` | Read・Write・Close | 0 | 0 | 无 |
| `TcpListener` | Close | 0 | 0 | 无 |
| `UdpSocket` | Close | 0 | 0 | 无 |
| `Fd` | Read・Write・Close | 0 | 0 | 无 |
| `Stdin` | Read | 0 | 0 | 无 |
| `Stdout・Stderr` | Write | 0 | 0 | 无 |
| `Int` | FixedWidthInteger・SignedInteger・Codec | 45 | 8 | 无 |
| `Int8` | FixedWidthInteger・SignedInteger・Codec | 37 | 0 | 无 |
| `Int16` | FixedWidthInteger・SignedInteger・Codec | 37 | 0 | 无 |
| `Int32` | FixedWidthInteger・SignedInteger・Codec | 37 | 0 | 无 |
| `Int64` | FixedWidthInteger・SignedInteger・Codec | 37 | 0 | 无 |
| `UInt8` | FixedWidthInteger・UnsignedInteger・Codec | 35 | 0 | 无 |
| `UInt16` | FixedWidthInteger・UnsignedInteger・Codec | 35 | 0 | 无 |
| `UInt32` | FixedWidthInteger・UnsignedInteger・Codec | 35 | 0 | 无 |
| `UInt64` | FixedWidthInteger・UnsignedInteger・Codec | 35 | 0 | 无 |

<a id="この階層で決めたこと"></a>

## 由该层级作出的决定

确定层级后需要回答的问题及其决定。判断同时来自 Swift 和 Almide 的标准（LLM 所写代码能否原样运行）。“保留待定”在 Dojo 测量后决定。

| 问题 | 事实 | 决定 |
|---|---|---|
| `Bool` 是否为 `Ord` | Swift 不让 `Bool` 成为 `Comparable`（真假无大小含义）。Rust、Kotlin、OCaml、Python、Elixir 赋予顺序。Almide 当前允许 `false < true`。 | 采纳（R-bool-02）。9 种语言中 7 种（Rust、Python、JS、Gleam、Elixir、OCaml、Kotlin）赋予顺序，仅 Go、Swift 不赋予。Almide 当前也允许 `false < true`。层级骨架来自 Swift，但每种类型加入哪个 protocol 按该类型的多数决定。 |
| `Repr` 方法名 | 内置 `Repr` 方法是 `repr`，模块函数却是 `int.to_string`。同职责有两个拼写。 | 采纳（R-L0-38、R-bool-03）。与 R-L0-22（`Ord.cmp` → `compare`）一样，`Repr` 方法改为 `to_string`。于是所有值类型都有 `<mod>.to_string`（`bool.to_string` 也无需新增实现便出现）。 |
| `regex.parse` 是否为 `Parse` | Swift `Regex` 不是 `LosslessStringConvertible`。`regex.parse` 不是读取值的字符串表示，而是组装模式。 | 采纳（R-regex-11）。不将 `Regex` 放进 `Parse`，把 `regex.parse` 拼为 `regex.compile`（同 Go、Python、Elixir）。 |
| 定宽整数算术 | `int` 有 `floor_div`、`modulo`、`pow`、位运算、`abs`、`sign`，但 `int8`～`uint64` 没有。加入 `Integer`、`FixedWidthInteger` 后这些直接成为缺口。 | 采纳（R-sized-10，重审 R-sized-05）。作为 `Integer` 要求放在所有整数上。内部只写一个默认实现，不按类型重复写。 |
| `bigint` 位运算 | Swift `BinaryInteger` 将位运算作为要求（任意精度也按二进制补码行为）。`bigint` 当前没有。 | 采纳（R-bigint-07）。纳入要求。不采纳把 `bigint` 移出 `Integer` 的替代方案，否则不能在同一泛型函数处理 `int` 与 `bigint`。 |
| `bytes.contains` 含义 | `Text` 的 `contains` 接收子序列，`Membership` 的 `contains` 接收元素。Almide 无重载，因此一种类型只能加入一方。 | `bytes` 与 `string` 一样设为 `Text`（保持 `bytes.contains(b, pat)`）。单个值用 `bytes.any(b, (x) => x == v)` 表达。 |
| `string` 是否为 `Indexed` | Swift `String` 仅到 `BidirectionalCollection`（按位置读取不是 O(1)）。Almide `string.get` 按码点。 | 仅到 `Ordered`，`string.get` 保留为 `string` 专属函数。 |
| `deque` 的 `push`、`pop` | `Growable` 的 `push`、`pop` 针对末尾，`deque` 拼为 `push_back`、`pop_back`。 | 采纳（R-deque-03）。提供 `deque.push`、`deque.pop` 作为 `Growable` 函数，不提供 `push_back`、`pop_back`（`push_front`、`pop_front` 保留为 `deque` 专属）。 |
| `int` 的 N bit 运算 | `int.wrap_add(a, b, bits)`、`rotate_left(a, n, bits)`、`bit_width`、`bshr_logical` 把 `Int` 值当作 N bit 数。固定宽度由类型决定，若同名却不带 `bits`，形状不一致。 | 采纳（R-int-12、R-int-11）。`wrap_*`、`rotate_*` 移除 `bits`，按类型宽度运行，成为 `FixedWidthInteger` 要求。N bit 与 `int.wrap(n, bits)` 组合。`bit_width` 易与 Swift `bitWidth`（类型宽度）混淆，改名 `bit_length`。 |
| `string` 的 `new`、`from_list`、`to_list` | `Text` 包含 `Container`，所以 `string` 也需三者。但 `to_list` 与 `string.chars` 同职责，`from_list` 等同 `list.join(xs, "")`。 | 采纳（R-string-17）。`chars` 改名 `to_list`，提供 `new`、`from_list`。 |
| `map`、`flat_map` | Swift `Sequence.map` 总返回 `Array`，`Optional`、`Result` 的 `map` 是具体类型函数。不使用以容器本身为参数的类型（`F[_]`）。 | 对齐 Swift，不做成 protocol，保持每类型函数。不向语言加入 `F[_]`。 |
| `list` 的 `new`、`from_list`、`to_list` | 属于 `Container` 要求，但 `list` 没有（已有字面量 `[]`，自身就是 `List`）。要在泛型函数写 `T.new()`，`list` 也必须具备。 | 采纳（R-list-20）。提供它们。实现显然（`[]` 与恒等），编写 `list` 代码时不必使用。 |
| 数值类型间转换 | Swift 以目标类型泛型初始化器（`Int8(clamping: x)`）统一。Almide 在来源模块排列每个目标的名称（`int.to_int8_saturating`）。 | 采纳（R-L0-40）。同 Swift 放在目标类型，采用 `<T>.from`、`from_checked`、`from_saturating`、`from_wrapping` 四个泛型函数。347 个函数变为 44 个。 |
| `map`（Map）是否为 `Container` | Swift `Dictionary` 是 `(key, value)` 对的 `Collection`。Almide `map.filter` 等 callback 为两个参数。 | 采纳（R-map-09）。callback 改为一个元组，引入可在元组位置写双参数匿名函数的规则（同 Swift），让 `map` 成为 `(K, V)` 的 `Sequence`。 |

<a id="言語に要るもの"></a>

## 语言所需功能

下表是实现该层级所列出的功能设计清单（#4000），不是当前未实现清单。正如开头所述，无接收者要求调用及内置 `Numeric`、`Integer`、`Text` 已实现。元组 callback 的匿名函数有 [R-map-09](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-map.md) 所记录的适用范围与 UFCS 限制。这不表示表中全部层级、默认实现、条件符合已经实现。

这些功能 Swift 均有。与 Swift 相同，不使用将容器本身作为参数的类型（`F[_]`）。函数数与符合表也只是预期设计汇总，不是当前实现符合性证明。当前公开函数请参阅[标准库参考](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/README.md)。

| 功能 | 示例 | 用途 |
|---|---|---|
| protocol 继承 | `protocol Integer: Ord + Hash + Numeric + Parse` | 层级自身 |
| 关联类型 | `Container` 元素类型（`List[A]` 为 `A`，`Bytes` 为 `Int`） | 全部容器层 |
| 默认实现 | 从 `compare` 得 `min`、`max`、`clamp`，从 `len` 得 `is_empty`，从 `fold` 得 `any`、`all`、`count` | 无需新增实现便补齐大量缺口 |
| 无接收者要求调用 | 泛型函数中的 `T.parse(s)`、`T.new()`、`T.max_value()`、`T.nan()` | `Parse`、`Container`、`FixedWidthInteger`、`FloatingPoint` |
| 运算符要求 | `Additive` 的 `+`、`-`，`Numeric` 的 `*` | 数值层（Swift `static func +`） |
| 可写在元组位置的匿名函数 | 在 `((K, V)) -> Bool` 位置可写 `(k, v) => …` | `map` 的 `Sequence`（R-map-09） |
| 条件符合 | `List[A]` 仅在 `A: Hash` 时为 `Hash`，`contains` 仅在元素为 `Eq` 时 | `Hash`、`Membership` |

<a id="その型だけの関数"></a>

## 类型专属函数

- `String`（21）: `byte_len`、`byte_slice`、`capitalize`、`from_codepoint`、`from_utf16`、`get`、`is_alpha`、`is_alphanumeric`、`is_digit`、`is_lower`、`is_upper`、`is_whitespace`、`lines`、`split_once`、`to_codepoint`、`to_lower`、`to_upper`、`to_utf16`、`trim`、`trim_end`、`trim_start`
- `Bytes`（106）: `append_bool`、`append_bytes`、`append_f16_be`、`append_f16_le`、`append_f32_be`、`append_f32_le`、`append_f64_be`、`append_f64_le`、`append_i16_be`、`append_i16_le`、`append_i32_be`、`append_i32_le`、`append_i64_be`、`append_i64_le`、`append_i8`、`append_u16_be`、`append_u16_le`、`append_u32_be`、`append_u32_le`、`append_u8`、`copy_from`、`copy_within`、`fill`、`from_string`、`get_bool`、`get_f16_be`、`get_f16_be_array`、`get_f16_le`、`get_f16_le_array`、`get_f32_be`、`get_f32_be_array`、`get_f32_le`、`get_f32_le_array`、`get_f64_be`、`get_f64_be_array`、`get_f64_le`、`get_f64_le_array`、`get_i16_be`、`get_i16_be_array`、`get_i16_le`、`get_i16_le_array`、`get_i32_be`、`get_i32_be_array`、`get_i32_le`、`get_i32_le_array`、`get_i64_be`、`get_i64_be_array`、`get_i64_le`、`get_i64_le_array`、`get_i8`、`get_u16_be`、`get_u16_be_array`、`get_u16_le`、`get_u16_le_array`、`get_u32_be`、`get_u32_be_array`、`get_u32_le`、`get_u32_le_array`、`get_u8`、`is_eof`、`is_valid_utf8`、`map`、`next_bool`、`next_bytes`、`next_f16_be`、`next_f16_le`、`next_f32_be`、`next_f32_le`、`next_f64_be`、`next_f64_le`、`next_i16_be`、`next_i16_le`、`next_i32_be`、`next_i32_le`、`next_i64_be`、`next_i64_le`、`next_i8`、`next_u16_be`、`next_u16_le`、`next_u32_be`、`next_u32_le`、`next_u8`、`put_bool`、`put_f16_be`、`put_f16_le`、`put_f32_be`、`put_f32_le`、`put_f64_be`、`put_f64_le`、`put_i16_be`、`put_i16_le`、`put_i32_be`、`put_i32_le`、`put_i64_be`、`put_i64_le`、`put_i8`、`put_u16_be`、`put_u16_le`、`put_u32_be`、`put_u32_le`、`put_u8`、`skip`、`to_string`、`to_string_lossy`、`xor`、`zeros`
- `List[A]`（16）: `binary_search`、`binary_search_by`、`binary_search_with`、`flat_map`、`flatten`、`index_of`、`iterate`、`join`、`map`、`range`、`range_step`、`repeat`、`swap`、`unzip`、`update`、`with_capacity`
- `Deque[A]`（3）: `map`、`pop_front`、`push_front`
- `Set[A]`（1）: `map`
- `Heap[A]`（2）: `new_by`、`new_with`
- `Map[K, V]`（1）: `map`
- `Float`（2）: `pow`、`to_fixed`
- `Uuid`（4）: `from_bytes`、`to_bytes`、`v4`、`v7`
- `Url`（7）: `build_query`、`decode_component`、`decode_form`、`encode_component`、`encode_form`、`parse_query`、`resolve`
- `Option[A]`（13）: `collect`、`collect_map`、`filter`、`flat_map`、`flatten`、`is_none`、`is_some`、`map`、`or_else`、`to_list`、`to_result`、`unwrap_or_else`、`zip`
- `Result[A, E]`（14）: `filter`、`flat_map`、`flatten`、`is_err`、`is_ok`、`map`、`map_err`、`or_else`、`partition`、`to_err_option`、`to_list`、`to_option`、`unwrap_or_else`、`zip`
- `Value`（29）: `array`、`as_array`、`as_bool`、`as_float`、`as_int`、`as_object`、`as_string`、`bool`、`float`、`get`、`get_array`、`get_bool`、`get_float`、`get_int`、`get_object`、`get_path`、`get_string`、`index`、`int`、`key`、`merge`、`null`、`object`、`omit`、`pick`、`remove_path`、`root`、`set_path`、`string`
- `Matrix`（25）: `broadcast_add_row`、`cols`、`concat_cols`、`concat_rows`、`dot_row`、`from_bytes_f16_le`、`from_bytes_f32_le`、`from_bytes_f64_le`、`from_lists`、`gather_rows`、`get`、`map`、`ones`、`rows`、`scale`、`shape`、`slice_rows`、`split_cols_even`、`to_bytes_f16_le`、`to_bytes_f32_le`、`to_bytes_f64_le`、`to_lists`、`transpose`、`zeros`、`zip_with`
- `Regex`（15）: `captures`、`captures_all`、`compile`、`escape`、`find`、`find_all`、`index_of`、`is_full_match`、`is_match`、`new`、`replace`、`replace_first`、`replace_first_with`、`replace_with`、`split`
- `HttpRequest・HttpResponse`（26）: `cancel`、`decode_json`、`get`、`get_response`、`json`、`method`、`mount`、`new_request`、`param`、`path`、`poll`、`query_params`、`read_new`、`redirect`、`request`、`response`、`route`、`router`、`send`、`serve`、`set_status`、`start`、`status`、`wait`、`with_headers`、`wrap`
- `Int`（8）: `bit_reverse`、`bshr_logical`、`log2_ceil`、`log2_floor`、`next_power_of_two`、`prev_power_of_two`、`to_hex`、`wrap`
