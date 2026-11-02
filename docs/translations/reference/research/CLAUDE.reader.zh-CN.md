<a id="almide-research-laboratory"></a>

# Almide 研究实验室

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](CLAUDE.reader.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/CLAUDE.md)（固定提交 35297a612fb29e3f78f6587aba253aa91d51f466）的非规范、不激活的阅读快照。这是历史参考资料，不是有效的代理指令或用于生成的项目模板，不授予运行命令的权限，也不自动同步。原文主张不是独立的当前测量；未执行命令或调用模型。

用实验数据支撑 Almide 设计决策的研究基础设施。

**原则：设计决策依据“数字”，而非“直觉”。** 新语法提案先在 Grammar Lab 中实验，再决定是否采用。

<a id="beyond-rust-lab-beyond-rust"></a>

## Beyond-Rust Lab（`beyond-rust/`）

在仍通过 rustc 与 LLVM 的前提下，通过实验寻找生成比手写 Rust 更快的代码的方法。
一次实验 = 一个（内核、杠杆）的四列测量（手写 Rust／当前生成代码／手动施加杠杆后的生成代码／专家 Rust）。
实现前先手动施加并测量，只有满足毕业条件的杠杆才交给编译器处理。方法与结果见 [beyond-rust/README.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/beyond-rust/README.md)。

<a id="grammar-lab-grammar-lab"></a>

## Grammar Lab（`grammar-lab/`）

语法变体的 A/B 测试框架。面向多个 LLM，定量测量语法差异对 modification survival rate 的影响。工具自身用 Almide 实现（dogfooding）。

<a id="構成"></a>

### 结构

```
grammar-lab/
├── src/mod.almd              Runner (Almide)
├── prompts/                  Prompt テンプレート (3 層設計)
│   ├── layer1_rules.md       共通ルール (~200 tok)
│   └── layer2_*.md           Variant 別の例示コード
├── experiments/              実験定義
│   └── lambda-syntax/        最初の実験
│       ├── experiment.json   実験設定
│       ├── variant-fn/       fn(x) => 版のソースコード
│       ├── variant-paren/    (x) => 版のソースコード
│       └── tasks/            タスク定義 + テスト
├── results/                  実験結果 (JSON)
├── outputs/                  LLM の生出力 (デバッグ用)
├── REPORT.md                 実験レポート
└── LESSONS.md                開発で学んだ知見
```

目录树注释依次说明：Almide runner、三层设计的提示词模板、通用规则（约 200 tok）、各变体的示例代码、实验定义、首个 lambda-syntax 实验及其配置、fn(x) => 与 (x) => 两版源码、任务定义与测试、JSON 结果、用于调试的 LLM 原始输出、实验报告和开发经验。

<a id="使い方"></a>

### 用法

```bash
cd research/grammar-lab
export ALMIDE_BIN=/path/to/almide
almide build src/mod.almd -o /tmp/grammar-lab-bin
/tmp/grammar-lab-bin experiments/law-spec/ --trials 3 --model claude-haiku-4-5
```

当前 runner 读取任务的 `source_dir`、visible/hidden tests、可选 laws 和 repair round。`lambda-syntax` 是保留旧实验配置的资料，并不是直接适配当前 runner schema 的 quick start。新实验应参考 `law-spec` 配置。运行实验会调用模型。

<a id="主な機能"></a>

### 主要功能

- **Claude Code provider**：无需 API key，通过 `claude -p` 调用 LLM
- **Transpile**：将尚未实现的语法转换为当前语法再编译，可以在实现语法之前测量 survival rate
- **Fisher's exact test**：用 Almide 实现，输出带 p 值的 summary
- **保存原始输出**：在 `outputs/` 保存 LLM 输出，可区分 transpile bug 与 LLM 错误

<a id="歴史的実験結果lambda-syntax"></a>

### 历史实验结果（lambda-syntax）

| 实验 | Variant A | Variant B | Sonnet 4.6 | Haiku 4.5 | p-value | 结论 |
|------|-----------|-----------|------------|-----------|---------|------|
| Lambda syntax | `fn(x) =>` | `(x) =>` | 100% / 100% | 86% / 86% | 1.0 | **无差异。决定移除 `fn`** |

<a id="実験設計の学び"></a>

### 实验设计经验

- **使用 Haiku 做基准** — Sonnet 触及天花板，较弱的模型更能显示差异
- **先确认 transpile 质量** — transpile bug 会主导结果；保留原始输出来区分原因
- **先消除实验基础设施 bug** — 在观察数据前，排除重复类型定义、重复 test block 等基础设施导致的失败
- **Layer 1 规则直接生效** — 加入一行 `if then` 规则便消除了失败

<a id="次にやるべき実験"></a>

### 接下来应做的实验

| 实验 | Variant A | Variant B | 目的 |
|------|-----------|-----------|------|
| Template keyword | `template name(...)` | `fn name(...) -> HtmlDoc` | `template` 关键字能否提高 survival rate？ |
| Builder insertion | keyword (`emit`/`embed`) | type dispatch (bare expr) | 没有关键字时是否同等？ |
| UFCS | `web.param(req, "id")` | `req.param("id")` | 方法风格能否提高答对率？ |
| `println` | `io.print(s ++ "\n")` | `println(s)` | 简短名称能否提高 survival rate？ |

<a id="新しい実験を追加するには"></a>

### 添加新实验

1. 创建 `experiments/<name>/experiment.json`
2. 在 variant 目录中放置源码
3. 在 `tasks/` 的 JSON 中指定 `source_dir` / `test_file`，按需提供 `hidden_file` / `laws_file` / `laws_compiled_file`（`law-spec/tasks/` 是当前示例）
4. 存在未实现语法时，设置 `transpile_regex`
5. 创建 `prompts/layer2_<variant>.md`

---

lambda-syntax 百分比与 p 值是历史实验结果，不是普遍性能保证，也不是独立重跑 Dojo。拟议实验仍是提案。目录树及日文注释逐字节保留，翻译说明放在代码块外。
