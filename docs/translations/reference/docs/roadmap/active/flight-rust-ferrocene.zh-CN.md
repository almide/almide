原文 description 元数据：native renderer 与拟议 Ferrocene 资格化路径；抽象 RC 引理不证明 Almide-to-Rust 翻译
<a id="flight-keystone-い--v1-mir--rust--ferrocene忠実性束縛"></a>

# 飞行基石（い）— v1 MIR → Rust → Ferrocene（忠实性绑定）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](flight-rust-ferrocene.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](flight-rust-ferrocene.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](flight-rust-ferrocene.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-rust-ferrocene.md)

> 原文注记：这是历史提案，不证明当前完整翻译忠实性、实际 Ferrocene 使用或监管受理。既有 native rendering、抽象 RC 安全、生成代码对应、下游资格化、物理 target 行为是不同义务。所列定理不涉及 rust_pattern 或 Rust 执行，文本存在/计数检查不能封闭接缝。旧工量比例、可读性/资格化主张是设计判断记录，不是经验证标准指导。将线性内存 self-host 本体 render 的旧建议与 native-trust-spine 拒绝该路线、采用 native shim 的方案相对；保留为历史，不作当前实现指令。clone/唯一性、scope drop、trap、ABI 主张需实际类型/runtime/表示前提；repr-c 本身不证明完整 ABI 或安全。未执行命令、代码示例、证明检查、toolchain 或测量。

> **目标**：[flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) 阶梯 gate **G-F3** 的具体实现设计 — **考虑 Rust → Ferrocene 的方案**。当前 native renderer 存在于 `crates/almide-mir/src/render_native.rs`，但其存在与完整 Almide→Rust 翻译忠实性证明是两回事。`.github/workflows/ferrocene-lane.yml` 明示在 Ferrocene 合同前使用 stock rustc，不是使用已资格化编译器的运行记录。后阶段资格化不会自动证明前阶段语义保持，也不可根据75%复用等比例断定证明工作量。
> **相关**：[flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §4 / [v1-mir-architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md) §1、§9 / [certificate-format-v1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certificate-format-v1.md)。

---

<a id="1-設計核一行"></a>

## 1. 设计核心（一行）

> **设计目标**：整理连接 native renderer 输出与显式语义的依据。现有 RC 事件序列定理可作为复用候选，但仅凭字符串模式表和 `exact eager_copy_refines_safety` 无法证明实际 Rust 执行忠实。明示目标语法、runtime、ABI、未证明边界，并与下游 toolchain 证据分开。

---

<a id="2-既存証拠の分割再利用可能性と未証明の接続"></a>

## 2. 拆分现有证据（复用可能性与未证明连接）

| 分类 | 文件 | 对飞行的含义 |
|---|---|---|
| **target 无关（原样复用）** | `OwnershipChecker.v`（`check_sound` 论证 **fold 的 `Z` 结果**，`:34-36`）/ 证书 i/a/d/m/r/b（= MIR ownership 事实）/ `certificate.rs`（零 target 认知）/ `RuntimeModel.v` / `ALS.v` / `NameTotality` / `CapabilityBound` / `TypeConcretization` / `Subset` / `Termination` / `FreeList` / `CowSafety` | 抽象事件模型定理可复用；与 Rust 执行连接是另一个义务 |
| **wasm 专用（唯一 target 层）** | `Translation.v:28-35`（`wasm_pattern: Op→string`）+ `WasmEncode`/`WasmExec`/`WasmRcDec` + V validator | `rust_pattern` 是语法匹配候选，语义保持证明深度未评估 |

`Translation.v` 的 eager instance **已证明**（`eager_translation_refines_safety
:= exact eager_copy_refines_safety`，`Translation.v:52-54` ← `ALS.v:62`）。

---

<a id="3-当初の-renderer-実装案a"></a>

## 3. 原始 renderer 实现方案（§A）

以下 demo/行号是原设计记录，不表示当前 renderer 尚未开始。实现入口为 `crates/almide-mir/src/render_native.rs` 和 `src/cli/emit.rs`。

**结构（镜像 `render_wasm.rs`）**：`render_rust_program(prog) -> String` 驱动 `render_rust_fn` + `rust_preamble()`（对应 `render_wasm.rs:648` 的 `preamble()`）+ self-host runtime 链接（与 `render_wasm.rs:591-644` 的 `self_host_runtime()` 同形）。CLI 入口是 `examples/render_program.rs` 的兄弟（当前在 :16/126 写死 `render_wasm_program`）。

**从 demo `render_rust.rs` 复用（保留）**：
- op→Rust 对应骨架 + §3.2 契约注释（`render_rust.rs:1-21`）= **预期映射、设计规范锚点**。
- `render_op` dispatch（`:120-198`）、`IntBinOp` lowering（`:172-189`，已有惯用 `a + b` / `(a < b) as i64`）、`CallFn`/`Call` 分支（`:190-225`）、`render_rust_fn`（`:32-56`，含 tail-return-by-move）。
- `value_reprs` 推断（`:85-112`）需 heap-result 扩展（读取 `CallFn.result` repr，`render_wasm.rs:339-341` 已具备）。

**生产级需重建（review-grade 障碍）**：
1. **`rust_ty`（`:75-81`）对全 heap 用 `Vec<i64>`**，最大障碍。依据 `Repr::Ptr{layout}` / layout registry 输出真实类型（`String`/`Vec<T>`/`Box<Node>`/named struct）。一律 `Vec<i64>` 无法通过 DO-178C“源码可读”。
2. **`var`（`:114-116`）为 `v{id}`**，`v0`/`v1` 编号不可 review。`ValueId(u32)`（`lib.rs:114`）无名称；**lowering 层通过 side-table 传递 `debug_name: Option<Sym>`**（IR `VarId` 有 source 名）。这是 **lowering 变更**，不能只改 renderer。
3. **控制流**：demo 把 `If/Loop` marker 设为 `None`（`:154-155`），错误。由 flat marker 序列**重建惯用 `if c { } else { }` / `while c { }`**（镜像 `render_wasm.rs:212-260` 的 `if_stack`/`loop_stack` 状态机）。**最大净新增代码**。
4. **heap 初始化器**：`Init::Str`→真实 `String`，`DynStr`→owned buffer，`OptSome`→`Some(x)`。
5. **struct/enum 声明**：demo 全无。以 `walker/declarations.rs:58-62`（`#[derive(Clone)] pub struct`）、`:135`（`pub enum`）为目标，从 `LayoutId` registry 输出 decl pass。

---

<a id="4-rust_pattern-表--各-op--イディオマティック-rustb"></a>

## 4. `rust_pattern` 表 — 各 Op → 惯用 Rust（§B）

把 §3.2 契约（`render_rust.rs:6-13`）4个锚点（`Dup→.clone()` / `Drop→scope-end` / `Consume→move` / `MakeUnique→no-op`）扩展到全 op 集（`lib.rs:152-257`）。`dst`/`src` 为**已解析真实名称**，不是 `v{id}`；类型为 `Repr`/`LayoutId` 的真实类型：

| Op | 惯用 Rust | 注 |
|---|---|---|
| `Alloc{Init::IntList}` | `let mut dst: Vec<T> = vec![e0,e1,…];` | 真实元素类型 |
| `Alloc{Init::Str(s)}` | `let dst: String = "s".to_string();` | 替换 `vec![bytes]` |
| `Alloc{Init::DynStr}` | `let mut dst = String::with_capacity(len);` | |
| `Alloc{Init::OptSome{p}}` | `let dst = Some(p);` | |
| `Alloc{Init::Opaque}` | `let dst = Name { … };` | 从 layout 得 ctor |
| `Dup{dst,src}` | `let dst = src.clone();` | §3.2。**唯一 ownership 决策 = eager COW** |
| `Drop{v}` | *（不输出）* RAII scope-end | §3.2。eager 安全来源（同 `Translation.v:32` 的 `Dec⇒""`） |
| `Consume{v}` | *（不输出）* 在 use 点 move | §3.2 |
| `Borrow{v}` | `&v` | |
| `MakeUnique{v}` | *（no-op）* `Dup` 的 `.clone()` 已令其 unique | §3.2 |
| `ConstInt{dst,value}` | `let dst: i64 = value;` | |
| `IntBinOp{Add/Sub/Mul}` | `let dst = a + b;` | demo `:172` |
| `IntBinOp{Div/Mod}` | `almide_div!(a,b)` / `almide_mod!(a,b)` | **trap-parity**（下文） |
| `IntBinOp{Lt/…/Ne}` | `let dst: i64 = (a < b) as i64;` | demo `:181` |
| `IfThen/Else/EndIf` | `let dst = if cond != 0 { then } else { els };` | 净新增，镜像 wasm `:232` |
| `LoopStart/BreakUnless/End` | `while cond != 0 { … }` | 净新增，镜像 wasm `:214` |
| `SetLocal{local,src}` | `local = src;` | |
| `CallFn{dst,name,args}` | `let dst = name(args);` | |
| `Call{RtFn::*}` | 调用 self-host Rust fn（§5） | |
| `Prim{kind,…}` | 信任底座 → Rust stdlib（§5） | 净新增 |
| `Pure{dst,uses}` | `let dst = a.len();` 等 | demo 暂代为 `0` |

**trap-parity（难点）**：`Div/Mod` 在 MIR/wasm 对除零 trap（`render_wasm.rs:457` `i64.div_s`）。裸 Rust `a/b` 为 panic，造成观测差。为 oracle 字节一致，走 `almide_div!`/`almide_mod!`（`rust.toml:36-40`），已有 total-checked `Error:\n` + exit-1 形态，review-grade。

**可读性策略**（使输出与 v0 不可区分）：相等 `almide_eq!`（`rust.toml:63`）、struct/enum `#[derive(Clone)]`（`declarations.rs:58`）、concat `format!`（`rust.toml:53`）、runtime 调用 `almide_rt_*`（`rust.toml:28`）、真实名称+真实类型。**试金石：经 `cargo fmt` 的输出通过人工 review，`cargo build` 零警告**（CLAUDE.md codegen 规则）。

---

<a id="5-runtime--mirrust-で-dogfoodstdlib-にマップしないc"></a>

## 5. runtime — MIR→Rust 自用验证，不映射到 stdlib（§C）

**建议：self-host runtime 经同一 MIR→Rust 路径 render（dogfood），仅 prim 底座映射 Rust stdlib。**

依据（[v1-mir-architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md) §4、§4.1）：runtime **用 Almide 编写，经同一 Core→MIR→target**。把 list/string/print 映射到 Rust `std`，会**重新引入 v1 为消除而存在的双实现漂移（约~136例程双重维护）**。runtime 已用 Almide 自托管、自动链接（`render_wasm.rs:591-644` lower `stdlib/*.almd`；`print_str.almd` 是 prim 底座上的 dogfood `println`）。Rust renderer 复用**同一 registry**：同 `.almd`、同 `lower_function`，只替换为 `render_rust_fn`。**仅 prim 底座**（`PrimKind`、`lib.rs:262-273`）映射 Rust primitive，这是各 target 特有的信任底座（`§4.1`）。`print_str` 本体降至 dogfood Rust，`prim.fd_write` 到达 `io::stdout().write_all`。

**但 prim 底座本身按 target 分支**（§7 难点3）：Rust 无共享线性内存，所以 `prim.handle(s)`（假定线性内存地址）不成立。**prim 底座以上全 dogfood，底座本身各 target 分支**（设计信任边界，仅此允许“不 dogfood”）。

---

<a id="6-忠実性証明d"></a>

## 6. 忠实性证明（§D）

<a id="61-抽象-eager-事象モデルの再利用例rust-翻訳証明ではない"></a>

### 6.1 抽象 eager 事件模型复用例（不是 Rust 翻译证明）

新增 `rust_pattern : Op -> string`（与 `wasm_pattern` `:28-35` 结构同构）：

```coq
Definition rust_pattern (o : Op) : string :=
  match o with
  | Inc | Alias => ".clone()"   (* 新規取得 / alias = clone *)
  | Dec | MoveOut | Reuse => "" (* eager: scope-end drop / move、明示 op 無し *)
  end.

Theorem eager_rust_translation_refines_safety :
  forall ops, increments_only ops -> no_double_free ops.
Proof. exact eager_copy_refines_safety. Qed.
```

保留 Coq 注释的译文：新获取 / alias = clone；eager scope-end drop / move，无显式 op。定理原文完整保留，不涉及 Rust 执行。

`Dec/MoveOut/Reuse` 为空字符串，**原因与 wasm 相同**（RAII scope-end 不发显式指令，eager wasm 也不发）。release-empty 引理（`is_release o → rust_pattern o = ""`）也原样迁移。

- **现有定理范围**：`eager_copy_refines_safety` 表明抽象事件序列性质。上述结论不出现 `rust_pattern` 或 Rust 执行语义，所以不能推出 Rust 输出正确。
- **另需依据**：实际生成代码、runtime、ABI 实现该模型的对应关系。不可仅凭字符串匹配或 `Print Assumptions` 宣称封闭此连接。
- **剩余连接义务**：需要 Rust 片段实现抽象 op 的依据。仅在下游采用 Ferrocene，不能省略上游生成 Rust 与模型对应。不指定某一形式化文件为必需，而按主张范围确定证据。

<a id="62-per-build-v-validatortranslation_validationrs-の-rust_pattern-兄弟"></a>

### 6.2 每次构建 V validator（`translation_validation.rs` 的 `rust_pattern` 兄弟）

镜像 `wasm_pattern`（`:49-98`）+ `validate_translation_perceus`（`:122-126`）：`rust_pattern(op)->Option<String>` + `validate_translation_rust(rust_src, mir)`。逐 op 扫描**输出 Rust source**：

| Op | wasm V（`:49-97`） | Rust V |
|---|---|---|
| `Dup`/`Alias` | `call $rc_inc` | `.clone()` |
| `Drop` | `call $rc_dec` | *（eager：无 = leak-count 检查）* / *（perceus：scope-end `}` 或 `drop(x)`）* |
| `Alloc{IntList}` | `call $list_new` | `vec!` / `Vec::` |
| `CallFn{name}` | `call $name` | `name(` |
| `IntBinOp{Add}` | `i64.add` | ` + ` |
| `Call{PrintInt}` | `call $print_int` | `print_int(` |
| `Consume` | None | None（move） |

leak-count 检查（`:122-126` `rc_decs >= drop_count`）原样迁移。非空虚测试（`:185` 剥掉 pattern 就使 V 失败）也直接移植。为 `validate_safety` 添兄弟 `validate_safety_rust`。

---

<a id="7-信頼基底の台帳変更--9-和解e"></a>

## 7. 信任基账本变更 + §9 调和（§E）

**纳入信任基（新增信任、未证明）**：
- **Ferrocene / 资格化 `rustc`** 负责 Rust source→机器码（ISO 26262 ASIL D / IEC 61508）。**已资格化**（不是由我们证明）= SCADE KCG 类比（`certification-grade.md:135`）。
- **Rust prim 底座**（`PrimKind`→Rust stdlib `io::Write`/ptr）= 小型封闭信任表面，wasm prim 底座的 Rust 对应物。

**未证明边界**：完整 Almide→Rust 翻译忠实性不由抽象 RC 定理或 `rust_pattern` 语法匹配确立。Native 路径验证对象不同于 Wasm 字节绑定，但与生成 Rust 对应的义务仍在。采用 Ferrocene 资格化不消除前阶段义务。

**§9 原始设计目标**（以下“proof-bound renderer”不是已达成事实）：保留以 MIR 模型为基准建立与生成 Rust 对应的意图。

**原设计调和案**（尊重 MIR 主权）：`v1-mir-architecture.md §9` 拒绝的是“**Rust 作为唯一真相**”（建在不存在的 Rust 形式语义上）。本设计**不这样做**：不建于 Rust 语义，而绑定经证书检查的 **MIR 事实**，Rust 仍只是 renderer，**MIR 仍是唯一真相**。唯一改变是把 Rust renderer 从“几乎自明降下的踏板”（§1）升级为“**证明绑定的第二个生产 target**”。§9 拒绝的“Rust = 真相”不同于此处“Rust = proof-bound renderer”。主权（wasm canonical）保留，Rust 是相同 witness 的第二消费者。

---

<a id="8-ferrocene-実証経路--難所f"></a>

## 8. Ferrocene 实证路径 + 难点（§F）

<a id="de-risking-順最小の端から端スライス先"></a>

### 降低风险顺序（最小端到端切片先）

**Slice 0 — `add` 程序（两 target 已有）**。`render_rust.rs:245-286` 的 `add_program`（`fn add(a,b)=a+b` + `main` 打印5）已通过 `rustc` 编译并打印 `5`（`:278-286`）。所需 op：`IntBinOp{Add}`/`CallFn`/`Call{PrintInt}`/`ConstInt`。**先把 `rustc` 换成 Ferrocene `rustc`，检查字节一致**。无新 renderer 代码，纯 toolchain 接线。

**Slice 1 — 值语义列表（ownership 基石）**。`render_rust.rs:313-342`（`var a=[1,2,3]; var b=a; a[0]=9; print a,b`）= `Alloc{IntList}`/`Dup→.clone()`/`MakeUnique→no-op`/`Drop`/`Call{ListSet,PrintList}`。已输出 `a=9,2,3 / b=1,2,3`。**实证 §3.2 契约**（1 `Dup` → 1 `.clone()` → 值语义按构造正确）。添加 V 检查。

**Slice 2 — heap 类型 + 控制流**。将 `Vec<i64>` 换为真实 `String`/`Vec<T>`（重建 `rust_ty`），传真实名（lowering side-table），添 `if`/`while` marker 重建。首个程序 = 列表求和 counted loop（`Loop*`/`SetLocal`/`IntBinOp`），**与基石（あ）的 counted-loop 子集相交**（[flight-wcet-loops](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md)），2个基石共用 fixture。

**首个可信 Ferrocene demo 所需子集**：`Alloc{IntList,Str}`/`Dup`/`Drop`/`Consume`/`MakeUnique`/`ConstInt`/`IntBinOp`（全部）/`CallFn`/`Call{PrintInt,PrintList,PrintStr}`/`SetLocal`/`If*`/`Loop*` + `print_str` prim 底座，正是 [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §7 飞行子集。G-F4 在其上让真实安全关键模块（控制律/状态机/watchdog）端到端贯通。

<a id="正直な難所"></a>

### 诚实难点

1. **disjoint-paths（最深）**：Rust 有2路 — demo `render_rust.rs`（有 op↔Rust 对应，但 `Vec<i64>`、非 review-grade、非 CLI）和 `walker/`+`rust.toml`（review-grade 但**无 op 对应对象**、非 v1-MIR 驱动）。G-F3 不能直接用 walker（无待验证 op↔fragment 绑定），也不能交付 demo（非 review-grade）。**构建第三种东西**：输出 walker 级 Rust、同时保留 demo per-op 对应的生产 v1-MIR→Rust renderer。`flight-profile.md:144` 诚实估量为“render_wasm.rs 全语言版、约5x”。**真正成本是工程，不是证明。**
2. **变量命名 = lowering 改动**：`ValueId(u32)` 无名称，review-grade 需真实名；IR→MIR lowering 传 `debug_name: Option<Sym>` side-table。触及 MIR 数据模型，会波及两个 renderer + verifier，做成 wasm 可忽略的 `Option`。
3. **runtime rendering 暴露 prim 底座在 Rust 上的缺口**：`print_str.almd` 用 `prim.handle/load32/store32/fd_write`（线性内存 raw 地址）编写。Rust 无共享线性内存模型。prim 底座要么 (a) 模拟 byte-addressed arena（忠实但丑，损害可读性），要么 (b) 在 `RtFn::PrintStr` 边界把 `print_str` 映到 Rust native `String` write。**建议：prim 底座以上全 dogfood，底座本身按 target 分支**（只在设计信任边界允许不 dogfood）。
4. **Repr/ABI 确定性**：真实 `String`/`Vec<T>`/struct 把 layout 委托 rustc，足以支持**观测**等价（stdout 字节），却非 **byte-layout** 相同。契约为观测等价（CLAUDE.md Behavior Contracts），因此可接受；`almide_div!`/`almide_mod!` 处理唯一 trap 差异。未来若主张 FFI/ABI 保证，`--repr-c`（`rust.toml:56`）有用。

**净结论**：可基于已有 native renderer 和抽象模型证据，但完整翻译忠实性、实际使用资格化 toolchain、当局受理是分别尚未确立的事项。不要以旧 ~5x/~75% 估计或“一张表就自明”作为当前工量、证明完成依据。
