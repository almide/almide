<!-- description: The retired v0 wasm emitter's Try/Unwrap/Fan early-return heap leak, kept as the historical record of the class -->
<a id="v0-wasm-codegen-tryunwrapfan-early-return-heap-leak"></a>

# v0 wasm codegen：Try/Unwrap/Fan 提前 return 的 heap leak

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v0-unwrap-early-return-leak.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v0-unwrap-early-return-leak.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v0-unwrap-early-return-leak.zh-CN.md)

> 非规范性历史阅读快照。以固定原文为准，不自动同步。未独立重跑测量、证明 gate 或当前实现检查。未执行引用命令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v0-unwrap-early-return-leak.md)
> 退役v0的历史bug记录。开头已实现／验证状态与较早推荐方案及Why not now并存，保留各阶段，不声称新增修复或重跑。最终emitter侧状态与Perceus注记建议不同，不据此推断当前实现。leak/double-free保证限于带明确排除及pass顺序条件的历史主张，不是普遍内存安全。旧no-return/auto-?语言主张不是当前语法指令。

**状态**：已实现并验证（emitter 侧 `emit_early_return_decs`）；v1 wall 已解除，恢复 −59 部分（in-profile 4083）。无 leak（100k err loop 完成）＋无 double-free（260-file wasm corpus green）。
**发现**：2026-06-14，由 v1 trust-spine 的 adversarial-verify workflow（`wf_54ac302a`）扫描 `(あ)` 小型 lowering 缺口层时发现。
**严重性**：MEDIUM — 是 leak，绝非 use-after-free / double-free。wasm linear memory 在 instance 销毁时整体回收，因此短程序成本有界，仅在长期运行且重复走错误提前 return 路径的 process 中无界。正常 Ok 路径不受影响。

<a id="the-bug"></a>

## Bug

在 `effect fn` 中，`x!`（`IrExprKind::Unwrap`）、effect-fn auto-`?`（`IrExprKind::Try`）和 `Fan` auto-unwrap 都用提前 return 传播 `Err`。v0 **wasm** emitter 将该 Err 路径 render 为直接提前 return：

```
// v0's emit_wasm/expressions.rs (deleted with #782 — read via git history)
//   Unwrap/Result : 846   local_get(scratch); return_;
//   Unwrap/Option : 826   ... return_;
//   Try           : 786   ... local_get(res); return_;
//   Fan single    : 715   ... return_;
//   Fan multi     : 749   ... return_;
```

函数中每个 heap local 的 `rc_dec` 释放，仅由 Perceus pass 插在末端 `Ret`/`Nop`（v0 的 `pass_perceus.rs:389-441 insert_decs_before_ret`，随 #782 删除；见 git 历史）。提前 `return_` 无条件转移到函数结尾，跳过全部这些 dec。因此，提前 return 时已经绑定且仍 live 的任何 heap local 都在 Err 路径泄漏。

**Rust 没有此 leak**：`x!` render 为 Rust `?`（`crates/almide-codegen/src/walker/expressions.rs:710-753`），其提前 `return Err(..)` 自动对 scope 内每个 local 运行离开 scope 的 `Drop`。所以这是 **wasm-only** codegen leak。

**为何 `break`/`continue` 已修而 Try/Unwrap 未修**：`flatten_exit_tail_blocks`（pass_perceus.rs:661-704）将尾位置的 `continue`/`break` 提升，让 `insert_decs_before_ret` 将其视为末端并在其前输出 dec。它只匹配 `Continue`/`Break`（`tail_block_ends_in_exit`），且仅匹配尾位置。`Try`/`Unwrap`/`Fan` 是表达式中途的值，从不是 block 尾部，因此尾部提升不适用；语言里也**没有 `IrExprKind::Return` node** 可作为脱糖目标（Almide 没有 `return` 关键字），故不能将提前 return 转成 Perceus 现有 return-dec 逻辑能看到的显式 IR 控制流 node。

<a id="v1-trust-spine-interim-done-commit-4fbbefe7"></a>

## v1 trust-spine 临时措施（完成，commit 4fbbefe7）

v1 MIR lowering 现在在 owned heap local 仍 LIVE 时，对 `Try`/`Unwrap` 设置 WALL（`expr_has_early_return` + `!live_heap_handles.is_empty()`，`lower_stmt`/`lower_tail` 中的 guard）。这阻止已证明 checker 认可泄漏形状，否则对 `no_leak` 条款会成为**接受但不安全**。代价：in-profile 4081→4022（−59）。**此 v0 修复可恢复这 59 个**：wasm render 无 leak 后，deferred-continue cert 再次忠实，即可解除 wall（移除 `live_heap_handles` guard）。

<a id="the-core-constraint-per-exit-liveness"></a>

## 核心约束：逐 exit 的 liveness

每个提前 return 点的 live heap local 集合不同（在其之前绑定且尚未 move/dec 的 local）。让单个函数 epilogue dec 全部 heap local，会 double-free 早期路径尚未绑定的对象，或 dec 已移出的 local。因此必须精确 dec 该 exit 仍 live 的集合。唯一权威来源是 **Perceus liveness 分析**，emitter 不能重新推导（emit 层 ownership 是手写的，会偏离分析；独立 emitter live-set 是典型 double-free 来源）。

<a id="recommended-approach-perceus-annotates-emitter-emits"></a>

## 推荐方案：Perceus 注记，emitter 输出

1. **Perceus**（pass_perceus.rs）在既有 liveness pass 中，为每个 `Unwrap`/`Try`/`Fan` node 计算其 LIVE 的 heap-local `VarId` 集合（此前分配、尚未 dec — 正是它已为末端 dec 放置跟踪的集合）。通过稳定 node ID 为 key 的 side-table，或 Unwrap/Try/Fan IR node 新增 `Vec<VarId>` field 附加（会改变 IR 形状，优先 side-table 以免改动全部 IR consumer）。
2. **wasm emitter**（emit_wasm/expressions.rs）在为 Unwrap/Try/Fan Err 路径发出每个 `return_` 前，先为该 node live 集合中的每个 `VarId v` 发出 `local_get(v); call(rc_dec)`（从 side-table 查询）。正在返回的值（Err Result ptr）不在 live 集合中（它是结果，不是 local），所以不会双重 dec。
3. **native emitter**：不变（Rust `?` 已运行 Drop，增加 dec 会 double-free）。

此方式无 drift（live 集合属于 Perceus，不属于 emitter），且改动局部（emitter 仅输出所提供集合）。

<a id="alternatives-considered-rejected"></a>

### 考虑的替代方案（拒绝）

- **emitter 自行跟踪 live-local**（每次发出 Alloc/Dec 时更新集合）：易漂移；emit 层已偏离分析，错误集合会 double-free。拒绝。
- **增加 `IrExprKind::Return` node，并在 Perceus 前把 Unwrap 脱糖为 `match{Ok=>v,Err=>return}`**：原则合理，但为中等严重性 wasm leak 做大型语言／IR 更改（新 node、全部 visitor/consumer、双 backend），比例不当。暂时拒绝。
- **经 `br` 的单一 cleanup epilogue**：仍需逐 exit liveness，否则 double-free，无法避开核心约束，也不更简单。

<a id="test-strategy-mandatory-before-shipping--rc-critical"></a>

## 测试策略（交付前必须 — RC 关键）

1. **fixture** `effect fn f(s) { let big = make_heap(); let n = parse(s)!; use(big,n) }` 用 `--target wasm` 编译；断言 Err 路径 dec `big`（wat2wasm trace 显示 `return_` 前有 `big` local 的 `rc_dec`）。
2. **无 double-free**：被 unwrap 的值就是唯一 heap 对象、无其他 live local 的 fixture，Err 路径不得额外 dec 任何东西（result ptr 被返回，不被释放）。
3. **跨目标 byte 稳定**：native 不变；wasm-vs-native 差分 gate（`spec/wasm_cross/`）保持 green，native 输出不变。
4. **`--target wasm` 下完整 `almide test`**＋wasm 确定性 harness。double-free 会 trap，leak 静默，因此向 fixture harness 增加 leak 计数断言（alloc/free 平衡）。
5. **重跑 v1 trust-spine** 并解除 `live_heap_handles` 的 Unwrap/Try wall（lower_stmt/lower_tail 中的 `expr_has_early_return` guard）；corpus-wall 应回到 in-profile ≈ 4081，且三个性质仍均 ACCEPT。

<a id="verified-algorithm-workflow-wf_dbf7590c-6-agents"></a>

## 已验证算法（workflow wf_dbf7590c，6 agents）

adversarial-verify workflow 解决了灾难性风险，并固定精确算法：

- **move 不会导致 double-free。**Perceus 模型是 *alias-by-RcInc、move-nothing-user*：传入 list/record/tuple/concat/call 的 heap local 通过对交出的 pointer 执行 `RcInc` 实现共享，来源保留自身末端 Dec（emit 侧“SHARE dup”，expressions.rs:1265-1308）。没有 user heap VDecl 会在函数中途被消耗而缺少配套 `FnBody::Dec`（`collect_heap_vdecls` 列出全部 heap VDecl；`insert_decs_before_ret` 给每个末端 Dec）。所以 `vdecld-minus-decd` 将仍 owned 的 local 保留在 `live` 中，恰是 err 路径该 dec 的集合。`collect_moved_out_vars` verifier（pass_perceus.rs:880-910）仅把 bare-Var block 尾部及 for-in-iterable 视为“moved”，两者都是 leak 诊断豁免，不是删除 Dec。⟹此修复整体最坏失败是漏 dec（LEAK），绝非 double-free，属于安全方向。

- **live 集合必须对包围它的 chain-stack 求并集。**每个 Block、`while`/`for-in` 循环体和 `if`/`match` arm 本体都是独立的 `FnBody` chain（`block_to_fnbody`），外层 chain 的 `collect_heap_vdecls` 看不到它。提前 `return_` 退出整个函数，因此 node 的 live 集合＝从 node 向外到函数 root 的每个包围 chain 上，（该 chain 中 node 前已绑定的 heap VDecl）−（此前 heap `Dec`）的并集。单次平坦 walk 会泄漏外 scope 和逐次迭代 local（两个 HIGH 严重性反证 hole 确认了这一点，两者都是 LEAK 而非破坏）。在 pass_perceus 内处理嵌套 chain 的同一递归中计算，将外层 live 集合传给每个 nested-chain walk。

- **排除项（必须精确镜像既有谓词，否则有 double-free 风险）：**
  (1) `__tco_`/`__br_`/`__perceus_*` temp — 与 insert_decs_before_ret:414-418、Assign move-exempt:309-315 相同的 prefix 检查；它们交出 ref 且从不获得 Dec，dec 它们就是 use-after-free。
  (2) `EnvLoad` 绑定的 borrow local — 与 collect_heap_vdecls:358 相同的 `!matches!(expr.kind, EnvLoad{..})` guard（由 closure env 拥有）。
  (3) 进行中的绑定 — `let n = …!` initializer 中的 Try/Unwrap 不得包含 `n`，因为表达式求值完才绑定。
  按 VarId 去重。

- **返回的 Err 值是 `ScratchAllocator` temp（原始 wasm local，没有 VarId），从不属于 live 集合** → result ptr 从不双重 dec。

- **交付形式**：node 上 field（Unwrap/Try/Fan 的 `early_return_decs: Vec<VarId>`）需要约 13 个 OR-pattern arm＋15 个 constructor，但 live 集合随 node 移动，无需 key。逐函数 pre-order node index 为 key 的 side-table 只需一个 struct，但仅在 Perceus 与 emit 之间没有 pass 重排／增加／删除 Try/Unwrap/Fan node 时才健全（key 不匹配可取错集合 → double-free）。按 Perceus pass 位置选择：若它是 emit 前最后 IR pass，side-table index 稳定且较便宜；否则优先 field 以保安全。

- **Emit**（expressions.rs 约706/770/798，三个提前 `return_` 位置）：对 live 集合每个（VarId v），用 var_map＋var_table 将 v 解析为（local L、ty T），完全同 statements.rs:358-392 的 RcDec handler；在 `local_get(scratch); return_` 前输出 `local_get(L);
  emit_typed_rc_dec(&T, L)`。不得破坏 `scratch`。

- **明确测试嵌套**（平坦 walk hole 证明 fixture #1 不够）：分别在 `while`/`for` 本体和 `if` arm 内放置 Unwrap，各自有跨越它仍 live 的外 scope heap local，断言 Err 路径确实 dec 外层 local（alloc/free 平衡）。平坦函数顶层 fixture 即使采用错误的逐 chain 集合也能通过。

<a id="why-not-now"></a>

## 为什么不现在做

这是手写、易漂移 wasm emit 层的 RC 关键更改；错误 live 集合会在每个已交付 wasm program 中造成 double-free（memory corruption），不只影响 trust spine。应在全新、休息充分的专门 session 中配合上述完整测试群进行，正如设定目标时标注的“専用セッション向き”（适合专门 session）。期间 v1 临时 wall 已让 trust spine 保持诚实。
