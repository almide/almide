历史描述：C-ASSERTED 的首次实地测量。让 LLM 阅读 law 块（对所有输入成立的性质）的 MSR 实验、同样的 law 能有多少仅靠 Lean 自动 tactic 关闭的实验，以及逐函数写 law 能否无需数学技法就组装证明的实验。记录数字、发现、距离 Lean 级还缺什么，以及下一步。
<a id="law-ブロック実験--c-asserted-を測ってみた記録-2026-09-27"></a>

# law 块实验 — 测量 C-ASSERTED 的记录（2026-09-27）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](law-blocks-experiment.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](law-blocks-experiment.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](law-blocks-experiment.zh-CN.md)

> 未改动固定原本的非规范性历史读者快照。不自动同步，未独立重跑实验、证明或实现检查。Lean 内核已检查的主张针对手工转写的 Lean 函数，不直接针对 Almide 源码；翻译及定理语句忠实性保留原有较弱验证地位。实验日期、小样本、边界、事后追加和未完成下一步均保留，不提升为当前保证。§4 的修复循环价值是解释，而 §2 明言未观察到反例驱动修复成功，两者不暗中统一。后来的7/16及旧4/16、引理辅助6条各留在原节语境。历史复现命令和任务清单不是新执行指令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/law-blocks-experiment.md)

> **这是什么**：对 [behavioral-contract](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/behavioral-contract.md) 的 **C-ASSERTED**
> （机器提出简短性质，人只批准语句，性质限制在可判定/可自动化片段）
> 不做设计而做一次**实测**的记录。起点是“无需数学技法，用程序员的书写方式获得 Lean 的保证”，
> 由此构建 `law` 块，并测量三个问题。
>
> 1. 展示 law 能否让 LLM 的修改更正确（MSR 实验）
> 2. 机器能否独自证明这些 law（Lean 实验）
> 3. **逐函数写 law，能否无需数学技法（引理、强化归纳、tactic）就组装证明**
>    （§3.5——直接回答起点问题）
>
> **状态**：实验已完成，下一步未着手（§7）。产物在 develop 的 `research/` 下（PR #2775），
> 原始运行数据在仓库外 `almide-docs-archive/law-spec-runs-2026-09-27/`。
> **设计**：[law-proofs-design](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/law-proofs-design.md)（根据此实测形成的证明层设计）。
> **相关文档**：[behavioral-contract](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/behavioral-contract.md)（上层设计）、
> [compile-time-contracts](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/compile-time-contracts.md)（`where` 前置条件，不可判定时回落到运行时检查）。

---

<a id="1-設計スケッチ実験の前提"></a>

## 1. 设计草图（实验前提）

```
law "reverse twice is identity" for (xs: List[Int]) {
  list.reverse(list.reverse(xs)) == xs
}
```

- `law` 是形如**针对所有输入的测试**的性质。命题写成 Bool 表达式。
- LLM **只写 law（语句）**，不让它写证明主体、引理名或 tactic。
- 判定**靠计算而非声明**：发现反例则**错误** / 找到证明则 `proved` / 两者都没有则 `tested`。
  `trusted` 只在源码外台账中（不创建让 LLM 逃避的注解）。
- **先给反例**：失败时展示缩小后的最小反例，而非 `unsolved goals`。
- 原则：**只有存在错误证据才拒绝。仅仅无法证明则不拒绝，而是降级。**

MSR 角度的预测（实验前）：手写证明因引理名幻觉和修改脆弱性而明显负面；
只用 law + 反例 + 自动判定则偏正面。本文是对此预测的测量结果。

<a id="2-実験-1--msrgrammar-lab-law-spec"></a>

## 2. 实验 1 — MSR（Grammar Lab `law-spec`）

**配置**：6 个任务（rle / paginate / rotate / chunk / merge / top_k）。每题含“修改指示 +
可见测试 + 不可见 hidden 测试”。变体为 `tests`（仅测试）和 `laws`
（测试 + 作为阅读材料展示的 law，由评分侧固定附加）。第一次未过 gate
（可见测试，laws 变体再加 law）则返回一次输出让其修复。hidden 仅用于评分——
**gate 通过 ∧ hidden 失败 = “存活但实际错误”**。模型 Haiku 4.5，每格 3 次试验。

| | tests | laws |
|---|---|---|
| gate 通过（第一次）= 相当于当前 MSR | **18/18（100%）** | 11/18（61%） |
| hidden 正确（最终） | 14/18（78%） | 15/18（83%） |
| **存活但实际错误（最终）** | **4** | **0** |

**解读**：

- 最大发现不是 law，而是 **MSR 的测法**：用弱测试判定“存活”，tests 变体看起来 100%，实际 78%。
  存活者约两成错误。law 与其说提高 MSR，不如说是**让 MSR 测量正确东西的机制**。
- law 起效的典型（merge）：tests 变体只处理可见测试体现的“列表间重复”，hidden 全败；
  laws 变体读到“结果严格递增”，还处理了列表内重复、空输入。
  起效的是**作为阅读材料的 law**。返回反例再修复的效果**尚未观察到一次**：
  唯一收到反例的一例因语法错误失败。
- laws 变体第一次失败的 7 例中，只有 1 例被 law 反例捕获。其余是提示歧义
  （遗漏类型声明 2 例）、已知编译器 bug #2600（2 例）、语法错误（2 例）。
  tests 变体语法失败为 0。可能存在**展示 law 会促使尝试完整解法，从而增加失败机会**
  的代价（n=3 无法区分）。

**应打折看待的原因**：n 小（1 个模型 × 3 次试验）。law 和 hidden 由同一个人（实验者）编写，
针对相同边界情况，因此 laws 变体与 hidden 一致，部分本就是设计预期。

<a id="3-実験-2--leanlaw-を機械だけで閉じられるか"></a>

## 3. 实验 2 — Lean（仅靠机器能关闭 law 吗）

**配置**：将 6 题参照解**手工**转写到 Lean 4.29.1
（在 hidden 测试全部输入上以 `#eval` 确认等价），将 16 条 law 写成定理，不由人写证明来尝试。
T0 = 自动 tactic（`omega` / `decide` / `simp_all [defs]` / `grind [defs]`），
T1 = T0 + 机器可穷举的归纳（列表参数 `induction`、定义展开后 `fun_induction`）。

| 任务 | 仅机器关闭 | 未关闭 |
|---|---|---|
| paginate | 小于 1 的页为空 | 拼接所有页恢复原值 |
| rotate | 保持长度 / 一整圈为恒等 | 反向旋转恢复原值 |
| top_k | 不超过 k 个 | 按数量、名字排序 / 不遗漏前列项 |
| rle / chunk / merge | — | 全部（8 条） |

**16 条中 4 条（25%）**。关闭的都是展开定义就结束的浅层性质。需要归纳的即使 T1 也为 0 条。
**“证明由机器负责”不能开箱即用。**

**卡点不在证明，而在“引理的形状”**：law 陈述包装函数（`chunk`、`merge`），
但归纳作用于内部递归辅助函数（`restChunks`、`mergeAll`），需要泛化其参数的引理（强化归纳）。
人只写 3 条引理的**语句**后，3 条引理和 2 条 law（chunk 重构、非空）都由相同机器过程关闭。
人的工作不是证明，而是“哪个函数满足什么”的陈述，形式上就是 law 本身。

**定义时发现**：chunk 原代码（Almide）在 `n ≤ 0` 时无限循环。
Lean 拒绝不终止的定义，且终止性证明自身也未自动通过（需要 `List.length_drop` 提示）。
没有任何 law 或测试捕获这个漏洞。

<a id="35-実験-3--関数ごとの-law-で証明は組み上がるか-2026-09-28"></a>

## 3.5 实验 3 — 逐函数 law 能组装证明吗（2026-09-28）

**问题**：数学技法的实质不是词汇，而是强化归纳、组装引理、终止性证明和查找库事实（§3）。
若替换为“**逐函数写 law**（像逐函数写测试）”，机器能沿调用结构组装证明吗？

**纪律（实验前固定）**：人只写 law 的**语句**（包括辅助函数在内，每函数 1〜3 条）。
所有 law 共用一个机器过程：展开目标函数定义、使用已证明 law、固定 16 条“标准库 law”、
沿目标函数递归做归纳、自动 tactic（`grind` / `simp_all` / `omega`）。
按调用关系自下而上，只在上层使用已证明的 law。Lean 作为**测量仪器**
（不意味着提议机制使用 Lean；是此纪律能关闭多少的上限参考）。

| | 实验 2（只有任务 law） | 实验 3（增加逐函数 law） |
|---|---|---|
| 原 16 条中仅机器关闭 | 4 | **7**（固定列表加一条通用 law 则为 8） |
| 人写了什么 | 3 条引理语句（仅 chunk） | 辅助函数等的 23 条 law 语句。tactic 为 0 |

已证明的 19 条一起复检，确认不依赖 Lean 标准公理
（`propext` / `Classical.choice` / `Quot.sound`）之外的内容——没有未证明的洞（`sorry`）。

**组装例（top_k“不遗漏前列项”）**：实验 2 需要人工的部分，按 law 自底向上堆叠：
`keyLe` 是全序（“一方在前”“传递性”两条）→ `topK` 结果按此顺序排列 → 目标，
一次 `grind` 就关闭。每条正是实现 `Ord` 时考虑的性质。
**证明结构与调用结构同形**的假设在此成立。chunk 2 条、rotate 2 条、paginate 1 条也同形。

**未关闭的 9 条分为三类**：

| 类别 | 对应 law | 内容 |
|---|---|---|
| 以变量做除法/余数 | chunk“只有第一个短”/ rotate“反向旋转”/ paginate“全部页恢复原值” | `len % n`、`(k % n + n) % n`、`count × size`——除数、乘数是变量，在线性算术之外 |
| fold 累加值 | merge 的 3 条 | `dedup` 用 `foldl`，需要累加值的 law。改写递归后成员关系 law 关闭，但“严格递增”“fold 版 = 递归版”未关闭 |
| 数字格式 | rle 的 2 条 | `Nat.toDigits`（十进制字符串化）的性质 |

剩下一条（top_k“数量顺序、名字顺序”）在固定列表加入 `List.Pairwise.imp` 后关闭，
`mergeAll` 排序 law 也用 `List.pairwise_cons` 关闭——不足的是列表，而非纪律。
属于事后追加，未计入主统计（7/16）。

**启示**：结构部分（引理组装、强化归纳、证明顺序）被逐函数 law 堆叠替代。
剩余障碍是**算术**和 **fold**。算术可通过语言设计避开：把余数/分页计算作为带 law 的标准函数
（`rotate`、`chunks_of` 等）提供，不让用户代码写变量余数。
fold 本质困难，需选择让用户写累加值 law，或将 fold 限于带 law 的标准形式。

**应打折之处**：law 语句由实验者写，LLM 或普通程序员能否写出相同语句未确认。
固定列表只选择过一次。引擎是 Lean 的 `grind`，不保证自制证明器同样有力。
翻译（Almide → Lean）仍为手工（§5）。

<a id="4-発見の要約"></a>

## 4. 发现总结

1. **弱测试会虚增 MSR**（本次两成）。不论是否采用 law，都值得测量 Dojo MSR 有多少虚增。
2. **law 的价值要包括修复循环才体现**。一次作答测量中 law 基本吃亏。
3. **law 粒度对齐递归函数单位**，才能进入自动证明。只写在包装函数上无法关闭。
   让机器猜引理形状（lemma speculation）属研究级。
4. **全域性是语言问题**。只要接受不终止函数，就无法说“对所有输入成立”。
5. **逐函数 law 能组装证明的结构部分**（§3.5，7/16 → 纪律成败归结为“算术”和“fold”）。
   无需数学技法的定理证明骨架就是这种形状。
6. 副产物：#2771（未定义类型先出现误导性 E013，根因 E029 最后且无位置——
   LLM 修复循环只收到开头的误导）。

<a id="5-lean-相当に足りないもの重い順"></a>

## 5. 距离 Lean 级还缺什么（从重到轻）

本实验中**靠手工处理的部分**就是剩余课题：

1. **可信的 Almide → 逻辑翻译**（最大缺口）。本次证明针对手写转录的 Lean 函数，
   不是 Almide 代码。转写时三处改变语义（String 改列表 / chunk 的 `n ≤ 0` / Int 余数）。
   已有 `proofs/*.v` 是所有权片段的语义，尚无用户函数值语义。
   先例是 Rust → Lean 的 Aeneas（本身是一项研究）。
2. **全域性**（终止性检查）。
3. **数值语义一致**（Lean 的 `Int` 无限精度，Almide 为 64 bit）。
4. **无法关闭时的退路**。不让 LLM 写证明，所以无法关闭的 law 降为 `tested`。
   现实终点不是“等同 Lean 的语言”，而是“每函数有时有 Lean 级保证、有时没有，而且比例可见的语言”。
5. **小内核**。背后使用 Lean 可借用（相对便宜）。

<a id="検証としての信頼の鎖今回どこまでが-lean-級か"></a>

### 验证信任链（本次哪里达到 Lean 级）

```
Almide のソース
   │ ① 手で Lean に写す          確認は hidden テストの約 25 入力の #eval だけ(= テスト級)
Lean の定義
   │ ② law を手で定理に写す      未確認
Lean の定理
   │ ③ 証明                     カーネル検査済み(Lean 級)
```

只有③是 Lean 级，链条由最弱环决定。本次展示的不是“Almide 函数正确”，
而是“**转写的 Lean 函数**满足 4 条（含引理帮助则 6 条）性质”。
[`proofs/ALS.v`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/ALS.v) 对所有权片段提出的问题
（*you proved a MODEL — does the REAL emitted artifact correspond?*），在函数值语义上仍未回答。

- ①的弱点：三处有意语义修改，25 个输入无法捕获无意偏差。
- ②的弱点：例如“严格递增”用 `getD i 0`（范围外为 0）书写。
  漏写范围条件，默认值可能使**错误性质平凡为真**。
- ③数字（16 条中 4 条）是“零人工帮助的关闭率”，不是验证难度。
  熟悉 Lean 的人通常能证明全部 16 条。只作为不让 LLM 写证明的设计适用范围指标理解。

<a id="6-成果物"></a>

## 6. 产物

| 路径 | 内容 |
|---|---|
| `research/spike/law-blocks/law.almd` | law 运行时侧（纯 Almide）：固定 seed 生成器、缩小（向 0 单调）、反例展示 |
| `research/grammar-lab/` | 恢复 runner（ADR-0008 以后的语法）+ hidden 测试 / 固定 law / 修复轮 / 任务并行 |
| `research/grammar-lab/experiments/law-spec/` | 6 题、参照解、`validate.sh`（验证参照解全通过、原代码 hidden 失败） |
| `research/spike/law-blocks/lean/` | 手工转写定义、`laws.py`（机器独自证明扫描）、`Assisted.lean`（引理辅助）、`modular.py`（自底向上证明逐函数 law 的扫描，§3.5；运行生成 `Modular.lean`） |

测量陷阱（防再发）：
- `almide test` 为**每个测试文件绝对路径**保留约 12 MB 工作目录 7 天（#2608）。
  每次试验用不同路径，填满磁盘——runner 按每条 lane 固定路径评估。
- 缩小若非**向 0 单调**会循环（0 → 1 → 0）。law checker 自身也需要终止性。
- `fun_induction` 若 law 指向包装函数，会因“目标中没有调用”而**一次都不运行**，
  却被算为“未关闭”。从输出确认是否运行。

<a id="7-次の一手未着手順不同"></a>

## 7. 下一步（未着手、无顺序）

按低成本顺序收紧信任链（§5）的两项——不会达到 Lean 级，但让每一环至少达到测试级，
可明确弱环在哪（behavioral-contract §5“明确哪个来源的哪项主张”）：

- [ ] **① 翻译差分检查**：把 law 生成器输入（数百例）同时送入 Almide 函数和 Lean 转写
      （`#eval`），比较输出。①由“25 输入目视”提升为“生成输入差分检查”。
- [ ] **② 定理语句执行比对**：将 Lean 定理语句作为可判定 Bool 在相同输入求值，
      比较是否与 Almide 侧 law 判定一致。捕获误写（如因默认值平凡为真的形式）。

- [ ] **最小自动翻译**：在只含递归、列表、Int 的片段中，自动生成 Almide 函数 → Lean 定义，
      确认与本次手写 6 题是否相同。运行成功后才是“关于 Almide 代码的证明”。开始前决定语言特性范围。
- [x] **逐函数 law 扫描** → §3.5（7/16）。
- [ ] **标准函数能否避开算术障碍**：把余数/分页计算移到带 law 的标准函数版本，
      测量 chunk / rotate / paginate 三条能否关闭。
- [ ] **LLM 能否写 law 语句**：以 LLM 替代实验者编写的 23 条，同样纪律下有多少关闭。
      回答“人只写 law 语句”对 LLM 是否也成立。
- [ ] **用 develop 编译器重跑 MSR 实验**（修复 #2600 的版本。去除仪器导致的 4 例失败，
      只测反例反馈效果）。
- [ ] **测量 MSR 虚增**：给 Dojo 题加 hidden 测试，只测“存活但实际错误”率。

保留的日文信任链图说明：Almide 源码 → ①手工转写 Lean，仅以约 25 个 hidden 输入做 #eval 检查（测试级）→ Lean 定义 → ②手工把 law 写成定理（未确认）→ Lean 定理 → ③证明（已由内核检查，Lean 级）。
