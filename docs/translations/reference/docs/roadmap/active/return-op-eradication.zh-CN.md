原文说明元数据：为 ! 运算符只做一次脱糖，删除按位置分化的 return-op 群
<a id="return-op-eradication-one-desugar-for--position-zoo-deleted"></a>

# 根除 Return-op 分化：`!` 一次脱糖，删除位置群

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](return-op-eradication.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](return-op-eradication.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](return-op-eradication.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/return-op-eradication.md)

> 原文注记：标题及根除目标不表示 R2/R3 已完成；开头明确保留探针门控脱糖，并将矩阵限定为8月活动。后续 lifted-carrier 工作改变了历史 blocker 背景。标题称5编译器调查但列出六个；52文件原因计数相加为51，42文件矩阵是稍后快照。原样保留，不统一数字。早期 ret event 草图与已落地 x/check_xc 表述并存，不据此推断等价或当前普遍安全。未独立重跑调查、verifier/kernel 主张或零 trap 计数；旧 v0 指令不定义当前交付路径。

<a id="probe-status-clarification"></a>

## 探针状态澄清

下列 R2 实测矩阵是8月活动快照，不是当前全语料计数。其 #1437 阻塞项后来获得了 [lifted-carrier-abi.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/lifted-carrier-abi.md) 所述的 L1 仅探针 wrapping 及 heap-family 准入。R2/R3 并未宣告完成：`bang_return_probe` 及探针门控的位置脱糖仍在 MIR lowering 中。保留 v0 的指令指历史活动；当前交付的 Wasm 路径是 structural（[wasm_route.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/wasm_route.rs)）。

状态：R2 进行中（批准 ○ 2026-08-15；R1 已以3358f26f6+53439f582落地 — 两个 renderer 的 op、verifier 退出规则、cert marker `x`、kernel format v5 `check_xc` 全绿；探针 `ALMIDE_BANG_RETURN=1` 门控9个位置行，2e73deceb）。

R2 统计（探针 ON，全 almd 套件；baseline 391/12/0）：
- 初始：335/64/**4 failed** — 这4个是 #1434 v0 回归（Err-turbofish，52326a54e修复），不是规则信号。
- clean（规则增量1-2生效：标量 bind + stmt 位置）：**339/64/0** — 每次探针 decline 都安全回退；矩阵是 baseline 12之外的52个 fallback 文件（下述 `scratchpad matrix52` 快照在 wall sweep 中，不持久 — 通过 ALMIDE_FALLBACK_NAMES=1 再生成）。

增量 H（3b6ceb7f8）：准入 HeapOk×HeapOk — 同 family carrier 直接 return（tag@16），heap payload = LoadHandle@12 + Dup + 类型化 seeding（String / 扁平 heap-elem list / 追踪 variant）。先 Dup 后延迟 carrier-drop 的形态是已验证惯例（Lean 的 `oproj` 规则：inc 严格先于任何父对象 release；ExplicitRC.lean:575-578 + 421-444 的 inc-before-dec 定律）。H 后函数级 decline 分布（build 可达）：**20 family-mismatch**（ResErrStr REBOX 切片 — 提取 err @12、Dup、drop carrier、构建函数 family 的 err、Return），10 fn-family（test-block Unit 函数 — ABORT 形态，不是 Return），3 callee-family（Option `!`），2 heap-payload-class（record/Value）。

Hoist 准入表（Lean Do/Basic.lean:805-909，移植设计）：从 if/while 条件、match 对象、无 binder 的 let RHS、任意语句子项 HOIST — 从左到右、从最内层开始、band-allocator 临时值。从 lambda body、match arm body、带 binder 的 let RHS、loop body（新语句作用域）、`&&`/`||` 右操作数则 HARD-ERROR（绝不静默 skip）。Lean 唯一文档化的瑕疵：通用下降会静默将短路改成急切求值；我们改为禁止。

<a id="r2-state-after-the-second-increment-batch-209def816"></a>

### 第二批增量后的 R2 状态（209def816）

探针 ON **349/54/0，零 trap**；OFF **391/12/0**（精确 baseline）。已落地切片：cross-family rebox（消除全部20个 family-mismatch decline）、tail + bare-root hoist、Unit 函数 ABORT 退出（die，不是 Return — Zig `DefersToEmit` mode-flag 形态）、lambda-root hoist guard、探针缩限为仅语句 continuation 重写。

探针暴露的3个真实 bug（全都预先存在，非探针伪影）：
1. 尾位置嵌套 `!` 不看标签就读取 payload（由 hoist 扩展修复 — 探针 OFF 时也生效）。
2. **#1437** — lifted heap-ret carrier：`result_family` 将其判为 HeapOk（tag@16），而 lift 实化 len-as-tag；而且（第二遍调查）lift 的 ABI 决策本身来自试探性 lowering，因此哪些函数获得 Result ABI 取决于传播机制当前接受集合。通过 decline lifted heap-ret 函数遏制；**余下42个矩阵文件中的30个在该类背后**，所以 #1437 是 R2 关键路径。
3. void-abort 谓词对任意无 channel 函数触发，包括返回 tuple 的 pure 函数，其 `!` 实际属于外层 fallible lambda（fallible_lambda L1）— 已缩限为声明返回 `Unit`。

剩余矩阵（42）：30个在 #1437 后，3个 callee-family（Option `!` — Option carrier 无 err channel 可传播，需要构建 none⇒err(msg)），1个 heap-payload-class（record/Value payload），其余仅在 test-block 函数内遇到障碍。

52个文件的障碍原因分布（每文件首个障碍）：
- 13× bind-rule decline（`unwrap ! bound to a let/var`）— 增量1门禁：heap payload / 非 Scalar callee family / auto-wrap 函数（准入已落地）/ err 不匹配。下一步：按门禁拆分（ALMIDE_DBG_BANG），开放 heap payload。
- 8× `scalar binding outside the value subset` — bind 值表达式内嵌 `!`：通用 HOIST 类。
- 7× `effect-unwrap desugar's reach` + 1× call-argument `!` + 1× loop var — hoist + loop 类。
- 3× heap-result `if` + 1× `match` + 1× Tuple return — 下游形态，规则掌管 err 路径后可能消解。
- 16× 仅 TEST-block 函数中遇到障碍（`almide build` 不显示；codec 系列）— 排优先级时通过逐文件 `almide test` 捕获。
定律：rot-eradication law 6 — 退出的 arm 丢弃合并 continuation。
先例：`??` 路径群删除（#1418，+148/−1054）— 同病高一层：识别器按 fixture 增加 case，这里脱糖表按语法位置增加行。

<a id="the-disease"></a>

## 病因

`!`（effect-unwrap-propagate）**按语法位置**脱糖 — `BRANCH_PASSES` 行表（`desugar_branch.rs`）约9个 pass，加上 `desugar_unwrap.rs` 的 stmt/tail 对：

| 位置 | pass | 代价 |
|---|---|---|
| stmt / let-bind | `desugar_effect_unwrap` | continuation 嵌入 ok-arm |
| tail | `desugar_tail_effect_unwrap` | pass-through 特例 |
| call-arg | `desugar_callarg_unwrap` | 临时拼凑的 ANF hoist |
| if-arm | `desugar_if_arm_unwrap` | 专用遍历 |
| `ok(e!)` | `desugar_unwrap_rewrap_identity` | 恒等折叠 |
| let continuation | `desugar_let_unwrap` | 专用遍历 |
| expr-nested scalar | `desugar_stmt_value_nested_unwrap`（#1183） | 第二个临时拼凑的 hoist |
| for-loop body | `desugar_loop_unwrap` | loop 携带**错误标志** + 循环后分派 |
| unit if/match stmt + cont | `desugar_stmt_control_unwrap` | 向每个 arm **复制尾部** |

根因在 `desugar_unwrap.rs` 自己的头注中：*v1 MIR 没有函数中途提前 return 的 Op*。因此各位置必须以不同方式重构 continuation。每一行都是 fixture 碰壁时加入的 — 与 `??` 路径群相同的增长特征。仅专用脱糖文件就约2,800行；`Unwrap`/`Try` 处理散布在30+个 lowering 文件。

<a id="research-verdict-5-compiler-survey-sealed-out-of-repo-2026-08-15"></a>

## 调查结论（5编译器调查，2026-08-15 在仓库外封存）

调查：rustc、Zig、Roc、Lean 4、Koka、Grain。核心一致：

1. **单一脱糖点，与位置无关** — 通过把 exit 变为一等终结（noreturn）表达式实现。在值位置，lowering “不赋任何值，后续不可达”（rustc `lower_expr_try` 是唯一 `?` 位置；Zig `tryExpr` 发出 error body 以 `ret_node` 结束的 block；Roc 将 `?` 一次脱糖为 `match + e_return`）。
2. **退出路径清理由推导得出，不按糖衣手写** — 在 exit 处遍历 live/scope 状态。rustc：7行 scope walk 进入共享 DropTree。Zig：`genDefers(inner→outer, mode)` — exit 的*目标*是遍历外边界。Grain（RC + structured wasm，恰是我们的 profile）：`return` = 对所有存活 managed binding decref，*跳过返回值*，在 return 前发出；`break`/`continue` = 仅 loop 层 binding，不动 live map。
3. **尾部复制 / 错误标志是缺少 primitive 的代价。** 参照实现若复制 continuation，也是*带阈值优化*（Koka：当且仅当 non-return branch ≤1；Lean：当且仅当普通 exit >1 才 join），绝非基本策略。
4. **不加新 op 的路线实测已走不通。** Lean 将“sum-type result + single dispatch”交付约5年后替换：variant 爆炸（结果类型 × exit-kind 组合）、每个 exit 传递已修改变量 tuple、代码膨胀未解决，最终内部仍重新发明 join point。
5. **对 `!`，目标为 frame 优于目标为 block。** Zig `ret_node` 点名函数 frame，任意深度都无需外层 block 配合，且 error edge 为 noreturn，不需 merge/phi 协调。完整 join point（Roc/Lean）是通用机制，确有 scope 不变式和 ownership 契约成本 — 对 error edge 始终指向 frame 的 `!` 过重。暂缓为后续事项（见非目标）。

<a id="the-cure-opreturn-one-hoist-one-rule"></a>

## 治疗：`Op::Return`、一个 hoist、一条规则

<a id="new-op"></a>

### 新 op

```rust
/// Frame-targeted early exit: return `val` (None = Unit) to the caller.
/// TERMINAL — nothing may follow it in the enclosing arm (mir_wellformed).
Return { val: Option<ValueId> }
```

代码注释翻译：以 frame 为目标的提前退出，将 val（None = Unit）返回调用方。终结性 — 外层 arm 中后面不得有任何内容（mir_wellformed）。

Render：wat `return`（任意 block 深度有效），native `return v;`。两 target 已有该指令；render 成本为零。

<a id="lowering-rule-the-one--rule"></a>

### Lowering 规则（唯一的 `!` 规则）

下述 hoist 之后，每个 `!` 都处于 let/stmt 位置：

```
let x = f()!   ⇒   v = lower(f())
                   IfThen(err_tag(v)) {
                     drops(live_heap_handles ∖ {err_carrier})   // derived, not authored
                     Return(err_carrier)
                   } EndIf
                   x = ok_payload(v)                            // straight-line, no nesting
```

代码注释翻译：drop 是推导而非手写；ok payload 后是直线流程，无嵌套。

`live_heap_handles` 已存在于 lowerer — 就是参照实现推导 drop 的 scope walk。continuation 保持线性：无嵌套 match、无尾部复制、无 loop flag。loop body 内 `!` 无需 loop 配合（wat `return` 退出全部，drop 覆盖 loop 的 live 状态）。

<a id="the-hoist-kills-the-position-dimension"></a>

### Hoist（消除位置维度）

一个通用 ANF 遍历替代 `desugar_callarg_unwrap` + `desugar_stmt_value_nested_unwrap` + if-arm/let 变种：将每个表达式内嵌 `Unwrap` 提升为独立 bind-position stmt。位置知识变成小表（禁止的 binder、分隔符种类），而不是 pass。

<a id="verifier-the-part-only-we-must-prove"></a>

### Verifier（只有我们需要证明的部分）

- `lib_c.rs` branch frame 增加 `diverged: bool`。以 `Return` 结束的 arm：(a) 不参与 `check_branch_agreement` — join 只从另一 arm 状态继续；(b) 在 `Return` 独立满足 frame-exit 义务：所有拥有对象 rc 0，唯返回值作为移动（`m`）至调用方计账。
- 证书：`Return` 发出关闭 stream segment 的 `ret` event；incs-before-decs 纪律不变（drop 先于 `Return`）。
- Rocq kernel（`OwnershipChecker.v`）：one-shot-branch 接受规则增加对应规则 — 运行以 `ret` 结束的 arm 当且仅当无故障达到 exit 义务才接受；仅不退出的 arm 间要求 agreement。witness corpus 增加专用 fixture（both-arms-ret、ret-with-live-handle ⇒ reject、ret-in-loop-body）。

<a id="stages"></a>

## 阶段

- **R1 — 端到端 op，未使用。** `Op::Return` + `mir_wellformed`（终结：arm 内无后续）+ 两 renderer + certificate event + verifier 退出规则 + kernel 规则 + witness fixture。门禁：op 从不发出时全套测试绿；kernel 对新 witness 通过证书检查。
- **R2 — 探针后的规则。** hoist + 唯一 lowering 规则，由 `ALMIDE_BANG_RETURN=1` 门控（#1418 readiness-probe 方法）。测量全套测试 + 3-way oracle 的 decline 矩阵，以 law-4 修复（类型化 seeding、ANF retries）降至零；两路径均接受时与旧路径做字节 A/B 比较。
- **R3 — 删除。** 矩阵为空 ⇒ 切默认，删除位置行（`desugar_effect_unwrap` 嵌套 match body、`desugar_loop_unwrap` error flag、`desugar_stmt_control_unwrap` 尾部复制、if-arm / let / nested / callarg 变种），仅当仍值得作为优化时保留 `desugar_unwrap_rewrap_identity`。同 PR 跟进台账：scalar-read audit、STAGE-STATUS、TCB 主张、walled-real ratchet、caps 计数（Return 不发出 call — 计数不变）。
- **R4 — 后续（独立 issue，不属本轮）。** 通过相同退出 arm 规则接受 #1421 match lowering；loop 内 `guard … else <value>`（当前诚实拒绝）；loop-as-join 推广；wasm 大小回归时在 render 阶段共享 drop-chain suffix。

<a id="non-goals"></a>

## 非目标

- 本轮不做通用 join point / labeled break。`!` error edge 始终以 frame 为目标；通用机制需尚不需要的 scope 不变式与 join-aware ownership 契约。保留后续可能（R4）。
- 不改 v0：v0 路径已将 `!` lower 为 Rust `?`，仅 v1 MIR 路径获得 op。3-way oracle 检查两条路径。

<a id="exit-criteria"></a>

## 退出条件

1. `BRANCH_PASSES` 中 `!` 专用行归零；`desugar_unwrap*.rs` 缩减为 hoist + 恒等变换（若保留）。
2. 新 `!` 位置（未来语法）需要零新增脱糖代码 — hoist + 一条规则吸收它。（rustc 示例：`do yeet` 免费复用 `?` 机制。）
3. Kernel 证书检查：witness corpus 覆盖 ret 退出 arm 的接受和拒绝情况；verify-trust 绿。
4. 删除收益：`lower/` 范围净减少 ≥1,500行。
