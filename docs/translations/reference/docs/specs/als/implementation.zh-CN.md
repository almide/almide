<a id="als--実装不変量implementation-invariants"></a>

# ALS — 实现不变量（Implementation Invariants）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](implementation.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](implementation.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](implementation.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/implementation.md)的非规范读者快照。下文的规范地位和要求属于原文，不属于本副本。不自动同步；本次翻译未独立复核历史实现、符合性及安全主张。代码与规则标识符保持不变。

> Last updated: 2026-10-27

不直接表现为观测（stdout/stderr/exit），但支撑内存安全、资源和确定性的实现规范。契约台账通过 `spec` 键引用本章。

<a id="als-i1-参照カウント規律perceus"></a>

## ALS-I1 引用计数纪律（Perceus）

堆值的 RC 遵循 Perceus 纪律：透传组合器返回自己的引用（+1）；插值、spread、Value 构造中的共享以 +1 配平；WASM 堆默认得到回收（规范要求程序结束时泄漏为 0；例外是 C-066 自身记录的 TCO 自调用循环每次迭代的泄漏，其 statement 将其记为到 Stage C 为止的已知遗留问题）。
验证由 Lean 证明、发射时 Σ-probe 和 RC 计数器夹具承担。
Contracts: C-041, C-066, C-071, C-086, C-121, C-122, C-146, C-149。

<a id="als-i2-コンパイラの決定性と資源"></a>

## ALS-I2 编译器的确定性与资源

代码生成不依赖宿主架构而具有确定性（相同输入 → 字节相同的输出）。编译宽大或深度嵌套的输入不会使原生栈溢出（专用 256 MiB 编译线程 + 语句链的迭代遍历——C-059；深度嵌套的表达式会被解析器的 MAX_DEPTH=500 干净地拒绝，根本不会进入编译）。
Contracts: C-040, C-059。

<a id="als-i3-v1-lowering-エッジの等価証拠"></a>

## ALS-I3 v1 lowering 边缘情形的等价证据

v1（MIR）路径特有的 lowering 边缘情形——标量值边界情况、tuple/list Ok 载荷往返、自制 base64、按条件保留／跳过的 filter_map、借用参数的拥有型绑定簇、标准库调用载荷直接绑定构造器、heap-Ok Result 的值类组合器、构造器的 if-payload、标量列表字面量不会静默变空的保证——与 v0 字节一致（固定 v1 验证过程成果的一组契约）。
链接两个及以上 self-host 模块时，如果以 `__` 开头的模块内部辅助函数同名冲突，**不得按名称匹配合并**：应以明确的链接错误停止（合并会将调用方重新绑定到错误签名，在 `Ok` 响应后于 wasm 验证阶段失败）。
在 test 模式下，每个 `test` 块都从声明时的初始化表达式开始观察 MUTABLE 模块 top-let（native 通过线程隔离，wasm 在每项测试前重新初始化）。
Contracts: C-075, C-107, C-109, C-116, C-120, C-138, C-139, C-143, C-144, C-152, C-212, C-321。

<a id="als-i4-コンパイルコストの予算"></a>

## ALS-I4 编译成本预算

编译器处理不可信输入（PR 的 CI、刚打开文件时的 LSP、评分器、playground）。工作量随输入增长的每个阶段，都受一个具名**预算**约束。清单是 `almide budgets` 输出的表（嵌入 `docs/specs/cli.md`），各行为 `nesting`（一个表达式的嵌套层数）、`type_size`（一个推断类型的节点数）、`generic_instances`（有界泛型函数的特化实例数；每个实例的类型参数也受 `type_size` 约束）、`exhaustiveness_steps`（一个 `match` 的穷尽性检查步数）、`manifest_bytes`（单个 `almide.toml` / `almide.lock` 的大小）、`dependency_count`、`dependency_depth`。

- 预算按**计数**衡量，不按时间或可用内存衡量。相同输入在任何机器上都得到相同判定。
- 属性、清单键、锁文件条目、环境变量均不能改变预算。只有命令行的 `--budget NAME=N` 能改变它；`almide check --profile critical` 拒绝该标志。
- 超预算输入以该预算的诊断和非零退出码结束，而非崩溃、因内存不足被强制终止或挂起。`type_size` 对应 E111 `type too large (N+ nodes, max M)`（位置为类型超预算的表达式或约束），`generic_instances` 对应 E112，`exhaustiveness_steps` 对应 E113 `match too complex to check for exhaustiveness`，`nesting` 对应语法错误 `expression nesting too deep (max N)`，项目侧的三项则以指明文件、数值、预算的错误结束。不会靠猜测接受超预算类型，也不会将未能完成检查的 `match` 视为穷尽。
- 类型共享结构。即使去掉 `type_size`，将 `let t(i) = (t(i-1), t(i-1))` 叠加 n 次（直接叠加，或通过 `fn dup[T](x: T) -> (T, T)`）的程序，也能以关于 n 线性的时间和内存通过检查。

测试：`tests/diagnostics/e111-type-doubling-chain/`、
`tests/diagnostics/e111-type-doubling-through-a-generic/`、
`tests/diagnostics/e113-bool-matrix-too-complex/`、`tests/compile_budget_test.rs`。
Contracts: C-440。

> 原文背景：带日期的要求包括到 Stage C 为止 C-066 的每次迭代泄漏例外，以及较强的编译资源主张。这里只将其保留为原文要求和历史陈述，不表示已独立确立适用于所有编译器构建和资源环境的保证。
