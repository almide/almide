原文 description 元数据：推动 stdlib 采用单一事实来源：`.almd` + 多 target ABI attribute
原文 done 元数据：2026-04-19
<a id="stdlib-declarative-unification--toward-a-single-source-of-truth"></a>

# Stdlib 声明式统一——走向单一事实来源

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](stdlib-declarative-unification.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](stdlib-declarative-unification.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](stdlib-declarative-unification.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/stdlib-declarative-unification.md)

> 原文注记：完成报告取代保留的历史计划，包括未勾选清单及较早 inline template 方案。Stage 2 写五个 module 却列六个；定宽类型计数也保留，不推测修正清单。历史全部移除目标与保留 runtime/intrinsic wrapper、逃生口语法并存。旧 MLIR/egg 计划、工期、测试及性能含义是原文主张，不是独立验证的当前实现或测量。


<a id="completion-status-2026-04-19"></a>

## 完成状态（2026-04-19）

此次工作跨 Stage 2 → 4 交付，由最后两个 commit 收尾：

- **PR 3b00b1ba**（2026-04-19）：最后 13 种 L3 dispatch pattern（12 个 bytes `Endian` dispatch wrapper + `http.serve`）从 `@inline_rust` 转为捆绑 Almide body + `@intrinsic` runtime wrapper。配套 compiler 工作已交付：`pass_borrow_inference` 中的 `check_needs_refmut`（捆绑 body 现在正确转发 `&mut`）、walker 对 `RefMut` parameter 自动再借用、mangled-key signature mirror，使 `ResolveCallsPass` 重写后的 Named call 仍可查找。
- **PR 49d341f6**（2026-04-19）：全部剩余 `@inline_rust` 声明退役。`error`/`string` 迁移至 `@intrinsic`。8 个定宽类型 module（`int8…uint64`、`float32`）重写为通过标准 `int`/`float` 中转的 pure Almide body。compiler 侧：`@intrinsic` 函数经 `pass_intrinsic_lowering` 获得 AST-default 支持；UFCS Method→Named 重写现在为小写开头的捆绑 module 前置 `almide_rt_`；WASM `dispatch_runtime_fallback` 除 `to_*` 外也接受 `from_*`。

完成后的 grep（2026-04-19）：

```bash
$ grep -c '^@inline_rust' stdlib/*.almd
0
```

219/219 spec test + 全部 cargo test green。保留 `@inline_rust` 语法本身和 `pass_stdlib_lowering` template dispatcher，作为未来 edge case 的逃生口（当前无使用者），但 stdlib 已不再需要它们。

<a id="original-plan-retained-for-history"></a>

## 原计划（保留供历史参考）

<a id="current-state-after-v0147"></a>

## v0.14.7 后的当前状态

每个 stdlib 函数在**三个**位置定义：

| 层 | 文件 | 作用 |
|---|---|---|
| Signature + Rust template | `stdlib/defs/<m>.toml` | 类型检查器读取；pass_stdlib_lowering 将 `{xs}` / `{f.body}` 代入 Rust template |
| Rust runtime body | `runtime/rs/src/<m>.rs` | `almide_rt_<m>_<f>(…)` 的手写 Rust 实现 |
| WASM inline emit | `crates/almide-codegen/src/emit_wasm/calls_<m>.rs` | 手写 `wasm!(self.func, { … })` sequence |

25 module × 约20 函数 = 约500 函数，每个都涉及 3 层。v0.14.7 Phase 3 把 **dispatch 层**变成单一来源（每个 backend 的 `CallTarget` 恰好进入一个位置），但**定义**层仍是三份。

三重定义模型的症状：

- **漂移**：Rust runtime 改动（如 `list.binary_search` 边界检查）不会传播到 WASM，直到人类注意到。
- **上手成本**：加入 `list.intercalate` 要编写 TOML signature + Rust template + Rust 实现 + WASM emit。一个概念变更需 4 处编辑。
- **捆绑 Almide 无法竞争**：`stdlib/list.almd` 自 v0.14.6 起可用 pure Almide 函数扩展 stdlib，却不能*替换* TOML 支持的函数，因为 TOML signature + dispatch 路径仍是必需支撑。

<a id="goal"></a>

## 目标

每个 stdlib 函数一个事实来源：`stdlib/<m>.almd`。多 target ABI 编码为 codegen 读取的 Almide attribute。

```almide
// stdlib/list.almd
@pure
@inline_rust("{xs}.iter().cloned().filter(|x| {f.body}).collect()")
@wasm_intrinsic(
  // declarative WASM recipe; emitter compiles directly
  iter_chain(elem=Auto, filter=f)
)
fn filter[T](xs: List[T], f: fn(T) -> Bool) -> List[T] =
  // pure-Almide fallback body (used when no intrinsic for the target)
  list.fold(xs, [], (acc, x) => if f(x) then acc + [x] else acc)
```

代码注释翻译：声明式 WASM recipe 由 emitter 直接编译；pure Almide fallback body 在该 target 没有 intrinsic 时使用。

随后：

- codegen 读取 `stdlib/list.almd`，看到 `@inline_rust` 就内联输出 template（替代 TOML template）。
- codegen 读取 `@wasm_intrinsic`，从 recipe 输出 WASM bytecode（替代 `emit_wasm/calls_list.rs`）。
- 对某 target 没有 attribute → fallback 到 Almide body（v0.14.6 捆绑 dispatch 路径）。
- `stdlib/defs/*.toml`、`runtime/rs/*.rs` 和 `emit_wasm/calls_*.rs` 全部消失。

<a id="scope-per-module"></a>

## 各 module 范围

每个当前三重定义的 module 是一步：

- [ ] math（done：v0.14.5，`StdlibOp::FloatUnaryCall` dispatch）
- [ ] string 部分（done：v0.14.5，经 `StdlibOp` 的 10 个函数）
- [ ] float 部分（done：v0.14.5，`to_string`）
- [ ] list——**最高价值、高复杂度**（closure + iter chain）
- [ ] option / result——generic、浅层
- [ ] bytes——低层、memory 专用
- [ ] value——动态多态，需要 tag dispatch
- [ ] map / set——基于 hash、大量 closure
- [ ] int——简单 scalar operation
- [ ] datetime / regex / random / io / process / fs / env / http / json / testing / base64 / hex / matrix——其他

每个 module 需要：

- 含 pure Almide body 的 `stdlib/<m>.almd`（fallback / WASM default）
- Rust fast-path 的 `@inline_rust(...)` attribute（手写 template 有价值之处）
- `@wasm_intrinsic(...)` attribute，或依赖 pure Almide body + 捆绑 dispatch（v0.14.6 路径）
- 每个新 attribute recipe 的 codegen 支持
- 回归：迁移后 Rust + WASM 通过 spec/，dojo MSR 不变

<a id="移行構造-stage-1-4"></a>

## 迁移结构（Stage 1-4）

- **Stage 1**（完成）：定义 attribute 语法 + codegen reader。经 `@inline_rust` 把 `int.to_string` 迁移到 `stdlib/int.almd`。TOML / runtime / emit 并行保留。
- **Stage 2**（2026-04-19 完成）：5 个无 closure module 迁到 `@intrinsic`：
  - `int`——22 函数（+ 删除 `stdlib/defs/int.toml`）
  - `float`——17 个 runtime-call 函数（其余 10 个是 `as T` 类型转换，没有 runtime 函数，保留 `@inline_rust` 才正确）
  - `base64`——全部 4 函数迁移
  - `hex`——全部 3 函数迁移
  - `bytes`——84 个 pass-through 函数迁移；剩余 12 个混合 Endian runtime dispatch 与 sized-numeric cast（L3），故保留
  - `env`——全部 9 函数迁移（同时修复 `pass_result_propagation::is_template_dispatch` 漏掉 `intrinsic` 的问题；effect fn + `@intrinsic` 组合曾因 bare fn-name 冲突导致跨 module retyping）

  Runtime Rust 函数（`runtime/rs/src/<m>.rs`）+ WASM `calls_<m>.rs` 继续保留（`dispatch_runtime_fallback` 由 symbol 反查）。
- **Stage 3**（2026-04-19 大部分完成）：含 closure 的 module。
  - `list`——30 个 closure / container 函数迁到 `@intrinsic` + 新 `@consume(xs[, ys])` attribute。接受 `Vec<T>` 的 List runtime 函数固定为 `Own` borrow，而非默认 `RefSlice`。调整 pipeline 顺序：`LambdaTypeResolve` + 第二次 `ConcretizeTypes` 现在在 Rust 和 WASM 两个 target 上提前运行（之前仅 WASM），使 `@intrinsic` 后的 `RuntimeCall` node 仍能解析 lambda parameter 类型，且 fold callback 中 `MatchSubject` 能看到 `c: String`。还修补 `MatchSubjectPass`，使其递归进 `RuntimeCall` / `TailCall` / `InlineRust` 参数；让 `LambdaTypeResolve::resolve_call_lambdas` + `resolve_list_elem_ty` 解读 `Named { almide_rt_list_* }` 与 `RuntimeCall`；让 `pass_concretize_types::resolve_call_ret_ty` 解读相同形态做返回类型替换。副作用：加入 `fs.read_bytes_raw` WASM emit stub；nn WASM 原先跳过的 `ggml_whisper.almd` 既有 ICE 现在 green 12/12。
  - `map`——已迁移 `map_values`；剩余 2 个 entry（用于 `List[Tuple]` 的 `from_list` 及其余部分）由批量工作覆盖。
  - `option` / `set`——已全部采用 `@intrinsic`。
  - `result`——**剩余**：`.map` / `.ok()` / `.err()` / `.and_then` 是 Rust method chain（没有 `almide_rt_result_*` runtime 函数）；迁往 `@intrinsic` 前，需要 `runtime/rs/src/result.rs` 中的 wrapper 层。另开工作线。
- **Stage 4**：effect module（fs、http、process、io、env〔done〕、datetime、random、regex、json、testing、matrix）。它们与 WASM runtime 交互，可能需要不同 attribute 形态。

目标 release 节奏：每个 `0.14.N` / 早期 `0.15.x` 一个 stage。完整统一落入 `0.15.0`。

<a id="technical-debt-during-migration"></a>

## 迁移期间的技术债

迁移刻意保留一些 patch 式接缝，需要在 Stage 3 前合并：

1. **重复的捆绑 source parse 路径。** `almide-frontend::bundled_sigs`（查找 `FnSig`）和 `almide-codegen::pass_stdlib_lowering::parse_bundled_inline_rust`（查找 `@inline_rust` template）各自独立 parse 每个捆绑 `stdlib/<m>.almd` source。source 字符串本身共享（`almide_lang::stdlib_info::bundled_source`），但双方各有 cache 与提取 pass。理想终态：捆绑 module 总是在 codegen 前处理 lowering 到 `IrModule`（包括绕过 `resolve.rs` 的 unit test），让两个 consumer 读取同一 typed IR。
2. **两个 `module_functions` API。** `almide-frontend::stdlib::module_functions` 返回仅 TOML 清单（main crate 剪除用），`module_functions_all` 则合并捆绑项。这与（1）相呼应：全部函数经 IR 后，区别可收敛为单一“所有已注册函数”查询。
3. **`@inline_rust` parameter borrow inference 短路。** `pass_borrow_inference::infer_function_borrows` 对有 `@inline_rust` / `@wasm_intrinsic` 的函数强制 all-Own，因为其 body 是空洞，针对空洞推断会产生错误的 `RefStr` / `RefSlice` borrow。template 是 borrow 语义唯一权威。若以后在 template 旁增加 `@pure` pure Almide fallback body，规则须变为“选中 template 时用 template，否则从 body 推断”。

<a id="mlir-backend--egg-arc-との関係"></a>

## 与 MLIR Backend + Egg 工作线的关系

本工作也是 [mlir-backend-adoption.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/mlir-backend-adoption.md) 的**热身**。各输出在下一轮（egg + MLIR）直接利用：

- `stdlib/<m>.almd` 的 pure Almide body → Almide dialect（MLIR）输入
- typed intrinsic（`@intrinsic(rust=..., wasm=...)`）→ MLIR FunctionImport
- `@rewrite` 声明规则 → 自动编译成 egg e-graph rewrite rule
- `@schedule` block → affine dialect schedule attribute

技术上可跳过本轮直接进入 MLIR，但 stdlib 若仍有 3 层定义，MLIR 移植工时会翻至 2 倍。不改变顺序。

<a id="non-goals"></a>

## 非目标

- 迁移期间不改手写 module 的 TOML *语法*（整体拔除会停滞工作）。
- 当捆绑 dispatch 路径仍需 Rust runtime 函数实现无法声明的 primitive（`alloc`、`panic_hook` 等）时，不删除 `runtime/rs`，只处理每个 stdlib 函数的实现文件。

<a id="success-signal"></a>

## 成功信号

- 增加 stdlib 函数（如 `list.intercalate`）只需一个文件变更：编辑 `stdlib/list.almd`，加入函数及可选 attribute。
- grep `stdlib/defs/` → 空（或只剩基础设施定义）。
- `grep -r "emit_list_call" crates/almide-codegen/src/emit_wasm/` → 单一 dispatch 点读取 attribute recipe，没有逐函数 match arm。

<a id="scope-estimate"></a>

## 范围估计

完整工作线：**集中工作 2-4 周**，拆成四次 release，不是一次工作时段能完成的任务。

<a id="decision-points-before-starting"></a>

## 开始前的决策点

- attribute 语法（Almide 原生 `@...` 还是 `#[...]`？）——争论虽小，却对 ecosystem 可见。建议用 `@name(args)`，贴近 Python / Swift decorator 风格，让 LLM 熟悉。
- WASM intrinsic recipe 格式——声明 DSL 还是嵌入 WASM？声明 DSL（如 `iter_chain(elem=T, filter=f)`）更易维护，但需要解释层。
- 迁移顺序：简单 scalar（`int`）优先还是最高价值（`list`）优先？大概先 scalar 证明基础设施，再 list 取得实际收益。

dojo run #11 建立 v0.14.7 基线后启动此工作；phase4 差值测量需要干净 baseline。
