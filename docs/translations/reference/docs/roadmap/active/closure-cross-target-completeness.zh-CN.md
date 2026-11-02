原文说明：历史 closure 扫描、后续架构背景及证据限制
<a id="closure-cross-target-completeness"></a>

# Closure 跨 target 完整性

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](closure-cross-target-completeness.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](closure-cross-target-completeness.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](closure-cross-target-completeness.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/closure-cross-target-completeness.md)

> 原文注记：这是历史计划，不是当前 gap 复现、全部 closure 程序证明或修改 CI 的指令。五类标签、退役 emitter 位置、数量和方案保留原范围。Class2 明示不健全性已修复，只剩 inner 引用对象泄漏，但后续顺序仍写不健全性＋泄漏，原样保留而不推断调和。Class4 大部归入 Class1，但 vp3 诊断仍待确认。无回归风险、不确定性消失、不能回归均为原设计主张，不是本译文保证。有限差分/golden 测试与 guard 不确立普遍正确性。workflow ID 和 memory 名是历史引用，不是此处访问核查的证据；未执行引用 copy/build 指令。


<a id="historical-sweep-not-a-current-gap-census"></a>

## 历史扫描，不是当前缺口普查

下文 PR #337–344 扫描与五类计划描述早期 closure 实现。240文件 corpus 和剩余发现不是本次重新复现的当前普查。已完成的 [closure 架构记录](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-architecture-v2.md) 与当前 [module-global storage pass](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-codegen/src/pass_top_let_storage.rs) 是后续背景；v0 `emit_wasm` 位置指已退役代码。

期望的完整性仍是目标。有限扫描通过或安装 guard 不证明每个 closure 程序正确；本次更新不关闭全部后续 closure/host 形状，也不重新认证旧结果。

**目标**：让 native（Rust）与 WASM 对*每个* closure 程序一致；不是靠更多敌对扫描通过（采样不能证明不存在），而是**通过构造消除 bug 类别**，添加**永久 CI 差分 gate**，保持表面闭合。

<a id="how-we-got-here"></a>

## 走到这里的经过

进行了六次 native 对 WASM 敌对 closure 扫描（生成多样 closure 程序，对比 `almide run` 与 `wasmtime`）。扫描1–3关闭 `() -> Unit` 表面（11项修复，PR #337–340）；扫描4–6打开**带参数 closure** 维度 `(T) -> U`：

| PR | 修复 |
|----|-------|
| #341 | fold ICE（HOF-lambda-param call 结果类型）＋WASM build-gate |
| #342 | field-type 嵌套 Fn → `Rc<dyn Fn>`（递归） |
| #343 | 返回 closure 的 closure（`Box`→`Rc`＋trait-object cast） |
| #344 | closure 值的 `Map`（map.set / `??` / from_list / HOF / variant），5/6 |

**第6次扫描**（权威版本，对已合并的*稳定* binary 执行）仍发现**12个真实偏离**。这正说明逐位置打补丁永不收敛。它们集中为下述 **5类 bug**：两类需要让整个类别不可能发生的结构性修复，三类是针对性修复，CI gate 将其永久固定。

> **扫描 binary 陷阱（教训）**：第4次扫描浪费了，因为 agent 用的是旧 `~/.local/bin` binary，而非刚构建的版本。始终 `cp target/release/almide ~/.local/bin/almide`，并让扫描手册在生成前 assert `--version`。扫描*期间*绝不 rebuild（版本变化会混淆结果）。

---

<a id="the-5-classes--the-gate"></a>

## 5类与 gate

<a id="class-1--boxing--type-unification-e0308-two-closures-cant-unify-e0562-impl-trait-in-field"></a>

### Class 1 — boxing / 类型统一（E0308“两 closure 无法统一”，E0562“field 中 impl Trait”）
**症状**：存储的 closure 保持具体匿名类型，第二个 closure（`??` fallback、不同 `map.set`、variant payload、`if`/`match` 分支）不能共享其类型。另有 **a6 回归**（#5 过度 boxing Computed-call 参数 → `Rc<Rc<dyn Fn>>`）及未解决 **sc4**（当前 box-pass 时机仍有未解析 `Ty::Fn` 的 let-bound closure）。

**修复 — Refactor1（通过构造）：`FnBoxingPass`＋一个不变量。**
> **INV-BOX**：closure 值（`Ty::Fn`）在*每个存储/统一 slot*中都是 `Rc<dyn Fn>`；raw `move ||` *只*允许在封闭枚举的三处：(a) 此处直接调用，(b) 按值传给 `impl Fn` 参数，(c) `fan.*` 的 Send+Sync 位置。

- 一个 classifier `classify_fn_site(parent, slot) -> {Boxed, RawCall, RawImplFn, RawSendSync}`。`Boxed` 为默认，三个 `Raw` 是唯一例外。把 `pass_rust_lowering.rs`/`walker/` 中约14条逐位置 IR 规则＋5条 walker 规则收为一个判断。
- **新 pass `FnBoxingPass`** 重新定位：在 `ConcretizeTypes` **之后**（所有 `Ty::Fn` 最终确定，消除 sc4 旧类型故障），`CloneInsertion` **之前**（boxed closure 的 clone 降为廉价 `Rc::clone`）。重排才是实质变化。
- 类型 renderer：合并 `render_type`（impl Fn）/`render_type_field_fn`/`render_type_rc_fn`，令“storage ⇒ `Rc<dyn Fn>`”成为一条规则；`let` 绑定 closure render 为具体 `Rc<dyn Fn>` 而非 `_`，消除 call-site 歧义。按构造修复 sc4＋a6：INV-BOX 隐含“尚未 box 才 box”，因为已有 `Rc` 的值在 `Boxed` site，不再包裹。
- **最大风险**：`Raw` 集合必须穷尽；direct-call/HOF/fan 处误判 `Boxed` 会在 Rust 要求 `impl Fn` 时提供 `Rc<dyn Fn>`（E0277/E0631），误判 `Raw` 则重引 E0308。测试：#337–344 全回归测试＋spec 240×2 target＋精选扫描 corpus（gate）。
- **涵盖**：全部 `box_node`/`box_fn_in_value` 规则、sc4、a6。**工作量 L**。

<a id="class-2--wasm-mutable-capture-cell-residual-leak-unsoundness-already-fixed"></a>

### Class 2 — WASM mutable-capture cell：残余 LEAK（不健全性已修复）
**状态（代码验证后纠正）**：**不健全性/trap 已修复** — closure-v2 **P6**。mutable-capture cell 的 `RcDec{var}`（`emit_wasm/statements.rs:342-358`）如今是**对 cell 做普通 `rc_dec`**（line352），明确不是 `emit_typed_rc_dec(inner_ty, cell_ptr)`。注释准确说明原因：typed dec 把 cell ptr 当对象遍历，`cell[0]` 当元素数量，element-drop loop 对垃圾 decref，导致 `List[String]` trap、`List[Int]` 存活。
**只剩 LEAK**：普通 `rc_dec` 释放 *cell 自身* allocation，却不 drop `cell[0]` 的**内部 heap 引用对象**；因此捕获的 `List[String]`/`Map`/record 在 cell 死亡时泄漏。仅 WASM（Rust 使用 `RcCow`/`thread_local RefCell<Rc<T>>` → RAII）。

**修复 — Refactor2（通过构造）：带类型 cell destructor，关闭泄漏。**
- 把 line352 普通 `rc_dec` 替换为 `emit_typed_cell_rc_dec(inner_ty, cell_ptr)`：减 cell RC，**达到0时**，若 `is_heap_type(inner_ty)`，load `inner = cell[0]` 并调用*现有* `emit_typed_rc_dec(inner_ty, inner)`。**复用，不重写**：同一递归 child-dropper，这次正确以 inner ptr 为根；之后释放 cell。Copy inner 类型无需 inner free，当前普通 `rc_dec` 对它们已正确。
- 按构造正确：capture 对 **cell** 做普通 `rc_inc`（inner RC 不动，cell 拥有一次），所以 `rc(cell)==0` gate 下 inner 恰好释放一次。
- **Lean 注记（MAP-DRIFT A）**：`ClosureRc.lean` 不存在，belt 是 `Basic/FnBody/Heap.lean`。“cell drop 恰好释放 inner 一次（inc==dec，无泄漏）”是全新证明，不是扩展；可选，但构成 memory model 的机器检查完整性。
- **工作量 M**（修复小，复用已测代码）；**若包含 Lean 证明则＋L**。

<a id="class-3--in-place-rewrite-on-a-non-owned-lvalue-silent-wrong---re-diagnosed"></a>

### Class 3 — 对非拥有 lvalue 作原地重写（静默错误）🔴（重新诊断）
**不是 closure bug**。`var g: List[Int] = []; g = g + [n]` 在 Rust/WASM **两者**都打印 `len = 0`，干净地编译运行；**完全没有 closure**也复现（`main` 顶层 `g = g + [1]`）。只有 `g = g + [single_literal]` 受影响，`g = [1,2,3]` 和 `g = g + other_list` 正常。扫描的 closure 叙事是误导，只让 global mutation 更显眼。

**根因**：`try_rewrite_push`（`pass_rust_lowering.rs:497`）**只看 lvalue 的 aliasing，不看 storage class**，把 `xs = xs + [v]` 改为 `xs.push(v)`（Method-call）。它已经为 shared closure cell 退出：line500 `if shared.contains(&var) { return None; }`，注释是“改为 `xs.push(v)` 会 push 到被丢弃的 clone”。但 module-level global 不在 `shared` 集合，所以 peephole 生效。method object 经 walker `expressions.rs`（只特判 `is_rc_cow`，不特判 `ModuleRc`）render 成丢弃临时值上的只读 clone `G.with(|c| (**c.borrow()).clone()).push(v)`，write-back 丢失。WASM 的 heap-global `Assign` 路径（`emit_wasm/statements.rs`）还有**独立的同类 bug**及吞掉它的静默 `drop` fallthrough。*根因经代码核查；“两 target、无需 closure”的复现范围来自设计 workflow `wf23aws8j` 实测。*

**修复 — guard 已存在，只是未覆盖 globals**：
- **主修复（S）**：扩展 line500 现有 `shared.contains(&var)` bail，当 `var` 为 module-global / `ModuleRc` lvalue 时也返回 `None`（把 module-origin 集合与 `shared` 一起传入），使原 `Assign` 走正确 `ModuleRc` write-back 路径。几行而非新机制，bail 模式就在原处。
- **防御（M）**：扩展 walker mutating-method write-back（`expressions.rs:961-980`）以处理 `VarStorage::ModuleRc`（仿照1206的 `is_rc_cow` arm），为所有 method-call source 关闭类别。修复 WASM heap-global `global_set` write-back，**将两处静默 `drop` fallthrough 替换为 hard `unreachable`＋emit 时诊断**（silent-discard 是放大器）。
- **通过构造**：加入供每种原地重写（push/index-assign/swap/reverse）使用的 `LvalueClass {OwnedLocal, ModuleCell, ModuleRc, RcCow}` query，防止未来 peephole 静默指向非拥有 lvalue。**工作量 M**。针对项中最高优先级：silent-wrong 且是**一般正确性 bug**，并非 closure 专用。

<a id="class-4--wasm-closure-env-width--mostly-collapses--not-a-separate-bug"></a>

### Class 4 — WASM closure-env 宽度（大部归并，不是独立 bug）
“捕获 Int 以 i32 存储／env_ptr 垃圾”假设已**被否证**：closure 值是指向 heap `[table_idx:i32][env_ptr:i32]` pair 的单个 i32 指针；env store（`equality.rs:561`）与 load（`expressions.rs:141-157`）为对称 i32，正确。vp3 扫描故障是 **curried-closure non-Clone**，即 **Class1**（Refactor1）范围，不是 env-width bug。
仅剩潜在不一致：`byte_size(Ty::Fn)=4`（`values.rs:49`）对硬编码8-byte env stride，目前偶然正确。**修复（S，强化）**：给 `Ty::Fn` 显式 `byte_size`/`ty_to_valtype` arm，并 `debug_assert` env stride ≥ `byte_size(Fn)`，让未来 closure 表示变化触发测试而不是破坏 layout。*尚有一点待协调：扫描见 vp3 native-OK/WASM-validator-fail，设计却归入 Class1 boxing；Refactor1 落地后须确认 vp3 完全吸收。*

<a id="class-5--enum-derive-through-closure-bearing-types-vp5--confirmed"></a>

### Class 5 — 经含 closure 类型的 enum derive（vp5）（已确认）
**症状**：`type Tree = Leaf | Node(Lazy[Int])` 中 `Lazy` 有 `() -> T` field，`Tree` 获得 `#[derive(Clone, Debug, PartialEq)]`，但 `Lazy<i64>` 既非 `Debug` 也非 `PartialEq`，导致 E0277＋E0369。
**根因**：derive 选择不对称。`render_type_decl` 为 record 计算 `has_fn_fields`/`has_non_eq_fields`（`declarations.rs:30,34`），但 **enum 分支（66-124）两者皆不计算**（仅 `has_hash`）；`enum_decl`（`rust.toml:317-326`）也没有 Clone-only / no-PartialEq variant。传递 `eq_blocked_types` 不动点（`compute_eq_blocked_types`，declarations.rs:373）**已正确处理 enum**，只是 consumer 不读取。
**修复（S/M）**：enum 分支仿照 record 计算 `has_fn_fields`/`has_non_eq_fields` 并传入 `enum_attrs`；为 `enum_decl` 添加 `when_attr="has_fn_fields"`（Clone-only）及 `"has_non_eq_fields"`（Clone+Debug）variant。**通过构造**：提取 struct/enum 共用的 `select_derives(field_tys,
eq_blocked, has_hash, repr_c) -> attrs`，使其不再漂移。没有回归风险（传递含 `Fn` 却依赖 `PartialEq` 的 enum 目前就不健全，根本无法编译）。

<a id="the-gate--extend-the-existing-cross-target-ci--already-partly-there"></a>

### Gate — 扩展现有跨 target CI（已部分存在）
`.github/workflows/ci-cross-target.yml` 已存在，但只检查 pass/fail 偏离，不检查输出正确性；`wasm_cross_target_spec` 测试对 `spec/wasm_cross/*` 做 native-vs-wasm diff。
**修复（S/M）**：把全扫描 corpus（`/tmp/sweep4_*`＋`/tmp/cbox/*`）放入 `spec/wasm_cross/` 或 `spec/closure_xt/`，带 **golden 期望输出**；Class3 说明跨 target *相等*不够，两者可能同样错误。扩展 `ci-cross-target.yml`，任何 `native != wasm` 或 `!= golden` 都失败。从“打过补丁”变为“不能回归”。

---

> **设计阶段细化（两个设计 workflow 的改变）**：第6次扫描的“五类”在分析后变小而非变大。**Class3 是一般正确性 bug**（忽略 lvalue storage class 的 peephole，无 closure、两 target 复现），不特属于 closure。**Class4 大多归入 Class1**（closure-env 表示正确，只剩潜在 layout assertion）。**CI gate 已存在**，只需输出正确性 corpus。因此实际工作为 **2个 refactor＋2个小针对修复＋1个 gate 扩展**，不是6个独立未知。

<a id="sequencing"></a>

## 顺序

1. **Class3**（S 主修复／M 完整，silent-wrong）最先：非拥有 lvalue 原地重写是与 closure 独立的一般正确性 bug，静默错误最危险。S peephole storage guard 立即落地，M `LvalueClass`＋移除静默 `drop` 的强化可后续。
2. **Class5**（S/M，独立）：快速解锁 variant/enum closure 形状，共用 `select_derives` 为按构造形式。
3. **Refactor1／Class1**（L，按构造）：主项，吸收 sc4＋a6＋全部 boxing edge，**以及 vp3/Class4 curried-closure 故障**。必须随精选 corpus gate 落地，才可安全删除14规则。
4. **Refactor2／Class2**（M，按构造）：WASM cell 不健全性＋泄漏，独立于1。
5. **Class4 残余**（S，强化）：Refactor1 后确认 vp3 吸收，添加 `byte_size(Ty::Fn)` arm＋env-stride `debug_assert`；不预期现存 bug patch。
6. **CI gate**（S/M）：扩展现有 `ci-cross-target.yml`，添加带 golden 的扫描 corpus，对 `native != wasm` 或 `!= golden` 失败。每类关闭时增量加入 corpus。
7. **可选 Lean 证明**：cell destructor 的机器检查 memory 完整性。

每步验证 spec 240×2 target＋完整 `cargo test`＋closure 回归套件＋精选 corpus diff。每步1PR（既有 root-cause→fix→verify→PR→CI→merge 循环）。

<a id="definition-of-done-completeness"></a>

## DONE 定义（完整性）
closure native↔WASM 表面**完整**的条件：(a) Class1/2 按构造消除（INV-BOX＋typed cell destructor，无逐位置规则、无泄漏、无不健全性）；(b) Class3/5 已修复覆盖，Class4 缩为通过的 layout assertion；(c) 扩展 `ci-cross-target.yml` 每次构建跑完整 golden 扫描 corpus，对 `native != wasm` 或 `!= golden` 失败；(d) 新敌对扫描返回**0个真实偏离**。工作量仍**远**（2大 refactor＋2小修复＋gate，多次专注工作），但**不确定性消失**：已是已知有限清单，不再是新 edge 的分形。

> Refactor1＋2 的 live 设计详情：workflow `wj7yrlb5h`。Class3/4/5＋gate：`wf23aws8j`。
> 各形式 boxing 注记与 sc4 edge 见 memory `project_closure_parametered_dimension.md`。
