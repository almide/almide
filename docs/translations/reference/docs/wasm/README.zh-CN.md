<a id="almide-wasm-documentation"></a>

# Almide WASM 文档

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)

> 非规范性、固定原文的读者快照。未修改的原本仍为权威来源；不自动同步，未独立执行、重新测量、验证安全性或进行第三方认证。下文“当前”“已实现”和断言描述固定原文，不是本次重新评估。嵌入式宿主不是通用沙箱；权限键缺省时及 core/component 默认值不同。保留原文所述证书范围不包括组件的限制。逐依赖授权及具名权限配置仍属未来提案。代码和注释保持原样。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/README.md)

Almide 通过**唯一一条 wasm 路径**——结构化路径（从 `src/cli/build.rs` 中的 `render_wasm_module_routed` 进入）输出 core Wasm 模块和 WASI 组件。未经验证的 v0 发射器已在 #782 退役，原有的 v1 路径（`crates/almide-mir` 中的 MIR→WAT 信任主干）在 #2752 失去最后一条路由，并在 #2761 删除。结构化路径无法 lowering 的形态会形成明确的边界：带有 `wall: <reason>` 行和 `--> in <fn>` 位置行的 `error[E082]`，绝不会悄悄回退。

| 路径 | 引擎 | 路由到此的对象 |
|---|---|---|
| **structural**（唯一路径） | 已投入使用的全新引擎：`almide::wasm_leg` 前端加 `crates/almide-wasm` 直接发射器。core 模块被打包用于标准 WASI 或支持的组件路由。所有权采用编译器放置的引用计数和写时复制。发射器记录证书包；`almide verify` 和 certified-corpus 门禁检查支持的 core 产物所覆盖的属性，不检查组件。差分和变异门禁提供独立的行为证据 | 在结构化发射器支持的形态以及所选宿主路由能力范围内的程序和导出库 |

强制使用原有路径的开关已随该路由删除（#2752）；现在 `ALMIDE_WASM_SKIP_STOCK_AUDIT=1` 仅跳过构建中的标准 WASI 宿主操作审计（用于开发能力边界的探针）。

<a id="what-ships-today"></a>

## 当前提供什么

| 属性 | 已提供的行为 |
|---|---|
| 目标 | core `wasm32-wasip1` 命令和支持的 WASI 组件路由；HTTP serve 形态的程序构建为 WASI 0.3 处理器组件 |
| 内存 | bump 分配 + **Perceus 风格引用计数**。对于支持的 core 产物（不包括组件），结构化证书由 `almide verify` 和 certified-corpus 门禁检查，并受[明确的覆盖范围与信任限制](../../../../design/MEMORY-SAFETY.md)约束；运行时字节由 `proofs/check-structural-bytes.sh` 对照 Coq 解码器模型检查 |
| 等价性 | 按契约逐一比较可观测输出；声明的宿主观测、资源限制及相对于 oracle 的行为仍保留各自文档规定的范围 |
| 边界 | 超出 lowering 子集 ⇒ 向用户显示指出边界和函数的 `error[E082]`；在 `almide test` 下，`ALMIDE_WALL_REASON=1` 指出哪个阶段拒绝处理 |

<a id="running-a-server-on-wasm"></a>

## 在 wasm 上运行服务器

`almide run app.almd --target wasm` 在**嵌入式宿主**（`almide` 二进制内的 wasmtime）上运行结构化路径模块，该宿主提供 `http.serve`（C-367，#2650）。服务循环由 guest 拥有：`main` 运行一次，调用 `http.serve`，之后在同一实例中每次从宿主接收一个请求并交回一个响应。因此，`main` 在 `http.serve` 之前计算的值对所有请求都相同，与原生执行一样。宿主只通过原生运行时使用的服务器代码（`crates/almide-rt-core/src/http_server_core.rs`）搬运字节（fs_call ops 70..=72），所以响应字节与原生相同。程序运行时，stderr 不缓冲，stdout 遵循原生规则（写向终端时每次写入都刷新，否则采用64 KiB缓冲），所以服务器输出会在运行过程中显示。

上述嵌入式路由不同于标准服务器产物。用 `almide build --target wasm` 构建受支持的 serve 形态程序，会生成供 `wasmtime serve` 使用的 `wasi:http/handler@0.3.0` 组件（C-375，#2659）。构建会验证该形态及组件可用的操作；`--host js` 不支持此服务器导出。路由选择请参阅 `src/cli/build_wasm.rs`，不要把每条 Wasm 路由当作同一个宿主。

<a id="what-the-embedded-host-grants-it-is-not-a-sandbox"></a>

## 嵌入式宿主授予什么权限（它不是沙箱）

`almide run --target wasm` 与原生二进制一样，以运行用户的权限执行程序。Wasm 对 guest 自身的内存访问施加边界。清单权限可以限制声明的能力和文件系统根目录，但这些控制不构成通用的进程约束保证。不要把嵌入式宿主视为任意不受信任代码的隔离环境。

| 通道 | 何时使用 | 文件 | 环境 | 其他宿主访问 | 内存 |
|---|---|---|---|---|---|
| core 模块（`almide.*` imports，`crates/almide-wasm-run/src/host.rs`） | 大多数程序 | 配置了 `[permissions] fs` 根目录时，通过限定范围的文件系统操作强制实施；没有该键时，此控制不限制路径 | 进程环境（读取；`env.set` 写入每次运行的覆盖层，而不是进程环境） | 进程启动、tcp / unix 套接字、共享内存、http 客户端和服务器、stdin、时钟，均与原生一样 | 每次运行只有一个预算，覆盖该运行的实例、其 `fan` worker 实例以及 `testing.assert_aborts` 子实例（#4135）：若测试框架指定上限则使用该值，否则取从环境读取的4 GiB（wasm32地址空间）、宿主 cgroup 内存限制与物理内存中的最小值（C-197） |
| p3 组件（`crates/almide-wasm-run/src/component_host.rs`） | core 通道仅能通过 WASI 0.3 提供其宿主操作的程序（http 客户端家族，ADR-0023 step 7） | 配置的 `[permissions] fs` 根目录以读写方式预打开；没有该键时，预打开工作目录和临时目录（不是 `/`） | 完整继承，另加 `ALMIDE_CWD` | 继承 stdio；wasi:http 通过原生客户端；wasi:sockets 由 wasmtime-wasi 提供 | 除 wasm32 每个内存4 GiB外，没有 store 限制 |

文件系统范围限制已实现（#4022）：core 宿主在64-bit Linux/macOS上检查并执行配置根目录之下的文件系统操作；其他平台保留基于路径的检查，仍受检查/使用竞争影响（`crates/almide-rt-core/src/fs_beneath_sys.rs`）。组件宿主预打开这些根目录，未配置根目录时预打开 cwd/temp。各通道默认值不同，且都不提供通用进程约束。标准产物获得外部运行时提供的权限，例如 `wasmtime run --dir=<dir>`。权限实现在 `crates/almide-wasm-run/src/host_fs_scope.rs` 和 `component_host.rs` 中。

在 core 模块通道中，超出 guest 内存的 guest 指针或长度会导致该 guest 的 trap（`Error: wasm trap: out of bounds memory
access`，退出码1）。它绝不会成为宿主 panic，宿主也绝不会按伪造的长度分配缓冲区（#4135）。组件通道中，wasmtime 的 canonical ABI 检查同样的范围。宿主输出按 guest 字节写入，绝不重新编码（C-399）。

权威参考如下：

- **架构** — [docs/ARCHITECTURE.md](../../../../ARCHITECTURE.md)（包括 wasm 路由的编译器流水线），以及 [docs/roadmap/active/v1-mir-architecture.md](../../../../roadmap/active/v1-mir-architecture.md)（为何所有权和布局只在 MIR 中决定一次——这是 #2761 已退役的原有路径设计；MIR 现在用于原生路径）。
- **跨目标等价性** — [docs/contracts/](../../../../contracts)：每项可观测的 native ⇄ wasm 承诺都有具名契约，可追溯到可执行 fixture。
- **所有权证书** — [docs/roadmap/active/certificate-format-v1.md](../../../../roadmap/active/certificate-format-v1.md)。

<a id="documents-here"></a>

## 此处的文档

| 文档 | 内容 |
|-----|------|
| [能力系统](../../../../wasm/capability-system.md) | 编译时最小权限强制实施——当前 |
| [JS 宿主异步 imports](../../../../wasm/JS-HOST-ASYNC-IMPORTS.md) | `--host js` 上的 `@extern(wasm, "js", ..., returns: promise)`：JSPI 挂起、哪些 exports 成为异步、串行化、无 JSPI 时拒绝、未标记 Promise 时拒绝（#3353，#3371）——当前 |

<a id="archive"></a>

## 归档

2026-04在 v1 信任主干成为唯一 wasm 路径**之前**编写的设计笔记（内存模型、WASM 3.0功能、agent container、hatch、bubblewrap、生态调查）已从树中删除。它们描述的是 agent-container 产品方向，以及已提供编译器未实现的 v0 时代内存模型（两个线性内存、只 bump 分配而不释放）；把它们留在 `docs/` 会让人将其当作现有功能说明找到。它们仍保留在 git 历史中；`git log --diff-filter=D -- docs/wasm/archive/` 可找到删除提交。
