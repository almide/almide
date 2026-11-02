<a id="llm-integration"></a>

# LLM 集成

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](llm-integration.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](llm-integration.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](llm-integration.zh-CN.md)

> [英文正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/llm-integration.md)的非规范性读者快照，固定提交为 `35297a612fb29e3f78f6587aba253aa91d51f466`。原文仍是正本。本快照不自动同步，也不是生成器／检查器输入。下文日期、计划、完成状态、测量和比较主张均保留为历史原文记述，不是独立复现或对当前版本的验证。代码块及注释保持原样。历史收集元数据以不生效的读者注记显示。

> 历史元数据：描述 — 用于库生成、自动修复和代码解释的内置 LLM 命令。

> 原文注意事项：这个搁置提案明确尚未实现。未来流程、速度／成本和独有性主张是提案理由，不是独立证实的能力或比较。“范围边界”中的 fix 保留为提案标签，不重新定义已发布的本地 `almide fix`，也不授权源码传输。对源码传输及所用提供商的同意，以及独立接口，仍是前提。

<a id="status-and-current-cli-boundary-2026-10-31"></a>

## 状态与当前 CLI 边界（2026-10-31）

这是**尚未实现的 AI 集成提案**，不是对当前 CLI 的说明。尤其是，已发布的 `almide fix` 应用在本地计算出的、可由机器应用的诊断修复并重新检查；它**不会**把源代码发送给 LLM。参见 [`src/cli/fix.rs`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/cli/fix.rs)。未来的 AI 辅助修复命令需要独立接口，以及对源码传输／提供商的明确同意；不得悄悄改变这一契约。下文的 `forge`、`pilot`、`[ai]` 和 `--no-ai` 都是拟议接口。按函数进行 AI `explain` 的提案，与已发布的 `almide explain <diagnostic-code>` 命令不同。

<a id="thesis"></a>

## 主张

把 LLM 嵌入“LLM 能最准确编写的语言”的编译器。LLM 编写、LLM 修复、LLM 扩充库 — 这一循环用一个编译器完成。

<a id="subcommands"></a>

## 子命令

<a id="almide-forge--library-generation"></a>

### `almide forge` — 库生成

指定主题和参考实现，即可自动设计、实现、测试并发布 Almide 库。

```bash
almide forge csv --ref python:csv,rust:csv,go:encoding/csv
```

1. 分析参考库 API（文档或源码）
2. 设计符合 Almide 惯例的 API（UFCS、effect fn、Result/Option、命名规范）
3. 生成实现和测试
4. 用 `almide test` 验证所有测试通过
5. 创建 GitHub 仓库并 push

**原因：** 启动生态系统。让 LLM 批量生成、由人类评审，比逐个手写库更快。

<a id="proposed-ai-assisted-repair--not-the-shipped-almide-fix"></a>

### 拟议的 AI 辅助修复 — 不是已发布的 `almide fix`

把编译错误交给 LLM 自动修复。

```bash
almide <proposed-ai-repair-command> app.almd
```

1. 用 `almide check` 收集错误
2. 将源代码和错误诊断发送给 LLM
3. 应用建议的修复
4. 再用 `almide check` 验证通过
5. 展示 diff 并等待批准（`--yes` 跳过）

**原因：** 扩展错误恢复。编译器不只是说“这样修”，而是真正修复。

<a id="almide-explain--code-explanation"></a>

### `almide explain` — 代码解释

```bash
almide explain app.almd
almide explain app.almd --fn parse_config
```

生成源代码的 Markdown 解释，可指定单独的函数。

**原因：** 自动生成文档。由 LLM 解释 LLM 编写的代码。

<a id="configuration"></a>

## 配置

```toml
# almide.toml
[ai]
provider = "anthropic"    # or "openai"
model = "claude-sonnet-4-20250514"
# api_key is read from ANTHROPIC_API_KEY / OPENAI_API_KEY env var
```

- `--no-ai` 标志禁用所有 AI 功能（作为离线编译器运行）
- API key 从环境变量读取（不硬编码在 toml 中）
- AI 功能不影响编译器核心代码路径（独立模块）

<a id="scope-boundary"></a>

## 范围边界

**包含（仅与 Almide 代码相关的内容）：**
- forge：库生成
- fix：自动修复编译错误
- explain：代码解释

**排除（不是通用 agent）：**
- 聊天 UI
- 任意任务执行
- Almide 之外的文件操作

<a id="implementation-plan"></a>

## 实现计划

<a id="phase-1-separate-ai-assisted-repair-command"></a>

### Phase 1：独立的 AI 辅助修复命令
- [ ] 添加 HTTP 客户端（`ureq` 或 `reqwest`）
- [ ] 加载 `[ai]` 配置
- [ ] `almide check` → 错误 + 源码 → LLM API → 修复 diff → 应用
- [ ] `--yes` / `--dry-run` 标志

<a id="phase-2-almide-forge"></a>

### Phase 2：`almide forge`
- [ ] `--ref` parser（`language:package` 格式）
- [ ] 参考库 API 分析的提示词设计
- [ ] Almide API 设计 → 实现 → 测试生成管线
- [ ] `gh repo create` + push 集成

<a id="phase-3-almide-pilot--path-optimization-constrained-decoding"></a>

### Phase 3：`almide pilot` — 路径优化（约束解码）

编译器实时引导 LLM 的 token 生成。生成过程中的每个 token 都由 parser／类型检查器验证，无效 token 立即被拒绝，并返回修正候选。

```
LLM → token generation → almide parser (incremental) → valid?
                                                          ├─ Yes → accept, next
                                                          └─ No  → return valid continuation candidates → LLM re-selects
```

- [ ] 增量 parser API（从部分输入返回“有效的下一个 token 集合”）
- [ ] 类型检查器的流式模式（在部分 AST 上运行类型推断）
- [ ] `almide pilot serve` — 作为类似 LSP 的 JSON-RPC 服务器启动，可由外部 LLM 调用
- [ ] 推测缓冲区：回退无效 token，将有效后续内容作为 hint 返回
- [ ] 基准测试：比较有／无约束解码时的编译成功率和生成速度

**原因：** 拟议的 AI 辅助修复是“写完后修”。pilot 是“在编写当下使其正确”。MoonBit 已有先行实现，但 Almide 的简单语法与类型系统使实现成本较低。当前编译器面向 Rust 和 Wasm；该提案必须使用二者共享的已检查 IR，而不是已退役的 TypeScript 目标。

<a id="phase-4-almide-explain"></a>

### Phase 4：`almide explain`
- [ ] 按函数生成解释
- [ ] Markdown 输出

<a id="differentiator"></a>

## 差异化因素

Rust、Go、TypeScript — 它们的编译器都没有内置 LLM。但这是因为它们是“人类编写的语言”。Almide 是 LLM 编写的语言。编译器侧拥有 LLM 是自然结果，也是 Almide 独有的优势。

> 保留的配置示例注释说明 provider 可选 anthropic 或 openai，API key 从 ANTHROPIC_API_KEY / OPENAI_API_KEY 环境变量读取。图示为拟议流程：LLM 生成 token，经增量 parser 检查；有效则接受并继续，无效则返回有效的后续候选，由 LLM 重新选择。
