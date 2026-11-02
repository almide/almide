<a id="map--set-data-structure-roadmap"></a>

# Map / Set 数据结构路线图

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](map-data-structure-roadmap.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](map-data-structure-roadmap.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](map-data-structure-roadmap.zh-CN.md)

> 非规范固定读者快照。未改变的原文仍为权威来源，不替换机器输入。不自动同步，也未独立执行。区分保留历史测量、提案及原文时点的实现更新，不构成当前正确性或性能的独立证明。代码与注释保持原样。 阅读历史正文须以开头的当前表示说明为准：native Map/Set 现在可带 index，但小型／不可哈希情况保留线性路径，hash-flood 有有序 fallback，持久化更新仍复制。保留旧正文不加限定的 O(1)、仅 Vec、dense Set 以及 HAMT“唯一结构”等历史主张，不将其作为当前最坏情况保证或独立比较研究。不宣称 HAMT 已实现。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/map-data-structure-roadmap.md)

> 原 metadata 非活动摘要：Map / Set 数据结构路线图，包括被否决的 seq-in-entry 设计及其错误原因。

<a id="current-representation-versus-the-earlier-design"></a>

## 当前表示与早期设计

native Map 与 Set 实现现在保留插入顺序的 entry，**并能添加 index**；native 已不再永久仅使用 Vec。见 [map.rs](../../../../../../runtime/rs/src/map.rs) 和 [set.rs](../../../../../../runtime/rs/src/set.rs)。小型 collection 及不可哈希的 key/element 类型（包括 Float）保留线性路径。带 index 的 lookup 不是无条件的最坏 O(1) 保证：hash-flood 处理也有有序 fallback，而持久化更新仍复制数据。

其余内容记录早期 compact-ordered-dict 选择、被否决的 seq-in-entry 实验和 HAMT 研究。v0 emitter 路径、dense Set 前提以及拟议的 native index 都是历史内容，不是当前源码分布；本次更新不宣称 HAMT 提案已实现。

`Map[K,V]`（以及 `Set[T]`）如何表示，以及应该走向何处。

<a id="decision"></a>

## 决策

**`Map` 是 O(1) 插入有序哈希映射——compact-ordered-dict（②）。** 它是标准的“正确”map（Python dict / `indexmap`）；既然两种选择都要同样的大规模整体重写，就用更好的结构。`Set` 保持 dense list（其 `PartialEq`、可能为 `Float` 的元素不需要 hash index）。两者都按**插入顺序**迭代；跨目标等价（native 迭代 == wasm）是由 gate 强制的硬不变量。

目前已交付：
- **native** `Set` + `Map`（`runtime/rs/src/{set,map}.rs`，PR #354）：`Vec<T>` / `Vec<(K,V)>`，按首次出现的插入顺序；`insert` 原地更新值并保留位置，或追加新值；`remove` 保持剩余元素顺序；相等性与顺序无关。key/elem 约束为 `PartialEq`（而非 `Eq + Hash`），因此 `Set[Float]` 和非 `Hash` key 的 map 可用。**这是跨目标迭代顺序的参考 oracle**——native 保持 `Vec`（不采用 native hash index 的原因见②）。
- **wasm `Set`**：dense `[len][cap][data…]` 列表（插入顺序）。

**已交付：wasm `Map` → compact-ordered-dict（②）。** 分支 `xtarget-wasm-map-order`（布局常量 `709b6764`、helper `715acc1e`、op 切换 `89a5833e`）。dense `(key,val)` entry（插入顺序，迭代 `0..len`）加上用于 O(1) lookup 的独立 hash INDEX 区域（tag + 从 1 开始的 slot pointer）；集中的 `emit_dict_*` helper 与命名 `layout::map` 常量为每个 offset 命名，消除 magic number。所有 map-layout 位置一并切换（`calls_map.rs` + `calls_map_closure.rs` + `control.rs` for-in、`expressions.rs` literal、`equality.rs` eq、`calls_list_closure2.rs` group_by、`calls_http.rs` headers）。已确认 `calls_value.rs` 不是 map（其 `=8` offset 是 `List[(String,Value)]` object payload）。在完整 spec suite 和所有 `spec/wasm_cross/*.almd` 上验证 native==wasm 逐字节相同；新增 `map_insertion_order.almd` 将顺序保证固定进 gate（原语料只检查与顺序无关的计数）。废弃 Swiss helper（`emit_map_resize`/`emit_alloc_table`/`emit_swiss_setup`）已移除；`set` 现在按 load factor 增长（修复超过 16 个 entry 的溢出）。完整笔记：memory `project_wasm_compact_ordered_dict`。

> ⚠️ 早期 **seq-in-entry** 尝试已放弃：它是改变 entry *stride* 的错误变通，迫使修改每个手写 stride 位置，导致 magic number 泛滥和 6 个 corruption trap。正确 COD 不改变 entry stride：entry 保持 `ks+vs` 的 dense 排列，INDEX 是*独立区域*。

<a id="rejected-alternative--①-plain-dense-list-on"></a>

### 被否决的备选——① 普通 dense list（O(n)）

类似 dense `Vec` 的 map（线性扫描 lookup），完全对应 wasm Set + native Vec：最简单、每操作风险最低、零 magic number。否决原因是它让 `Map` 暗中成为 O(n)，对可变／读密集代码是陷阱，而 hash map *应当*是 O(1)。（持久化 clone-on-write 模型无论如何都让*构建*成为 O(n²)，削弱②的 O(1) 优势；这是①曾有吸引力的唯一原因。）

<a id="②-compact-ordered-dict--the-chosen-map-structure"></a>

## ② Compact-ordered-dict——选定的 Map 结构

“正确哈希映射”——Python 3.7+ `dict`、Ruby `Hash`、Rust `indexmap`：在 **dense 插入有序 entry** 数组上建立 hash **INDEX**。O(1) lookup，迭代仍为插入顺序，遍历 dense entry。

- wasm：TAGS（h2 快速拒绝）+ INDEX（指向 dense entry、从 1 开始的 pointer）+ DENSE ENTRIES。lookup 探测 index，迭代遍历 dense entry。增长时 entry 不移动，只有 index rehash。完整设计及集中的 `dict_*`/`layout::map::{INDEX_SLOT_SIZE,EMPTY_INDEX,INITIAL_CAP,GROWTH_SHIFT,LOAD_NUM,LOAD_DEN,H2_SHIFT,H2_MASK}` 常量已搭好脚手架（见 workflow `wf_1a3614dd`、memory `project_wasm_compact_ordered_dict`）。
- native：dense `Vec<(K,V)>` + hash index。标准 `HashMap<K,usize>` index 强制 `K: Eq+Hash`，从而**拒绝 `Map[Float]` 和 `Map[record-without-derive-Hash]`**，与通过 raw byte 哈希任意 key 的 wasm 分歧。要在②下保持跨目标 parity，要么两侧采用通用的 **“map key 必须 hashable+eq”** 契约（简洁，但重新限制当前 `PartialEq` 放宽），要么为 native 提供与 wasm hash 一致的定制 **byte-hashing** index（代码更多，但支持任意 key，才是真正等价的选择）。

**选择原因：** “key 必须可哈希”是通用标准（每种严肃语言都要求），O(n) map 是陷阱，因此②是正确的 *map* 设计；②与①都需同样的大规模整体重写，更好结构胜出。**已接受的限制：** 持久化 clone-on-write 模型中 O(n²) 构建占主导，所以②的 O(1) lookup 主要帮助可变 `insert`/`delete` 链和稳定 map 上的重复 `get`；真正原因是结构正确性（“Map 是 O(1)”），而不是 profile。seq-in-entry 变通中出现的 magic number 风险，通过*正确*实施②处理：独立 index 区域，entry stride 不变，集中 `dict_*` helper，绝不内联字节运算。在添加定制 byte-hashing index 之前，或若不添加，native 侧保持 `Vec`（顺序 oracle）；不采用标准 `HashMap` index，其 `K:Hash` 约束会导致上述回归。

<a id="③-hamt--the-true-persistent-ideal-research"></a>

## ③ HAMT——真正的持久化理想（研究）

持久化 map 的最终目标：**hash array mapped trie**（Clojure / Scala / immutable.js），通过**结构共享**实现 O(log n) 操作，使 `map.set` 不进行 O(n) clone。这是解决①和②共有的 O(n²) 持久化构建的唯一结构。代价是规模远大于其他方案的实现；使其在 native（Rust）与手工发射 wasm 运行时之间逐字节相同，本身就是独立项目（也强力支持“单一运行时”终局：将一个 Rust runtime 编译到 wasm32，或用 Almide 自身定义 map）。作为持久化 collection 的长期方向跟踪。

<a id="cross-cutting"></a>

## 横切要求

无论表示是什么，**硬不变量都是可观察的跨目标等价性**（native 迭代 == wasm 迭代，由 `tests/wasm_runtime_cross_target.rs::wasm_cross_target_spec` 强制）。①→②→③ 的任何迁移都必须保持每个 `spec/wasm_cross/map_*` 案例逐字节相同，且不得新增 `@xt-allow`。Set 可以保持 dense list（Set[Float] 只需 `PartialEq`）；index 问题只针对 Map。
