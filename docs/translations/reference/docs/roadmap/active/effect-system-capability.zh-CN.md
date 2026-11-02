原文 description 元数据：用于 sandbox AI agent container 的 capability-based effect system
<a id="capability-based-effect-system"></a>

# 基于 capability 的 effect system

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](effect-system-capability.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](effect-system-capability.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](effect-system-capability.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effect-system-capability.md)

> 原文注记：基线纠正优先于旧 phase 草图：pure 不等于零 category，callback effect 透明，检查不证明任意 host 行为。MCP 是未交付提案；旧配置注释称直接可用并非验证结果。search 示例过滤文件名，并非搜索文件内容。directory preopen 不等于只读，共享 import/handle right 限制了把 import 裁剪作为强制执行论据的范围。未执行引用命令。


> **当前范围**：permission 分析/检查已存在，剩余工作是更强的 capability chain 及下述提案。使用 `almide check` / `almide check --effects`，而非未实现的 `--capabilities`。检查通过不是整个 runtime 的 sandbox 证明。
> **设计裁定 2026-10-03**：[ADR-0026](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0026-effect-categories-ride-the-transparent-fn-type.md)——类型层面（Phase 4）的形态、类别层级（替换下述 `IO` 简写）、静态⊆manifest⊆host 链。

<a id="implemented-baseline-and-remaining-design"></a>

## 已实现基线与剩余设计

`crates/almide-ir/src/effect.rs` 定义 14 个 leaf 和四个 parent（`FS`、`Net`、`IO`、`Env`）。`IO` 仅含 stdin/stdout/stderr，不含 filesystem access；`Abort` 是 leaf。这取代最初 13 category 草案。`src/cli/check.rs` 与 `src/cli/mod.rs` 针对推断 effect 强制 project permission；`src/project.rs` 解析 grant。effect bit 与 category 分析不同：pure `fn` 不承诺零 category（ADR-0022 允许 output/abort）。

期望最小权限链为推断 effect ⊆ manifest ⊆ host（ADR-0026）。既有分析不证明任意 foreign code 或 host 行为遵循它。`@extern` body 不透明，推断时获全部 leaf；native permission 与 process command grant 各有边界。仍需 host 配置。

下述 phase 草图记录原始实现分解，不是五项全未实现任务，也不是已交付 end-to-end 保证集合。尤其声明 category 本身不会移除 WASI import：read/write operation 可共享 import 并依赖 handle right。import 检查不能证明所有 application 或 host 行为。host 是独立信任边界，不是安全的万能证明。

<a id="implementation-phases"></a>

## 实现阶段

<a id="phase-1-original-checking-plan-core-enforcement-now-exists"></a>

### Phase 1：原检查计划（核心强制机制现已存在）

**目标**：almide.toml 中 `[permissions]` → 违规时 compile error。

1. 在 compiler 中定义 capability → stdlib 函数映射
   - 每个 stdlib module 函数注记 capability category
   - `fs.read_text` → `FS.read`、`fs.write_text` → `FS.write` 等
   - 存为 `almide-frontend` 静态 table
2. 从 almide.toml 解析 `[permissions]`
   - `allow = ["FS.read", "IO.stdout"]` → 计划中的 `CapabilitySet` 表示（当前 registry 有 14 leaf）
   - 缺省 `[permissions]` 时允许全部 capability（向后兼容）
3. 类型检查器的 capability 检查 pass
   - 类型检查后遍历 call graph
   - 对每个 stdlib 调用查询必要 capability
   - 不在 `allow` 内 → 输出可采取行动的 hint 和 error：
     ```
     error[CAPABILITY_VIOLATION]: capability violation: fs.write_text requires FS.write
       --> agent.almd:5:3
       |
     5 |   fs.write_text(path, content)
       |   ^^^^^^^^^^^^^
       = note: [permissions] allows: FS.read, IO.stdout
       = hint: add "FS.write" to [permissions].allow in almide.toml
     ```
   - 上述符号 diagnostic 是示意，并非分配过的 code（`E010` 是穷尽性）。
   - 传递性：若 `fn foo` 调用 `fn bar`，再调用 `fs.write`，则 `foo` 需要 `FS.write`
4. effect 函数推断更新
   - 不带 [permissions] 的 `effect fn` → 全部 capability（现有行为）
   - 带 [permissions] 的 `effect fn` → 仅声明的 capability
   - pure `fn` 不意味着零 category：output/abort 仍允许（ADR-0022）。

诊断例翻译：写入函数需要 FS.write，但允许列表仅有 FS.read、IO.stdout，因此提示向 almide.toml 的允许项加入 FS.write。

**要修改的文件**：
- stdlib `.almd` source——逐函数声明 capability（stdlib/defs TOML pipeline 已不存在；逐函数 metadata 位于 .almd body）
- `crates/almide-frontend/src/check/`——capability 检查 pass
- `src/project.rs`——当前 manifest 解析（原先归于 resolver 已过时）
- `crates/almide-base/src/`——CapabilitySet 类型（u16 bitset）

**测试**：编写 agent test，在 `allow = ["FS.read"]` 下能编译，调用 `fs.write_text` 时失败。

<a id="phase-2-wasm-import-pruning--manifest-output"></a>

### Phase 2：WASM import 裁剪 + manifest 输出

**目标**：WASM binary 只含 capability set 所需 WASI import，并输出机器可读 manifest。

1. 将 capability 映射至 WASI import
   - `FS.read` → 需要 `path_open`、`fd_read`、`fd_close`、`fd_seek`、`path_filestat_get`
   - `FS.write` → 需要 `path_open`（write）、`fd_write`、`path_create_directory` 等
   - `IO.stdout` → 需要 `fd_write`（仅 fd=1）
   - capability 不允许 → 不输出相应 WASI import
2. 更新 WASM emitter import section
   - 原假设：导入全部 WASI 函数。这不是当前 emitter 清单。
   - 新方案：仅导入 CapabilitySet 所需 WASI 函数
   - 引用被裁掉 import 的 runtime 函数 → 替换为 `unreachable` trap
3. 在 `.wasm` 旁输出 `manifest.json`
   ```json
   {
     "name": "file-reader-agent",
     "version": "0.1.0",
     "capabilities": ["FS.read", "IO.stdout"],
     "wasi_imports": ["fd_read", "fd_write", "fd_close", "path_open", "path_filestat_get"],
     "entry": "_start"
   }
   ```
   orchestrator 在加载 WASM binary 前读取它，核实 capability 符合 policy。

**要修改的文件**：
- `crates/almide-wasm/src/`——当前 structural emitter；旧修改对象 `render_wasm*` / `self_host_registry.rs` 已退役
- `src/cli/build.rs`——输出 manifest.json

**测试**：以 `allow = ["IO.stdout"]` build → 验证 WASM binary 无 `path_open` import。

<a id="phase-3-proposed-per-dependency-category-restriction"></a>

### Phase 3：按 dependency 限制 category 的提案

**目标**：限制 dependency package 可使用的 capability。

```toml
[dependencies.json_parser]
version = "1.0"
allow = []                    # pure only — no I/O

[dependencies.http_client]
version = "2.0"
allow = ["Net.fetch"]
deny = ["Proc", "FS.write"]  # explicit deny
```

配置注释翻译：空允许列表意在表示仅 pure、无 I/O；deny 表示显式拒绝。

1. 从 almide.toml 解析逐 dependency permission
2. 加载 module interface 时计算 dependency 所需 capability
   - 遍历公开函数的传递 call graph
   - 与 consumer 针对此 dependency 的 `allow` / `deny` 比较
3. dependency 超过授权 capability 时 compile error

**要修改的文件**：
- `src/resolve.rs`——逐 dependency permission 解析
- `crates/almide-frontend/src/check/`——跨 module capability 传播
- module interface 格式——包含 capability 要求

<a id="phase-4-original-tyfn-sketch-superseded-by-category-flow-analysis"></a>

### Phase 4：category-flow 分析取代原 Ty::Fn 草图

原计划将 `CapabilitySet` 存于 `Ty::Fn`。当前 `crates/almide-codegen/src/effect_flow/mod.rs` 改为在类型检查之后计算集合，经函数值/位置追踪，结果存入 `EffectMap` 而非 `Ty::Fn`。追踪位置与保守 pool 规则界定已实现范围，并不完成所有未来 phase。

bare fn 类型 callback parameter 是透明的（ADR-0009/0026）：调用方继承传入 callback 的 effect。向此类 parameter 传 effectful callback 并不会一概拒绝。显式 purity 边界则不同，见 `crates/almide-frontend/src/check/pure_attr.rs`。旧强制拒绝规则不是当前语言指南。

<a id="phase-5-proposed-mcp-server-stdlib-module"></a>

### Phase 5：MCP server stdlib module 提案

没有已交付的 `mcp` stdlib module。compiler 的 MCP tooling 是不同 interface；以下是设计草图，不是可运行部署指令。

**目标**：实现 Model Context Protocol（stdio 上 JSON-RPC）的 `mcp` stdlib module。不自创 protocol，不改语言。

MCP 是 AI agent tool 通信（Claude Code、Cursor、Windsurf、GitHub Copilot）的事实标准。提议 module 需要显式 client/protocol 兼容测试；仅使用 JSON-RPC 不能确立与所有 MCP client 兼容。

```almide
import mcp

effect fn main() -> Unit =
  mcp.serve([
    mcp.tool("read_file", "Read a file at the given path", (params) => {
      fs.read_text(mcp.arg(params, "path"))
    }),
    mcp.tool("list_dir", "List directory contents", (params) => {
      fs.list_dir(mcp.arg(params, "path")) |> list.join("\n")
    }),
  ])
```

```json
// Claude Code mcpServers config — works directly
{
  "mcpServers": {
    "code-reviewer": {
      "command": "wasmtime",
      "args": ["run", "--dir", "/workspace", "agent.wasm"]
    }
  }
}
```

配置例注释称“Claude Code mcpServers 配置——直接工作”，但这是受上述未实现设计草图限定的原文，而非本次运行确认。tool 描述为读取指定 path 文件及列出 directory 内容。

- `mcp.serve(tools)`——JSON-RPC stdio loop（initialize → tools/list → tools/call）
- `mcp.tool(name, description, handler)`——tool 定义
- `mcp.arg(params, key)`——parameter 提取
- `mcp.resource(uri, handler)`——resource 暴露（未来）
- 全部是 stdlib，无语言变更。遵循 MCP spec，而非自定义 protocol。

<a id="proposed-ai-agent-container-example-not-a-shipped-end-to-end-recipe"></a>

## AI agent container 提议示例（不是已交付 end-to-end 配方）

```toml
# almide.toml
[project]
name = "code-review-agent"
version = "0.1.0"

[permissions]
allow = ["FS.read", "IO.stdin", "IO.stdout"]
```

```almide
// agent.almd
import mcp

effect fn main() -> Unit =
  mcp.serve([
    mcp.tool("read_file", "Read file contents", (params) => {
      fs.read_text(mcp.arg(params, "path"))
    }),
    mcp.tool("list_dir", "List directory", (params) => {
      fs.list_dir(mcp.arg(params, "path")) |> list.join("\n")
    }),
    mcp.tool("search", "Search for pattern in files", (params) => {
      let dir = mcp.arg(params, "dir")
      let pattern = mcp.arg(params, "pattern")
      fs.list_dir(dir)
        |> list.filter((f) => string.contains(f, pattern))
        |> list.join("\n")
    }),
  ])
```

```bash
# Proposed build/manifest output — not current CLI behavior
almide build agent.almd --target wasm -o agent.wasm
# → also outputs agent.manifest.json

# Use from Claude Code directly
# claude_code_config.json:
# { "mcpServers": { "code-reviewer": {
#     "command": "wasmtime",
#     "args": ["run", "--dir", "/workspace", "agent.wasm"]
# }}}

# Or run standalone with sandboxing
wasmtime run --dir /workspace agent.wasm
```

示例描述及注释翻译：tool 标为读取文件内容、列出 directory、在文件中搜索 pattern。build/manifest 输出是提案，并非当前 CLI 行为；设想同时输出 agent.manifest.json。展示从 Claude Code 直接使用的配置，或在 sandbox 中独立运行的方式。未执行所列命令。

<a id="required-boundaries-before-claiming-a-secure-deployment"></a>

### 宣称安全部署前所需边界

- compiler permission 检查覆盖所选路径已实现的分析，不覆盖任意 external/host code。提议的 manifest 输出和 import 裁剪需各自实现及证据。
- `wasmtime --dir /workspace` 是 directory preopen，**不是 read-only grant**。read/write right 及全部其他 host capability 必须另行配置、验证。这些草图不确立 read-only container。
- MCP module 提案需要 protocol/schema 兼容测试。带类型 Almide handler 不证明与每个 client 兼容。
- 此计划不推出相对所有其他语言的唯一性。

<a id="implementation-priority"></a>

## 实现优先级

| Phase | 影响 | 工时 | 可独立交付？ |
|-------|--------|--------|---------------------|
| **Phase 1** | compile-time capability 检查 | M | ✅ 可以——即时价值 |
| **Phase 2** | WASM import 裁剪 + manifest | S | 可能——import 清单，不是完整行为证明 |
| **Phase 3** | 逐 dependency 限制 | S | 可能——一种 supply-chain 控制，不是完整安全 |
| **Phase 4** | 类型层 effect（HOF）| L | ✅ 可以——精度 |
| **Phase 5** | agent stdlib module | S | ✅ 可以——仅 stdlib，无语言变更 |

Phase 1 提供静态 permission 检查，不足以将任意 agent 部署称为 sandbox 化。

<a id="what-we-dont-do"></a>

## 不做什么

- 代数 effect handler——过于复杂，capability 检查不需要
- 用户定义 effect category——当前 registry 有 14 个内置 leaf
- 函数 signature 中的 effect 语法——capability 声明于 almide.toml，而非 code
- 动态 capability request 不属于本提案；host grant 与 runtime 强制仍重要
