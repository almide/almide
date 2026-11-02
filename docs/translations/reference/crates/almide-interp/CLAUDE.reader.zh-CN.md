<a id="almide-interp"></a>

# almide-interp

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](CLAUDE.reader.zh-CN.md)

> 这是固定于 35297a612fb29e3f78f6587aba253aa91d51f466 的[正本代理指示](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-interp/CLAUDE.md)的非活动、非规范性阅读译本。原始 CLAUDE.md 仍为正本且不变；独立的 CLAUDE.reader 名称不是自动加载的指示文件。引用规则是历史参考资料，不是新的指示层级，也不授权执行命令、修改代码、提交、推送或发布。本快照不自动同步，未针对当前版本独立执行或验证实现、覆盖、独立性及安全主张。inline code 完全保留。

遍历 **codegen 之前**的 `IrProgram` 树的解释器 — 跨目标 oracle 的第三执行分支，也是一份可执行规范。

<a id="why-it-exists"></a>

## 为什么存在

跨目标门禁（`spec/wasm_cross/*.almd`，由 `tests/wasm_runtime_cross_target.rs::wasm_cross_target_spec` 强制）比较 **native** 与 **WASM** 后端，要求 `(stdout, stderr, exit)` 逐字节一致。这样的两方投票在结构上看不到*两者以相同方式出错*的缺陷：如果 codegen 和 WASM emitter 共享的 lowering pass 有错，两者仍会一致，门禁保持通过，而行为却错误。

此 crate 增加第三个独立裁判。它在 `lower → optimize → mono → ir_link` **之后**，但在任何 `almide-codegen` 目标 lowering pass（`ClosureConversion` / `Perceus` / `StdlibLowering` / `IterChain` / …）**之前**的切点求值 IR。因此它与任一后端*完全不共享* codegen 的目标 pass。约 22 种由 codegen 插入的 `IrExprKind` variant 在此不可达，`eval.rs` 将其断言为 `unreachable!` 以守住边界。

`tests/wasm_runtime_interp_oracle.rs::interp_cross_target_spec` 是三方 harness（root-package binary；与 equivalence 和 wasm-opt 门禁共享语料源码 `tests/wasm_runtime_test_parts/corpus.rs`，各 binary 构建自己的 fixture 表）：
- `interp == native == wasm` → 由不可能共享 codegen 缺陷的规范加以佐证。
- `native != wasm` → **由两方 `@xt-allow` 门禁负责**的后端分歧；本 harness 仅记录 interp 支持哪个后端（打破平局的诊断），不会判失败。
- `native == wasm` 但 interp 不同意 → **醒目的失败**（`BOTH-BACKENDS-WRONG` banner）。需要诊断：修复解释器，或者你刚发现了两方门禁看不到的双后端缺陷。

<a id="module-map"></a>

## 模块映射

- `value.rs` — 动态 `Value` 模型及两种显示模式（`display_bare` = `println`/Display，`almide_repr` = 复合/容器形式）。两者复现 `almide_repr_prelude`（`crates/almide-codegen/src/lib.rs`），因此与 native 逐字节一致。
- `env.rs` — 以 `VarId` 为键、由 `Rc` 共享的 frame。复现 native `RcCow` 捕获语义。
- `eval.rs` — 所有可求值 IR node 的 tree-walker、fuel 记账、pattern engine（包括越过切点仍保留的 list pattern），以及 record-repr 名义名称恢复。
- `dispatch.rs` — `Call` 路由（`Named` / `Module` / `Method` / `Computed`）、variant-ctor registry，以及 **HOF allowlist**（`is_hof`）。
- `hofs.rs` — interp 内的 HOF（map/filter/fold/…）、interp-native 容器操作，以及原地修改谓词。
- `inplace.rs` — `mut` receiver 修改器（`list.push`、`set.insert`、…），建模为对 receiver 绑定的读取 → 变换 → 写回。对绑定自身存储 slot 的 `Rc::make_mut` 就是 COW 规则：唯一 owner 原地修改（push 循环保持线性），存在 alias 则强制复制（C-033）。
- `bridge.rs` — `prim.*` 标量基础层、self-hosted 本体在 pool 层内部作为基础使用的 scalar/string 分支，以及两个边界 fallback（固定位宽数值的 `to_string` observer、`bytes.zeros`）— 每个分支都是语料会到达的分支，由 fallback 台账设门禁。
- `surface.rs` — 写在同捆模块内的公开 fn（不是 registry source，因此 pool 中没有其本体）：转发到 registry body 的薄 wrapper（`int.pow` → `math.__pow`）、重新表述的本体（`int.parse_radix`、`bytes.get_*`/`next_*`、`list.binary_search`、`deque.*`）、以实参静态类型选择 `Numeric` pivot 的泛型 `<T>.from` 家族，以及按后端将其 lower 为的 `matrix.*` 调用实现的 Matrix 运算符。
- `stdlib_pool.rs` — self-hosted stdlib 本体（与 wasm 分支链接的源码相同），只 lower 一次；在容器层以下，它们就是投票方（#2185）。

<a id="coverage-model--does-a-new-stdlib-fn-get-covered-automatically"></a>

## 覆盖模型 — 新 stdlib fn 会自动被覆盖吗

**若其在 scalar/heap prim 基础层上自托管，则会 — interp 求值注册本体自身。** Dispatch 依序为：

1. `dispatch.rs::is_hof` — 接收闭包的 combinator（allowlist）。
2. `hofs.rs::eval_container_op` — 非 HOF 的结构化容器操作（这里的容器是 `Value`，而非线性内存，因此此层保持 interp-native）。
3. `dispatch_module.rs::prim_floor` — 持有每次运行的解释器状态的 prim（block heap、argv/env、沙箱化 fs、`prim.die`）。
4. **lower 后的 self-hosted 本体**（`stdlib_pool.rs` — registry 源码，即两个后端运行的同一定义）— 位于 pool 边界（`pool_depth == 0`，fixture code 调用 stdlib）。它自身在层内的 helper call 优先走 bridge：那里 bridge 是本体所用的基础，类似 `prim.*`（在 block heap 上嵌套第二个本体是此层尚不能忠实做到的事情 — 首次 body-first 测量在 9 个 fixture 上投票错误，在 2 个上崩溃）。
5. `bridge.rs::dispatch` — 在边界处，仅当步骤 4 没有本体、无法接受该调用的 arity 或弃权时使用：固定位宽数值的 `to_string` observer 和 `bytes.zeros`。自 #2185 起，在任一层中 bridge 回答的每个非 `prim` 名称均记入 `RunOutcome::bridge_fallbacks`（标记 floor 或 fallback），由 `tests/wasm_runtime_interp_ledger.rs::interp_bridge_fallback_ledger` 保持与 `interp-bridge-fallback-ledger.txt` 双向相等 — Rust 副本代替本体投票必须经过已审阅的台账编辑，而语料不再到达的分支是应删除的死分支。

未被处理的任何内容都落入 `Flow::Unsupported(...)`，三方门禁将其记为**数据驱动、有理由的 skip**（输出具体原因；没有硬编码 skip-list）。覆盖缩减并不静默：abstain 集合由 `tests/wasm_runtime_interp_ledger.rs::interp_abstain_ledger` 对照已提交的 `interp-abstain-ledger.txt` 审计（无需后端，从不自行 skip — 其 binary 仅包含 interp 分支，绝不包含语料表，因为触及表就会触发 native+wasm 构建，恰好破坏这一属性）。fixture 弃权却没有台账项会使 CI 失败 — 因此新 stdlib fn 的 fixture 若无法由 interp 运行，必须在同一 PR 中加入 interp glue 或经过审阅的台账项 — 现在已能求值的 fixture 所对应的陈旧项也会失败（台账只缩不增；CG-1 gap-audit ratchet，issue #564）。扩大本体可使用的 prim 基础层是手段：每个获支持的 prim 都把 skip 转为真正的第三票，并删除台账行。

本体自身的步骤按 pool 预算（`POOL_FUEL`，十个程序的量）计量，而不是 fixture 的预算：可执行规范的成本属于规范自身，fixture 的 `DEFAULT_FUEL` 仍限制 fixture 自己的循环。此处不要通过 path 依赖 `almide_rt` — 该 crate 会引入 interp 不需要的 rustls/webpki（network/TLS）及带 effect 的表面。

<a id="rules"></a>

## 规则

- **interp 必须匹配后端，而非抽象地“正确”。** 后端共享某种特殊行为时（例如匿名记录字段按排序后的顺序显示；`${float}` 使用无 `.0` 的普通 `{}` Display；`0.30000000000000004` 的最短 roundtrip），interp 复现该行为。这里的差异是第三票，而错误第三票比 skip 更糟。
- **错误投票比诚实 skip 更糟。** interp 无法忠实建模时（本 frame 无法赋值到绑定的 `mut` receiver 修改 — 例如 `mut` 参数，其 effect 被后端返回至调用者的 slot（C-132，issue #1022），或 record-field/临时值 receiver；与后端随附 musl-libm 在最后 ULP 上不同的 platform-libm 超越函数；非确定性 `fan.*`），返回带原因的 `Flow::Unsupported`，绝不猜测。
- **按名称而非家族弃权。** 当某 capability 的一种形状可建模、另一种不能时，abstain 必须指出缺失哪种形状，让台账记录可由人填补的缺口，而非放弃整个家族。`inplace.rs` 正因此分为三类：写回、`mut` 参数 receiver（#1022）、字节级 buffer writer（#1021）。
- **保持在 codegen 前的切点。** codegen 插入的 IR node（`Clone`、`Borrow`、`IterChain`、`ClosureCreate`、`RuntimeCall`、…）按构造为 `unreachable!`。若其中某个变得可达，说明切点移动了 — 修正边界，不要静默处理它。
- **skip 数据驱动且醒目。** 每个 skip 都是解释器自身的 `RunStatus::Unsupported`/`FuelExhausted`（或 harness 无法与另一分支匹配的 `StackExhausted`），并附原因记录。绝不向 harness 添加硬编码 skip-list。
- **fuel 必需。** 每个 eval step 消耗一单位（`DEFAULT_FUEL`）；深递归受 `MAX_DEPTH` 限制，即解释器声明的 C-196 栈阈值，并以 `StackExhausted` 结束（`Error: stack overflow`，exit 1 — 与 step fuel 不同）。对抗性循环必须以 `FuelExhausted` 结束，绝不能挂起。
- **显示契约从 codegen 导出，且须引用出处。** 修改 `value.rs` 显示时，指出所镜像的 `almide-codegen` / `runtime/rs` 精确行号（如现有注释所做的那样）。

## 原本注意事项

原本的独立性主张针对 codegen 前的切点及不共享目标 lowering pass，不表示独立解析源码的 oracle。更早阶段和 self-hosted stdlib 源码的共享仍明确保留。本 harness 不会因归两方门禁负责的 native/wasm 分歧而失败；interp 异议也可能是 interp 自身缺陷，而非仅后端缺陷。pool 边界的 body-first dispatch 与层内 helper call 的 bridge-first 区分保留。历史错误投票/崩溃、fallback 与 abstain 台账双向检查、独立的 fixture/pool fuel 与栈耗尽、诚实的 unsupported 结果均保留。匹配后端特殊行为的指示仍是引用的原本规则，不是“一致证明普遍正确”的主张。此处不执行任何代码或台账变更。
