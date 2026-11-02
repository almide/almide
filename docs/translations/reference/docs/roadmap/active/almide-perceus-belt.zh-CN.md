原文 description 元数据：历史 Perceus belt 设计、已退役的生成阶段类型状态，以及有范围限制的证明证据
<a id="almideperceusbelt"></a>

# AlmidePerceusBelt

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](almide-perceus-belt.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](almide-perceus-belt.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](almide-perceus-belt.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/almide-perceus-belt.md)

> 原文注记：开头的退役纠正限制了后文历史绝对安全标语与 Phase B 保证的范围。已退役类型状态边界及旧逃生口不是当前 CLI 指令。零违规仅指当时 394 个文件的语料库，不涵盖所有程序。纠正后的事件是误报，不是缺少 Dec；添加 Dec 会二次释放。旧未来 Phase A 计划与后续证明工作并存。Lean 所有权证明和 Coq runtime 模型范围不同；renderer store 与记录假设仍是信任边界。未重新执行命令或证明。


<a id="historical-type-state-design-and-present-proof-scope"></a>

## 历史类型状态设计与当前证明范围

下面 Phase B 的 `Verified`/`PerceusVerify` 生成边界属于已退役的 v0 emitter（[退役记录](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-codegen/src/lib.rs)）。它不是当前结构化 emitter 的入口条件，旧 `--emit-unverified` 指令也不是当前 CLI 指南。

检查器确立的是其所接受见证上的建模性质；它不证明所有编译器缺陷都不可能随产品交付。Lean belt 对所有权变换建模，后续 Rocq runtime 与证书工作各有自己的覆盖范围和记录假设。当前边界见[内存安全](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/MEMORY-SAFETY.md)。下面的事件纠正和后面的 Coq 附录是历史证据，不表示旧 Phase A 计划至今仍全部未实现。

> Almide 中对应 RustBelt 的机制。由类型引导、由编译器证明的内存安全。

<a id="architecture"></a>

## 架构

```
Phase B (Rust type-state): 検証を通らないと emit できない    ← 今ここ
Phase A (Lean 4 proof):    検証ロジック自体が数学的に正しい  ← next
A + B = 完全体
```

图示译文：Phase B（Rust 类型状态）：不通过验证就不能生成 ← 当时所在阶段。Phase A（Lean 4 证明）：验证逻辑自身在数学上正确 ← 下一阶段。A + B = 完整体。

<a id="phase-b-rust-type-state-enforcement-current-session"></a>

## Phase B：Rust 类型状态强制（当时的会话）

使用 Rust 类型系统强制“未经验证的 IR 不能生成”。

```rust
struct Verified<T>(T);  // 検証済みマーカー

impl PerceusPass {
    fn run(fb: FnBody) -> Verified<FnBody> { ... }
}

impl WasmEmitter {
    fn emit(fb: Verified<FnBody>) -> Vec<u8> { ... }  // Verified のみ受付
}
```

代码注释译文：已验证标记；只接受 Verified。

<a id="status-hard-error-化済みb5-完了-2026-06-07"></a>

### 状态：已改为硬错误（B.5 完成，2026-06-07）

PerceusVerifyPass（perceus-belt）在 **spec 语料库上为 0 次违规**（将 394 个文件生成 wasm 后测量）。violation > 0 是**硬错误**（`almide build --target wasm` 以 `error: [COMPILER BUG] Perceus RC verification failed` 拒绝）。
- 全局：所有 heap var 都有 Dec（无泄漏）
- 全局：dec_count <= inc_count + 1（无 double-free）
- 逐分支：所有 if/else/match arm 的 Dec 一致性
- 排除项（仅限泄漏判定，均表示“所有权离开此 block”= Perceus 的 “moved” 关系）：
  - `EnvLoad` 绑定（从闭包环境借用 = 非拥有）
  - **`returned_vars`**（函数 return 尾部 = 所有权逃出函数）
  - **`moved_out_vars`**（`Block{expr:Some(Var x)}` 中的 `x` = 裸 Var block 尾部。所有权转到 block 的消费方，由该方持有 Dec。结构化排除 ANF 产生的 `__perceus_ret`/`__anf_*` 尾部临时值的误报 — 用 `IrVisitor` 穷尽遍历收集，并非名称前缀补丁）
  - `__tco_*` / `__br_*` 名称前缀（TCO/branch 临时值。Dec 确实存在，但和 Bind 在不同 block 中；扁平的逐 block `count_decs` 在本地读到 0，造成误报。后续需替换为感知作用域的 Dec 记账）

<a id="what-phase-b-guarantees"></a>

### Phase B 保证什么

“未通过验证的 IR 无法转换为 WASM”
= 防止编译器内部缺陷未经检测就交付

<a id="what-phase-b-does-not-guarantee"></a>

### Phase B 不保证什么

“验证逻辑本身正确”
= 如果验证函数有缺陷，无效 IR 可能作为 Verified 通过

<a id="phase-b5-warn-mode--hard-error2026-06-07-完了"></a>

## Phase B.5：warn-mode → hard-error（2026-06-07 完成）

**实证数据**：v0.25.0 发布时对 `[Perceus] N RC violation(s)`（HOF-closure `__perceus_ret`；复现 = `research/benchmark/stdlib/precise_all.almd` 中的 `bench` fn；最小化 = `f()` 后以 `println("[" + name + "]")` 结尾的 HOF）**仅发出警告就交付**。`Verified::verify`（almide-codegen/src/lib.rs）是有意设置的 warn-mode，即使存在违规也发出 `Verified` — 当时 `Verified` 表示“执行过验证”，而不是“验证通过”。类型文档中的 “unverified programs cannot reach emission”（未验证程序无法进入生成阶段）与实际情况矛盾，也违反了契约 C-041。

**根因纠正（重要）**：这**不是**最初设想的 RcDec **遗漏**。RC 平衡，是**误报**。`__perceus_ret` 是其定义 block 的裸 Var 尾部（`Block{expr:Some(Var ret)}`）= **move-out**；所有权转移给消费方的 `Bind __anf_*`，该方持有 `RcDec __anf_*`（dump_rc_trace 实证：对 `__perceus_ret` 没有 `RcInc`，所以是移动；消费方的 `__anf_1` 确实有 Dec）。此处**添加 Dec 会导致二次释放**。真正原因是 verify 未区分“经函数 return 逃出的 var”（`returned_vars`）与“移出 block 的 var”，把后者误判为泄漏。扁平 `collect_all_tail_vars`（作为尾上下文分析，不深入 `Block{expr:None}` 的 `Expr` 语句）未捕获 `__perceus_ret`，使 `returned_vars` 为空。

**修复**：使用 `collect_moved_out_vars`（`IrVisitor` 穷尽遍历，构造上完备）结构化收集所有 `Block` 的裸 Var 尾部，引入独立集合 `moved_out_vars`，仅豁免泄漏判定。没有简单合并进 `returned_vars`（= 跳过所有检查），而只豁免泄漏，是为了继续检测已移动值上的 Dec = 二次释放。将 5 个参数集中到 `VerifyCtx`。这是**结构化** move-out 识别，不是名称前缀补丁。

**判断：警告的接收对象不对。** 违规是编译器插入 RC 的缺陷，不是用户代码缺陷，用户无法修复。因此诊断类别为 **ICE**。“leaks, not unsoundness”（泄漏而非不健全）讨论的是严重程度，不是开关门禁的理由（严重程度只决定消息语气）。

- [x] `Verified::verify`：violation > 0 时为**硬错误** — `error: [COMPILER BUG] Perceus RC verification failed`，“这是编译器缺陷，请报告”，加 `--emit-unverified` 指引。受控 `process::exit(1)`（与 `assert_types_concretized` 相同方式、相同 `[COMPILER BUG]` 格式；既不是 `panic!` 回溯，也不是数字 E-code — 避免后者是因为容易和 rustc 的 `E060x` 空间混淆）
- [x] 逃生口 `--emit-unverified`（仅 `almide build`）：仅在显式传入标志时继续生成并警告。`run`/`test` 不支持豁免，始终硬错误（不默默执行会泄漏的产物）。**尚未实现向 manifest 记录 `perceus_verified: false`** — 构建 manifest 机制本身尚未完善，因此转交 trust-layer receipts 工作（trust-layer.md）作为后续事项
- [x] 结构化修复 `__perceus_ret` 误报（见上）+ **在 spec/ 添加 HOF-closure fixture**（`spec/wasm_cross/hof_closure_string_tail.almd`，`// @contract: C-041`）。真正空白在语料库 — spec 没有这个模式，导致 CI 仍绿时回归进入发布。fixture 覆盖 HOF/nested/returned-String/map-churn 中的 move-out，保证 native==wasm 字节一致（扩充 C-041 证据）
- [x] 将上方状态的“0 次违规”纠正为“**在 spec 语料库上** 0 次违规”

<a id="follow-upb5-で構造化しきれなかった分明示的トレードオフ"></a>

### 后续事项（B.5 中未完全结构化的部分，显式权衡）

- 将 `__tco_*` / `__br_*` 名称前缀排除替换为**感知作用域的 Dec 记账**。当前它们正确抑制误报（Dec 实际存在，只是与 Bind 不同 block），但依赖名称并不理想。需研究能否用类似 `moved_out_vars` 的结构规则（move-via-`Assign` 流）替代。不过 TCO 的 RC 约定与 ANF move-out 不同（`Assign loopvar = Var tmp` 和 `RcDec tmp` 同时存在），草率合并有漏报风险，应作为独立任务谨慎处理。
- `--emit-unverified` 的 manifest 记录（见上）。

<a id="phase-a-lean-4-formal-proof-future"></a>

## Phase A：Lean 4 形式证明（未来）

在 Lean 4 中机器证明 Perceus 规则正确性，再用 lean4-rust-backend 转为 Rust。

<a id="plan"></a>

### 计划

1. 创建 Lean 4 项目（almide-perceus-belt）
2. 在 Lean 4 中形式化 FnBody、VarId、Ty
3. 在 Lean 4 中定义 6 条 Perceus 规则
4. 证明定理：
   ```
   theorem perceus_sound :
     ∀ (fb : FnBody) (vt : VarTable),
       well_typed fb vt →
       rc_balanced (perceus fb vt)
   ```
5. 用 lean4-rust-backend 转换为 Rust 代码
6. 将生成的 Rust 函数集成进 almide-codegen
7. 与 Phase B 的 Verified&lt;T&gt; 集成

<a id="dependencies"></a>

### 依赖

- lean4-rust-backend: ~/workspace/github.com/O6lvl4/lean4-rust-backend
- Lean 4 工具链：v4.28.0（通过 elan 安装）

<a id="references"></a>

### 参考文献

- Reinking et al., "Perceus: Garbage Free Reference Counting with Reuse" (ICFP 2021)
- Ullrich & de Moura, "Counting Immutable Beans" (IFL 2019)
- Jung et al., "RustBelt: Securing the Foundations of the Rust Programming Language" (POPL 2018)

<a id="exit-criteria"></a>

## 退出标准

- Phase B：WASM emitter 输入必须为 `Verified<FnBody>` 类型
- Phase A：在 Lean 4 中证明 `perceus_sound` 定理
- A + B：已证明逻辑通过类型强制 = AlmidePerceusBelt 完整体

<a id="post-v0270-proof-coverage-gap-2026-06-11"></a>

## v0.27.0 后的证明覆盖空白（2026-06-11）

belt 的 Lean 定理证明 IR 级 Inc/Dec 平衡。v0.27.0 让 runtime 侧成为真实实现（free-list push/reuse、rc==0 二次释放哨兵、rc_inc 复活陷阱、清空 free list 的 region reset）— 当时这些均未进入证明范围。

**这一半进入了 Coq 信任主干，而非 Lean belt** — 应阅读 [`proofs/FreeList.v`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/FreeList.v)，而非 `.lean` 文件：

- 具有再利用安全性的 allocator 状态机（bump frontier、free-list、ghost live set）— `alloc_not_live`：经过验证的分配永远不会是当前存活 block（#909 / A1.2）；
- **region reset 不留下陈旧 free-list entry** — `RegionSave`/`RegionRestore` 跨任意 window 主体保持 allocator 不变式，所以 free-list entry 不会指向回收区域，reset 后仍保有再利用安全性（`region_window_reuse_safe`、`region_reset_leaves_no_free_into_the_region`）。保留 window free-list 的朴素替代方案已在具体 trace 上被证明既破坏不变式，又重新交出存活 block；
- **PINNED_RC 永生性** — `$alloc8` / `__alloc_pinned` block（WASI scratch、preopen table）在任意运行中从不释放、不进 free-list、不由 `$alloc` 返回，并能跨 region reset 存活（`pinned_stays_immortal_forever`、`p_region_reset_preserves_PINV`）。这就是 C-042 违反的不变式。

- **“再利用恢复 rc=1”** — 配套 [`proofs/FreeListRc.v`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/FreeListRc.v) 中的 COUNT 一侧。它把引用计数以 `RuntimeModel.read_rc` 纳入范围（与 `WasmRcDec`/`WasmExec` 绑定至生成字节的同一 cell），并分别证明两半：allocator 交回的再利用 block 不带陈旧计数，到达时正好为 0（`reuse_hands_back_a_zero_count_block`）；constructor store 后正好为 `RC_INITIAL` = 1（`reuse_restores_rc_1`）。“free-list block 读数为 0，存活 block 读数 ≥ 1”不变式在任意运行（`r_steps_preserve_RINV`）及 region reset（`r_region_reset_preserves_RINV`）中保持，因此 `$rc_dec` 哨兵成为一道墙：`double_release_traps`。这一对被证明是必要的，而非装饰 — `omitting_the_rc_store_traps_on_the_first_release` 运行了 renderer store 缺失、再利用 block 在首次 release 就 trap 的 trace。

仍需信任的，仅是这一点：emitter 在每个受 RC 管理的 `call $alloc` 位置写入 `RC_INITIAL`。这是 renderer 的性质，不是 allocator 的性质，故 allocator 证明无法触及。`almide-mir` 的 `every_rc_managed_alloc_site_initializes_the_rc_cell` 以可执行方式门禁：枚举 renderer source 中全部 16 个 `$alloc` 位置，要求每个位置初始化 RC cell 或属于具名 raw-scratch 位置（已建模并证明的 `alloc_raw` 类别：从不释放的 argv/envp buffer），并从 `.v` 文件读取 `RC_INITIAL`，防止常量漂移。emitter 级 RC 操作（已存储 field 的 dup、runtime 内 inc）也因结构设计而仍对 IR verifier 不可见。
