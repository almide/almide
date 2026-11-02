# Grammar Lab

[English](./README.md) | [日本語](./README.ja.md) | [简体中文](./README.zh-CN.md)

用于测量语法选择如何影响 LLM modification survival rate 的 A/B 测试框架。

## 快速开始

```bash
cd research/grammar-lab

# Set paths (required if not in PATH)
export ALMIDE_BIN=/path/to/almide          # or add almide to PATH
export CLAUDE_BIN=/path/to/claude          # optional, auto-detected via `which`

# Build the runner
almide build src/mod.almd -o /tmp/grammar-lab-bin

# Run the current-schema experiment (configured models, 3 trials; invokes the model)
/tmp/grammar-lab-bin experiments/law-spec/

# Run with specific model
/tmp/grammar-lab-bin experiments/law-spec/ --model claude-haiku-4-5

# Override trial count
/tmp/grammar-lab-bin experiments/law-spec/ --trials 1
```

### 路径解析

按以下顺序解析二进制程序路径：
1. 环境变量（`ALMIDE_BIN`、`CLAUDE_BIN`）
2. `which` 查找（如果二进制程序位于 PATH 中）
3. 回退路径（相对路径 `../../target/release/almide`）

## 当前 schema 与历史实验

`src/mod.almd::load_task` 读取每个任务的 `source_dir` 和 `test_file`，以及可选的 `hidden_file`、`laws_file`、`laws_compiled_file`。变体可设置 `with_laws`，实验可以请求一轮修复。`law-spec` 提供此 schema。下方的 `lambda-syntax` 属于历史实验：它的旧任务 JSON 和 `paren-to-fn-lambda` 实验不能直接用作当前编译器的快速入门。运行器本身可以通过 `almide check src/mod.almd` 检查，无需调用模型。

## 历史 lambda-syntax 布局

```
grammar-lab/
├── src/mod.almd                    # Runner (Almide)
├── prompts/
│   ├── layer1_rules.md             # Common rules (~200 tok)
│   ├── layer2_variant_fn.md        # Examples for fn(x) => variant
│   └── layer2_variant_paren.md     # Examples for (x) => variant
├── experiments/
│   └── lambda-syntax/
│       ├── experiment.json         # Experiment config
│       ├── variant-fn/             # Source files using fn(x) =>
│       ├── variant-paren/          # Source files using (x) =>
│       └── tasks/                  # Task definitions + test files
└── results/                        # Output JSON
```

## 提示词设计

基于研究的三层体系：

| 层 | 内容 | 随什么变化 |
|-------|---------|-----------|
| Layer 1 | Almide 规则（常见错误） | 不变（共享） |
| Layer 2 | 示例代码（3 个函数） | 变体 |
| Layer 3 | 任务专用的函数提示 | 任务 |

设计参考：
- “一个好例子胜过五个形容词”（Anthropic）
- Grammar Prompting（NeurIPS 2023）：每个任务的最小语法
- Few-shot 的适宜范围：2-5 个示例
- 系统提示词的适宜范围：150-300 个单词

## 输出

结果为 JSON 数组：

```json
[
  {
    "experiment": "law-spec",
    "variant": "laws",
    "model": "claude-haiku-4-5",
    "task": "t01_rle",
    "trial": 1,
    "compiled_1": true,
    "gate_1": true,
    "hidden_1": true,
    "repaired": false,
    "gate_2": true,
    "hidden_2": true
  }
]
```

通过可见测试／law 关卡但未通过隐藏测试时，会报告为 "survived but wrong"；仅通过关卡不代表正确。上例展示当前输出 schema，并非一次实测结果。

## 添加实验

1. 创建 `experiments/<name>/experiment.json`。
2. 创建包含 `.almd` 源文件的变体目录。
3. 参照 `law-spec/tasks/`，创建包含 `source_dir`、`test_file` 及可选 hidden/law 路径的 `tasks/` JSON；源码路径相对于实验目录，test/law 路径相对于 `tasks/`。
4. 添加变体专用的 `prompts/layer2_<variant>.md`。
5. 运行：`almide run src/mod.almd -- experiments/<name>/`
