<a id="the-rumbling--almide-oss-rewrite-campaign"></a>

# The Rumbling — Almide OSS 重写活动

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](rumbling.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](rumbling.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](rumbling.zh-CN.md)

> 非规范性的固定读者快照。以原文为准，不自动同步，未独立重新验证实现、测量或安全性。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/rumbling.md)
> 此活动搁置中。语言比较、小体积目标及拟议强项是待验证假设或历史定位，不代表已确立当前优势。4.5KB 观察没有步骤记录，分发候选不是已有产物或发布授权。


> 原文描述：用 Almide 重写 OSS 工具，以展示 WASM 大小与 LLM 准确性的活动

**状态**：搁置（语言功能成熟后开始 Block 0）
**优先级**：战略性——推动语言采用的主要动力
**前提**：WASM 直接 emit 完成、stdlib 稳定、self-hosting Phase 0-1

<a id="why"></a>

## 原因

语言采用取决于“能用它构建什么”。Go 凭 Docker、Kubernetes 取胜；Rust 凭 ripgrep、fd 占据“快速 CLI 工具”的位置。

活动将检验三个假设，不预设已有优势：
1. **Wasm 二进制大小**——最初 4.5KB Hello World 的观察在此缺少版本／构建步骤记录；再次使用前应测量当前产物。
2. **Native 与 Wasm 输出**——TS/JS 代码生成目标已退役。
3. **LLM modification survival rate**——通过 Almide Dojo 评估限定任务／模型，不推断普遍的语言排名。

The Rumbling 是用 Almide 重写许可证兼容 OSS、展示这些武器的计划。

<a id="execution-principles"></a>

## 执行原则

- **Block 0 未通过就不往后推进**——语言还不能写自身工具，却重写别人的工具，是矛盾的
- **每个 block 内项目独立**——可从任一项开始
- **每完成一项就发布文章**——只交付能单独成文的成果
- **不做降级复制品**——即便减少功能，也仅在某个维度明显优于原作时交付
- **跳过庞大或不适用 Almide 优势的项目**——数据库、加密库、OS 级工具不在范围内

---

<a id="block-0-dogfood-our-own-tools"></a>

## Block 0：Dogfood（自己的工具）

用 Almide 编写自身工具链，作为自托管的入口。

| 工具 | 证明什么 | 规模 |
|---|---|---|
| `almide fmt` | 字符串处理达到生产级 | 小 |
| `almide test` 运行器 | 能编写 CLI 工具 | 小 |
| 测试框架 | 能自然编写 assert/matcher DSL | 小 |

**门禁条件**：Block 0 全部 3 项正常工作后才能开始 Block 1。

---

<a id="block-1-wasm-showcase-measure-size-for-the-actual-workload"></a>

## Block 1：Wasm 展示（测量实际工作负载的大小）

展示极小的 WASM 二进制大小。全部配浏览器运行演示。

| 工具 | 现有工具 | Almide 优势 |
|---|---|---|
| markdown → HTML | marked.js（原始比较，未记录版本） | 针对等价行为测量 runtime＋host＋产物大小 |
| JSON formatter / validator | jq（原始比较，未记录版本） | 比较等价负载／构建；此处尚未确立大小优势 |
| TOML parser | toml-rs（crate） | 本记录未验证浏览器部署比较 |
| base64 encode / decode | btoa/atob | 自包含 WASM |

**成功条件**：能说“此页面运行的 WASM 为 X KB”。

---

<a id="block-2-nativewasm-showcase-distribution-adapters-are-separate-work"></a>

## Block 2：Native/Wasm 展示（分发适配器是独立工作）

展示 native/Wasm 复用。npm 或 crates.io 打包需要明确的包装器、API 与发布工作；npm 分发不能与已退役的 TS/JS 编译目标混淆。下表列的是分发候选，不是已有输出，也不是发布包的授权。

| 库 | 用途 | 分发 |
|---|---|---|
| Slug 生成 | URL slug | npm + crate + WASM |
| Semver parser | 版本比较 | npm + crate + WASM |
| 颜色转换 | hex / rgb / hsl | npm + crate + WASM |
| uuid v4 | ID 生成 | npm + crate + WASM |

**成功条件**：形成“人们不知不觉在使用 Almide 编写的库”的状态。

---

<a id="block-3-llm-modification-showcase-proving-survival-rate"></a>

## Block 3：LLM 修改展示（证明 Survival Rate）

展示 LLM 能准确修改代码。每个工具附演示：“让 LLM 做 X，运行成功且未破坏原有行为”。

| CLI | 现有工具 | Almide 强项 |
|---|---|---|
| HTTP client | httpie, curlie | LLM 理解全部代码，不破坏行为地添加头部 |
| File watcher | watchexec | 简单到 LLM 可理解全部代码 |
| Env 管理 | direnv lite | 配置文件读写 |
| Task runner | just lite | TOML 解析＋进程执行 |

**成功条件**：能产出实测 modification survival rate 数据。

---

<a id="block-4-platform-ecosystem"></a>

## Block 4：平台（生态）

在 Block 0-3 建立可信度后。

| 项目 | 现有项目 | 意义 |
|---|---|---|
| 包注册表 | crates.io / npm | Almide 生态基础 |
| Playground | Rust Playground | WASM 编译器在浏览器运行（自托管的结果） |
| LSP server | — | 正式的开发者体验 |

---

<a id="not-doing"></a>

## 不做的事

| 目标 | 原因 |
|---|---|
| DB 引擎 | 不适用 Almide 优势，需要低层 I/O |
| 加密库 | 安全证明超出语言成熟度 |
| OS 级工具 | 需要直接 syscall，不符合 Almide 抽象层级 |
| 巨型框架 | 不值得投入，改为交付更多小而快的东西 |
