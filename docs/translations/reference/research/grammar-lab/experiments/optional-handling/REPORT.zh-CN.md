<a id="experiment-optional-handling"></a>

# 实验：optional-handling

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](REPORT.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](REPORT.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](REPORT.zh-CN.md)

> 固定原文的非规范性历史阅读资料。下述测试、分支状态、测量及修复均为带日期报告，不是新执行的验证或普遍正确性保证。保留原代码，未运行基准测试。关于显著性、?. 语法和其他模型的结论是原作者的解释，并非由此小样本独立确立。不自动同步，以原本为准。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/grammar-lab/experiments/optional-handling/REPORT.md)

<a id="hypothesis"></a>

## 假设

组合子式 Option 处理（`option.map` + `option.unwrap_or` 管道链）的修改生存率高于 match 式处理（`match some/none`）。

<a id="setup"></a>

## 设置

- **模型**：Claude Haiku 4.5
- **试验次数**：每任务、每变体3次
- **任务**：10
- **总运行次数**：60（10任务 × 2变体 × 3次试验）

<a id="variants"></a>

### 变体

| | match | 组合子 |
|---|---|---|
| Option 分解 | `match opt { some(x) => ..., none => ... }` | `opt \|> option.map((x) => ...) \|> option.unwrap_or(default)` |
| 转译 | 无 | `option.map` → `match`，`option.unwrap_or` → `match` |
| Layer2 提示 | match 式示例 | 组合子式示例 |

<a id="results"></a>

## 结果

| 变体 | 通过 | 比率 |
|---------|------|------|
| **match** | **21/30** | **70%** |
| **组合子** | **12/30** | **40%** |

<a id="per-task-breakdown"></a>

### 各任务明细

| 任务 | match | 组合子 | 说明 |
|------|-------|-----------|-------|
| t01 change_default | 3/3 | 3/3 | 两者均通过 |
| t02 add_format | 3/3 | 0/3（编译） | 转译缺陷：`option.map` 体内字符串插值 `${}` 破坏正则匹配 |
| t03 add_function | 3/3 | 3/3 | 两者均通过 |
| t04 apply_discount | 3/3 | 3/3 | 两者均通过 |
| t05 chain_fields | 3/3 | 0/3（编译） | 转译缺陷：`option.map` 体内 `int.to_string()` 调用破坏正则匹配 |
| t06 change_predicate | 3/3 | 0/3（编译） | 转译缺陷：`option.map` 体内 `int.to_string()` 调用破坏正则匹配 |
| t07 change_none_msg | 0/3 | 0/3 | 两者均失败——LLM 难以处理 none 分支消息中的 `target` 变量捕获 |
| t08 sort_direction | 0/3 | 0/3 | 两者均失败——LLM 未正确删除 `list.reverse()`，或更改了函数名 |
| t09 add_condition | 3/3 | 3/3 | 两者均通过 |
| t10 add_step | 0/3 | 0/3（编译） | 两者均失败——LLM 难以创建多步管道 |

<a id="adjusted-results-excluding-transpile-bug-tasks-t02-t05-t06"></a>

### 调整后的结果（排除受转译缺陷影响的 t02、t05、t06）

| 变体 | 通过 | 比率 |
|---------|------|------|
| **match** | **12/21** | **57%** |
| **组合子** | **12/21** | **57%** |

**完全相同。** 转译基础设施正常运行时，match 与组合子的生存率相同。

<a id="conclusion"></a>

## 结论

**match 式与组合子式 Option 处理之间没有统计显著差异。**

原始70%与40%的差异完全可由转译基础设施缺陷解释（正则表达式无法处理字符串插值中嵌套的括号）。排除这些任务后，两种变体均达到相同的57%生存率。

<a id="implications-for-almide-design"></a>

### 对 Almide 设计的启示

1. **Match 已足够。** 在标准库中加入 `option.map` / `option.flat_map` 不会改善 LLM 准确率。
2. **`?.` 语法也无助益。** 如果组合子不能优于 match，组合子的语法糖（`?.` / `??`）也不会。
3. **任务难度起主导作用。** 差异在任务之间（简单：100%，困难：0%），而非风格之间。改善 LLM 准确率需要更好的错误消息、示例和语言规则，而不是改变语法。

<a id="caveats"></a>

### 注意事项

- 每个单元 N=3 次试验太少，应以 N=5以上复现。
- 转译缺陷影响了组合子侧3/10的任务。标准库中正确实现 `option.map` 可消除此混杂因素。
- 仅测试 Haiku。Sonnet 可能出现天花板效应（两者均100%）。
- 失败任务（t07、t08、t10）可能是任务设计问题，而非 LLM 能力问题。

<a id="lessons"></a>

## 经验

1. **转译质量主导组合子结果**——与 lambda-syntax 实验教训相同。基于正则的转译无法处理字符串中的嵌套括号。
2. **Layer 1 规则比语法风格更重要**——两种变体在相同任务上成功或失败，说明语言规则提示可能是关键因素。
3. **Haiku 适合用作基准测试模型**——总体70%的比率呈现有意义的差异。Sonnet 很可能两者都达到90%以上。
