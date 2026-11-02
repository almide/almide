<a id="architecture"></a>

# 架构

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](ARCHITECTURE.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](ARCHITECTURE.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](ARCHITECTURE.zh-CN.md)

> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/ARCHITECTURE.md)的非规范读者译文；原文仍是权威版本，本译文不会自动同步。




> 非规范性读者译本。固定于原文提交 `35297a612fb29e3f78f6587aba253aa91d51f466`，不自动同步。以原文和所引用实现为准，保留代码、注释及命令。性能值、所有权检查和证明描述限于原文范围及时间点，并非翻译时独立执行或安全保证。表 B 的 `Target` 描述与代码生成规范中 Wasm tombstone 的表述差异、第 37 行结构化路径单元缺失、图文中的历史路径描述均保留原样。

> 原文将 interp 称为“可执行规范”的表述，以及表 B 第 21 行对 Result/String、Try 的概括，也按原文保留。规范性效力以及按上下文区分的返回值与传播规则，应参阅权威原文 [SPEC.md](../../../SPEC.md) 和 [STABILITY.md](../../../STABILITY.md)。不应将这些概括理解为独立规范，或无条件采用 String 错误和自动传播的保证。

Almide 是 Rust 工作区，包含编译器、运行时和后端 crate，以及独立验证工具。[`Cargo.toml`](../../../../Cargo.toml) 是工作区成员和排除项的权威来源；默认构建同时包含 `almide` 及独立检查器 `almide-verify`。Lean 4 证明项目 `almide-perceus-belt` 不是 Cargo crate。主要依赖：`serde` + `serde_json`（AST 序列化）、`toml`（模板加载）、`clap`（CLI）、`lasso`（字符串驻留）、`wat` + `wasmparser` + `wasm-encoder`（WASM 汇编／验证）、`lsp-server` + `lsp-types`（语言服务器）。

<a id="pipeline"></a>

## 流水线

```
  .almd source
       │
       ▼
  ┌─────────┐   ┌──────────┐   ┌─────────┐   ┌──────────────┐
  │  Lexer   │──▶│  Parser  │──▶│   AST   │──▶│ Type Checker │   almide-syntax / almide-frontend
  └─────────┘   └──────────┘   └─────────┘   └──────┬───────┘
                                                     │  expr_types + env
                                                     ▼
                               ┌───────────────────────────────┐
                               │   Lowering (AST → Typed IR)   │   almide-frontend
                               └──────────────┬────────────────┘
                                              ▼
                               ┌───────────────────────────────┐
                               │ optimize → verify → mono →    │   almide-optimize
                               │ ir_link                       │
                               └──────────────┬────────────────┘
                          ┌───────────────────┼──────────────────────┐
                          ▼                   ▼                      ▼
                 Rust target          WASM target             WGSL target
                 codegen v3           structural leg          emit_wgsl
                 (nanopass + TOML     (wasm_leg →             (almide-codegen)
                 templates + walker)  almide-wasm, direct
                                      emit; wall = E082)
                          │                   │
                          ▼                   ▼
                 native trust-spine      .wasm (optional
                 (v1 Rust render,        --wasm-opt with
                 v0 fallback on wall)    parity gate)
                          │
                          ▼
                 rustc/cargo → binary
```

`almide-interp` 是与后端并列的第三个执行器：对链接后的 IR 进行树遍历的解释器（在 `lower → optimize → mono → ir_link` 之后、任何目标 lowering pass 之前）。它不与任一后端共享代码生成 pass，充当跨目标判定基准／可执行规范。

<a id="targets"></a>

### 目标

- **native（默认）**——codegen v3 输出 Rust 源码；v1 native trust-spine 渲染器（`almide-mir`）在可 lowering 的范围内取代它（遇到 wall 时回退至 v0 codegen 源码）；`rustc`/`cargo` 生成二进制。
- **`--target wasm`**——只有结构化这一条路径（`src/cli/build.rs::render_wasm_module_routed`）：先运行低成本 PROJECT-SHAPE 检查，结构化 wall 是硬性 E082 错误（见下文）。路由统一为一个库函数 `almide::wasm_route::route_wasm`（src/wasm_route.rs，#2554）：CLI 从环境读取 probe 开关到 `RouteOptions`，并渲染 `RouteError`；wasm32 使用方（playground）以预解析的标签页（`ModuleSource::Provided`）调用 `render_wasm_routed`，得到相同路径（`tests/wasm_route_playground_test.rs` 测试此形式）：
  - **正式采用的结构化路径**（默认）：`almide::wasm_leg`（parse→check→lower→self-host link→`link_ir`）将输入交给 `almide-wasm::emit_program`，结构化输出 wasm 字节（wasm-encoder，无 WAT 文本）。差分门禁在版本化 wasm_cross 语料上将可观察输出与 native 对比。`almide run` 在嵌入式 `almide-wasm-run` 宿主上运行（限于其支持的操作集合）。`almide build` 通过 `to_wasi` 封装核心模块，或根据标志及宿主操作封装组件（`src/cli/build_wasm.rs::WasmPackage`）；serve 形态的 HTTP 程序变为 WASI 0.3 handler 组件。标准运行时必须支持产物接口；进程操作需要 Almide 的私有宿主能力。标准运行时的 parity 门禁覆盖其声明的 fixture 子集。`almide-wasm-vm` 接受的封闭核心模块子集可在这个限定认证范围的解释器上运行（#865）；它不是组件或任意宿主运行器。加载时验证器拒绝不支持的形态。`tests/wasm_vm_parity_test.rs` 将其接受的语料与标准运行时比较，并将 Critical 配置 fixture 与 native 比较。
  - **原有 WAT trust-spine**（`almide-mir` 渲染 WAT）已不存在：#2752 移除最后的路由，#2761 删除渲染器。`almide-mir` 保留 MIR lowering（native 路径）、native 渲染器和 MIR 证书见证生成器（proofs/gate.sh、corpus-wall.sh；自 #4011 起，`almide verify` 判定结构化路径的 bundle）。
  代码树读取的每个 `ALMIDE_*` 开关——门禁绕过、消融、跟踪通道——都属于同一注册表 `almide_base::env::SWITCHES`（`almide switches` 列出它；`docs/specs/cli.md` 嵌入其表格；#2205）。编译器本体只通过 `almide_base::env::{flag, var}` 读取：统一的布尔语义，以及从环境继承的强制路由或绕过门禁都会在 stderr 通告一次，使此条件下的判定明确说明这一点。
  结构化 WALL 是不可回退的诊断错误 `error[E082]`——包含 `wall: <reason>` 行和 `--> in <fn>` 位置行——绝不回退（#782 的问题是落入未验证 v0 codegen；`ALMIDE_VERIFIED_DEBUG=1` 给出 wall 名称）。`--wasm-opt` 需主动启用，并由与已验证模块进行差分比较的 parity 门禁保护（`tests/wasm_runtime_opt_parity.rs::wasm_opt_parity_spec`）。
- **`--target wasm32` / `wasi`**——用裸 `rustc --target wasm32-wasip1` 编译生成的 Rust 源码（启用 SIMD128）。与 `--target wasm` 不同。
- **`--target wgsl`**——通过 almide-codegen 中的 `emit_wgsl` 生成 GPU 计算着色器。

`--verified` 为默认（`--no-verified` 已弃用并发出警告）。`almide run --target wasm` 与 `almide build --target wasm` 共享 `compile_to_wasm_bytes`，因此 run 和 build 采用同一输出过程。在结构化路径上，BUILD 产物按所选 core/component 路径封装；嵌入式宿主模块与交付产物的字节不必相同。

<a id="crate-structure"></a>

## Crate 结构

下列是部分组件的导航图，并非完整工作区清单。全部成员见 [`Cargo.toml`](../../../../Cargo.toml)。

```
almide/                    Workspace root
├── src/                   CLI binary (almide)
│   ├── main.rs            Subcommands: init, run, build, test, check, lsp,
│   │                      explain, fmt, compile, clean, add, deps, dep-path,
│   │                      install, self-update, ide, fix, docs-gen
│   │                      (no subcommand → REPL)
│   ├── compile_driver.rs  Shared front half: parse → check → lower → optimize
│   ├── resolve.rs         Module resolution (filesystem + git deps)
│   ├── project.rs         almide.toml parsing, PkgId
│   ├── project_fetch.rs   Git dependency fetching
│   └── cli/               Subcommand implementations; lsp.rs is the LSP server
│                          (lsp-server over stdio); repl.rs; ide.rs = semantic
│                          queries for agents (outline, doc, stdlib-snapshot)
│
├── crates/
│   ├── almide-base/       Foundation: Sym interning (lasso), Span, Diagnostic
│   ├── almide-syntax/     Lexer, recursive-descent parser, untyped AST
│   ├── almide-types/      Resolved Ty, unification, protocol defs,
│   │                      stdlib_info.rs (module registry, auto-import lists,
│   │                      bundled stdlib sources via generated/embedded.rs)
│   ├── almide-lang/       Facade crate: re-exports syntax + types (no logic)
│   ├── almide-frontend/   Type checker (Infer → Solve → Resolve), AST→IR
│   │                      lowering, canonicalize, ir_link
│   ├── almide-driver/     Shared post-typecheck driver: optimize → mono → ir_link
│   ├── almide-ir/         Typed IR: IrProgram/IrExpr (every node carries ty),
│   │                      VarId/VarTable, visitors
│   ├── almide-mir/        v1 Middle IR — ownership/layout for its native
│   │                      renderer (Perceus). #![forbid(unsafe_code)].
│   │                      MIR lowering + the native trust-spine renderer
│   │                      + MIR certificate witnesses (proofs/gate.sh).
│   │                      No wasm/WAT renderer since #2761.
│   ├── almide-optimize/   Monomorphization, DCE, constant propagation,
│   │                      stream fusion
│   ├── almide-codegen/    Codegen v3 for Rust (+ WGSL): nanopass pipeline,
│   │                      TOML template renderer, target-agnostic walker;
│   │                      templates/rust.toml = Rust syntax templates (~330 rules)
│   ├── almide-wasm/       Structural wasm emitter and emission witnesses
│   ├── almide-wasm-run/   Embedded host for emitted wasm modules
│   ├── almide-wasi/       Transform emitted modules into stock-WASI artifacts
│   ├── almide-verify/     Independent certificate checker, shipped beside almide
│   ├── almide-anchor-check/  Check located witness events against shipped wasm bytes
│   ├── almide-interp/     Pre-codegen IR interpreter — 3rd cross-target oracle
│   ├── almide-tools/      Formatter, module interface (almide compile), ALMDI
│   ├── almide-dialect/    Pure-Rust MLIR dialect schema (no FFI)
│   ├── almide-egg-lab/    Equality-saturation engine used by EggSaturationPass
│   ├── almide-kernel/     [cargo-excluded] target-specific SIMD numeric kernels
│   └── almide-perceus-belt/  [Lean 4, not a cargo crate] Perceus RC/ownership
│                             proof belt
│
├── grammar/               Git submodule → almide/almide-grammar: descriptive
│                          keyword/precedence data consumed by the tree-sitter
│                          and TextMate generators (not by the compiler build)
├── stdlib/                Self-hosted .almd modules (see below)
└── runtime/rs/src/        Native Rust runtime for @intrinsic functions
```

<a id="codegen-v3-rust-target-three-layer-architecture"></a>

## Codegen v3（Rust 目标）：三层架构

所有语义决策都在输出文本之前于 IR 中完成。walker 只看到有类型的 IR 节点，从不检查自己在为哪个目标渲染。

1. **Nanopass 流水线**——每个 `pass_*.rs` 接收 `&mut IrProgram`，执行一次语义重写（StdlibLowering、ResultPropagation、CloneInsertion、BuiltinLowering、FanLowering 等）。顺序是声明的，而不只是写出来：每个 pass 指出所需前置 pass（`depends_on`）及必须排在它之后的 pass（`run_before`）；三个表示边界构成屏障；`scripts/check-pass-shuffle.sh` 运行这些声明允许的带 seed 随机顺序，并对输出 Rust 作逐字节差异比较，以检验声明完整表达了约束（#2186）。
2. **模板渲染器**——TOML 文件定义语法模式；walker 调用 `templates.render_with("if_expr", ...)`。所有字符串渲染都在这里发生。
3. **Walker**——目标无关的 IR 树渲染器；没有 `if target == Rust` 检查，也不作自己的所有权决策（#2186）：某处值是移动、克隆、借用还是拥有，由 pass 判定，以 IR 节点（`Borrow`、`Clone`、`Deref`、拥有式方法调用）或 `CodegenAnnotations` 条目（`var_storage`、`shared_mut_vars`、`borrowed_loop_vars`、`param_borrows` 等）传给 walker。walker 按存储类别选择拼写，仅按节点种类渲染。它不进行分析遍历，不保存逐函数所有权集合，也不根据刚渲染的文本作决策；`scripts/check-walker-reads-annotations.sh` 维护此边界。所有权判定本身不仅靠 fixture，还由性质支撑：`tests/native_borrow_oracle_test.rs` 针对每种可借用类型，生成函数体对参数的全部用法与全部调用处形态的组合，断言程序通过检查、原生构建并输出与 wasm 路径一致的结果；在 `ALMIDE_ALLOC_COUNT=1`（原生构建注入的计数分配器）下，还精确匹配 `tests/golden/native-borrow-oracle-alloc.txt` 为每个程序固定的 (allocs, deallocs) 对。可以借用却克隆不会改变 stdout 或构建，只改变计数（#2228）。wasm 路径也有对应机制：`ALMIDE_WASM_ALLOC_COUNT=1` 让结构化发射器在四个导出全局量中记录 `$alloc` / `$free`（关闭时是不存在，而不是为零），`crates/almide-wasm/tests/alloc_ledger.rs` 在 `__heap` 高水位旁为每个语料 fixture 固定 `allocs reused bytes frees`，因此高水位看不到的 churn 也受到棘轮约束（#2407；[docs/wasm/WASM-OUTPUT.md](../../../wasm/WASM-OUTPUT.md)）。
   判定本身在每次 DEBUG native 构建及主动请求的 release 构建中认证（`ALMIDE_CERTIFY_OWNERSHIP=report|fail|off`、`crates/almide-codegen/src/certify_ownership.rs`、#2231）：重新遍历最终 IR，将每个参数的借用模式及每个 `Clone` 与实际使用方式核对——借用参数被消费、拥有值在最后一次使用时被克隆、没有使用处需要拥有却拥有的参数。语料台账（`proofs/ownership-certifier-baseline.txt`、`scripts/check-ownership-certifier.sh`）只允许缩减且为空；违反条件就是缺陷，debug 构建拒绝输出它。

<a id="wasm-backend-structural-leg"></a>

## WASM 后端（结构化路径）

wasm 后端是结构化路径（`src/wasm_leg.rs` → `almide-wasm::emit_program`）；曾由 `almide-mir` 提供的 v1 MIR→WAT 流水线已在 #2761 删除。

- 所有权／RC 遵循 Perceus（线性内存中的 Perceus 式 RC）。Coq 定理 WasmEncode/WasmExec/WasmRcDec 针对的是建模运行时（已退役旧实现的运行时）；结构化运行时字节由 `proofs/check-structural-bytes.sh` 对照 `StructuralDecode.v` 检查。结构化路径记录逐构建见证；`almide verify` 将其检查委托给独立的 `almide-verify`。对受支持的核心产物（`almide verify` 拒绝组件），所提供或在默认路径发现的产物通过 SHA-256 绑定至 bundle（#4011）。认证语料由 `proofs/structural-wall.sh` 执行。这不是语义正确性或整个产物内存安全的证明：覆盖及记录假设仍然存在。参见 [已证明与受信任的边界](../../../contracts/proven-vs-trusted.md) 和 [内存安全缺口表](../../../design/MEMORY-SAFETY.md#the-gap-table-2026-10-24-4011)。
- stdlib 调用是*自托管*的：`stdlib/*.almd` 的实现与用户代码一起编译。新的自托管函数体声明 `@implements("module.fn")`，构建从声明生成 `crates/almide-types/src/generated/implements.rs`。`self_host_registry.rs` 将声明条目与只缩不增的旧表组合（`tests/name_keyed_sites_test.rs`），该表不是用于扩展新函数体的。未链接的 stdlib 调用形成 wall（硬错误）。
- `wasmparser::validate` 保护测试框架。`almide test --target wasm` 通过与 `build`/`run --target wasm` 相同的路径渲染（#2179）：结构化路径（`almide-wasm`，包含来自 `almide_driver::test_runner` 的共享 `__test_runner` 合成及结构化测试内 assert lowering）；遇到 wall 的文件报告为 WALL，随后默认 `almide test` 路径以 native 运行它。

<a id="optimization-pass-roster-per-target"></a>

## 各目标的优化 pass 清单

此处列出每个优化或重写 pass 的运行位置和目标（#929）。门禁 `scripts/check-pass-roster.sh` 枚举 `almide-codegen` 的 `pass_*.rs` 文件和 `NanoPass::name()` 字符串，以及 `almide-optimize` 的 pass 模块与 `ALMIDE_ONLY_PASS` 轴名称；本节漏掉任一项就失败，因此清单不能偏离代码。差异的测量成本是 [docs/benchmarks/wasm-runtime.txt](../../../benchmarks/wasm-runtime.txt) 中各程序的 wasm/native 运行时间比，由 `scripts/check-wasm-runtime-ratio.sh` 作棘轮约束。

各路径运行什么：

| 路径 | 选择条件 | 分流前（共享） | 目标侧 pass |
|---|---|---|---|
| native，v1 MIR 渲染 | `almide run`/`build` 默认（`--verified`），在 `almide-mir` 可 lowering 的范围 | `link_ir` | almide-mir native 阶段（表 D 的 native 行） |
| native，codegen v3 | `almide test`（native），以及 v1 native wall 的回退；`--target rust` 输出；`--target wasm32`/`wasi` | `link_ir` | almide-codegen nanopass 流水线 Rust 分支（表 B） |
| `--target wasm`，结构化（唯一 wasm 路径） | `src/wasm_leg.rs` → `almide-wasm::emit_program` | `link_ir` | almide-wasm 发射器重写（表 C） |
| `--target wgsl` | `almide-codegen::emit_wgsl` | `link_ir` | almide-codegen nanopass 流水线 Wgsl 分支（表 B，4 个 pass） |
| interp（判定基准） | `almide-interp` | `link_ir` | 按设计不运行 |

<a id="a-shared-pre-split--almide-driverlink_ir-cratesalmide-optimize"></a>

### A. 共享、分流前——`almide-driver::link_ir`（`crates/almide-optimize`）

所有路径和 interp 判定基准使用的唯一阶段顺序（若出现第二种写法，`tests/one_driver_test.rs` 会失败）。`optimize_half` = 第 0–7 行；`link_half` = 第 8–9 行。`ALMIDE_DISABLE_OPT=1` 跳过四个性能行（性能棘轮的消融路径）；`ALMIDE_ONLY_PASS=sink|fold|dce|propagate` 仅运行其中一个（由 CI 的 `checks` 作业对 `spec/pass_isolated/` 运行 `scripts/check-pass-isolated.sh`）。

| # | Pass | 模块 | 类别 | 作用 |
|---|---|---|---|---|
| 0 | `sink` | `optimize/sink_inline.rs` | 优化（性能） | 将整个函数体仅为一次 `list.*` 调用、且唯一参数作为第一实参的根函数（读取一次，无其他自由变量）内联进实参为 `list.*` 调用的调用处，绑定变量重命名为新名称，使各路径自己的融合看到一条链（#3584、#3590）；不采用此优化的路径仍按原调用求值 |
| 1 | `fold` | `optimize/mod.rs` (`constant_fold`) | 优化（性能） | 自底向上以 wrapping 方式求值字面量算术，替换前面的不可变标量 top-let（#809） |
| 2 | `dce` | `optimize/dce.rs` | 优化（性能） | 移除值为纯计算的未使用绑定，在传播后再次运行 |
| 3 | `propagate` | `optimize/propagate.rs` | 优化（性能） | 将绑定到字面量的变量替换为该字面量 |
| 4 | 无符号再折叠 | `optimize/mod.rs` (`refold_unsigned_lane`) | 正确性 | 以无符号方式重新折叠传播暴露的 `UInt64` `/`/`%` 字面量对（#872），绝不消融 |
| 5 | optional-chain 脱糖 | `optimize/optional_chain.rs` | lowering 支撑 | `p?.f` → 两个后端均证明的合成尾部辅助函数调用，绝不消融 |
| 6 | 分支提升 | `optimize/branch_lift.rs` | lowering 支撑 | 堆类型 `let` 绑定的 `if`/`match` → 尾部辅助函数（trust-spine 渲染器可 lowering 的形态），绝不消融 |
| 7 | top-let 重新分类 | `almide_ir::reclassify_top_lets` | 分析 | 折叠后对 top-let 作 const／运行时分类 |
| 8 | monomorphize | `mono/` (`monomorphize`) | 特化 | 按调用处类型克隆带结构约束的泛型函数；最后执行 `mutual_tco`（`mutual_tco.rs`：相互递归尾调用 → 一个循环） |
| 9 | ir_link | `almide-frontend::ir_link` | 链接 | 单态化调用图形成后，将依赖模块链接到根模块 |

<a id="b-almide-codegen-nanopass-pipeline-native-codegen-v3--wgsl"></a>

### B. `almide-codegen` nanopass 流水线（native codegen v3 / WGSL）

执行顺序为 `crates/almide-codegen/src/target.rs::build_pipeline`。pass 的 `Target` 枚举为 `{Rust, Wgsl}`——**wasm 路径从不进入此流水线**；Targets 列的“all”指 `targets()` 声明的 Rust + Wgsl，这只是使 pass 有资格在分流前运行，不是应该如此的证据。Wgsl 分支为标有 W 的四行。类别：

- **按设计仅用于 Rust**——因 Rust 的所有权、借用、`Result`、宏或 `LazyLock` 而存在，移植到 wasm 没有意义。
- **支撑**——TOML walker 所需的重写／分析；结构化路径有自己的具名对应实现，不需移植。
- **优化**——性能重写；“结构化路径”列说明是否存在对应实现，分流前迁移决策见 E。

| # | Pass（`name()`） | 文件 | 目标 | 类别 | 作用 | 结构化 wasm 路径 |
|---|---|---|---|---|---|---|
| 1 | `UnifyVarTables` | `pass_unify_var_tables.rs` | 全部，W | 支撑 | 将所有 `IrModule.var_table` 合并到程序表 | 读取逐模块表（`build_globals`） |
| 2 | `ListPatternLowering` | `pass_list_pattern.rs`, `pass_list_pattern_nested.rs` | 全部 | 支撑 | 列表模式 → 长度检查 + 索引；构造器／option／记录位置下的列表就地重写（#3413） | 原生 lowering `IrPattern::List`（`patterns.rs`） |
| 3 | `LambdaTypeResolve` | `pass_lambda_type_resolve.rs`, `pass_lambda_type_lookup.rs` | 全部，W | 支撑 | 从 stdlib 被调用方签名推导闭包参数类型（自顶向下） | `TypeTable` （`types_table.rs`） |
| 4 | `ConcretizeTypes` | `pass_concretize_types.rs`, `pass_concretize_types_call_ret.rs`, `pass_concretize_types_signatures.rs`, `pass_concretize_types_unresolved.rs`, `pass_concretize_types_walker.rs` | 全部，W | 支撑 | 将每个 `IrExpr.ty` 与其权威具体类型同步 | `TypeTable` |
| 4b | `MeteredRegionDelegate` | `pass_metered_region_delegate.rs` | Rust | 按设计仅用于 Rust | 当整个程序在 v1 遇到 wall 时，计量区域函数（`fan.bounded` / `fan.min` / `fan.timeout`）成为对 CLI 链入的 v1 渲染区域模块的委托（#3669）；计量器在 MIR 中，此 codegen 没有 | 不适用（结构化路径自行计量其区域） |
| 4c | `MeterCharge` | `pass_meter_charge.rs` | Rust | 按设计仅用于 Rust | 当 CLI 在此 codegen 中计量程序时（v1 子集外的区域，#3673），在结构化路径计量的 IR 位置施加确定性消耗：共享 `almide_ir::meter_plan` 未豁免的入口、循环头、闭包跳转、字符串拼接，各带其 cut；`MeteredRegionDelegate` 和 `LICM` 退让 | 相同计划（`fuel.rs` 读取 `almide_ir::meter_plan`） |
| 5 | `PatternLiteralGuard` | `pass_pattern_literal_guard.rs` | Rust | 按设计仅用于 Rust | 将载荷内嵌套的字符串字面量提升到守卫（`as_deref` 对象形式） | 不适用 |
| 6 | `ResolveCalls` | `pass_resolve_calls.rs` | 全部 | 支撑 | 验证并将每个 `CallTarget::Module` 重写为已知函数 | `FnTable` 解析（`emit.rs`） |
| 7 | `RegionWindow` | `pass_region_window.rs`, `pass_region_window_clone.rs` | Rust | 优化 | `consume(produce(scalars))` 位置使用前导部分线程局部 bump arena 中的 `__rgn_` 对应函数及 `Copy` 对应 enum（#1991）；v1 接受根模块函数及带标量／region-enum 元组载荷的根 variant enum | 对应：`region.rs`（分配器上的 `RegionSave`/`RegionRestore`，#1961） |
| 8 | `BoxDeref` | `pass_box_deref.rs` | Rust | 按设计仅用于 Rust | 对从 `Box` 字段绑定的模式变量使用 `*deref` | 不适用 |
| 9 | `LICM` | `pass_licm.rs`, `pass_licm_hoist.rs`, `pass_licm_purity.rs` | 全部 | 优化 | 将循环不变的纯表达式提升为循环前的 `let` | **无对应实现**——见 E |
| 10 | `EggSaturation` | `pass_egg_saturation.rs` | 全部 | 优化 | 矩阵链的等式饱和融合（`almide-egg-lab`，规则来自 stdlib `@rewrite`）；列表分支自 #2045 起关闭（其 lambda 替换重复执行回调副作用） | **无对应实现**——见 E |
| 11 | `MatrixShapeSpec` | `pass_matrix_shape_spec.rs` | Rust | 按设计仅用于 Rust | 小形状矩阵乘法 → 完全展开的 `InlineRust` | 不适用（手写内核，`matrix_kernels.rs`） |
| 12 | `ConstFold` | `pass_const_fold.rs` | 全部 | 优化 | 折叠第 10–11 行留下的字面量算术 | 已由 A.1（`fold`）替代——见 E |
| 13 | `IntrinsicLowering` | `pass_intrinsic_lowering.rs` | 全部 | 支撑 | `@intrinsic` stdlib 调用 → `RuntimeCall { symbol }` | 自托管注册表链接（`src/wasm_leg.rs`）；intrinsic 是 wall |
| 13b | `StreamFusion` | `pass_stream_fusion.rs` | Rust | 优化 | 带 lambda 字面量的 `RuntimeCall { almide_rt_list_* }` → `IterChain`，在 `BorrowInsertion` 之前执行；链 lambda 成为 borrow／capture-clone／clone pass 不视作闭包的作用域，因此借用读取内容，渲染时无 `move`。源起初为消费模式，`BorrowInsertion` 根据步骤如何使用每个元素决定模式，而非对应实现的 `@consume` 槽（#2287：`element_reads_only`——`Copy` 元素或所有接收 lambda 都只读的堆元素，使源保持 `&[T]`，由克隆 pass 从 `.iter()` 绑定 `&T`）。只有所有回调纯净且交错执行不可观察时，`\|>` 链才平坦化为单个迭代表达式（#2045）；`ALMIDE_STREAM_FUSION_OFF` 用于消融 | **无对应实现**——规范顺序逐阶段执行；见 E |
| 14 | `BorrowInsertion` | `pass_borrow_inference.rs`, `pass_borrow_inference_call_sites.rs`, `pass_borrow_inference_ownership.rs`, `pass_borrow_inference_scoped_closures.rs`, `pass_borrow_inference_seq_hoist.rs` （`pass.rs` 内的包装器） | Rust | 按设计仅用于 Rust | Roc 式默认借用签名、调用处 `Borrow` 节点；迭代源跟随函数体对元素的操作（#2287）；函数类型参数在可调用值不逃逸时为 `&dyn Fn`，此槽中的 lambda 字面量是写作 `&(lambda)` 的作用域（#2288）；在嵌套 `mut` 实参调用前读取的操作数（插值部分、嵌套调用的参数）按从左到右顺序提升（#3230） | RC-3 借用／fresh 分类器（`rc_ownership.rs`） |
| 14b | `OwnedSourceHoist` | `pass_owned_source_hoist.rs` | Rust | 优化 | 后续参数仅以标量方式读取源的范围操作（`list.slice(xs, 1, list.len(xs))`）先绑定这些参数，使 TailCallOpt / CloneInsertion 能将不再活跃的源移动进 `_owned` 对应实现（#3519） | 不适用 |
| 15 | `TailCallOpt` | `pass_tco.rs`, `pass_tco_loop_rewrite.rs`, `pass_tco_owned_reads.rs` | 全部 | 优化 | 自递归尾调用 → 循环 | 对应：`tco.rs`（对编码后函数体运行 `loop_convert`，其他情况使用 `return_call`） |
| 16 | `CaptureClone` | `pass_capture_clone.rs`, `pass_capture_clone_bindings.rs`, `pass_capture_clone_mut_param.rs` | Rust | 按设计仅用于 Rust | 预先克隆 `move` 闭包捕获的变量 | 不适用 |
| 17 | `CloneInsertion` | `pass_clone.rs`, `pass_clone_calls.rs`, `pass_clone_interp.rs`, `pass_clone_compare.rs`, `pass_clone_places.rs`, `pass_clone_projection.rs`, `pass_clone_head.rs`, `pass_clone_select.rs`, `pass_clone_record_fields.rs`, `pass_clone_loops.rs` | Rust | 按设计仅用于 Rust | 为堆类型复用及已证明的投影借用／移动插入 `Clone` 节点（列表头读取借用，#3453；活跃局部变量的只读选择借用，#3532） | RC inc/share 守卫（`rc_ownership.rs`） |
| 17b | `ChainSourceBorrow` | `pass_chain_source_borrow.rs` | Rust | 优化 | 克隆 pass 不得不克隆的链源（`(v.clone()).into_iter()`、来自 `&Table` 的 `t.sizes`、借用源下的活跃局部变量），或直接作为消费参数使用的 `&[T]` 参数，在回调不写该变量时从借用迭代（`.iter().cloned()`，#2098） | 不适用 |
| 18 | `MatchSubject` | `pass_match_subject.rs` | Rust | 按设计仅用于 Rust | 对 match 对象使用 `.as_str()` / `.as_deref()` | 不适用 |
| 19 | `EffectInference` | `pass_effect_inference.rs` | 全部 | 分析 | 从传递性的 stdlib 使用推断能力类别 | 经另一条路径共享：`cli::check_permissions` 对每条路径的 mono 前 IR 独立运行此 pass |
| 20 | `StdlibLowering` | `pass_stdlib_lowering.rs`, `pass_stdlib_lowering_ufcs.rs` | Rust | 按设计仅用于 Rust | `Module` 调用 → 带参数修饰的 `Named` 运行时调用 | 自托管 stdlib 函数体输出为 wasm 函数 |
| 21 | `ResultPropagation` | `pass_result_propagation.rs` | 全部 | 按设计仅用于 Rust | effect 函数 `T → Result[T, String]`，调用处插入 `Try` | 自己的 effect lowering（`effect_raw`、`emit.rs`） |
| 22 | `BuiltinLowering` | `pass_builtin_lowering.rs` | Rust | 按设计仅用于 Rust | `assert_eq`/`println`/… → `RustMacro` | 不适用 |
| 23 | `DecodeSlotHint` | `pass_decode_slot_hint.rs` | Rust | 按设计仅用于 Rust | 派生 `T.decode` 的借用字段查找携带声明索引（`@codec_slots`），渲染为 `almide_rt_value_field_ref_at`（#1679）；运行时扫描前先尝试 `pairs[i]`，错误不变（C-084） | 不适用（wasm 路径既不读取属性，也不读取 `_at` 符号） |
| 23b | `DecodeErrFrame` | `pass_decode_err_frame.rs` | Rust | 按设计仅用于 Rust | 派生 `T.decode` 的逐字段错误帧（`T___erratw_*`，#1675）变为字段自身结果上的 `.map_err(..)`，使成功路径为简单的 `?`，没有载荷克隆和键分配（#2050） | 不适用（wasm 路径 lowering worker 本身） |
| 24 | `Peephole` | `pass_peephole.rs` | 全部 | 优化 | 惯用列表循环 → `ListSwap`/`ListReverse`/`ListRotateLeft`/`ListCopySlice` 节点 | **这些节点无 lowering**——见 E |
| 25 | `RustLowering` | `pass_rust_lowering.rs`, `pass_rust_lowering_stmts.rs`, `pass_rust_lowering_fan.rs`, `pass_fan_local_state.rs` | Rust | 按设计仅用于 Rust | push 优化、借用索引提升；将 `fan.map` 及 `fan { }` 下对 Send 安全标量使用纯 lambda 的列表操作路由至每核一线程的运行时对应实现（`almide_rt_fan_map_par`、`almide_rt_list_par_*`；`ALMIDE_FAN_SEQUENTIAL=1` 消融，#2044）；闭包为字面量的回调操作（`map.upsert`）→ 静态闭包 `_fn` 对应实现，避免每次调用装箱 | 不适用 |
| 26 | `FanLowering` | `pass_fan_lowering.rs` （`pass.rs` 内的包装器） | 全部，W | 支撑 | 从 fan spawn 闭包中移除 auto-try | 自己的 fan lowering（`fan.rs`） |
| 27 | `NormalizeRuntimeCalls` | `pass_normalize_runtime_calls.rs` | Rust | 按设计仅用于 Rust | 旧 `Named { almide_rt_* }` → `RuntimeCall` | 不适用 |
| 28 | `IrLinkFlatten` | `pass_ir_link_flatten.rs` | Rust | 按设计仅用于 Rust | 为 walker 将模块平坦化到根模块 | 保留模块和限定名称 |
| 29 | `SharedCellBorrow` | `pass_shared_cell_borrow.rs` | Rust | 按设计仅用于 Rust | 对语句证明安全的读取，就地借用捕获的单元（#1143） | 不适用 |
| 30 | `VarStorage` | `pass_var_storage.rs` | Rust | 按设计仅用于 Rust | 确定哪些非 Copy 的 `var` 局部变量被闭包捕获、从而位于 `AlmideRcCow` 中（`var_storage`）；此前由 walker 的程序准备扫描完成（#2186） | RC-3 借用／fresh 分类器（`rc_ownership.rs`） |
| 31 | `RangeCountingVars` | `pass_range_counting.rs` | Rust | 优化 | 仅作为 `for-in` 头部读取的 `let` 绑定范围保持为裸 `Range<i64>`，而非实体化 `Vec<i64>`（#1857）；对应 MIR 的 #1400 `range_counting_vars` 接受规则，并最后运行，使集合命名最终 IR | `ranges.rs` 计数循环（#1400）——已有 |
| 32 | `TopLetStorage` | `pass_top_let_storage.rs` | 全部 | 分析 | 供 walker 使用的统一 top-let 存储属性（§4 Stage 1） | 自己的全局计划（`build_globals`） |
| 33 | `BorrowLowering` | `pass_borrow_lowering.rs`, `pass_borrow_lowering_take.rs` | Rust | 按设计仅用于 Rust | 对每个 `Borrow` / `Clone` / 按引用参数的存储读取作最终处理：去掉 `&T` 参数已有的 `&`，对拥有读取使用 `.to_string()` / `.to_vec()` / `.clone()`，对 spread 基值用 `.clone()`，借入列表（`almide_index_ref!`），使用 `_ref` 字段查找对应实现，对 `&String` 循环绑定用 `.as_str()`，在相等比较中解引用 `&T` 参数，并发布 `param_borrows`；walker 按所见渲染（#2186）。若赋值用自身旧值覆盖 `mut` 参数，且右侧没有任何内容能看到空洞，则用 `std::mem::take` 将其移出（#3170） | 不适用（wasm 路径没有引用） |
| 34 | `SliceBinders` | `pass_slice_binders.rs` | Rust | 优化 | 对 `split_once` / `strip_prefix` / `strip_suffix` 的 `match`，若载荷绑定仅读为 `&str` 或被消费，则绑定为对象的切片（运行时 `_ref` 对应实现）；消费的绑定在消费处复制，任何其他用途均保留拥有式对应实现 | 不适用（wasm 路径没有引用） |
| 35 | `StrMapKey` | `pass_str_map_key.rs` | Rust | 优化 | 键为 `k.to_string()` 的 String 键 `map.set` / `map.upsert` 通过运行时 `_str` 对应实现接收 `&k`，仅插入时复制键 | 不适用 |
| 36 | `ThreadShareMarks` | `pass_thread_share.rs` | Rust | 按设计仅用于 Rust | ADR-0013：启动的 fan 分支或跨线程运行时辅助调用（`crosses_threads`，为 `RuntimeCall` 或 `fan.map_par` 这样的 `Call { Module }`）可达的每个变量，都在线程启动前通过 `InlineRust` 块标记 `(v).almide_share();`；walker 仅渲染，不自行遍历 `free_vars`（#2186） | 不适用 |
| 37 | `BoxTakeLowering` | `pass_box_take.rs` | Rust | 按设计仅用于 Rust | #4251：闭合递归类型环的字段为 `AlmideBox`（drop / clone / `==` 的栈使用有界），不能通过 `*b` 移出；对它的每次读取（box 模式绑定、递归记录的装箱字段），在父节点仅观察时写作位置 `(*x)`，消费时写作 `almide_box_take(x)` |
| 38 | `StrViewRebind` | `pass_str_view_rebind.rs` | Rust | 按设计仅用于 Rust | #4296：对 `String` 对象的 `&str` 视图进行 match（`match &*x { "a" => .., other => .. }`）时，若绑定分支需要将绑定作为值，或分支触及对象变量，则以自己的拥有副本开头；从最终 IR 发布 `str_view_owned_binders` 供 walker 拼写 | 不适用 |

第 14、16、17 行从同一次遍历 `use_kind.rs`（#2186）读取所有权事实：每个局部变量的每次使用都标记其父节点放置的位置（`Site`），以及是否被闭包、融合链或 `&mut` 包围。`BorrowInsertion` 的“此参数需要拥有／`&mut` 吗”是在这些位置上的谓词，`SlotOracle` 从不动点快照提供被调用方参数槽模式；`CaptureClone` 的“闭包写入哪些捕获”以及 `CloneInsertion` 的使用次数和循环绑定判定，都是在同一遍历上使用显式借用 oracle 的谓词。不动点单调上升（`Ref` → `RefMut` → `Own`，各轮会发布的每个键均乐观初始化），不设轮次上限：槽向下移动或运行超出格高度都会成为 ICE。

<a id="c-structural-wasm-leg--cratesalmide-wasm-default---target-wasm"></a>

### C. 结构化 wasm 路径——`crates/almide-wasm`（默认 `--target wasm`）

发射器对链接 IR 或编码后函数体应用的重写。没有任何一个是 nanopass 意义上的 IR→IR pass；各自都是 `emit_program` 内的一条路径。

| Pass | 文件 | 作用 |
|---|---|---|
| 透明 newtype 消除 | `newtype.rs` | 消除透明别名，让两个输出 pass 读取同一棵树（#1423 stage 4） |
| 可达性 DCE | `emit.rs`（两遍） | 第 1 遍记录 `main` 可达内容；第 2 遍仅对可达集合重新输出 |
| 确定性计量计划 | `fuel.rs` | 哪些函数计量、哪些入口豁免（ALS-DT2，对应 interp） |
| 自尾调用循环转换 | `tco.rs` | `return_call $self` → 参数 `local.set` + 到外围 `loop` 的 `br` |
| map/filter → fold 融合 | `list_fuse.rs` | 对无可观察作用的回调消除中间列表（B.10 的列表部分） |
| 计数 while 部分展开 | `unroll.rs` | native 路径上 LLVM 获取的循环控制优化空间 |
| RC-3 所有权守卫 | `rc_ownership.rs` | 借用／fresh 分类、可 drop 集合、inc/share/arg 守卫（B.14/B.17 的 wasm 对应实现） |
| 堆上限 | `heap_cap.rs` | 测试框架设置的线性内存上限（#1729），不是优化 |

此路径任何位置都不输出 SIMD（`v128`）；矩阵例程是标量内核（`matrix_kernels.rs`）。按决定不属于 #929 范围。

<a id="d-v1-native-trust-spine--cratesalmide-mir-retiring-under-1696"></a>

### D. v1 native trust-spine——`crates/almide-mir`（在 #1696 下退役中）

为完整性而列出；清单门禁不枚举此 crate，因为这些行会随退役而移除。其 wasm 阶段（WAT 渲染器及 WAT 级 pass：操作数融合、peephole、前导 DCE、边界检查消除、`br_table` 分派、局部槽复用、自追加重写、标量包装内联、区域分配）已在 #2761 删除。

| Pass | 文件 | 阶段 | 作用 |
|---|---|---|---|
| 计量 probe | `charge_probe.rs` | native（`ALMIDE_FUEL_PROBE` 构建） | 函数入口及每个 `LoopStart` 后的 `Charge` |
| 无效 `MakeUnique` 消除 | `alias_safety.rs` | native | 移除无别名的 copy-on-write 守卫（#824） |
| native Result carrier 重写 | `native_result_rewrite.rs` | native | 用于 Rust 渲染的 T1-3 Result 生产者／消费者重写 |

<a id="e-pre-split-decision-closure-of-929"></a>

### E. 分流前迁移决策（#929 的闭合）

共享子集**已经在分流前运行**：表 A 是整个 `almide-optimize`，首表中的每条路径都通过唯一的 `link_ir` 使用它。问题仅在于表 B 中目标中立的行是否应加入。逐候选判定：

| Pass | 现在移至分流前？ | 阻碍／原因 |
|---|---|---|
| `ConstFold` (B.12) | 无需移动 | A.1 在分流前折叠相同字面量算术；Rust 分支的 pass 仅清理其他路径不存在的 B.10–B.11 产物 |
| `TailCallOpt` (B.15) | 无需移动 | 结构化路径在编码后函数体层面转换自尾调用（`tco.rs`），其余保留 `return_call`——相同的恒定栈保证 |
| `LICM` (B.9) | **现在不迁移——后续工作** | (1) 分层：分流点所有者 `almide-driver` 依赖 `almide-optimize` 而非 `almide-codegen`，pass 针对后者的 `NanoPass`/`Target` 编写——需迁移三个文件，不是一行流水线修改；(2) 分流点也是 interp 判定基准和 v1 native 渲染的输入，因此分流前提升落地前需 `ALMIDE_ONLY_PASS=licm` 轴和 `spec/pass_isolated/` 行；(3) 结构化路径收益未测量——先在循环主导的台账行（`nbody`、`spectralnorm`）测量 |
| `EggSaturation` (B.10) | **现在不迁移——后续工作** | 会将 `almide-egg-lab` 引至 driver 之下；矩阵部分重写为仅 Rust stdlib lowering 使用的融合 `matrix.*` 形式，而列表部分已有结构化对应实现（`list_fuse.rs`） |
| `Peephole` (B.24) | **现在不迁移——后续工作** | 其输出节点（`ListSwap` 等）在 `almide-wasm` 中无 lowering；分流前运行会让命中模式的每个 fixture 都遇到 wall（仅 almide-mir 的 `lower/mod_p5.rs` 与 interp 使用它们） |

`--target wasm32`/`wasi` 则不同：它用 `rustc` 编译 codegen v3 的 Rust 源码，因此得到表 B 完整 Rust 分支及 LLVM。

<a id="stdlib"></a>

## 标准库

标准库是自托管的：每个函数位于 `stdlib/<module>[_part].almd`（旧 `stdlib/defs/*.toml` 流水线已不存在）。有两条使用路径：

- **Native / 类型签名**——模块源码嵌入已提交的 `almide-types/src/generated/embedded.rs` 表，在检出仓库中由 `almide-types/build.rs` 根据 `.almd` 文件再生成。`stdlib_info.rs` 暴露它们；前端提取签名，codegen 提取 `@inline_rust` 模板。`@intrinsic("almide_rt_*")` 声明分派到 `runtime/rs/src/<module>.rs` 的手写 Rust。
- **WASM**——注册在 `almide-types/src/self_host_registry.rs` 的纯 Almide 实现（与 interp 判定基准共享），由结构化路径与用户代码一起编译。

自动导入是 `almide-frontend/src/import_table.rs` 中 seed 列表与 `stdlib_info.rs` 中 `AUTO_IMPORT_BUNDLED` 的并集。`json`、`fs`、`http`、`env`、`io`、`random`、`regex`、`testing` 等模块需要显式 `import`。

<a id="build-system"></a>

## 构建系统

根 `build.rs` 嵌入 `almide explain` 的诊断文档，并设置版本／来源行。crate 特定构建脚本包括：

- `almide-codegen/build.rs`——从重写声明生成嵌入式运行时注册表和矩阵脱糖后备实现。旧 `arg_transforms.rs` 表已退役。
- `almide-types/build.rs`——从 `stdlib/*.almd` 生成已提交的嵌入 stdlib 源码和由声明派生的表。
- `almide-frontend/build.rs`——将方言台账和 regex 运行时复制到已提交生成文件；签名从随附源码提取，而非生成的 `stdlib_sigs.rs` 表。

这些 crate 生成器只在检出仓库中再生成检出输入；独立包使用其已提交的生成副本。

<a id="type-system"></a>

## 类型系统

采用立即合一的基于约束推断：

1. **Infer**——遍历 AST，分配新的类型变量，收集约束
2. **Solve**——合一约束并传播解
3. **Resolve**——将 `expr_types` 中的推断变量替换为具体类型

检查器的 UFCS 解析考虑接收者和作用域：例如 List 上的 `xs.map(f)` 可解析为 `list.map(xs, f)`；约定方法及本地／导入函数有各自解析规则（`calls_ufcs.rs`）。管道和已解析调用在 codegen 前 lowering。字符串插值可以保留为 `IrExprKind::StringInterp`，由 Rust walker 显式渲染（`walker/expressions.rs`）。

<a id="module-system"></a>

## 模块系统

1. **Resolve**（`src/resolve.rs`）——遍历 `import` 声明，查找 `.almd` 文件（本地、git 依赖；stdlib 通过随附源码处理）
2. **Register**——将带前缀的模块签名加入 TypeEnv
3. **Check** → **Lower** → 具有独立命名空间的逐模块 IR
4. **Codegen**——以 `almide_rt_{module}_{func}` 前缀输出模块函数

<a id="diagnostics"></a>

## 诊断

[诊断参考](../../../diagnostics/README.md) 列出当前代码。诊断报告位置和源码上下文，已知修复方法时附带提示。输出为人类可读形式或 `--json`。

```
error[E005]: argument 'xs' expects List[Int] but got String
  at line 5
  in call to list.sort()
  hint: Fix the argument type
  |
5 | let sorted = list.sort("hello")
  |                        ^^^^^^^
```

<a id="verification-spine"></a>

## 验证主干

- **契约**——每个可观察的跨目标承诺都是 [contracts/contracts.toml](../../../contracts/contracts.toml) 中的具名 `[[contract]]`，具有可执行依据（fixture / fuzz / Σ-probe / 定理）。
- **三方判定基准**——native、wasm 和 `almide-interp` 的 stdout/stderr/退出码必须一致。
- **Lean belt**——`almide-perceus-belt` 证明 `almide-mir` 实现的 Perceus RC 纪律。
