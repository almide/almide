<!-- description: Historical platform survey with current core/component/embedded route pointers -->
<a id="wasm-platform-frontier--beyond-core-wasm-30"></a>

# WASM 平台前沿 — 核心 Wasm 3.0 之外

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](wasm-platform-frontier.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](wasm-platform-frontier.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](wasm-platform-frontier.zh-CN.md)

> 非规范性历史阅读快照。以固定原文为准，不自动同步。未独立重跑测量、证明 gate 或当前实现检查。未执行引用命令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/wasm-platform-frontier.md)
> 开头2026-10-31路径边界明确更新下方旧Phase 0–1、env.cwd wall、宽泛filesystem preopen及顺序fan前提。6–8月标准/runtime调查未重新核实。旧共享thread愿景保留为历史计划，不是当前已采纳架构；广泛等价性与跨语言platform主张属于原文，不是独立普遍证明。

<a id="current-route-boundary-2026-10-31"></a>

## 当前路径边界（2026-10-31）

下述 Phase 0–1 状态属于历史。Almide 现在有结构化 core Wasm emitter、受支持的 component 打包、嵌入式 WASI 0.3 HTTP 路径，以及嵌入式 filesystem/network/process 操作。但可用性仍按操作和路径划分，**不代表每个 host 都提供每种操作**（child-handle process family 的 Wasm 移植仍待完成）。

参见[host 概览](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/README.md)、[target availability](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/target-availability.toml)、[build_wasm.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/cli/build_wasm.rs) 和 [component_host.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-wasm-run/src/component_host.rs)。嵌入式路径提供 `env.cwd`；不能从下方旧 `--dir=/` 草图推断当前 filesystem scope 与 preopen 默认值。嵌入式 fan 使用隔离 worker instance，因此“Wasm fan 总是顺序”的前提也已过时。

下述标准／版本调查保留为六月至八月快照，不是新核实的外部 roadmap 或部署建议。当前仓库工具 pin 见 [toolchain-pins.txt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/scripts/toolchain-pins.txt)。下方实现计划不表示其未来完整架构已落地。

> **活动范围：Phase 0-1** — 明文定义 deterministic profile，并设计 `almide_host` ABI。
> Phase 2-4（component target、0.3 async、threads）等待标准成熟后分阶段引入。
> **退出标准（Phase 1）**：同一 .wasm 在浏览器（fetch shim）及 wasmtime（host fn）两种 host 上均可执行 `http.get/post`。

对核心 Wasm 3.0（2025-09 标准化）的跟进**大致完成**。剩余投资在核心规范之外：WASI / Component Model 与 post-3.0 proposal。本 roadmap 记录已完成、决定不采纳及等待的内容。

<a id="core-wasm-30--現状監査-2026-06-05"></a>

## Core Wasm 3.0 — 状态审计（2026-06-05）

<a id="採用済み"></a>

### 已采用

| 功能 | 实现 |
|------|------|
| tail calls | almide-mir 的 `tail_call_indexes`（render_wasm_b.rs）分类函数末尾 CallFn，输出 `return_call`（#864 重新移植，C-178）。self 递归走上游 TCO loop。`return_call_indirect`（closure 尾部）也**已移植，2026-08-03**：同一分类准入 CallIndirect（fixture：closure_tail_recursion＋指令 gate：tail_call_indirect_test.rs）。纯 indirect 无限循环因 creator 侧 drop 正当地 decline，深度主张按 fixture header 如实限定 |
| SIMD (v128) | 旧 almide-codegen 时代的 4× v128 展开未移植到 almide-mir（默认输出无 v128，#864 调查确认）；扩大适用范围见 [wasm-optimization-roadmap](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/wasm-optimization-roadmap.md) |
| 严格验证 | 以 wasmtime 45+ / V8 strict validator 为前提（StackBalancePass） |

<a id="取る-小さくてミッション直結"></a>

### 采用（小而直接服务使命）

- **明文确定 deterministic profile 合规** — **完成，2026-08-03，C-210**。审计结论：NaN 原始 bit 到达观测面仅有 `float.to_bits` 与 bytes float write family 两处（Float 不可 hash，to_string 固定为 "NaN"）。两处在观测边界规范化为 canonical NaN（f64 0x7FF8000000000000 / f32 0x7FC00000），一并阻断 x86 sign-set NaN、engine payload propagation 差异及经 from_bits 偷带 payload；比 profile 更强，**双 target＋所有 host 架构相同**。两个 nan_canonical_* fixture＋指令子集 gate（deterministic_profile_test.rs，机械检查不使用 relaxed SIMD / atomics / shared）。副产品：审计发现 self-host 链接同名异签名冲突＝invalid-wasm-as-Ok 逃逸（#1068）；linker 改为完全匹配才 merge，冲突则 wall。注意范围是**所生成程序的执行确定性**；编译器自身输出确定性（emitter = pure fn of (IR, target)）由 [determinism-belt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/determinism-belt.md) 负责。
- extended const expressions：让 global 初始化更灵活，改动很小。

<a id="検討-実測で問題になってから"></a>

### 考虑（实测出现问题以后）

- **异常处理（`try_table`/`throw`）**可不检查 tag 即传播 effect fn 错误。阻挡因素：**Perceus RC 与 unwinding 交互**，回退时跳过 RC decrement 会 leak，需要类似 landing pad 的 cleanup 设计。Result 是一等值，值表示仍保留，只有传播 fast path 形成双编码。ROI 低至中。

<a id="取らない-理由付き"></a>

### 不采用（附理由）

| 功能 | 理由 |
|------|------|
| GC | linear↔wasm-gc 双 backend 付出 builtin parity 成本；自行管理 RC/COW 正是 byte-identical gate 基础 |
| relaxed SIMD | 规范允许实现相关结果，与等价保证直接冲突；SIMD 改为扩大 2.0 fixed v128 适用范围 |
| typed function references（`call_ref`） | ref 类型无法存在线性内存；只要 closure 采用线性内存 struct＋table 编号，结构上就不可用（仅采用 GC 时有意义） |
| memory64 | 对 4GB 足够的用途只增加 bounds check 成本 |
| multiple memories | 为兼容 iOS Safari 有意使用单内存，见 `emit_wasm/mod.rs` Memory section 注释 |

<a id="wasm-レッグの-fs-パス契約2026-08-03-決定--他言語実装との整合を確認済み"></a>

## Wasm 分支的 fs 路径契约（2026-08-03 决定，已对照其他语言实现）

WASI p1 **没有 cwd**（无 chdir/getcwd syscall，路径相对 preopen fd）。各语言在 userspace 重新制造这一抽象，决定时检查了实现：

| 语言 | wasip1 的 cwd |
|------|----------------|
| Go | userspace 变量：`$PWD`（guest 绝对化），没有则 **`preopens[0].name`**；模拟 Chdir/Getwd（syscall/fs_wasip1.go） |
| Rust | `env::current_dir()` 返回 unsupported error 而**放弃**；相对路径依赖 preopen prefix 匹配 |
| wasi-libc (C/Python) | userspace chdir 模拟（`__wasilibc_cwd`），按 prefix 匹配解析 |

**Almide 契约**：确认 #874 原设计兼容 Go，经 Windows 支持补全。

1. guest 相对路径解析是 env 驱动的 cwd 抽象：**`ALMIDE_CWD`（launcher pin，优先）→ `PWD`（Go 兼容 fallback）**。launcher pin 消除 Go 已知 PWD-staleness（如 Node execFileSync）。
2. launcher（`almide run/test`）用“guest 可见的写法”pin cwd：**unix**＝host 绝对路径（在 `--dir=/` 下直接对 guest 有效，与 Go PWD 值同构）；**Windows**＝`.`（`--dir=.` preopen 的 guest 名，与 Go `preopens[0].name` fallback 等价）。
3. temp 为 `$TMPDIR ?? "/tmp"`（C-189，Go/Python 约定）；Windows launcher 将 host temp 映射到 guest `/tmp` 并注入 `TMPDIR=/tmp`。
4. host 绝对路径透明性**限 unix host**（`--dir=/`）。没有语言能在 Windows host 直接使用 unix 绝对路径；相对路径＋`/tmp` 是可移植写法，CHEATSHEET 如此指导。
5. **不引入 chdir**：类似 Go Chdir 的可变 userspace 状态违背确定性。`env.cwd` 在 wasm 仍是有诊断的 wall。
6. WASI 0.2/0.3 的 wasi:filesystem 是无 ambient cwd 的 capability handle 模型，此 preopen 契约可直接向前兼容。

实现：`wasmtime_fs_args`（src/cli/run.rs）＋WAT shim 的 ALMIDE_CWD/PWD 扫描（render_wasm_fs_wat.rs）。验证：ci.yml `Test WASM (Windows host)`（#1066）。

<a id="フロンティア--30-の外側"></a>

## 前沿 — 3.0 之外

<a id="1-wasi-0203--component-model--httpasync-の標準-abi"></a>

### 1. WASI 0.2/0.3＋Component Model — HTTP/Async 标准 ABI

当时 wasm target 只有 WASI preview 1（fs/clock/random/stdio/proc_exit）。无 socket、HTTP client 或 process 启动；`calls_http.rs` 只有 `http.response`/`http.json` 纯 builder。

标准状态（2026-08 重新核实）：
- WASI 0.2 stable；wasmtime 有 `wasi-http` host 实现，浏览器／Node 使用 jco。
- **WASI 0.3.0 于2026-06-11正式发布，同月 ratify 为 stable**（RC 首见于2025-11的 Spin v3.5，正式支持 wasmtime 43+）。canonical ABI 级 native async（`async func` / `stream<T>` / `future<T>`）；`wasi:io` 被 Canonical ABI 吸收并废除。此后0.3.x为 release train，WASI 1.0仍以2026年后半为目标。
- 浏览器不直接使用 WASI；0.2或0.3都需要 jco 或 shim。
- 本 repo CI pin wasmtime 47.0.3（2026-08从42.0.1提升，满足0.3 host 43+及严格验证45+两前提）。

可解锁：`wasi-http`＋native async stream＝面向 LLM API 的 SSE streaming。用 Almide 写 agent loop，以单 component 部署到 wasmtime / Spin / wasmCloud / edge host。host 侧 outgoing request 过滤（capability-scoped network）直接契合 [effect-system-capability](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effect-system-capability.md) 的 Layer 2/3。

成本：canonical ABI lifting/lowering（string UTF-8 边界转换、list/record/variant、resource handle）仅同步版就是大工程。0.3 async ABI 即使对 Rust 本体也是2026年目标前沿。wasmtime p1→p2 adapter 只映射既有 p1 API，所以**不能经 adapter 得到 HTTP**。

分阶段引入：
1. **Phase 1（当时）：custom `almide_host` import ABI** — 由 host import 提供 `http.get/post` 等；浏览器＝fetch shim，服务器＝wasmtime host fn。编译器仅增加 import，无需 canonical ABI。
2. **Phase 2：p1→p2 adapter** — 将既有 module component 化（fs-only，几乎免费）。
3. **Phase 3：emit 0.2同步 canonical ABI** — 直连 wasi-http。
4. **Phase 4：0.3 async** — 等 WASI 1.0可见，再将 custom ABI 收为薄兼容层并退役。

<a id="2-stack-switching-post-30-proposal--wasm-上の-async-実行モデル"></a>

### 2. Stack Switching（Post-3.0 Proposal）— Wasm 上的 Async 执行模型

coroutine/async 基础；追踪 proposal phase，与 Phase 4 async canonical ABI 汇合。

<a id="3-shared-everything-threads-post-30-proposal--fan-の-wasm-側本格化"></a>

### 3. Shared-Everything-Threads（Post-3.0 Proposal）— Wasm 侧完整 `fan.*`

wasm 单线程限制曾是 `fan.*` 跨 target 语义差异根源（fan.timeout 因此在0.29.0从语言删除）。真正的共享内存线程可令 wasm fan 实现与 native 同构。追踪 proposal phase。

<a id="追跡すべきバージョン番号"></a>

## 应追踪的版本

“Wasm 4.0”不存在，也未规划。追踪：
- **WASI 0.3.x → 1.0**（release train，1.0目标为2026年后半）。
- **stack switching / shared-everything-threads 的 proposal phase**。

<a id="references"></a>

## 参考资料

- [Wasm 3.0 announcement](https://webassembly.org/news/2025-09-17-wasm-3.0/)
- [WASI roadmap](https://wasi.dev/roadmap)
- [WebAssembly proposals tracking](https://github.com/WebAssembly/proposals)
- [wasmtime-wasi-http](https://docs.wasmtime.dev/api/wasmtime_wasi_http/index.html)
