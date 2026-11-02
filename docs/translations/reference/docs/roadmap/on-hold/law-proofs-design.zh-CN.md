说明：以 Lean 为 kernel 证明逐函数 law 的层设计。纯片段自动翻译、计算 proved/tested/refuted、Int 回绕与余数语义、终止性、翻译信任链、Lean 许可和商标、分阶段计划。
<a id="law-証明層--設計-draft-2026-09-28"></a>

# law 证明层 — 设计（草案，2026-09-28）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](law-proofs-design.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](law-proofs-design.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](law-proofs-design.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/law-proofs-design.md)

> 原文注记：9月28日未实现提案，不是已交付law命令或已证明compiler。即便Lean检查model证明，阶段1翻译与语句对应仍属有限测试级；开头Lean同等强度目标受§5限制。算术附加条件与Int64方案未决，Map/Set在阶段2或3引入的记载并存。节号跳过1。同名Result映射、数学及自动终止性说法保留为设计主张，不是独立验证翻译。6任务1模型、7/16、约两成是历史实验报告，不是新测量或普遍语言优势。许可/商标为当时原文摘要，非当前法律意见，捆绑需重新审查。未运行Lean、生成命令、模型调用或付费benchmark。


> **这是什么**：用**以 Lean 为 kernel 的证明**支持 [behavioral-contract](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/behavioral-contract.md) 的 **C-ASSERTED**（机器提出短性质，人只批准文字）的层设计。实测依据见 [law-blocks-experiment](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/law-blocks-experiment.md)。
>
> **一句话**：用户按函数写 `law`（对全部输入成立的 Bool 表达式性质）。编译器自动把纯函数及 law 翻为 Lean，沿调用关系自下而上，以固定机器程序尝试证明。闭合的 law 获得**与 Lean 同等强度的保证**；未闭合则退到生成输入检查（`tested`）。找到反例的 law 是**编译错误**。
>
> **状态**：仅设计，尚未实现。从 §9 阶段 0 开始。

---

<a id="0-先行例との関係と主張の範囲"></a>

## 0. 与先例的关系及主张范围

核心“用户补充引理（law）的**语句**，机器以归纳和重写找证明”并非新想法。

- **Boyer–Moore 证明器／ACL2**（1970 年代起）：用户在证明器卡住处补引理语句，机器以归纳和重写搜索证明，正是该方法论。实验 3（逐函数 law 达到 7/16）是在 Almide 场景复验。
- **沿调用关系组合逐函数契约**是 Dafny / Why3 / Liquid Haskell 的标准做法。
- **证明＝程序，归纳＝递归**是 Curry–Howard 对应（类型理论基础）。
- **让 LLM 写规范／引理，再由证明器检查**的研究，在 2024–25 年也有多项针对 Dafny / Verus。

此设计带入两点：(1) 假设编写者为 **LLM**，仅让其写 law 语句，将计算判定层级及先出反例纳入**语言表面**；(2) 用 **MSR** 测量。两者仍处于小实验（6 任务、1 模型）阶段。

**范围限定**：“证明与程序同形”的观点仅适用于**对归纳构造数据（如列表、自然数）的程序证明**。依赖排中律或选择公理（无对应程序）的证明，以及实数、连续性等无限对象证明，不属此层，非对全部数学的主张。

“Lean 等价”有三义，本层仅以其一为目标（[law-blocks-experiment](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/law-blocks-experiment.md) §5）。

| 含义 | 本层 |
|---|---|
| **保证强度**：关于 Almide 代码有 kernel 检查的证明 | **目标**（仅纯片段） |
| 表达力：能证明 Lean 可证明的一切 | 非目标（重现依赖类型理论和 Mathlib 等于重建 Lean） |
| 自动性：写 law 就一定找到证明 | 原理上不可能，只能不断提高天花板 |

**目标**

- G1. 用户仅写 law **语句**，不写 tactic、引理名、证明项，也不让 LLM 写。
- G2. 标为 `proved` 的 law 对**该 Almide 代码**成立，不因翻译错误给出假 `proved`（§5）。
- G3. 证明结构与代码调用结构一致，失败时能指出哪个函数缺 law。
- G4. 判定为**计算**而非声明。源码不设“此项已证明”栏。

**非目标**

- 含 `var`、effect fn、IO、closure capture 的代码证明（阶段 3 后再议）。
- Float 性质证明（IEEE 754 建模是另项研究）。
- 手写证明语言。未闭合 law 的退路是 `tested`，不是人写证明的接口。
- 自己的证明 kernel。最终检查交 Lean。

<a id="2-利用者から見た姿"></a>

## 2. 用户看到的形式

```almide
fn rest_chunks(xs: List[Int], n: Int) -> List[List[Int]] =
  if list.len(xs) == 0 or n <= 0 then []
  else [list.take(xs, n)] + rest_chunks(list.drop(xs, n), n)

law "rest_chunks rebuilds its input" for (xs: List[Int], n: Int where it >= 1) {
  list.flatten(rest_chunks(xs, n)) == xs
}

law "rest_chunks makes no empty chunk" for (xs: List[Int], n: Int) {
  rest_chunks(xs, n) |> list.all((c) => list.len(c) > 0)
}
```

- `law` 为与函数同位置的顶层声明，主体为 **Bool 表达式**。
- `for (...)` 是全称参数，`where` 是参数前置条件（仅可判定谓词）。
- 由主体调用函数推断所属函数；需明确时可写 `law rest_chunks "..."`（用于 §4.3 证明顺序）。

`almide check --laws` 输出示例：

```
proved   rest_chunks rebuilds its input          (by recursion on rest_chunks)
proved   rest_chunks makes no empty chunk        (by recursion on rest_chunks)
proved   chunk rebuilds its input                (from: rest_chunks rebuilds its input)
tested   only the first chunk may be short       (1000 cases, no counterexample)
         not proved: needs arithmetic with a variable divisor (len % n); see §6.1
error[E1xx]: law "unique keeps length" is false
  counterexample (shrunk in 7 steps):
    xs = [0, 0]
```

输出译文：rest_chunks 重建输入、rest_chunks 不产生空 chunk 均由对 rest_chunks 的递归证明；chunk 重建输入由前一性质推出。只有第一个 chunk 可能较短仅测试 1000 例，无反例；未证明原因是需要含变量除数的算术 len % n，见 §6.1。unique 保持长度的 law 为假，E1xx 错误，反例经 7 步缩小为 xs=[0,0]。

<a id="3-判定のレベル"></a>

## 3. 判定层级

| 层级 | 意义 | 计算条件 |
|---|---|---|
| `refuted` | 假，**编译错误** | 生成输入发现反例 |
| `proved` | 全输入成立（Lean kernel 检查，附 §5 链） | 翻译与证明闭合 |
| `tested` | 未找到反例，但未证明 | 不属于前两者 |
| `trusted` | 人声明负责 | 仅能写**源码外**台账（如 `proofs/law-trust.toml`） |

两原则：**仅有错误证据才拒绝**（仅证明不了则降级而非拒绝）。**层级是计算而非声明**（不设源码 `@proved` 栏，避免 LLM 躲入注释）。

`tested` → `proved` 升级和 `proved` → `tested` 降级（代码变更后未重新找到证明）均作为差异报告。CI 可采用“不减少 `proved` 数量”的 ratchet。

<a id="4-アーキテクチャ"></a>

## 4. 架构

```
.almd ─ parse ─ check ─┬─ law の型検査(Bool 式・決定可能な where)
                       │
                       ├─ [tested] 生成器 + 縮小(seed 固定)で反例探索 ── 反例 → refuted(エラー)
                       │
                       └─ [proved] 純粋断片の抽出 → Lean 翻訳 → 証明手順 → lean(外部プロセス)
                                     │                                   │
                                     └── キャッシュ(IR ハッシュ + law 文 + 手順版)←┘
```

图译文：.almd → parse → check 分到 law 类型检查（Bool 表达式、可判定 where）；tested 生成器与固定 seed 缩小搜索反例，反例 → refuted 错误；proved 纯片段提取 → Lean 翻译 → 证明程序 → 外部 lean 进程。缓存键为 IR hash + law 语句 + 程序版本。

<a id="41-生成と反例探索tested--refuted"></a>

### 4.1 生成及反例搜索（`tested` / `refuted`）

把 spike `research/spike/law-blocks/law.almd` 内置到编译器。

- 生成器**从类型导出**（Int / Bool / String / List / Option / Result / record / variant）。
- 将 `where` 解释为生成器约束（`it >= 1` → 有下限生成）；无法解释的谓词用拒绝采样。
- 缩小必须**单调趋向 0／空**，spike 曾发生循环。
- seed 从文件路径与 law 名确定性导出，同 law 每次检查相同输入。

<a id="42-翻訳almide-ir--lean"></a>

### 4.2 翻译（Almide IR → Lean）

对象为**纯片段**（§4.4）的函数及其 law。**从有类型 IR 而非源码**翻译（`IrProgram` 每个 node 有类型）。

- 函数 → Lean `def`，递归 → Lean 递归，`match` → `match`。
- record → `structure`，variant → `inductive`，`Option` / `Result` → Lean 同名类型。
- law → `theorem`，`for` 参数全称绑定，`where` 为假设，Bool 主体为 `= true` 命题。
- stdlib 函数 → Lean 模型映射表（§6.2）。

数的语义见 §5.2。

<a id="43-証明手順固定"></a>

### 4.3 证明程序（固定）

将实验 3 的纪律直接化为程序。

1. 按**目标函数**分组 law，沿调用关系逆拓扑（自下而上）排序。
2. 每项用相同固定程序：
   `grind [対象関数の定義, 証明済みの law, 標準ライブラリの law]` →
   `simp_all [...]` → 沿目标函数递归的 `fun_induction` → 对列表参数 `induction`。
   保留的日文 tactic 项分别是目标函数定义、已证明 law、stdlib law。
3. 上层仅可使用已证明 law，不假设未证明 law。
4. 未闭合时从失败形状推断并报告**缺 law 的位置**，例如归纳中无法描述 callee `f` 结果而停住 → “`f` 缺 law”。

程序内容（tactic 名、顺序、时间上限）作为**程序版本**管理并加入缓存键。变更后重新判定全部 law。

<a id="44-純粋な断片段階-1-の範囲"></a>

### 4.4 纯片段（阶段 1 范围）

| 包含 | 不包含 |
|---|---|
| 非 effect 的 `fn`、自递归／互递归 | `effect fn`、IO、`!` |
| `Int` / `Bool` / `String` / `List` / `Option` / `Result` / record / variant | `Float`、`Bytes`、`Map` / `Set`（阶段 2） |
| `match` / `if` / `let` / pipe / 无 capture lambda | `var`、`for`、`while`（阶段 3） |
| 有 Lean 模型的 stdlib 函数 | 无 Lean 模型的 stdlib 函数 |

调用片段外函数的 law 自动停在 `tested`，不尝试 `proved`。

<a id="45-キャッシュ"></a>

### 4.5 缓存

证明缓存为派生物，不放源码。键为“目标函数及传递 callee 的 IR hash + law 语句 + 程序版本 + Lean 版本”。与 Almide build cache 按 IR 键相同，改名／注释变更不重证。

<a id="5-信頼の鎖--偽の-proved-を出さないために"></a>

## 5. 信任链 — 避免假的 `proved`

实验证明的是“手抄模型”（[law-blocks-experiment](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/law-blocks-experiment.md) §5）。收紧此链是核心课题。

```
Almide の関数 ─①翻訳─ Lean の def ─②文の翻訳─ Lean の theorem ─③証明─ カーネル検査
```

图译文：Almide 函数 → ①翻译 → Lean def → ②语句翻译 → Lean theorem → ③证明 → kernel 检查。

<a id="51-各輪の締め方"></a>

### 5.1 收紧各环

| 环 | 阶段 1（便宜） | 阶段 2 后（更强） |
|---|---|---|
| ① 函数翻译 | **差分检查**：law 生成器输入（数百）同时经 Almide 执行和 Lean `#eval`，检查一致。不一致不给 `proved` | 相对于与 `proofs/ALS.v` 同系语义，对翻译正确性进行翻译验证 |
| ② 语句翻译 | **执行核对**：把 Lean 定理语句作为可判定 Bool 在相同输入求值，与 Almide law 判定比较 | 纳入与①相同框架 |
| ③ 证明 | Lean kernel | 同左；每次以 `#print axioms` 确认无 `sorryAx` |

阶段 1 的①②仅为**测试级**信任。因此诚实显示阶段 1 `proved`：“**在翻译通过差分检查的范围内**，已由 kernel 检查”（behavioral-contract §5：明确何来源的何主张）。

<a id="52-数の意味--実測で確認した二つの落とし穴"></a>

### 5.2 数的语义 — 实测确认的两个陷阱

Almide 0.64.0 实测（2026-09-28）：

| 表达式 | Almide | Lean `Int` |
|---|---|---|
| `int.max_value() + 1` | `-9223372036854775808`（**静默回绕**） | 无限精度，不回绕 |
| `(0 - 7) % 3` | `-1`（**截断除法余数**） | `2`（非负余数，`Int.emod`） |

直接将 `Int` 映为 Lean `Int`，会给**Lean 为真、Almide 为假**的 law 标 `proved`（如 `x + 1 > x` 在 Lean `Int` 为真，在 Almide `max_value` 为假），违反 G2。

**方针**：

- `%`、`/` 映到 Lean **截断版** `Int.tmod` / `Int.tdiv`，不能混淆。已确认 `Int.tmod (-7) 3 = -1`、`Int.tdiv (-7) 3 = -2` 与 Almide `(0 - 7) % 3`、`(0 - 7) / 3` 一致，普通 `(-7 : Int) % 3` 为 `2`，不一致（Lean 4.29.1）。
- `Int` 分两层处理：
  1. **先在无限精度 `Int` 上尝试证明**，自动化强。
  2. 同时为翻译函数的每个算术生成并证明“**此运算不溢出**”附加条件。
  3. 全部附加条件闭合才 `proved`，否则退到 `tested`。
- 另一方案：将 `Int` 映为 Lean `Int64`（`BitVec 64` 上回绕算术），用 `bv_decide`（SAT，核心 `Std.Tactic.BVDecide`，无需 Mathlib）。已确认 `Int64` 的 `max + 1` 与 Almide 同为 `-9223372036854775808`。固定宽算术更强，与列表归纳组合尚未知。阶段 1 小规模比较后决定（未决 Q1）。

<a id="53-停止性"></a>

### 5.3 终止性

Lean 不接受不终止定义。翻译函数未通过 Lean 终止性检查，依赖它的 law 不能 `proved`。

- 结构递归（如对列表的尾部递归）自动通过。
- 其余交 Lean 自动推断；不通过则诊断问“什么在减少？”用户仅能写 `shrinks len(xs)` 这样短声明，证明由机器做。
- 实验中，检查在**定义时**发现 `chunk` 对 `n <= 0` 无限循环（测试和 law 均未捕获）。终止性失败本身值得警告。

<a id="6-天井を上げる仕組み"></a>

## 6. 提高天花板的机制

实验 3 未闭合 9 项分算术、fold、数字格式三类。

<a id="61-算術--標準関数に閉じ込める"></a>

### 6.1 算术 — 封装进标准函数

变量除法、余数、乘法（非线性整数算术）一般不可判定。通过不让用户在代码直接写来规避。

- 提供 `rotate(xs, k)`、`chunks_of(xs, n)`、`page(xs, i, size)` 等**带 law 标准函数**，law 在 stdlib 侧（包括人工）只证明一次。
- 用户 law 用标准函数 law 组合，无需碰余数算术。
- 直接对变量用 `%` / `*` 的函数 law 不闭合时，诊断指出对应标准函数。

<a id="62-標準ライブラリの-law"></a>

### 6.2 stdlib 的 law

标准函数配套 **Lean 模型 + law + 证明**。

- Lean 模型：用 Lean 写标准函数语义，为 §5 差分检查对象。
- law：用户证明可用性质，如 `list.take(xs, n) + list.drop(xs, n) == xs`。
- 同时也是可读文档的 law，刊于 `docs/stdlib/`。

实验中仅因固定列表缺一个一般 law 而未闭合的有 2 例（`Pairwise.imp`、`pairwise_cons`）。stdlib law 覆盖直接影响天花板。像 API family 覆盖一样，设**矩阵覆盖检查 gate**（CLAUDE.md API family 规则）。

<a id="63-畳み込み--不変条件の候補を機械が出す"></a>

### 6.3 fold — 机器提出不变量候选

`foldl` 中途累积值的不变量不写在任何代码位置。选项：

1. 让用户写累积值 law，如 `law for fold in dedup ...`。
2. 将 fold 靠拢带 law 标准形，如 `list.dedup_sorted`。
3. **机器出候选**：从执行记录（生成输入上 fold 每步状态）学习不变量候选，证明器检查。`almide-graphics/nn` 的可微逻辑门网络（训练后可导出逻辑电路）可能学习原子谓词组合。候选错只会被证明器拒绝，不损健全性。

阶段 2 小试选项 3，能否闭合 `dedup` 的“严格单调增加”。

<a id="7-lean-の利用条件"></a>

## 7. Lean 使用条件

2026-09-28 依据一手资料确认。

<a id="71-ライセンス"></a>

### 7.1 许可

- Lean 4 本体：**Apache License 2.0**（[LICENSE](https://github.com/leanprover/lean4/blob/master/LICENSE)）。
- Mathlib：Apache License 2.0（本层假定不依赖 Mathlib）。
- Lean 分发含第三方组件（[LICENSES](https://github.com/leanprover/lean4/blob/master/LICENSES)）：LLVM（Apache 2.0 with LLVM Exceptions）、**GNU C Library（LGPL 2.1）**、**GNU MP（LGPL 3）**、CaDiCaL（MIT）等。

应用到本层：

| 用法 | 条件 |
|---|---|
| Almide 作为**外部进程调用 Lean**（用户经 elan 装官方 toolchain） | Apache 2.0 范围内无问题，Almide 侧不产生再分发义务 |
| Almide 生成 `.lean` 文件 | Almide 输出而非 Lean 代码，可按 Almide 侧许可处理 |
| Almide 分发**捆绑 Lean toolchain** | 除 Apache 2.0 告知义务（含 LICENSE / NOTICE），还有 **LGPL 组件（glibc、GMP）义务**，如提供源码获得方式 |

**方针**：阶段 1–2 **不捆绑**，用户经 elan 装官方 toolchain，以 `lean-toolchain` 固定版本（repo 现有 Lean 项目为 `leanprover/lean4:v4.29.1`）。需要捆绑时另行确认 LGPL 处理。

<a id="72-商標"></a>

### 7.2 商标

“Lean”名称和 logo 是 Lean FRO 商标（[Lean Trademark Policy](https://lean-lang.org/trademark-policy/)）。

- **无需许可可用**：准确描述与软件关系，例如“兼容 Lean programming language and proof assistant”“使用 Lean”，商业非商业均可；文档论文提及。
- **需许可**：在其他商标包含 Lean、由 Lean logo 派生 logo、将大幅修改版以 Lean 名义分发。
- 用作**形容词并搭配通用名词**，如“Lean proof assistant”。

**方针**：功能名、命令名、文档标题不把 Lean 当**产品名**纳入（不采用“Almide Lean”“Lean for programmers”）。说明“用 Lean proof assistant 检查证明”无问题。功能名用 `law` / `almide check --laws` 等不含 Lean 的名字。

（本节为一手资料摘要，非法律建议。决定捆绑或商业分发形态时重新确认。）

<a id="8-msr-と-llm-の観点"></a>

## 8. MSR 与 LLM 视角

- LLM 只写 law 语句（G1），从开始排除幻觉引理名及不耐修改的手写证明。
- 反例直接服务修复循环：已缩小、带参数名。
- 实验 1 发现：弱测试判定的 MSR 虚高约两成。law 判定（`refuted`）去除虚高，MSR 可能看似下降；引入时同时报告“存活但其实错误”比例。
- law 自身改写（弱化 law 以去反例）与代码变更区分显示。
- 未验证前提：**LLM 能否写出可闭合的 law，特别是内层函数 law？** 纳入阶段 1 出口条件（§9）。

<a id="9-段階計画"></a>

## 9. 分阶段计划

| 阶段 | 内容 | 出口条件（测量项） |
|---|---|---|
| 0. 脚手架 | 编译器内置 `law` 语法、类型检查、生成器和缩小；仅 `tested` / `refuted` | spec 测试的 `refuted` 输出缩小反例。用 develop 编译器重跑轻量 MSR（Grammar Lab `law-spec`） |
| 1. 翻译 | 纯 IR → Lean、差分①和执行核对②、固定程序、缓存 | **自动翻译**复现实验 6 任务，与手抄版相同 7/16 为 `proved`；差分捕获故意破坏的翻译 |
| 1.5 LLM | LLM 写 law 语句 | LLM 逐函数 law 多少能到 `proved`，与实验者写的 7/16 比 |
| 2. 天花板 | 带 law 标准函数（§6.1–6.2）、`Int` 附加条件（§5.2）、fold 候选试验（§6.3） | 算术 3 项（chunk / rotate / paginate）经标准函数闭合；候选生成找到 `dedup` 不变量 |
| 3. 扩展 | 含 `var` / `for` 函数、`Map` / `Set`、翻译形式验证 | 另行设计 |

<a id="10-未決事項"></a>

## 10. 未决事项

- **Q1**：`Int` 用无限精度 + 附加条件，或 `Int64`（`BitVec`）（§5.2）？阶段 1 比较。
- **Q2**：`law` 放置：紧随函数，或 `*_laws.almd` 独立文件？测 LLM 易写性及 edit-locality。
- **Q3**：`where` 谓词范围限可判定片段（线性算术和长度）吗？是否与 [compile-time-contracts](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/compile-time-contracts.md) 的 `where` 共用语法？
- **Q4**：无 Lean 环境（部分 CI、playground、wasm）行为：降到 `tested` 继续，还是必须复现 `proved`？
- **Q5**：程序版本升级使某些 `proved` 降到 `tested`，是否视为回退？
- **Q6**：与 ALS 台账（als repo）关系：law 判定层级是否列入契约台账证据类别？

<a id="11-参照"></a>

## 11. 参考

- 实测：[law-blocks-experiment](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/law-blocks-experiment.md)（MSR 实验、Lean 自动证明、逐函数 law）
- 上层设计：[behavioral-contract](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/behavioral-contract.md)（C-ASSERTED）
- 前置条件：[compile-time-contracts](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/compile-time-contracts.md)
- 所有权片段语义及翻译验证：`proofs/ALS.v`
- 先例：Boyer–Moore / ACL2（补引理语句，机器找证明），Dafny / Why3 / Liquid Haskell（逐函数契约模块验证），Aeneas（Rust → Lean）、hs-to-coq（Haskell → Coq）、CLN2INV（以连续松弛逻辑学习 loop 不变量）
- Lean：[LICENSE](https://github.com/leanprover/lean4/blob/master/LICENSE)、[LICENSES](https://github.com/leanprover/lean4/blob/master/LICENSES)、[Trademark Policy](https://lean-lang.org/trademark-policy/)
