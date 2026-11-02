<a id="grammar-lab-lambda-syntax-実験レポート"></a>

# Grammar Lab：Lambda 语法实验报告

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](REPORT.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](REPORT.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](REPORT.zh-CN.md)

> 非规范性、固定原文的历史读者快照。未修改原本仍为权威来源；不自动同步，未独立运行或重新测量。当时的应对方法和命令不是当前指示。代码注释保持原样。LESSONS 开头明确更正了旧 auto-unwrap、API 名称和 argv[0] 假设，不应把后文旧建议作为当前 API 指南。REPORT 的等同／显著性主张、一般建议及 bug 状态属于原作者当时的解释，不是普遍保证。保留重复的第5节、四次改进与三行表格的差异、Sonnet 规则效果与运行表的叙述差异，以及后文 t05 难度／模式枚举描述，不擅自统一。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/grammar-lab/REPORT.md)

<a id="実験概要"></a>

## 实验概况

**假设：** 简写 lambda `(x) => expr` 与当前语法 `fn(x) => expr` 具有相同的修改生存率。

**方法：** 让 LLM 以两种语法变体执行同样的修改任务，比较 compile + test 通过率。

**特点：** Almide 尚未实现 paren-lambda。先把 LLM 输出转译为当前语法，再 compile + test（无需实现语法即可实验）。

---

<a id="結果"></a>

## 结果

<a id="最終結果"></a>

### 最终结果

| 模型 | fn-lambda | paren-lambda | p 值 |
|-------|-----------|-------------|---------|
| claude-sonnet-4-6 | **100%** (25/25) | **100%** (25/25) | 1.0 |
| claude-haiku-4-5 | **86%** (26/30) | **86%** (26/30) | 1.0 |

**结论：无差异。** 引入 paren-lambda 不影响生存率。

<a id="改善の推移"></a>

### 改进过程

| 运行 | 变更 | Sonnet fn/paren | Haiku fn/paren |
|-----|------|-----------------|----------------|
| 1 | 首次 | 100% / 93% | — |
| 2 | 修复测试类型重复定义 | 100% / 100% | — |
| 3 | 在 Layer1 加入 `if then` 规则 | — | 80% / 72% |
| 4 | 去除 test block + 改善 prompt | 100% / 100% | 88% / 96% |
| 5 | 转译 bug 修复版（模式枚举） | — | 90% / 26% ← 转译 bug |
| 6 | 转译通用化（括号匹配） | — | **86% / 86%** ← 正确结果 |

<a id="タスク別-breakdown-haiku-run-6-10-tasks--3-trials"></a>

### 各任务明细（Haiku，Run 6，10任务 × 3次试验）

| 任务 | fn-lambda | paren-lambda |
|------|-----------|-------------|
| t01（修改 filter 条件） | 100% | 100% |
| t02（增加 map step） | 100% | 100% |
| t03（修改 sort key） | 66% | 100% |
| t04（增加 fold 函数） | 100% | 100% |
| t05（修改复合 pipe） | 100% | 100% |
| t06（提取 lambda） | 100% | 100% |
| t07（复合转换） | 0% | 66% |
| t08（Option handling） | 100% | 100% |
| t09（修改类型） | 100% | 0% |
| t10（增加 variant） | 100% | 100% |

t07 和 t09 的变体间存在差异，但方向相反（t07 有利于 paren，t09 有利于 fn）。总计完全相同。

---

<a id="知見"></a>

## 发现

<a id="1-paren-lambda-は-fn-lambda-と完全に同等"></a>

### 1. paren-lambda 与 fn-lambda 完全等同

最终结果（转译通用化后）：fn 86% = paren 86%，p=1.0。没有统计显著差异。

**废除 `fn(x) => expr`、统一为 `(x) => expr`，生存率也不会改变。** 基于此结果，决定废除 `fn` lambda 语法。

<a id="2-実験基盤の品質が結果を支配する"></a>

### 2. 实验基础设施质量主导结果

四次改进中，三次并非 LLM 问题，而是**实验基础设施 bug**：

| 运行 | 原因 | 是 LLM 的错吗 |
|-----|------|------------|
| 1→2 | 测试文件类型重复定义 | ❌ runner 的 bug |
| 2→3 | 缺少 `if then` 规则 | △ Layer 1 不足 |
| 3→4 | LLM 输出 test block → 重复定义 | ❌ runner 没有去除 |

**经验：要得到可信实验数据，必须先提高基础设施质量。**

<a id="3-layer-1-のルール-1-行が直接効く"></a>

### 3. Layer 1 的一行规则直接生效

仅增加一行 `if` 语法规则，Sonnet 的失败就消失了。应以 LLM 不知道 Almide 为前提，准确传达容易犯错的要点。

<a id="4-トランスパイルで未実装構文を事前テストできる"></a>

### 4. 用转译可提前测试未实现语法

Almide parser 尚未实现 paren-lambda，但通过 `(x) =>` → `fn(x) =>` 文本转换即可通过 compile + test。**在实现语法之前测量生存率，可作为决定是否实现的依据。**

<a id="5-transpile-の品質が結果を支配する"></a>

### 5. 转译质量主导结果

Run 5 中 paren-lambda 降至26%是转译 bug。枚举模式的转译很脆弱，无法覆盖 LLM 使用的全部变量名。Run 6 改为通用括号匹配算法后恢复到86%。

**经验：应怀疑转译质量。LLM 结果不好时，首先检查转译 bug。** 已加入把 LLM 原始输出保存到 `outputs/` 的机制，以便区分原因。

<a id="5-弱いモデルで差が出る"></a>

### 5. 较弱模型能显现差异

Sonnet 在所有任务上均100%，达到天花板。使用 Haiku 才首次看到差异。**评估语法设计应使用较弱模型作为基准。**

---

<a id="実験設計の反省"></a>

## 对实验设计的反思

<a id="うまくいったこと"></a>

### 有效之处

- **三层 prompt 设计**（Layer 1：规则，Layer 2：示例，Layer 3：任务提示）——基于研究的设计奏效
- **Claude Code provider**——无需管理 API key 即可运行实验
- **转译**——让未实现语法可被测试
- **用 Almide 编写 runner**——发挥 dogfooding 作用，也发现了语言问题

<a id="改善が必要なこと"></a>

### 需要改进之处

- **N 不足**——25难以得出统计显著差异，需要30+
- **任务难度不均衡**——t01-t04 太简单，达到100%，只有 t05 困难
- **t05 指令含糊**——任务设计质量直接反映在结果中
- **转译基于字符串**——无法使用 regex 反向引用，故采用模式枚举；每次新语法实验都需编写转译规则

---

<a id="次のステップ"></a>

## 下一步

1. **改善 t05 指令 + 添加任务**——平衡难度
2. **以 N=30 重跑**——检验统计显著差异
3. **其他语法实验**——template keyword、builder emit/embed、UFCS 等也可使用同一基础设施
4. **支持 regex 反向引用**——提高转译通用性
5. **自动分析结果**——Fisher's exact test、错误分类自动化

---

<a id="技術的な副産物"></a>

## 技术副产物

Grammar Lab 开发过程中发现了 Almide 编译器的三个问题：

1. **effect fn main 的 Err 静默退出** → 已修复（加入 `eprintln!`）
2. **嵌套循环中的 ownership move** → codegen bug，未修复
3. **process.exec_with_stdin 缺少 `?`** → stdlib TOML bug，未修复

若没有 Grammar Lab 就不会发现这些问题。这就是 dogfooding 的价值。
