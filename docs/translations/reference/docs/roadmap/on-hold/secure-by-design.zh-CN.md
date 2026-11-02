<a id="secure-by-design"></a>

# 通过设计保障安全

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](secure-by-design.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](secure-by-design.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](secure-by-design.zh-CN.md)

> 这是固定于 35297a612fb29e3f78f6587aba253aa91d51f466 的[正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/secure-by-design.md)的非规范性阅读译本，不会自动同步，原本仍具权威性。安全性、安全防护、比较、实现和测量主张仅作为该设计时点原本的陈述保留，未经独立执行、计时测量、审计或认证，不构成当前版本的新保证。代码块和注释原样保留，元数据为已停用的阅读注记。

> 原本说明（已停用元数据）: 提议的分层安全模型、已实现的基础以及剩余的信任边界。

> **状态：搁置 — 设计提案，而非安全认证。**
> 于 2026-10-31 对照仓库进行了审阅。不透明类型、SafeHtml/SafePath 辅助函数以及 effect/capability 分析提供了基础。它们并不确立普遍的 Web 安全，也不确立下文提议的按包划分的策略接口。

<a id="goal-and-limits"></a>

## 目标与限制

让危险输出和外部访问变得显式且可审计。类型化包装器只有在构造器强制执行预期策略、且每个相关 sink 都要求该包装器时才有帮助。仅靠包装器无法防止注入、路径遍历、数据外泄或恶意依赖。

本文保留五层设计及其余下的集成工作。当前保证与包行为请参阅[通过构造实现安全](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/security-by-construction.md)和[包规范](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/package-system.md)。

<a id="layer-1-effect-isolation--checked-calls-not-a-sandbox"></a>

## 第 1 层：effect 隔离 — 检查调用，而非沙箱

checker 区分普通 pure 调用和 effect 调用。这有助于识别带 effect 的操作，但存在内建输出例外。pure API 不能证明没有可观察行为、能够终止、资源使用有界或包无害。effect 检查不是不受信任原生代码的隔离边界。

**现有基础：** effect 检查与 capability 分析。不得将其当前范围扩大为 pure 函数无法执行 I/O 的笼统主张。

<a id="layer-2-explicit-external-bridges"></a>

## 第 2 层：显式外部桥接

`@extern` 让部分外部声明可见。标准库 intrinsic、host import 和原生依赖也会跨越信任边界。因此，搜索 `@extern` 不是完整的安全审计，也不等同于 Rust 的 `unsafe` 边界。

**余下提案：** 平台标签及其策略语义。早先的 `@extern(platform: web, ...)` 示例是提议的语法，不是当前已强制执行平台限制的证据。capability 推断已经存在，不依赖于先发布该标签语法提案。

<a id="layer-3-opaque-types-and-safe-sinks"></a>

## 第 3 层：不透明类型与安全 sink

语言支持通过 `mod type T = U` 定义具有名义类型身份、模块私有的包装器。现有辅助函数请参阅 [`html.SafeHtml`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/stdlib/html.almd) 和 [`path.SafePath`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/stdlib/path.almd)。

- `html.escape` 转义文本，而 `html.raw` 刻意包装**受信任、未转义**的输入。因此 SafeHtml 不能证明每个值均已转义。
- SafePath 构造会拒绝某些不安全的字符串。文件系统范围约束还取决于 sink、符号链接、竞态及平台语义。
- 提议中的 SafeSql 包装器与参数化数据库 sink 需要数据库实现及明确的验证/编码契约。
- 要求已验证包装器的框架 sink 仍是独立的集成工作；并非所有标准库 I/O 都只接受不透明包装器。

**余下提案：** 定义安全 sink 签名，验证构造器与上下文专用转义，再测试是否拒绝不安全构造和绕过 sink 的行为。早先的 `Response.html` / HTML-builder 草图是设计示例，而非已发布的框架 API。

<a id="layer-4-package-capability-policy"></a>

## 第 4 层：包 capability 策略

capability 推断有助于描述函数可达的 effect。额外提案是由使用者授予、具有传递性的**逐依赖策略**，例如：

```toml
# Proposed interface, not a current manifest contract.
[dependencies.example]
version = "1.0"
capabilities = ["fs"]
```

> 代码注释译文：此 TOML 示例是提议的接口，不是当前 manifest 契约。

在实现该接口前，应规定：

- 推断出的 effect 和 native/host 实现如何映射到 capability；
- 强制执行是静态、运行时还是两者兼有，包括未知调用；
- 传递依赖的授权与诊断；
- 更新期间请求的 capability 变化时采用的策略；
- 在已授权 capability 内的行为（文件系统授权不能证明使用无害）。

不得将现有 capability 推断描述为未实现，也不得混同于该包策略提案已被强制执行的证明。

<a id="layer-5-supply-chain-identity-and-provenance"></a>

## 第 5 层：供应链身份与来源

提案结合锁定身份、可信来源和 capability 变更审查。已实现的 lock/trust 行为请参阅当前包规范；托管 registry 仍是[独立提案](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/package-registry.md)。

内容哈希检测与可信预期身份的不一致。它不能证明行为无害、不能防止恶意初次发布，也不能独立解决名称拼写仿冒。哈希验证不会对原生构建步骤实施沙箱隔离。registry 是一种可能的分发机制，不是哈希的前提。

**余下提案：** 定义可信来源、身份固定、更新审查，以及与原生依赖信任的交互。capability 变化和哈希不匹配需要显式策略与诊断，而非假定已强制执行。

<a id="implementation-sequence-and-why-this-remains-on-hold"></a>

## 实现顺序与继续搁置的原因

1. **现有基础：** 不透明类型、HTML/path 辅助函数及 effect/capability 分析。保留这些基础，但不主张普遍安全的 sink。
2. **sink 集成：** 规定并测试安全的 HTML、SQL、命令和文件系统边界，包括受信任的逃生口及验证失败行为。
3. **包策略：** 定义授权、传递性强制执行和外部代码信任；平台标签设计可能有所帮助，但它不是现有分析器。
4. **分发/来源：** 将可信身份和 capability 变更审查整合进包工具及未来可能的 registry。

余下工作包含策略和研究问题，而非仅仅排期：强制哪些性质、信任哪些组件、授予的权限如何可能被滥用，以及什么证据确立每项保证。语言没有 JavaScript prototype 或动态 eval，并不能单独约束外部主机或原生依赖。

<a id="intended-protections-and-residual-risks"></a>

## 预期保护与残余风险

不主张任何覆盖率百分比。之前的 100%/95% 估计是愿景，不是测量或证明，不应当作安全保证。

| 边界 | 潜在保护 | 剩余义务 |
|---|---|---|
| HTML/SQL/命令输出 | 在类型化 sink 处拒绝不兼容值 | 正确的转义/参数化、构造器和受信任原始输入 |
| 文件系统路径 | 拒绝某些无效路径字符串 | 范围约束、符号链接、竞态、平台行为和权限 |
| 依赖 effect | 识别并限制选定的 capability | 完整的强制执行，以及已授予 capability 内的滥用 |
| 包身份 | 检测相对可信预期身份的变化 | 初始信任、来源、恶意更新和原生构建行为 |

授权逻辑、验证正确性、部署配置、侧信道、SSRF 策略和资源耗尽仍需要显式设计和审查。这些层支持纵深防御，但并未确立对 OWASP 类别的消除，也不会让应用安全审查变得不必要。

## 原本注意事项

搁置状态、仓库审阅日期、已实现基础及余下提案均按原本报告保留。提议的策略语法不是当前 manifest 契约。不把已否定的覆盖率恢复为保证，也不添加认证、普遍安全或免除应用审查的主张。
