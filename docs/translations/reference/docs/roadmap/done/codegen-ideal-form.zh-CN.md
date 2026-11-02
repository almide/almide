说明：面向声明式 dispatch 与显式 symbol 解析的 WASM codegen 重设计。
历史完成元数据：2026-04-19。
<a id="codegen-ideal-form"></a>

# Codegen 理想形态

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](codegen-ideal-form.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](codegen-ideal-form.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](codegen-ideal-form.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/codegen-ideal-form.md)

> 原文注意事项：这是 4 月 13–19 日的历史设计与收尾记录，不是当前实现审计。收尾包括拆分到后续里程碑的工作，不表示已实现理想形态的全部目标。早期关于 default 无害与剩余布局风险的描述、VarTable 范围变化、延期与完成的各阶段，以及不同数量估计均作为不同历史层次保留。理想的符号解析／错误设计不同于所记录的 Named 类型／panic 实现。原文的不可达／已证明措辞描述其记录的有限测试证据，并非独立确立的普遍证明。后续分派与定义表统一工作仍与已完成工作区分。

> **工作线状态（2026-04-19）：CLOSED。** 全七项已落地或拆分为独立工作线。各项落地及有意排除在范围外的后续事项见下方“最终状态”。

逐步将 v0.13 系列累加的 codegen 重构至**理想形态**。此次 nn/WASM 支持暴露的 bug（类型解析散乱、test 函数名称冲突、lifted closure var_table 串线、stdlib emit 的 `list_elem_ty` 20处 bug），根因全是**设计层腐化**。

<a id="動機"></a>

## 动机

今天 nn WASM 工作遇到的地雷，共性都是“**临时修补未在正确位置进行**”：

| 症状 | 真因 | 对症处理 |
|---|---|---|
| nested lambda 类型未解析 | Closure Conversion 内含类型推导 | `has_deep_unresolved`、VarTable 优先 |
| `p.0 * p.1` 变成 `MulInt` | op lowering 固定 BinOp，emit 修正 | `fix_binop_type` |
| `list.slice(row, ...)` 按 i32 复制元素 | `args[0].ty` 过时 | 20处替换为 `resolve_list_elem` |
| `t.broadcast_add` 跳到 test 函数 | module 注册漏掉 test 函数 name prefix | 添加 `is_test` 判定 |
| lifted closure VarId 损坏 | program.var_table 与 module.var_table 边界管理 | module 的 lifted 留在 module.functions |
| 函数未解析 → 运行时 `unreachable` | resolve 职责未在编译时封闭 | （未解决） |

都是“再改一处就能跑”。下一个相同 bug 只是时间问题。

<a id="ターゲット-理想形"></a>

## 目标：理想形态

<a id="1-関数解決を独立パスに-最優先"></a>

### 1. 函数解析独立为 pass（最优先）

**现状**：WASM emit 见 `CallTarget::Module { module, func }`，从 `func_map` 查 `almide_rt_{module}_{func}`。找不到则 bare name fallback，再失败则 `emit_stub_call` 发出**运行时 `unreachable`**。即最坏 UX：“类型检查过、build 过、WASM validation 过、运行时 trap”。

**理想**：

- `pass_resolve_calls.rs`（新 nanopass，适用于 Rust/WASM 两者）。
- walk 所有 `CallTarget::Module` / `CallTarget::Named`。
- canonical 化 module 名（alias → 实际 module，规范 nn.tensor vs tensor）。
- 改为对目标 `IrFunction` 的引用（`Sym`），如新 variant `CallTarget::Resolved { func_ref: Sym }`。
- 无法解析的调用为**编译时错误**，诊断带“Did you mean...?”。
- emit 仅 call 已知 symbol，**删除** `emit_stub_call`。

后置条件：所有 `CallTarget::Module` / bare `CallTarget::Named` 均改成 `Resolved`。

<a id="2-stdlib-関数の宣言駆動化"></a>

### 2. stdlib 函数声明驱动化

**现状**：`emit_wasm/calls.rs`（1000行+）、`calls_list.rs`（1000行+）、`calls_list_closure.rs`、`calls_list_closure2.rs`、`calls_list_helpers.rs`、`calls_map.rs`、`calls_option.rs`、`calls_set.rs`——match 堆积。同一个 `elem_ty = self.list_elem_ty(&args[0].ty)` 模式写在20+处，修一个 bug 要改20处。

**理想**：

- Rust target 已由 TOML 驱动（`stdlib/defs/*.toml`），WASM 也用同法。
- 各 stdlib 函数 = 声明式描述：
  ```toml
  [list.slice]
  params = ["list", "int", "int"]
  ret = "list_of[0]"  # 型式
  elem_source = "arg[0]"  # 要素型をどこから取るか
  emit = """
  # alloc: 4 + new_len * elem_size
  # loop: copy [start..end] from src to dst
  ...
  """
  ```
  原注释译文：类型表达式；元素类型从哪里取得。emit sketch 分配 4 + new_len × elem_size，并循环将 src 的 [start..end] 复制到 dst。
- Emitter 是由声明生成 WASM 的**通用引擎**，各 stdlib 专用代码消失。
- “从哪里取得元素类型”是声明的一部分，不会选错 `list_elem_ty` 与 `resolve_list_elem`。

效果：emit_wasm calls*.rs 大量消失（估计 −4000行）。

<a id="3-irmutvisitor-trait"></a>

### 3. `IrMutVisitor` trait

**现状**：`rewrite_var_ids` 是200行手写 match，每加 IrExprKind variant 就坏。相同模式散布多个 `pass_*.rs`。

**理想**：

- 向 `almide-ir` 添加 `IrMutVisitor` trait：
  ```rust
  pub trait IrMutVisitor {
      fn visit_expr_mut(&mut self, expr: &mut IrExpr) { walk_expr_mut(self, expr); }
      fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) { walk_stmt_mut(self, stmt); }
      fn visit_pattern_mut(&mut self, p: &mut IrPattern) { walk_pattern_mut(self, p); }
  }
  pub fn walk_expr_mut<V: IrMutVisitor + ?Sized>(v: &mut V, expr: &mut IrExpr) { /* recurse */ }
  ```
  原注释 recurse 意为递归遍历。
- `rewrite_var_ids` 变30行：
  ```rust
  impl IrMutVisitor for VarIdRewriter {
      fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
          if let IrExprKind::Var { id } = &mut expr.kind {
              if let Some(new) = self.mapping.get(id) { *id = *new; }
          }
          walk_expr_mut(self, expr);
      }
  }
  ```
- 与已有只读 `IrVisitor` 对称。

<a id="4-型解決の一元化"></a>

### 4. 类型解析统一

**现状**：类型解析散布多层：
- `LambdaTypeResolve` pass（top-down）。
- `propagation.rs`（mono）。
- `emit_wasm/closures.rs::resolve_expr_ty`（emit 时）。
- `emit_wasm/collections.rs::emit_tuple_index`（判断 VarTable vs expr.ty 优先顺序）。
- 各 `calls_*.rs` 的 `resolve_list_elem`。

同一个“`p.0` 是什么类型”在3处求解，无法一致而出 bug。

**理想**：

- `LambdaTypeResolve` 后运行 **`TypeConcretization` pass**，使全部 IrExpr 的 `.ty` 与 VarTable 一致。
- 后置条件：可执行路径不剩任何 `Ty::Unknown` / `Ty::TypeVar`。
- Emit **只信任** `expr.ty`，无需查 VarTable。
- 可删 `resolve_expr_ty` / `resolve_list_elem` 等 emit 时 type lookup helper。

若达成，便解决此次 `is_unresolved_structural()` vs `has_deep_unresolved()` vs `is_unresolved()` 地狱。

**剩余 gap（v0.14.7，WASM `ALMIDE_CHECK_IR=1` audit 21项）**：

以 `ALMIDE_CHECK_IR=1` 在 WASM 跑 spec/，15-21项因 audit 违反而 panic。细分：

| pattern | 例 | 数量 |
|---|---|---|
| empty list/collection literal 元素类型反推失败 | `fn fan.map empty list: List[Unknown]` / `fn empty for: List[Unknown]` | 10+ |
| Codec auto-derive 空 field | `fn decode_container: List[Unknown]` / `fn derive Codec: empty list field` | 3-4 |
| Result/Option ok/some() inner 类型未定 | `fn safe_div: Result[Unknown, String]` / `fn validate_age` | 2-3 |
| OpenRecord → concrete record 未解析 | `fn chain_b__Unknown: OpenRecord { name: String }` | 1 |
| 深 generic context | `fn is_balanced: Option[List[Unknown]]` | 1 |
| Event/Tuple empty payload | `fn extract_click_positions: List[Tuple[Unknown, Unknown]]` | 1 |

default（无 env var）下 WASM emit defensive fallback（`resolve_list_elem` 四段 fallback、`Unknown` → i32 假设）吸收问题，故203/206 pass。潜在风险：empty list 按 i32 元素 alloc，之后 push i64，layout 不一致等。只是 spec test 无对应模式，用户代码可能踩中。

**Fix 拆分（0.14.8 起 sub-phase 候选）**：

- (a) 从 consumer 反推 empty literal：`fn f(xs: List[Int]) = f([])` 推出 `[]: List[Int]`。checker 扩展，半天至1天。
- (b) `ok(x)` / `some(x)` wrapper inner 类型推导：由 expected ret_ty 反推 wrapper 参数类型。半天。
- (c) Codec auto-derive empty field：在 `derive.rs` 将空 list field 元素类型固定到 default。1-2小时。
- (d) OpenRecord → concrete resolve：整理 checker open → closed 路径。数天。
- (e) Event/Tuple empty payload：从 variant sig 传播 payload 类型。半天。

(a)+(b)+(c) 解决17/21，(d)+(e) 解决剩4项。总范围1周。

**暂时立场**：default 下无害，故非 release blocker。若 dojo measurement 出现用户代码暴露模式，则提高优先级解决。适合在 stdlib-declarative-unification 间隙推进（同为提高 WASM emit 精度）。

<a id="5-vartable-の責務明確化"></a>

### 5. 明确 VarTable 职责

**现状**：
- `program.var_table` 与 `module.var_table` **分离**。
- ClosureConversion lifted closure 因“放哪”破坏 VarId 引用（今天已修）。
- test 和 WASM emit 每次判断“此函数 var_table 是哪个”。

**理想**（2选1）：

A) **VarId 带 region 信息**：`VarId(ModuleId, u32)`，lookup 统一为 `program.lookup(var_id)`。
   - 优点：参数传递简单。
   - 缺点：VarId 变大（u32 → 相当于u64）。

B) **每函数 var_table**：持有 `IrFunction.var_table`，封闭在函数 scope。
   - 优点：VarId 局部性明确。
   - 缺点：跨函数 VarId 引用（closure capture）需重写。

二选一统一。B 更符合“函数 = 封闭单元”。

<a id="6-test-namespace-完全分離"></a>

### 6. Test namespace 完全分离

**现状**：`fn broadcast_add` 与 `test "broadcast_add"` 具有相同 `func.name`，注册冲突。今天在3处加 `is_test` 检查处理。

**理想**：
- Parser/lowering 时，test 名规范为 `__test::{name}` 等始终不冲突的形式。
- 保留 `IrFunction.is_test`，但**名称已 unique**，下游全部 pass 无需判断冲突。

<a id="7-emit_stub_call-廃止"></a>

### 7. 废除 `emit_stub_call`

**现状**：无法解析调用以 `drop args; unreachable` trap。
- validation 通过 → 运行时 trap。
- 难调试哪个 call 未解析。
- 说明类型与函数层未分离。

**理想**：上方 #1 独立解析后，**emit 时不存在未解析调用**。删掉 `emit_stub_call` 本身。

<a id="実装順-推奨"></a>

## 实现顺序（推荐）

1. **#1 函数解析 pass**——最大价值（运行时 trap → compile error），独立性高，其他改造基础。
2. **#3 IrMutVisitor**——机械工作，使其他 pass 易重构。
3. **#4 TypeConcretization**——有 #1/#3 后更易写。
4. **#5 VarTable 整合**——与 #4 互依，一起做。
5. **#6 Test namespace**——独立，随时可做。
6. **#2 stdlib 声明驱动**——删行最多，但需谨慎分阶段。
7. **#7 emit_stub_call 废除**——#1 副产物。

<a id="進捗ログ"></a>

## 进度日志

- **2026-04-13**：开始 #1（Phase 1a）。
  - 添加 `pass_resolve_calls.rs`，接入 Rust/WASM 双 pipeline。
  - 后置条件 `Custom(verify_all_calls_resolved)` 检测未解析用户 module call。
  - Phase 1b（IR 重写）与1c（删 `emit_stub_call`）留后。
- **2026-04-13**：开始 #3。
  - 新建 `crates/almide-ir/src/visit_mut.rs`，`IrMutVisitor` + `walk_expr_mut` / `walk_stmt_mut` / `walk_pattern_mut`。
  - `pass_closure_conversion.rs::rewrite_var_ids` 160行→60行。
  - 全183 spec + nn WASM 60 test 通过。
- **2026-04-13**：开始 #4（ConcretizeTypes pass）。
  - 新建 `crates/almide-codegen/src/pass_concretize_types.rs`，bottom-up concrete 化全部 IrExpr.ty。
  - BinOp 规范化：Float 操作数上的 `AddInt` → `AddFloat`，在 IR 层修正。
  - 效果：**完全删除** `emit_wasm/expressions.rs::fix_binop_type`（20行修正函数）。
  - `ALMIDE_CHECK_IR=1` IR verify 通过（此前 AddInt/Float 类型不匹配失败）。

- **2026-04-13**：#4 扩展——Call 返回类型解析 + canonical `has_unresolved_deep`。
  - 给 `almide-types` 加 `Ty::has_unresolved_deep()`，删2份临时副本。
    - 以前有 `is_unresolved()` / `is_unresolved_structural()` / `has_deep_unresolved()` 3种，今天因误用出3次 bug。
  - ConcretizeTypes 加 **SymbolTable**，构建 `(module, func) → ret_ty`。
    - 直接解析用户模块函数（如 `tensor.broadcast_add`）返回值。
    - stdlib `list.*`（map/filter/zip/fold/reduce 等）也以类型表达式解析。
  - 效果：
    - `ALMIDE_AUDIT_TYPES=1` 下 nn 6/7 module 零 warn（仅 gguf `Option[Unknown]` 6项剩余，无实际损害）。
    - `resolve_list_elem` 四段 fallback **简化为2段**（可信任 ConcretizeTypes）。
  - 全183 spec + nn WASM 60 test 通过。

- **2026-04-13**：完整删除 `resolve_expr_ty` + 确认 `emit_stub_call` 不可达。
  - 删除 `emit_wasm/closures::resolve_expr_ty`（**90+行**，emit 时临时类型解析 helper）。
    - 外部2处调用（`closures.rs:62`、`mod.rs:827`）换 `expr.ty.clone()`。
    - ConcretizeTypes 可信，emit 变成“只信类型”。
  - 给 `emit_stub_call` 加 `ALMIDE_WASM_STUB_PANIC=1` 模式。
    - 触发即 compile-time panic，用于 CI。
    - **全183 spec + nn WASM 60 test** 以 panic 模式运行 → **零触发**。
    - 因此 stub_call 在当前代码库已不可达，可安全推进 #7（删 stub）。
  - 相当于 #7 Phase 1 完成：**实测证明“未爆弹”并非未爆弹**。

- **2026-04-13**：开始 #2（stdlib 声明驱动 Phase 1）。
  - 新建 `emit_wasm/stdlib_dispatch.rs`，`StdlibOp` enum 表达 `Call1` / `Call2` / `Call3` / `FloatUnaryCall` 模式。
  - `emit_math_call` / `emit_float_call` / `emit_string_call` 转向声明表：
    - math：sin/cos/tan/log/exp/log10/log2（7个）。
    - float：to_string。
    - string：trim/trim_start/trim_end/reverse/len/contains/split/join/count/replace（10个）。
  - 模式证明完成，可将同 dispatcher 展开至全 module（option/map/bytes/value/list 等）。
  - 全183 spec + nn WASM 60 test 通过。

- **2026-04-13**：删 pass.rs dead code + 拆 emit_wasm/calls.rs（4609→1144行）。
  - 删除 `pass.rs` 中 stub `OptionErasurePass` / `CloneInsertionPass`（stub）/ `TypeConcretizationPass`（stub）/ `StreamFusionPass`（stub）/ `ResultPropagationPass`（stub）/ `default_pipeline()`（无人调用）。
    - 清除展示时致命的“无操作 pass 排在 pipeline”虚假。
    - 将 `TypeConcretizationPass`（Rust stub）/ `OptionErasurePass`（Python stub）移出 pipeline。
  - 按功能拆 `emit_wasm/calls.rs`（225KB / 4609行）：
    - `calls_env.rs`（68）、`calls_random.rs`（219）、`calls_datetime.rs`（481）。
    - `calls_http.rs`（332）、`calls_fs.rs`（1728）、`calls_io.rs`（419）、`calls_process.rs`（276）。
  - calls.rs 成1144行，职责归为“主 dispatch + stub + assert_eq + fan + env/random/datetime/http/fs/io/process 以外 scalar ops”。
  - 全183 spec + nn WASM 60 test 通过。

<a id="非ゴール"></a>

## 非目标

- **重设计 Rust TOML template** 不在范围（已工作）。
- **新增功能**（新 stdlib、新语法）不在路线图，仅结构重构。
- **性能优化**——正确性优先。

<a id="測定"></a>

## 测量

每阶段：
- **全部 spec/ test 通过**（零回归）。
- **全部 nn module 在 Rust/WASM 均通过**（保持今日基准）。
- **almide-dojo MSR** 不下降。

<a id="関連"></a>

## 相关

- `docs/roadmap/active/whisper-almide.md`——运行于此基础。
- `docs/roadmap/active/stdlib-defs-runtime-consistency.md`——与 #2 相关。
- `crates/CLAUDE.md`——当前设计原则。
- `crates/almide-codegen/CLAUDE.md`——当前三层架构。

---

<a id="phase-3-arc-v0147--ideal-form-migration-done-2026-04-17"></a>

## Phase 3 工作线（v0.14.7）— 理想形态迁移［DONE 2026-04-17］

作为 v0.14.7 发布。六个发布点（S1 / S2 / S3 / S4 / B / A）关闭 `done/bundled-almide-ideal-form.md` 所列全部5项。完整 patch 层审计及各步说明见 CHANGELOG `[0.14.7]`。

Phase 3 重点：**dispatch 层去重**。定义层（TOML + `runtime/rs` + `emit_wasm/calls_*.rs`）仍三重编写，此范围属于 Stdlib Declarative Unification，见 `active/stdlib-declarative-unification.md`。其后 egg + MLIR 由 `active/mlir-backend-adoption.md` 接续。

<a id="historical-phase-3-plan-below"></a>

### 历史 Phase 3 计划（下文）

0.14.6 发布 bundled-Almide dispatch 时积累多层 patch（option/result signature 共依赖、stub_call 残留、mono coverage 缺口）。`bundled-almide-ideal-form.md` 的5项 debt 中 (1)(2)(3) 与此处 #1/#4/#7 重合，(4)(5) 新增。合并为4步工作线回收。

<a id="step-s1--optionresult-signature-normalization-0147-phase31"></a>

### Step S1 — Option/Result signature 规范化 `0.14.7-phase3.1`

- TOML `Fn[Unit] -> X` 改 `Fn[] -> X`（option.toml 2处，其他 module 无，已调查）。
- `stdlib_codegen.rs::parse_type` 的 `Fn[...]` 分支处理 empty params（当前仅 `fn()` 侧处理，`Fn[]` 不支持）。
- 确认各 target template 正确 render `Fn[] -> X`（Rust：`impl Fn() -> X`，WASM：closure convention）。
- 删除 `stdlib/option.almd`、`stdlib/result.almd`（无需 signature override）。
- 从 `BUNDLED_MODULES` / `AUTO_IMPORT_BUNDLED` / `get_bundled_source` 删除 option/result。
- 确认 `spec/stdlib/coverage_misc_test.almd`（`() => x` caller）以新 signature 通过。
- 既有 spec 全通过 + 双 target smoke。

**估计**：2-3h，1 commit。须在 #7（废 stub）前做，因 option/result 会阻塞。

<a id="step-s2--concretizetypes-hard-postcondition-0147-phase32-shipped"></a>

### Step S2 — ConcretizeTypes 强制后置条件 `0.14.7-phase3.2` *（已发布）*

完成 codegen-ideal-form #4。phase3.1 达到常开 audit + `ALMIDE_CHECK_IR` panic escalate，phase3.2 完成 **flip to hard**：

- spec/ WASM sweep ConcretizeTypes audit：3类→0（PRs #194–#198）。
  - 含 list.zip `Option[Unknown]` 的15-21真实案例，通过 empty-list elem ty / ResultErr Ok slot / fold acc back-prop / Match subject → pattern bindings / OpenRecord skip 组合关闭。
- 删除 `pass.rs` 的 `ALMIDE_CHECK_IR` / `ALMIDE_VERIFY_IR` gating。
- debug build 对 IR verify + Postcondition 违反常时 panic；release 向 stderr 发 diagnostic（非 panic）。
- CHANGELOG 记载“`expr.ty` 现依契约可信”。

剩余 WASM lifted-lambda TypeVar 来自 `ClosureConversion` pass 边界问题，预计 S3（pass_resolve_calls Phase 1b-c）自然解决。

<a id="step-s3--pass_resolve_calls-phase-1b-d--stub-廃止"></a>

### Step S3 — pass_resolve_calls Phase 1b-d + 废 stub

分阶段完成 codegen-ideal-form #1/#7 的工作线。

- **Phase 1b** *（已发布，`0.14.7-phase3.5`）*——bundled-Almide stdlib fn `CallTarget::Module` → `CallTarget::Named { almide_rt_<m>_<f> }` 重写。
- **Phase 1c** *（已发布，`0.14.7-phase3.2`）*——`emit_stub_call*` 转 compile-time panic（废运行时 trap）。
- **Phase 1d** *（已发布，`0.14.7-phase3.3`）*——彻底删 `emit_stub_call_named` / `emit_stub_call` helper，将各 WASM dispatcher `_` fallback 换 inline `panic!("[ICE] emit_wasm: no WASM dispatch for ...")`。S2 flip 下已为 compile-time ICE，故行为不变，诊断变 module/dispatcher 专用。

剩余（Phase 1e+）：

- Rust + WASM dispatch 入口统一到 `dispatch_module_call`。
  - priority：(a) IR user/bundled fn → user-fn call / (b) TOML → inline emit / (c) 两者皆无 → compile-time ICE。
- WASM emit 的 `emit_list_call` / `emit_int_call` 等 match arm 不再需要 fallback（resolve pass 预解析）。

**估计**：Phase 1e 4-6h，扩 resolve pass + Rust/WASM dispatch 整合，为工作线主轴。

<a id="step-s5--test-namespace-normalization-shipped-2026-04-19"></a>

### Step S5 — Test namespace 规范化 *（2026-04-19 已发布）*

完成 codegen-ideal-form #6。`lower_test` 为 `IrFunction.name` 加 `__test_almd_` prefix，所有下游 pass（walker / emit_wasm / module emit）的 `is_test` 名称避碰逻辑消失：

- 新公开 `almide_ir::TEST_NAME_PREFIX` + `IrFunction::display_name()`。
- `lower/mod.rs::lower_test` 分配带 prefix 的 `Sym`。
- 删 `walker/mod.rs`、`emit_wasm/mod.rs` 的 `if func.is_test { format!("__test_…") }` 分支，直接用 `func.name`。
- 下游全 spec + WASM 全 test green（219 + 213）。

`is_test` 留作语义用途（mono 保留 / auto_unwrap / template 选择），但**上游名称已 unique**。

<a id="step-s6--vartable-consolidation-deferred-to-activevar-table-unificationmd"></a>

### Step S6 — VarTable 整合 *（延至 `active/var-table-unification.md`）*

codegen-ideal-form #5 剩务：删除 `program.var_table` / `module.var_table` 两层结构本身。本工作线核心痛点（lifted closure 跨 var_table）已由 `pass_closure_conversion` 保持 module-local 解决，故**结构整合拆独立工作线**。追踪：`active/var-table-unification.md`。

<a id="step-s4--monomorphize-coverage-拡張-0147-phase33"></a>

### Step S4 — Monomorphize 覆盖扩展 `0.14.7-phase3.3`

bundled-almide-ideal-form #5。0.14.6 的 `monomorphize_module_fns` 窄（Module call → 仅 bundled stdlib generic），扩展：

- `CallTarget::Method` + UFCS lower 后成为 Module 的路径。
- 用户 package generic fn（非 stdlib，纯用户定义）。
- 跨 module generic chain（bundled A generic 调 bundled B generic）。

**估计**：2-3h，1-2 commits。S3 落地后更干净（仅对 resolved call target discover）。

<a id="release-cadence"></a>

### 发布节奏

- `0.14.7-phase3.1`：S1 + S2
- `0.14.7-phase3.2`：S3
- `0.14.7-phase3.3`：S4
- `0.14.7`：正式发布（4步完成 + dojo regression sweep）

每个 `.N` 测量 dojo v0.14.6 baseline + subset，确认 MSR 无回归再继续。

<a id="non-goals"></a>

### 非目标

- Phase 3 不增 stdlib fn（等 infra 完成）。
- 词汇新增（UFCS / ? chain 扩展候选）后延至0.15起。
- dojo task bank 扩展由 dojo 团队并行。

---

<a id="final-status-2026-04-19"></a>

## 最终状态（2026-04-19）

| # | 项目 | 状态 | 落地 |
|---|---|---|---|
| #1 | Resolve-calls 独立 pass | **完成** | v0.14.7-phase3.5（PR #202） |
| #2 | stdlib 声明式 dispatch | **拆分** | → `active/stdlib-declarative-unification.md` |
| #3 | `IrMutVisitor` | **完成** | phase 1a（2026-04-13） |
| #4 | ConcretizeTypes 强制后置条件 | **完成** | v0.14.7-phase3.2（PR #199） |
| #5 | VarTable 统一 | **完成** | commit `ba50fd7b` → `done/var-table-unification.md` |
| #6 | Test namespace 规范化 | **完成** | commit `496c51e4` |
| #7 | `emit_stub_call` 删除 | **完成** | v0.14.7-phase3.3 |

仅 `#2` 仍未完：dispatch 层去重在此完成，但定义层（TOML + `runtime/rs` + `emit_wasm/calls_*.rs`）仍三重编写，故重划范围为独立工作线。

<a id="what-this-arc-actually-shipped"></a>

### 实际交付内容

- **Call 解析是 pass**，不再 emit 时猜测。未解析 call 编译时 ICE；`emit_stub_call_*` helper 已删，各 WASM dispatcher fallback 为 inline `panic!("[ICE] ...")`，spec sweep 已证明其不可达。
- **`expr.ty` 依契约可信。** `ConcretizeTypes` 为强制后置条件。emit 时类型再推导（`resolve_expr_ty` / `resolve_list_elem` 四段 fallback）已删。debug 对剩余 `Unknown` panic，release 发诊断。
- **`IrMutVisitor`** 使 VarId 重写 pass 从200行变30行，新 IR variant 不会静默破坏下游 pass。
- **test block 在上游带 `TEST_NAME_PREFIX`**，walker / emit_wasm 无需逐点 `if is_test { format!("__test_…") }` 名称冲突 guard。
- **VarTable 已统一**：首个 codegen pass 将 `IrModule.var_table` 归入 `IrProgram.var_table`。name-keyed `top_let_globals_by_name` mirror 现为备用，而非主要跨 module lookup。

<a id="follow-ups-not-part-of-this-arc"></a>

### 后续（不属于本工作线）

- `active/stdlib-declarative-unification.md`——去除定义层三重编写。
- `active/mlir-backend-adoption.md`——egg + MLIR 编译器世界工作线。
- `active/dispatch-unification-plan.md`——统一 Rust、WASM stdlib dispatch 入口。
