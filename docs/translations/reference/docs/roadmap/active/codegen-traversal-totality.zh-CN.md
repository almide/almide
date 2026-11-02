原文说明 metadata：带有已实现 fold/lint 基础及其有限覆盖范围的 traversal-totality 设计。

<a id="codegen-traversal-totality"></a>

# Codegen 遍历完整性

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](codegen-traversal-totality.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](codegen-traversal-totality.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](codegen-traversal-totality.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source (English) / 原文（英語）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/codegen-traversal-totality.md)

> 原文注记：开头更新称穷尽 fold 递归与 legacy debt 为空的 lint 已实现。后续 Phase 0/1、fold 表的 NO 单元及首个 PR 提案仍是历史记录，不是当前待补基础设施。pass 清单和约80%是旧审计快照。通过构造获得全面保证的广泛措辞是设计目标；实际 lint 仅覆盖列出文件与语法形式，差分测试仅覆盖有界输入，不是所有 traversal 或语义误编译。


<a id="implemented-foundations-and-remaining-guarantee-scope"></a>

## 已实现基础与剩余保证范围

`fold_expr` 现在通过穷尽的 `map_children` primitive 递归（[fold.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-ir/src/fold.rs)）。强制 lint 已存在于 [traversal_totality_lint.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/tests/traversal_totality_lint.rs)，`LEGACY_DEBT` list 为空。因此下方 Phase 0/1 和首个 PR 指示是历史实现计划，不是缺失的基础设施。

lint 管辖列出的 source file 和语法上的 catch-all pattern；它不证明任意遍历完整，也不消除所有语义误编译。下方 pass 清单和“约80%”估计是原审计快照，不是当前普查。

**目标：**让“pass 忘记递归到某种 node kind”成为 **compile error 或 CI failure**，绝不成为静默的 native↔WASM divergence。这将剩余最大的*渐近式*差异类别（“持续扫描，持续发现”）转为通过构造获得的*静态*保证。

<a id="how-we-got-here"></a>

## 事情的由来

`DIV2`（PR #347）：`xs |> group_by(..)`，然后 `map.keys(g) |> map(k => …get(g,k)…)`，再 `map.values(g)`，native 构建失败（E0382，借用已 move 的 `g`），WASM 却正常运行。根因**不在** map 或 ownership，而在 `CaptureClonePass::transform_expr` 手写了 `match &expr.kind { …; _ => {} }`，其中 `_ => {}` **静默丢弃了被 `Borrow` 包装的 subtree**（`list.join(&list.map(keys, k => …g…), …)`），于是 `&(…)` 下的 closure 对 capture analysis 不可见，其捕获的 `g` 从未提前 clone。

修复添加了缺失的 wrapper arm。但对全部 37 个 codegen pass 的审计表明，**这是一类问题，而非单一事件**：约24个 pass 手写 `match expr.kind` 递归，以不递归的 `_ => {}` / `other => other` 收尾，每个都是等待合适嵌套触发的潜在 divergence。渐近式 sweep 一次次发现它们。我们要封住整个类别。

<a id="the-key-insight"></a>

## 核心洞察

修复过的每个 native-only divergence 都有同一形状：**native 要求 rustc 重新推导 IR 已知的信息，而 emit 指定不足**；或（本类）**pass 对 IR 遍历不足，静默跳过 subtree**。二者都是“pass 依赖某物，而非携带事实 / 访问 node”。静态解法是让携带 / 访问*在构造上完整*。

具体到遍历，pass 应**仅对其变换的 node** 做 `match expr.kind`，并**将其他所有递归委托给单个、由编译强制穷尽的 primitive**，绝不委托给手写 `_ => {}`。

<a id="the-good-news-the-infrastructure-already-exists"></a>

## 好消息：基础设施已经存在

| Primitive | File | 修改方式 | 穷尽（无 `_=>`） | 递归每个 wrapper child |
|-----------|------|---------|----------------------|------------------------------|
| `IrExpr::map_children` | `almide-ir/src/lib.rs` | 按值 | **是**（已文档化的统一关口） | 是 |
| `IrMutVisitor` / `walk_expr_mut` | `almide-ir/src/visit_mut.rs` | 原地 | **是** | 是 |
| `IrVisitor` / `walk_expr` | `almide-ir/src/visit.rs` | 只读 | **是** | 是 |
| `substitute_var_in_expr` | `almide-ir/src/substitute.rs` | 按值 | 是 | 是 |
| `fold_expr` | `almide-ir/src/fold.rs` | 按值 | **否——L123 的 `_ => {}`** | 是（目前） |

`IrExpr::map_children` 是标准范例，其 doc 已说明：*“所有 variant 显式列出（无 wildcard），因此添加新的 `IrExprKind` 会在此导致 compile error，迫使作者决定怎样遍历其 child。”* `free_vars.rs` 是**传递 context** 分析的示范 consumer：只 override binder node（`Lambda`/`Block`/`Match`/`ForIn`）以 push/pop `bound` set，其余均转给 `walk_expr`，所以不可能丢弃 subtree。`pass_concretize_types` 和 `pass_result_propagation` 的 `resolve_err_types` 已正确使用 `IrMutVisitor`，是安全的。

所以已**完成约80%**。工作是 (a) 封住唯一的基础设施缺口（`fold.rs`），(b) 把手写 pass 迁移到这些 primitive，(c) 用 lint 锁定，避免退步。

<a id="plan"></a>

## 计划

<a id="phase-0--infra-small-pure-safety"></a>

### Phase 0——基础设施（小型，仅安全性）
- 删除 `fold_expr` 的 `_ => {}`，基于 `IrExpr::map_children` 重写递归（或让 match 穷尽）。当前无行为变化；添加 variant 将成为 compile error。

<a id="phase-1--enforcement-lint-anti-regression-the-static-lock"></a>

### Phase 1——强制 lint（防退步，“静态”锁）

**敌人是*沉默*，不是 wildcard。** DIV2 是 `_ => {}`——*什么也不做*并静默丢弃 subtree 的 catch-all。Wildcard 没问题；*静默 no-op* wildcard 才是 bug。catch-all 有三种形式，只禁止一种：

| 形式 | 示例 | 判定 |
|------|---------|---------|
| **委托**（正确默认递归） | `_ => walk_expr_mut(self, e)` | ✅ 通过构造封住 DIV2 类别 |
| **响亮的暂定实现**（Swift `Never` model） | `_ => unreachable!("…")` / `_ => todo!()` / `panic!` | ✅ 在问题程序的 *codegen 时* trap，暴露缺口，绝不交付 divergence。允许逐步编写 pass。 |
| **静默 no-op** | `_ => {}` / `_ => e`（不递归就返回） | ❌ 唯一禁止形式，静默丢弃 subtree |

这对应*语言自身*的 `hole`/`todo`：暂定 body 可通过 typecheck（其类型与 context 进行类型合一），并生成 `todo!()`（Rust 的 `Never`——到达时响亮 trap）。编译器内部 walk 应遵守同一纪律：尚未处理的 node kind 必须*响亮失败*（`unreachable!`）或*正确递归*（`walk_*`），不能*沉默*。

- 添加 CI test（`tests/traversal_lint.rs` 或 build-script check），扫描 `crates/almide-codegen/src/pass_*.rs` 和 `crates/almide-ir/src/*.rs` 中的禁止形式：对 `IrExprKind` 值的 `match`，其 catch-all body 是**静默 no-op**（空 `{}`，或不递归就返回 node）；同时**允许**委托到 `walk_*` primitive 或 diverge（`unreachable!`/`todo!`/`panic!`/`Never` 类型 expr）的 catch-all。正准 primitive 自身加入 allow-list。新的静默丢弃 walker 使 CI 失败，响亮的暂定 walker 可通过。
- 这使保证成为*静态*而非*时间点快照*，同时保留增量 / 暂定 pass 的可编写性（Swift 的 `Never` escape hatch），符合 Almide 的 modification-survival-rate 使命。

<a id="phase-2--migrate-the-mutating-rewriters-prioritized-by-audit-risk"></a>

### Phase 2——迁移修改式 rewriter（按审计风险排序）
每次迁移都**行为微妙**（部分 catch-all 原本有意不递归）。每个 pass 迁移后都做差分检查：spec sweep 上的 `native == wasm` + 既有 suite。

| 层级 | Pass | 审计注记 |
|------|--------|------------------|
| **HIGH** | `pass_clone` (CloneInsertion) | 遗漏 `IterChain`/`RcWrap`——**仍存在的 DIV2 同类**：clone insertion 看不到 fused chain。先修它，这是 DIV2 的另一半。 |
| | `pass_borrow_inference` | 5 个手写 walker，多处丢弃式 catch-all；是 native borrow mode 的核心。 |
| | `pass_stdlib_lowering` | `rewrite_expr` 的 `other => other` 覆盖约36个 variant；“DIV2 风险就在这里”。 |
| | `pass_perceus` | WASM RC；每个 helper 都手写，遗漏几乎所有 wrapper。 |
| **MED** | `anf`, `box_deref`, `capture_clone`, `fan_lowering`, `lambda_type_resolve`, `list_pattern`, `match_subject`, `mut_param_lowering`, `peephole`, `result_propagation`（lift 路径）, `rust_lowering`（idx 路径）, `tco` | 各自丢弃一些 wrapper/collection subtree。`capture_clone` 已部分修复（#347），但应移至 `IrMutVisitor`+binder-stack pattern，以免再次漂移。 |
| **LOW / 分析** | `auto_parallel`, `builtin_lowering`, `closure_conversion`, `const_fold`, `effect_inference`, `licm`, `match_lowering`, `matrix_shape_spec` | 大多覆盖广或只读。**`effect_inference` 遗漏 `IterChain` 是真实潜在 bug**（fused chain 内 effect 被误分类），需要验证。只读分析迁移到 `IrVisitor`。 |
| **DEAD** | `pass_shadow_resolve` | `targets() == None`；从不运行，保留或删除。 |

<a id="phase-3--differential-ci-gate-shared-with-closure-cross-target-work"></a>

### Phase 3——差分 CI gate（与 closure cross-target 工作共享）
well-typed IR generator + CI 中的 `native` 对 `wasm` 执行 assertion（扩展 Perceus proptest 与字节相同的 determinism gate），能在有界输入空间上捕获残留 traversal/codegen divergence。在持续机器检查的意义上是“静态”，作为 lint 背后的后备防线。

<a id="sequencing"></a>

## 顺序安排

Phase 0+1 风险低、作用大，用于安装保证。Phase 2 是工作主体且行为微妙；应按优先层级分成**多个经过评审的 PR**，各自差分检查，而非一个巨大 diff。Phase 3 与 `closure-cross-target-completeness.md` 共用基础设施。

**首个 PR 提案：** Phase 0（`fold.rs`）+ Phase 1（lint）+ HIGH 层 `pass_clone` 迁移（已证明的 DIV2 同类）。这从端到端验证 pattern、关闭已知现存 bug，同时由 lint 防止新问题。
