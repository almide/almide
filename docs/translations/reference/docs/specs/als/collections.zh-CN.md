<a id="als--コレクションcollections"></a>

# ALS — 集合（Collections）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](collections.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](collections.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](collections.zh-CN.md)

> 权威原文：[docs/specs/als/collections.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/collections.md)，固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`。本页是非规范性的读者快照，不替代原 ALS 或检查器、台账、生成器输入，不自动同步。需求、历史状态、证据和安全主张均保留原文范围。本版未独立执行、测量、认证或证明。原文语言版为读者副本，其他语言版为翻译。

> 原文范围说明：ALS-C5 在此未讨论 `mut` 参数，只陈述函数参数的一般值语义；保留该陈述，不扩展为否定另行规定的显式写回契约。恒定时间主张保留 ALS-T31 的范围，不增加与长度无关或全程序时间信息的保证。


> 最后更新：2026-10-27

集合类型（List / Map / Set / 元组 / 记录）的可观察规范。引用方式与 [strings.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/strings.md) 开头相同，即由契约台账的 `spec` 键引用。

<a id="als-c1-map-の順序規範"></a>

## ALS-C1 Map 的顺序规范

Map 是**保留插入顺序的有序字典**。`keys`、`values`、`entries` 和迭代始终按插入顺序。重新赋值已有键不改变顺序；删除后再次插入则追加到末尾。依赖哈希顺序或排序顺序的实现不符合规范。
Contracts: C-013。

<a id="als-c2-set-の順序規範"></a>

## ALS-C2 Set 的顺序规范

Set 保留插入顺序，迭代和 `to_list` 确定性地按插入顺序返回。重复插入不改变顺序。
Contracts: C-014。

<a id="als-c3-構造的等価"></a>

## ALS-C3 结构相等

`==`/`!=` 是**结构深度相等**：列表、元组、记录、Map、Set、Option/Result 及嵌套复合值逐元素递归比较。引用（指针）比较不符合规范。Float 分量使用数值比较，而非 IEEE-754 位比较（NaN != NaN）。
`Secret[String]` 与 `Secret[Bytes]` 的 `==` 以恒定时间比较内容（ALS-T31）。其他 `Secret[A]`，以及含 Secret 的 variant、Map、Set 的 `==`，在检查时以 E103 拒绝（`spec/wasm_cross/secret_eq_and_decode.almd`）。
Contracts: C-015, C-426。

<a id="als-c4-範囲外アクセスの縮退規則"></a>

## ALS-C4 越界访问的退化规则

越界读取**安全退化**：`list.get` 返回 none，`slice` 钳制范围，`take`/`drop` 退化为空／全部。负值**按 0 读取**（ALS-T16 的负数量规则，almide/almide#4147）：`slice` 的负 start 和负 end 均为 0；`take(xs, 負)` 为空，`drop(xs, 負)` 为全部（負 表示负值）。
`list_slice_oob.almd` 和 `negative_count_rule.almd` 固定此行为。旧规则“负 start → 空、负 end → len、`take(xs, 負)` 返回全部”源自 `as usize` 的重新解释，因为 `?limit=-1` 会取消数量上限而被撤回。
反向范围为空。上述情况均不得访问堆外或触发 trap。（下标**语法** `xs[i]` 的越界 abort 由 ALS-T6 另行规定。）
Contracts: C-034。

<a id="als-c5-値意味論copy-on-write"></a>

## ALS-C5 值语义（写时复制）

`var b = a` 之后对 `b` 的修改，通过 `a` **不可观察**（值语义）。实现可使用写时复制等方式共享存储，但若能通过别名观察到变化，则不符合规范。函数参数同样如此：被调用方的修改对调用方不可见。
Contracts: C-033, C-150。

<a id="als-c6-型変換コンビネータ"></a>

## ALS-C6 改变类型的组合子

`map.map` 和 `set.map` **可以改变**元素类型（`Map[K,A] → Map[K,B]`、`Set[A] → Set[B]`）。结果集合遵循转换后类型的相等和顺序规范（ALS-C1/C2/C3）。
Contracts: C-039。

<a id="als-c7-レコード変種パターンマッチ"></a>

## ALS-C7 记录、变体与模式匹配

记录（具名／匿名）、变体（ADT）的构造，字段读取，以及模式匹配（字面量／ctor／元组／守卫／嵌套）的求值结果，在目标之间可观察等价。匹配从上到下执行第一个匹配的分支。
Contracts: C-036。

<a id="als-c8-幅指定整数フィールド"></a>

## ALS-C8 定宽整数字段

放入定宽整数类型（Int8/16/32 等）字段的字面量**收窄到声明宽度**（C-038 承诺范围内字面量的表示收窄）。超出声明范围的字面量以 **E024** 拒绝，不回绕（ALS-M14 的范围纪律）。
读取获得声明宽度本身的值（`Int8` 就是 `Int8`），没有隐式数值拓宽；需要 `Int` 的位置通过 `int.from(x)` 显式转换（0.59.1 实测）。
Contracts: C-038。

<a id="als-c9-順序系コンビネータの全域性"></a>

## ALS-C9 排序组合子的全域性

`list.min/max/sort/sort_by/unique_by` **由元素类型引导**且全域：Int/String 采用全序，**Float 采用 IEEE-754 totalOrder**（无论符号或来源，NaN 都视为唯一的 +qNaN 0x7FF8000000000000，是 +inf 之后的最大值；-0 < +0）。即使混入 NaN 也确定性完成（panic 或未定义顺序不符合规范）。String 键的 `sort_by` 即使元素在堆上，也以稳定顺序全域执行（结果共同拥有每个元素）。
Contracts: C-053, C-055, C-276。

<a id="als-c10-空コレクションの型要件静的規範"></a>

## ALS-C10 空集合的类型要求（静态规范）

无法推断元素类型的空集合（例如对单独的 `[]` 做 fold 时类型信息不足）是**编译错误**，不是运行时任意行为。fold 的初值与**累加器类型**合一，与元素类型无关：`list.fold([1,2,3], "", …)` 符合规范并返回 "123"。空集合需要元素类型注解。
Contracts: C-052, C-058。
