<a id="bundled-almide-stdlib--ideal-form"></a>

# 内置 Almide Stdlib — 理想形式

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](bundled-almide-ideal-form.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](bundled-almide-ideal-form.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](bundled-almide-ideal-form.zh-CN.md)

> [英文正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/bundled-almide-ideal-form.md)的非规范性读者快照，固定提交为 `35297a612fb29e3f78f6587aba253aa91d51f466`。原文仍是正本。本快照不自动同步，也不是生成器／检查器输入。下文日期、计划、完成状态、测量和比较主张均保留为历史原文记述，不是独立复现或对当前版本的验证。代码块及注释保持原样。历史收集元数据以不生效的读者注记显示。

> 历史元数据：描述 — 内置 Almide stdlib 的理想形式：单一 dispatch 路径，无补丁层特例；完成日期 — 2026-04-17。

> 原文注意事项：完成横幅说明五项债务均已关闭，保留的最初正文却仍有“开放项”和剩余工作估算。这是不同历史层次，不是重新打开当前问题。MSR 理由和工时估算原样保留，不构成新基准结果或交付承诺。

> **状态**：v0.14.7-phase3.N 这一系列工作已关闭全部 5 项技术债（`codegen-ideal-form.md §Phase 3 Arc` 中 S1 → A）。关闭时的补丁层审计见 `CHANGELOG.md §0.14.7-phase3.5`。下文保留最初限定范围的理由。

`bundled-almide-dispatch` 工作（v0.14.6 已发布）使 `stdlib/<m>.almd` 可用，但初次实现把内置 fn 当成不同于顶层用户 fn 的东西，因此增加了几个补丁层特例。理想形式把内置 fn 视为所属模块的一等成员 — 整条管线都没有旁路。本文列出剩余技术债，以便在 0.14.7-phase3 中逐一消除。

<a id="guiding-principle"></a>

## 指导原则

> 从类型检查器、单态化器和 codegen dispatcher 的角度，内置 stdlib fn 应与顶层用户 fn 无法区分。只有 **TOML 运行时映射**知道 fn 是否具有 Rust/WASM 运行时函数体。

以下每个剩余项，都是当前代码含有“内置／非内置”特例、而理想形式应消除的位置。

<a id="open-items"></a>

## 开放项

<a id="1-single-dispatch_module_call-entry-per-target"></a>

### 1. 每个目标只有一个 `dispatch_module_call` 入口

目前 codegen 有两处可到达模块 fn：

- **Rust**：若 fn 有 TOML 条目，`pass_stdlib_lowering` 将 `Module { m, f }` 改写为 `Named { almide_rt_m_f }`；否则保留原样（walker 把剩余项输出为模块调用）。
- **WASM**：`emit_wasm/calls.rs` 有手写的 `match module`，分派到 `emit_list_call` / `emit_int_call` / 等；每个模块都有尝试 `func_map.get("almide_rt_m_f")` 的 fallback。

理想情况是每个目标只有一个入口，优先级如下：
1. IR 模块有名为 `f` 的 fn → 用户 fn 调用路径（涵盖内置 fn）。
2. TOML registry 有 `(m, f)` → 现有 inline emit。
3. 两者都没有 → **编译期 ICE**，而不是运行时 `stub_call → unreachable`。

理由：目前 `list.catt(xs)` 这类拼写错误会静默编译，并在首次调用时 trap。理想形式把它转为诊断。

<a id="2-retire-emit_stub_call--unreachable"></a>

### 2. 移除 `emit_stub_call → unreachable`

`emit_wasm/calls.rs` 的 `emit_stub_call_named` 按结构总是可达（WASM emit 在每个模块 dispatch 中都有 `_` 分支）。这些 fallback 存在，是因为 WASM emitter 缺少统一的“未知 fn” ICE 路径，又不想在输出部分有效的 IR 时崩溃。

(1) 落地后，stub 路径消失。WASM emit 要么成功，要么返回指出缺失 fn 的编译期错误。

<a id="3-concretizetypes-hard-postcondition"></a>

### 3. `ConcretizeTypes` 的硬性后置条件

`ConcretizeTypes` 有由 `ALMIDE_AUDIT_TYPES=1` 启用的软审计，指出 pass 后仍未解析的类型（参见 `pass_concretize_types.rs` 顶部注释）。目前有两个已知残留来源（部分 `list.zip` 情况的 Call 返回类型）。解决它们，将审计设为硬性后置条件（debug 下 panic，release 下 error），并移除环境变量。

这为何对内置 Almide 重要：内置 fn 可能把 TypeVar 带到 WASM emitter fallback 为 `i32` 的位置，这正是此次工作中掩盖 split_at 失败的 bug 类别。硬性后置条件能在 IR 验证时捕捉这类问题。

<a id="4-unify-optionresult-almd-signatures-with-toml"></a>

### 4. 统一 option/result 的 `.almd` 与 TOML 签名

`stdlib/option.almd` / `stdlib/result.almd` 覆盖 TOML 签名（`fn() -> X` 对 `Fn[Unit] -> X`）。删除 .almd 文件会破坏测试套件，因为每个调用者都写 `() => x`。三条路径与建议见 `option-result-bundled-cleanup.md`（方案 A：将 TOML 规范化为 `Fn[] -> X`）。

这阻塞项 (5)：只要签名覆盖仍存在，就不能诚实地宣称“内置 fn = 一等成员” — 类型检查器仍赋予它们凌驾于 TOML 签名之上的特权。

<a id="5-first-class-mono-for-module-fns--done-verify-coverage"></a>

### 5. 模块 fn 的一等单态化 — 已完成，验证覆盖范围

v0.14.6 增加了 `monomorphize_module_fns`，在 Rust 和 WASM 上都处理泛型内置 fn（`list.split_at[T]`、`list.iterate[T]`）。剩余工作是**覆盖范围**：目前扫描很窄（仅面向内置 stdlib 的 `CallTarget::Module`）。扩大到：

- 最终 lower 为 Module 的 `CallTarget::Method` + UFCS 形式。
- 用户包中的内置 fn（不仅是 stdlib）。
- 跨模块泛型链（开始在 `stdlib/option.almd` 中编写真正的内置 fn 后，其中的 `list.iterate`）。

为每条扩展路径增加回归测试。

<a id="why-ship-v0146-before-fixing-all-of-these"></a>

## 为何在修完这些之前发布 v0.14.6

项 (5) 已完成。(1)–(4) 是不会阻碍 MSR 叙述的完善工作 — dojo 框架编写的惯用代码已被当前 dispatch 支持。现在发布 0.14.6，可让下游用户从 `cargo install --git --branch` 转向带标签的版本，同时我们在 `develop` 上逐步清理。

<a id="scope"></a>

## 范围

0.14.7-phase3 系列工作的粗略估算：

- (4) option/result 签名规范化：2–3 小时
- (1) 统一 dispatch 入口 + (2) 移除 stub：合计 3–4 小时
- (3) ConcretizeTypes 硬性后置条件：2 小时（补齐已知缺口并切换）
- (5) 扩展：由 dojo 证据驱动，每个扩展方向 1–2 小时

总计约 10 小时集中工作，可分成约 4 个提交。
