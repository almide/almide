<a id="capability-system"></a>

# 能力系统

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](capability-system.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](capability-system.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](capability-system.zh-CN.md)

> 非规范性、固定原文的读者快照。未修改的原本仍为权威来源；不自动同步，未独立执行、重新测量、验证安全性或进行第三方认证。下文“当前”“已实现”和断言描述固定原文，不是本次重新评估。嵌入式宿主不是通用沙箱；权限键缺省时及 core/component 默认值不同。保留原文所述证书范围不包括组件的限制。逐依赖授权及具名权限配置仍属未来提案。代码和注释保持原样。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/capability-system.md)

编译时能力检查与宿主权限是不同的控制。`[permissions].allow` 限制从程序推断出的类别；宿主与所选 Wasm 路由决定产物可执行哪些操作。类别检查和 import 裁剪都不是针对任意原生代码或子进程的通用沙箱。

<a id="capability-categories"></a>

## 能力类别

当前词汇是 `crates/almide-ir/src/effect.rs`（`Effect::ALL`）中的14个叶节点：

| 父节点 | 叶节点 |
|--------|--------|
| `FS` | `FS.read`, `FS.write` |
| `Net` | `Net.fetch`, `Net.listen` |
| `IO` | `IO.stdin`, `IO.stdout`, `IO.stderr` |
| `Env` | `Env.read`, `Env.write` |
| — | `Proc`, `Time`, `Rand`, `Fan`, `Abort` |

父节点授予所列叶节点。`IO` **不**授予文件系统访问；`Proc` 没有父节点。`All` 不是授权名称。未知名称由 `src/project.rs::allowed_effects` 拒绝。推断和清单检查使用 `Effect` 集合，而不是早先提议的13-bit `u16` 表示。

类别按操作分配。注册表与标准库 `@category` 注解为权威依据；参阅 `crates/almide-ir/src/effect.rs` 中的 `STDLIB_MODULE_EFFECTS` 和 `stdlib_call_effect`。当前函数签名应查阅[生成的标准库参考](../../../../stdlib/README.md)，而不是在此另建一份 API 列表。

输出和中止内建操作可以出现在普通 `fn` 中，但仍携带类别：`println` 使用 `IO.stdout`，`eprintln` 使用 `IO.stderr`，`panic`／失败的断言同时使用 `Abort` 与 `IO.stderr`。返回 `err(...)` 不是 `Abort`。effect 位、可失败性和类别集合是不同概念；参阅 [effect 规范](../../../../specs/effect-system.md)与 [ADR-0026](../../../../adr/0026-effect-categories-ride-the-transparent-fn-type.md)。

<a id="declaration"></a>

## 声明

```toml
# almide.toml
[permissions]
allow = ["FS.read", "IO.stdout"]
```

接受带点的叶节点和上述四个父节点。没有 `allow` 键时，不应用类别限制（向后兼容）。显式 `allow = []` 不授予任何类别；它不是某个已实现 `pure` 配置的别名。仅缺少时钟或随机能力，不能证明确定性，也不能消除计时侧信道。

其他权限键有独立用途：

- `proc` 限制进程命令；见下文。
- `fs` 限制文件系统根目录；相对路径以项目根目录为基准。空列表不授予任何根目录；嵌入式宿主默认值见下文。
- `native` 允许依赖包引入原生代码；它不是类别名称，也不是通用 OS 沙箱。参阅 `src/native_grant.rs`。

<a id="inference-and-diagnostics"></a>

## 推断与诊断

`EffectInferencePass` 记录 `IrProgram.effect_map`。其 `effect_flow` 求解器通过直接调用和函数值传播集合，包括传入高阶函数的回调。这已经实现，而非未来的 Phase 4。编译器的类别输入是每个操作的标准库元数据和内建元数据，不是把某模块所有函数一律映射到一种 effect。

外部 `@extern` 函数和非标准库 `@inline_rust` 模板对推断不透明，携带全部叶节点（top）。只有显式类别上限授予所有叶节点时，它们才可通过该上限检查。这不证明外部函数体做了什么。参阅 `crates/almide-codegen/src/pass_effect_inference.rs` 和 `effect_flow/mod.rs`。

使用 `almide check --effects` 查看推断集合。权限检查把传递性需求与清单比较，并在可用时附带来源路径报告缺少的叶节点。诊断以 `error: capability violation in ...` 开头；它**不是 E010**（E010 是模式匹配诊断）。渲染器位于 `src/cli/check.rs` 与 `src/cli/mod.rs`。不要依赖旧的模拟诊断或固定20轮推断限制。

<a id="process-commands"></a>

## 进程命令

`Proc` 授权允许进程操作，后者可通过子程序访问其他资源。命令允许列表不是对子进程 effect 的沙箱：授予 `cargo` 或 shell 可能赋予很大权限。

<a id="what-the-checker-enforces-today"></a>

### 检查器当前强制实施什么

`Proc` 是独立叶节点（ADR-0026）：`[permissions] allow = ["Proc"]` 允许进程操作，没有任何父节点隐含它。`[permissions] proc` 单独限制命令（ADR-0025）：

```toml
[permissions]
allow = ["IO", "Proc"]
proc = ["git", "cargo"]
```

- `exec` 必须指定列表中的命令字面量。`output`、`run`、`start` 接受 `ProcessCommand` 记录，其 `cmd` 必须是这样的字面量。计算得出的命令或作为值传递的启动函数，如果无法检查就会被拒绝。当前 API 请参阅 `src/cli/mod.rs::PROC_SPAWNING` 与 `proc_command_literal`，而不是已退役的 helper 名称。
- `process.kill` 不启动子进程，但向进程发送信号。其 `@signals_process` 注解将它作为命令 `kill` 加以限制；嵌入式宿主也检查这一点（`stdlib/process.almd`、`host_process.rs`）。
- 没有 `proc` 键时命令不受限制；`proc = []` 不授予任何命令。授予 `Proc` 却没有命令列表时会警告，在 `--profile critical` 下则拒绝。

<a id="on-wasm-the-private-almideprocessspawn-import"></a>

### wasm 上：私有 `almide:process/spawn` import

私有子进程接口定义在 [`spawn.wit`](../../../../../crates/almide-wasm-run/wit/process/spawn.wit)。其操作 case 包含旧 ABI slot，并非全部是当前源码级函数。源码 API 请参阅[进程参考](../../../../stdlib/README.md)与 `stdlib/process.almd`。

- 嵌入式 core 宿主通过进程宿主分派提供受支持的同步操作，与原生运行时共享操作。
- 需要进程宿主操作的标准 p1 产物会导入 `almide:process/spawn::call` 并导出 `cabi_realloc`。没有实现此私有 import 的运行时会在实例化时拒绝产物。
- 子句柄家族（`start`、`poll`、`wait`、`stop`、`read_new_stdout`、`read_new_stderr`、`child_pid`）在等待 Wasm 子资源（#4308）期间仅支持原生。不能用“嵌入式宿主提供进程 I/O”概括为已经覆盖它们。
- 组件路由不声明此私有子进程接口。

逐函数、逐路由的可用性记录在 [`target-availability.toml`](../../../../../proofs/target-availability.toml)，不能从 WIT case 的存在推断。ADR-0025 记录了私有宿主决策。

<a id="wasm-imports-and-host-scoping"></a>

## Wasm imports 与宿主范围限制

imports 取决于可达操作和所选产物／宿主路由，不只是允许列表的文本。单个 import 可以服务多个操作：例如，嵌入式 core 宿主通过 `almide.fs_call` 分派文件系统调用。因此 import 的有无不是类别集合的一一对应证明。早先按能力划分的 import 表和二进制大小表是设计草图，不是当前产物保证。参阅 [Wasm 输出](../../../../wasm/WASM-OUTPUT.md)、[宿主概览](../../../../wasm/README.md)和[操作能力注册表](../../../../../proofs/target-availability.toml)。

外部运行时必须提供产物声明的 imports；未解析的 imports 会使实例化失败。标准 WASI 产物获得其运行时授予的权限，例如预打开的目录。这不意味着仅授予 `FS.read` 就等同于运行时的只读 descriptor 策略。

对于嵌入式宿主，`[permissions] fs` 已实现：

- core 模块的文件系统操作在配置根目录下检查并执行。64-bit Linux 和 macOS 已实现 descriptor 相对约束；其他平台使用仍受检查／使用竞争影响的路径检查（`fs_beneath_sys.rs`）。没有 `fs` 键时，此控制不限制路径。
- 组件宿主以读写方式预打开配置的根目录。没有该键时，预打开工作目录和临时目录，而非 `/`。

实现在 `crates/almide-wasm-run/src/host_fs_scope.rs` 与 `component_host.rs`。这些控制不会把嵌入式宿主变成任意不受信任代码的通用隔离环境。尤其需要单独考虑进程权限和外部原生代码。

<a id="per-dependency-restriction-future"></a>

## 按依赖限制（未来）

```toml
[dependencies.json_parser]
version = "1.0"
allow = []              # pure only -- no I/O, no net, nothing

[dependencies.http_client]
version = "2.0"
allow = ["Net.fetch"]   # can fetch, nothing else
deny = ["Proc"]         # explicitly deny even if parent allows

[dependencies.code_formatter]
version = "0.5"
allow = ["FS.read"]     # read-only access to source files
```

提议的行为：拒绝传递调用图超出其逐依赖授权的依赖。当前顶层 `[permissions].allow` 检查尚未实现此语法与强制机制。

该提案将建立信任层级：顶层项目声明自身能力，各依赖被限制为其子集。这不是当前的约束保证。

<a id="proposed-profile-names-not-implemented"></a>

## 提议的配置名称（未实现）

早先设计把常用权限集合命名为 `pure`、`readonly-agent`、`readwrite-agent`、`network-agent` 和 `full-agent`。`[permissions] profile` 不是已实现的清单键；不要用它请求限制。CLI 的 `--profile critical` 是独立功能，不是这些别名之一。

应改为编写显式 `allow` 列表，例如：

```toml
[permissions]
allow = ["FS.read", "IO.stdin", "IO.stdout"]
fs = ["workspace"]
```

这授予所列类别和文件系统根目录；并不证明所有可观测行为都是确定的，也不证明每项宿主资源均被约束。需要其他操作时，显式扩展列表。
