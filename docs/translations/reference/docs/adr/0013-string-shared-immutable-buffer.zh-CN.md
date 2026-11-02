<a id="adr-0013-string-becomes-a-shared-immutable-buffer--the-codec-ceiling-is-a-representation-not-a-law"></a>

# ADR-0013：`String` 改为共享不可变缓冲区——codec 上限来自表示，而非定律

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0013-string-shared-immutable-buffer.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0013-string-shared-immutable-buffer.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0013-string-shared-immutable-buffer.zh-CN.md)

> 固定历史决策的非规范性读者副本，以原文为准，不自动同步。保留记录中的测量、比较和实现主张，但未独立重新验证。代码示例及注释逐字保留，包括原有语言。 当时的采纳、分阶段计划、数值改善和回退阈值均为历史原文主张，不确认所有阶段目前已实现。原文明确说明 Roc 行无一手资料，另有比较未提供资料；本副本不独立证实这些比较，也不证明当前跨目标等价性或内存安全。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0013-string-shared-immutable-buffer.md)

- **状态**：Accepted（2026-09-04 通过。初稿“String 保持拥有式，将上限写入台账”在重新调查 ../almide-references 后被否决：高级语言都采用“不可变共享缓冲区”，没有默认进行拥有式复制的语言。本稿采用 Alternative A 重写。）
- **日期**：2026-08-30（初稿）/ 2026-09-04（修订、批准）
- **决策范围**：Almide `String` 值的 native **表示**（拥有式 `String` → 引用计数共享不可变缓冲区），以及 `mut` 表面（`string.push` / `string.clear`）的写时复制。不包括 `Value` 的表示（#1679，另一个工作方向）。
- **相关**：#1673 / #1678 / #1679 / #1004、`docs/design/MEMORY-SAFETY.md`（RcCow）、ADR-0004（stdlib 的 String 终点）、C-132（mut 参数写回约定）。

<a id="context"></a>

## 背景

以“不输给上游模式验证库”（位于 `../almide-references`，v4.5.2）为目标，对相同工作负载（含 8 个字段、嵌套 record、`List[String]` 的 record 解码 100 万次，arm64，ns/op）中所有可改进的机制进行优化后，得到：

| | v0.59.2 | develop (2026-08-30) | 上游 |
|---|---:|---:|---:|
| `for v in data { User.decode(v) }` | 1693 | **205** | `safeParse` ~100 |
| `json.parse`（同一文档） | 948 | **515** | `JSON.parse` ~500 |
| parse + decode（端到端） | 1576 | **735** | ~600 |

剩余差距已缩小到一个原因。`User.decode` 的 205 ns 中约 160 ns 用于 record 的 8 个 `String` 字段的**拥有式复制**，上游不承担这项成本（JS 字符串是共享的）。用 Rust 手写相同形状，借用字段的 record 为 53 ns，拥有 `String` 字段的 record 为 168 ns（#1679）。

同一根源也出现在另一项基准中。strchurn（#1004）差距的 **3/4** 来自 `string.split` 返回 `Vec<String>`：Almide 没有借用字符串类型，因此子串总要复制（`research/benchmark/perf/string-gap-1004.md`）。

<a id="参照コンパイラはどうしているかalmide-references2026-09-04-再調査"></a>

### 参考编译器如何处理（../almide-references，2026-09-04 重新调查）

| 语言 | `String` 表示 | 一手资料 |
|---|---|---|
| Koka（本设计 Perceus 的直接源头） | 不可变、引用计数共享。4 种表示：空 singleton / ≤7 B 内联短字符串 / 普通 / raw buffer。计数**非原子**，只有跨线程共享的值通过负数计数切换为原子计数 | `koka/kklib/include/kklib/string.h:13-22`、`kklib.h:103-114` |
| Swift | 写时复制 + 15 B 内联短字符串，字面量为 immortal（省略 ARC） | `swift/stdlib/public/core/StringObject.swift:28-45` |
| Roc | 内联短字符串 + 引用计数，通过 seamless slice 共享缓冲区 | （tree 较浅，无一手资料。广为人知的设计。） |
| Gleam / Erlang | 共享不可变 binary，通过 sub-binary 共享子序列 | — |
| Rust / Zig | owned / borrowed 双轨 | — |

没有高级语言默认采用拥有式复制。Rust 虽为 owned，但有 `&str`；Almide 恰好缺少它，形成 #1004 的差距。

<a id="almide-側の事実"></a>

### Almide 一侧的事实

- **wasm 分支已经使用共享表示**：String 是带有 `rc@0` 的引用计数块（`crates/almide-layout`）。只有 native 进行拥有式复制。“改变表示会增加 byte-identity 的证明对象”恰好相反：这是对齐两个分支的方向（初稿的撤回条件“若 wasm 改为共享表示”其实早已成立）。
- **只有两种破坏性操作**：`string.push(mut s)` 和 `string.clear(mut s)`。可以直接应用 List / Map 已使用的 RcCow（唯一时原地修改，共享时复制），值语义不变。
- **初稿测得“慢”的原型是另一回事**：`Value` 一侧的 `Arc<str>` + HashSet intern，包含哈希查找（parse +6% / decode +20%）。这不是简单共享（clone = 计数 +1，~1-5 ns）与分配 + memcpy（~20 ns/字段）的比较。

<a id="decision"></a>

## 决策

**native 的 `String` 改为引用计数共享不可变缓冲区。** `mut` 表面（`push` / `clear`）用 RcCow 检查唯一性，选择原地修改或复制。子串（`split` / `slice` / `trim` …）以指向同一缓冲区的 seamless slice 返回。与 wasm 分支表示（rc 块）在可观察行为上等价。

分阶段实施（每阶段独立落地，ratchet 保护上一阶段）：

1. **先建立 ratchet**：把 strchurn / fasta / decode（`User.decode` 100 万次）三项列入 `research/benchmark/perf/` 的 A/B 台账，纳入 `scripts/check-perf-ratio.sh` 的检查对象。
2. **切换表示**：把生成 Rust 和 `runtime/rs` 的 `String` 改为 `AlmideStr`（`Rc<str>` 系，非原子）。`push` / `clear` 使用 RcCow。跨 fan 采用 Koka 的 thread-shared 方式（只对传给 fan 的值原子化）。若阶段 2 测量显示原子 RC 使 strchurn 恶化，就在此停止并恢复表示（ratchet 变红 = 不落地）。
3. **seamless slice**：`split` / `slice` / `trim` / `lines` 系列共享缓冲区。#1004 的 3/4 在此消除。
4. **内联短字符串**（Koka ≤7 B / Swift 15 B）另立 ADR；由于会改变 ABI 并与 wasm 分歧，不在本稿范围内。

实现在 0.62.0 **之后**进行（表示变更不跨越发布边界）。

<a id="rationale"></a>

## 理由

1. **COW 保护值语义。** 程序可观察的结果与拥有式复制相同。RcCow 已用于 List / Map，扩展至 String 并非新机制。LLM 编写者需要知道的规则不会增加。
2. **两个分支采用相同形态。** wasm 已经共享。对齐 native 后，C-132 的 mut 写回和 alias_safety 可在“两个分支表示相同”的基础上讨论。
3. **差距来自表示，而非上限。** “decode ≒ 100 ns + 20 ns × String 字段数”是拥有式复制的线性规律，共享缓冲区可降至 ~1-5 ns × 字段数（手写的 53 ns 是下界的证据）。

<a id="alternatives"></a>

## 备选方案

- **A'. 保持拥有式，将上限记入台账（初稿）。** 否决。既违背高级语言设计，也违背 wasm 分支已有表示。
- **B. 解码侧使用借用 record。** 语言没有引用类型，无法表达。否决。
- **C. 共享 `Value`。** 实测更慢（#1679）。否决（另一个工作方向）。
- **D. 仅使用 SSO。** 可消除分配，但复制和 `split` 的问题仍在。作为阶段 4，在本决策之后考虑。

<a id="consequences"></a>

## 后果

- 改善：decode 的 String 字段成本 ~20 ns → ~1-5 ns/字段；消除 strchurn 的 split 复制；两个分支的 String 表示对齐。
- 代价：`runtime/rs` 和生成 Rust 的 String 类型改变（ADR-0004 的终点是 API 而非表示，因此不触及冻结表面）。`push` / `clear` 增加唯一性分支。跨 fan 原子化成本在阶段 2 的 ratchet 中测量后才接受。
- 不变：契约（可观察结果相同）、`String` API、wasm 分支。

<a id="falsifier"></a>

## 可推翻条件

若阶段 2 的 ratchet 显示 strchurn / fasta 任一项**恶化 5% 或更多**，且 thread-shared 方式也无法恢复，则回退阶段 2，把本 ADR 标为 Superseded，并回到 A'。

<a id="references"></a>

## 参考资料

- #1673, #1678, #1679, #1004、PR #1680–#1687
- `../almide-references/RESEARCH.md` §5（JSON 字符串扫描）、§6（Koka 的借用/拥有环境）
- `../almide-references/koka/kklib/include/kklib/string.h`、`kklib.h`（表示和 thread-shared RC）
- `../almide-references/swift/stdlib/public/core/StringObject.swift`
- `research/benchmark/perf/string-gap-1004.md`
