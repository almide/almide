<a id="native-bug-fixes-research-and-validation-2026-09-09"></a>

# Native bug 修复：研究与验证（2026-09-09）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](bugfix-research-2026-09-09.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](bugfix-research-2026-09-09.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](bugfix-research-2026-09-09.zh-CN.md)

> 非规范性历史阅读快照。以固定原文为准，不自动同步。未独立重跑测量、证明 gate 或当前实现检查。未执行引用命令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/bugfix-research-2026-09-09.md)
> 所记回归通过及质量分数为历史资料，未独立重跑。不把 legacy WASM skip 计为通过。timeout 处理为 best effort 而非隔离；Windows cleanup 需要单独 Windows CI。

<a id="reference-checkouts"></a>

## 参考 checkout

要求的 `../almide-references` 目录包含浅 checkout：

- Gleam：`gleam-lang/gleam`，`19bf207ebb7d953ea1391f041da48c214ee1440a`。
- Rust：`rust-lang/rust`，`0d31508599a7814a7044e9a7a871e3dc5f037753`；MIR 构造及标准库 process 内部的 sparse checkout。

<a id="2062-an-outlined-branch-must-preserve-its-assignment-destinations"></a>

## #2062：抽出的分支必须保留赋值目标

Rust 的 [`builder/expr/stmt.rs`](https://github.com/rust-lang/rust/blob/0d31508599a7814a7044e9a7a871e3dc5f037753/compiler/rustc_mir_build/src/builder/expr/stmt.rs) 通过求值 RHS、将 LHS 解析为 place、再赋值给该 place 来 lowering 赋值。把分支移入值参数 helper，不能保留外围 local 的存储。Gleam 的 [`javascript/expression.rs`](https://github.com/gleam-lang/gleam/blob/19bf207ebb7d953ea1391f041da48c214ee1440a/compiler-core/src/javascript/expression.rs) 分别分派 case 表达式和 function literal；其不可变绑定语义不能证明 Almide 可变 capture 可以复制。

决策：若分支写入的变量既非分支内绑定也非 module global，则放弃 branch outlining。复用既有 mutation collector，包括 write-only 和 collection-write 目标。dense expression 路径与 let-bound 路径都经过此 guard。local mutation 和 global access 保留既有行为。保守地包含 nested-lambda 写入。不为保留此优化而引入 reference-parameter ABI 或新的所有权规则。

原 retry loop 在 native 上从 `cost=0.0 n=2` 变成 `cost=0.5 n=2`；`almide run --target wasm` 也打印后者。新 spec 覆盖 retry-loop 状态、loop 外 write-only capture、list 写入及 branch-local mutation。legacy explicit-WASM 的 *test* runner 在 rendering wall 跳过此 fixture；该 skip 不算成功的 WASM 回归测试。

<a id="2063-adapt-the-function-value-abi-at-the-runtime-boundary"></a>

## #2063：在 runtime 边界适配 function-value ABI

Almide 生成 `Rc<dyn Fn(String)>`，HTTP streaming 实现却接受 `impl FnMut(String)`。诊断显示 `Rc` 不满足该 generic bound。既有 SSE caller 需要借用 stack 的可变 closure，因此把全部 callback 改为拥有的 `Rc` closure 会改变 lifetime 和 mutation 行为。

决策：在 `almide_http_request_stream` 暴露 Almide function-value signature，经小 closure 转发到既有 `FnMut` 实现。native SSE caller 直接调用该实现。loopback integration test 检查表达式、block 和已绑定 callback，共享 list 修改，以及 content-length 和 chunked 两类 response。

<a id="2065-the-deadline-covers-pipe-eof-as-well-as-child-exit"></a>

## #2065：期限同时覆盖 pipe EOF 与子进程退出

Rust 的 [`sys/process/unix/unix.rs`](https://github.com/rust-lang/rust/blob/0d31508599a7814a7044e9a7a871e3dc5f037753/library/std/src/sys/process/unix/unix.rs) 用 `setpgid` 实现配置的 process group。直接子进程退出后，后代仍可保持 stdout/stderr 打开，因此无条件 reader join 不能成为有界 timeout 路径的一部分。

决策：建立 private Unix process group，在同一期限下轮询子进程退出和两个 reader 完成，timeout 时 kill group。Windows 使用既有带 `/T` 的 taskkill 方法。仅在 reader 完成后 join；error 路径绝不等 pipe EOF。故意逃离 group 的后代可让 detached reader 存活到它关闭继承 pipe，但不能通过 reader 把 caller 拖过期限。process tree 终止是 best effort，不是 process containment 机制。

Unix integration test 检查存活子进程、已退出子进程、继承 stderr 及成功的非零退出。Windows tree cleanup 需要 Windows CI。

<a id="quality-measurement-and-existing-prs"></a>

## 质量测量与既有 PR

使用从 O6lvl4/codopsy `fabfbdccac8bee5fc71055b6859c0684fd6fce5b` 构建的 codopsy **2.2.0**，保持 `.codopsyrc.json` 不变。PATH 已安装 binary 为 2.1.0，报告不同分数。

commissioned CI gate 测量 `crates/almide-wasm`：**91/A**。修改后的 optimizer crate 为 **96/A**；`runtime/rs/src/process.rs` 为 **100/A**。更广的 `crates` tree 为 **82/B**，codegen 为 **89/B**，runtime tree 为 **83/B**。这些是已有的广泛质量不足；本 batch 不声称全仓库为 A，也不放松规则制造该结果。

实现前检查了既有工作：#2054 由 PR #2053 处理，#2059 由 PR #2064，#2044 由 PR #2055。检查时 #2053、#2055、#2064 与 develop 冲突。本 batch 不重复这些修改，也不因 PR 存在就标记 issue 已解决。

<a id="validation"></a>

## 验证

- `almide test`：438/438 文件；425 WASM，13 native fallback。
- `almide test --target rust`：438/438 文件。
- optimizer/codegen Rust 测试：78 项通过。
- interpreter 回归：通过，包括后续 write-only 覆写之前 loop 累积值。
- HTTP streaming integration：通过（三种 callback 形式、两种 encoding）。
- Unix timeout integration：通过（三种 orphan-pipe 形态、正常 exit 7）。仅 timeout 情况使用 100ms；正常退出给出宽裕的 30s 上限，避免机器负载把成功断言变为不稳定的计时测试。
- AST corpus parity：1 项通过；diagnostics corpus parity：3 项通过。只加新 fixture 的测量行；没有重写已有 oracle hash。
- file discipline、pass roster、layout discipline、libm determinism、fixture formatting 及 `git diff --check`：通过。

<a id="ci-coverage-follow-up"></a>

### CI coverage 后续

legacy v1 `almide test` 路径拒绝处理 list-element mutation 情况，而 structural `almide run --target wasm` 路径支持它。三个 scalar/record spec 情况留在 v1 WASM 路径。list 情况另行固定在 `tests/branch_lift_heap_mutation_test.rs`，运行 native 与 structural WASM target，检查最终元素为 2。fallback baseline 不变。
