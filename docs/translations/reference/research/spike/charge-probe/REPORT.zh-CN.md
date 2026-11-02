<a id="stage-1-charge-trace-preservation--first-empirical-run"></a>

# Stage 1 计费轨迹保持——首次实测

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](REPORT.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](REPORT.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](REPORT.zh-CN.md)

> 固定原文的非规范性历史读者快照。未修改原本仍为权威来源；不自动同步，未独立测试、重跑基准或证明当前安全性／性能。8月2–3日分支指示、等待merge GO、实现主张和测量均属历史，不是当前执行授权。旧auto-unwrap、可省clock语法和路径不是当前API指南。保留1000→50→3 ns校准历程及后续更正，不合并成一次测量。Stage 1同时列出不可比较native wall及广泛peephole主张，不能视为新的BCE验证。台账标题23项与后加tier、首次2/6与“初次失败2项”、11+2与6×2矩阵计数均保留为原文不一致。timeout-ω切片不等于通用effect replay；轮次间修改后的自测Dojo结果不是独立固定编译器基准。保留claim4/5全文适用限制。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/spike/charge-probe/REPORT.md)
> 结尾 List[Result] 范围注记与前文 tuple 已实现的后记并存；保留两个原文层，不予调和。

状态：**通过——9/9个可比较运行三点一致（2026-08-02）**
规范：[docs/roadmap/active/logical-time-implementation.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-implementation.md) §4
重跑：`ALMIDE_FUEL_PROBE=1 almide run fixtures/<f>.almd [--target wasm]`（使用此分支构建）

<a id="何を検証したか"></a>

## 验证了什么

对 logical-time-proofs.md 明确列为“不证明”的唯一前提——

> 两个 renderer 保持共享 MIR 的 charge 事件（同样事件、同样顺序、各一次）

——进行了实现和实测。在 IR→MIR 边界（函数入口 + 每个 LoopStart 之后，W1 放置）插入 `Op::Charge`，比较 native（生成 Rust + thread-local 计数器）与 wasm（导出的 mutable globals + guest 内 epilogue 打印）两条路径的 **(stdout, consumed, trace_hash)** 三点。轨迹对顺序敏感（wrapping `t*1000003+site`，site = 函数名 FNV + 函数内序号，与函数顺序无关）。

<a id="結果"></a>

## 结果

| fixture | 覆盖路径 | 判定 |
|---|---|---|
| loop | while 循环 | **MATCH** (1002) |
| recursion | 递归 fib(20) | **MATCH** (21892) |
| branch | if/else + match | **MATCH** (602) |
| strings | 字符串拼接循环 | **MATCH** (102) |
| mutual | 5000层相互尾递归 | **MATCH** (5002) |
| switch | 12-arm 密集 match（**br_table 路径**） | **MATCH** (1002) |
| fusion | 密集算术（**fuser 路径**） | **MATCH** (2002) |
| nested | 双重循环 | **MATCH** (2602) |
| recursion + `--wasm-opt` | **经过 Binaryen -Oz** | **MATCH** (21892) |
| list | 带 lambda 的 filter/map | native wall（v0 fallback）——不可比较 |
| bce | list.get 循环（BCE 路径） | native wall——不可比较 |

**零差异。** 最可疑的 renderer-local peephole（fuser、br_table 转换、bounds-check-elision、wasm-opt）均未丢失 charge，也未打乱顺序。

<a id="消費値の意味論的検証数字が正しいことの独立確認"></a>

## 消耗数值的语义验证（独立确认数字正确）

- fib(20)：调用树大小21891 + main 入口1 = **21892** ✓
- 循环1000次：入口1 + 经过循环头1001次（退出判断也经过）= **1002** ✓
- is_even(5000)：调用5001 + main 1 = **5002** ✓

<a id="既知の限界stage-1-スコープ外として明示"></a>

## 已知限制（明确属于 Stage 1 范围外）

1. **native wall 导致 probe 静默消失**：native v1 碰壁并回退 v0 时不输出 probe 行。正式实现需要把 fallback 检测接为 probe 失败。
2. **fan{} 的并行 native**：thread-local 计数器不合计 arm 线程消耗。Stage 3（race）需要 arm 串行化或 atomic + 顺序规范化；deterministic-bounds 的核心问题在此出现。
3. **trap 路径**：直接 proc_exit，不运行 epilogue，因此没有 probe 行（两条路径一样）。
4. 计量仅入口和循环头（粒度 v0）。可变成本（Dyn charge）未实现。

<a id="判定"></a>

## 判定

在此粒度与路径集合中，计费轨迹保持**成立**。已获得进入 Stage 1 正式实现（spec/wasm_cross fixture、charge certificate、property test）的依据。没有反例，但搜索空间为9个程序，并不穷尽。

<a id="昇格2026-08-02同-branch"></a>

## 晋升为门禁（2026-08-02，同分支）

把 spike 发现固定成门禁：

1. **wall honesty**：probe 中的 native v0 fallback 是硬错误（`src/cli/mod.rs`、`render_v1_native_or_fallback`）。静默未计量的运行在结构上已不可能。
2. **静态 certificate**：`charge_probe::{wasm,native}_charge_sites` + 顺序敏感的首次出现比较。`translation_validation::wasm_pattern` 的 Charge claim 也强化为 site 特定（明确拒绝被丢失的那一 charge）。
3. **常驻门禁**：`tests/charge_probe_test.rs`——一个测试覆盖动态三点 × 8 fixture + 静态 certificate × 8 + wall honesty × 2（`cargo test --release
   --test charge_probe_test`，约2.4s；没有 wasmtime 时仅跳过动态层）。

almide-mir 现有605个 lib 测试全部绿（增加 Op::Charge 未造成破坏）。

<a id="stage-2-垂直スライス--fanbounded-が両ターゲットで着地2026-08-02同-branch"></a>

## Stage 2 垂直切片——fan.bounded 在双目标落地（2026-08-02，同分支）

`fan.bounded(compute.ms(100)) { heavy(1000) } ?? fallback` 在 native v1 / wasm 两条路径运行，实证了**确定性边界**：

- **翻转点**：`heavy(1000)` 的 probe 实测消耗为1002 charge units（入口1 + 循环头1001）。`compute.us(1001)` 为 EXHAUST，`compute.us(1002)` 为 OK——两目标在**同一个1µs刻度点**切换（boundary.almd，门禁断言）。理论值与实测精确一致。
- 同时开启 probe 仍三点一致（含 bounded，consumed=503010／trace 一致）。
- 四类诊断按 ADR-0001 触发：裸 Int／混入 Duration／非 call 本体／未知单位（枚举封闭集合）。

<a id="実装形logical-time-implementationmd-からの差分"></a>

### 实现形态（与 logical-time-implementation.md 的差异）

- **提取式 desugar**：`fan.bounded` 提取为合成 fn `__almd_bounded_N(budget, args…) -> T`（enter → body call → exit），exit **持久保存判定**（$__b_verdict），调用方按标量读取。`bounded ?? fb` 是**融合形式**（Result 值从未存在的全标量 If），解决 native rung 没有 heap-Result ABI 的问题。裸 bounded 为 ResultOk/ResultErr 节点（wasm 可用；native 是既有 rung wall）。
- budget prims 为 `PrimKind::{BudgetEnter,BudgetExhausted,BudgetExit}`（scalar prim floor）。min-cap（EIP-150）使用 enter/exit 两个 op。fuel 从 i64::MAX 递减，probe consumed 为 MAX − fuel。
- `compute.*`/`duration.*` 为 checker 名义类型（防火墙）+ lowering 中擦除为 Int(ns)。

<a id="仕様からの-deviation--全件解消2026-08-02-深夜"></a>

### 与规范的偏差——**全部解决（2026-08-02深夜）**

1. ~~没有 metered-clone~~ → **解决**（T1-2，de7c2d20）：budget-only 模式仅计量 region + `__fuel` clone，在两个产物中机械检查非 region 路径 charge 为零。剩余：FuncRef 可达时 `__lambda_*` 全域计费（无法复制 table，已提升为规范注记）。
2. ~~没有饱和／负值 trap~~ → **解决**（T3-1/T2-5）：构造器 guard + S3 饱和代数，由 fixture 逐单位精确固定。
3. ~~单 call 限制~~ → **解决**（T2-1 块化、T2-2 Result arm、T1-3 裸形式）。
4. ~~无法截断不终止~~ → **解决**（T1-1）：check-and-return 严格截断。`while true` 本体被预算截断，可见窗口（直到下一 charge site）确定（内联扩大窗口，但双路径一致，fuel_trap_window 固定）。
5. ~~没有 UFCS 诊断／S6 gate~~ → **解决**（T3-2 / T3-3）。
6. ~~fan{} × budget 未定义~~ → **解决**（T3-8裁定：类型系统排除）。

<a id="stage-3-垂直スライス--fanmin-が両ターゲットで着地2026-08-02同-branch"></a>

## Stage 3 垂直切片——fan.min 在双目标落地（2026-08-02，同分支）

`fan.min(budget?) { arm; arm }` 在 native v1 / wasm 两条路径运行，**已证明的 lockstep ≡ (spend, index) lex-min 语义**成为实现：

- 胜者 = 消耗最少的完成分支（cheap(7)=~2 units 胜过 heavy(2000)=~2003 units）
- **同值按源码顺序**（两个 arm 的 spend 相同 → arm 0）
- budget 只改变候选集合（全灭 → 台账常量 Err → `??` fallback；筛选 → 剩余 arm）
- **胜者出现边界**：heavy(500) 的 spend = 502 units。`compute.us(501)` 全灭，`compute.us(502)` 时 arm 1 成为胜者——双目标同一1µs点（门禁断言）
- 开启 probe 仍三点一致（consumed 9018／trace 同一）
- legacy `fan.min(thunks)` 由 parser 重建旧 AST，并以**签名迁移提示发出 E027**（按设计修订，fixture 已更新）

<a id="実装形"></a>

### 实现形态

- 用 `outline_metered_arm`（与 Stage 2 bounded 相同 outliner）将每个 arm 变成计量区域。BudgetExit 除 verdict 外还**保存 spend**（$__b_spend／新 prim BudgetSpend），调用方逐 arm 读取。
- 胜者选择是**标量 if-value 的顺序 fold**（candidate = 未耗尽；better = 无胜者或 spend 严格更小，自然实现同值源码顺序）。`?? fb` 融合形式完全标量、无 Result 值，可通过 native rung。
- 无预算形式采用 i64::MAX 哨兵（无不终止 guard = 与 fan {} 相同的终止约定）。

<a id="stage-3-の-deviationstage-2-の-6-件に追加"></a>

### Stage 3 偏差（加于 Stage 2 六项之外）

7. ~~trap 保守~~ → **解决**（T1-1）：可见窗口之外的落败 trap 被视为“未曾发生”（fuel_trap_cut），窗口内则双路径同样失败（fuel_trap_window）。
8. ~~不跳过 Err~~ → **解决**（T2-2，fuel_race_err_skip）。

<a id="p0-修正-2-件2026-08-02同-branch"></a>

## 两项 P0 修复（2026-08-02，同分支）

1. **修复 fmt 破坏代码**：formatter wildcard 将 `fan.bounded` / `fan.min` 替换为 `/* unformatted */`，**删除了代码**（数据丢失类别）。新增格式化分支：bounded 内联，race 为逐 arm 换行的块。确认 roundtrip（fmt → 重解析绿）与幂等。
2. **实测重校准 CM-1（v0.1 → v0.2）**：草案1000ns/unit 相对实测**高估21倍**，违背 ADR-0001 D5 自己声明的范围（5倍）。参照测量：heavy(1000) = 1002 units，在 release 为47.0µs → 46.9ns/unit。固定为 **50ns/unit**（范围比1.07）。`compute.us(51)` 通过 bounded，`compute.us(26)` 让 race 出现胜者；新翻转点也双目标一致（门禁更新）。经验：“称为 ms”的诚实性系于一个常量，merge 前必须常驻 D5 校准 CI 门禁。常量有三处（charge_probe 常量／wasm BudgetEnter render／native BUDGET_SHIM），单一来源化放在正式实现 PR。
   **后记（v0.2 → v0.3）**：47µs参照测量本身也被污染——1002 units 的执行为µs级，经过ms级 process spawn 无法分辨。D5 门禁常驻的瞬间（T3-7），就以 ratio 0.05（约高估18倍）拒绝；heavy(100M) = 1e8 units 的 min-of-3 实测（native 0.25s / wasmtime 0.28s——**双目标均约2.5ns/unit**）重新固定为 **3ns/unit**。边界 fixture 用 ns 构造器加强到1 unit精度：bounded 恰为3006ns（=1002×3）、race 为1506ns（=502×3）时双目标同时翻转。声明时钟上1ns改变判定，是当前确定性主张的形式。两次错误校准（高估21倍 → 高估18倍）均漏过人工审核，只有门禁捕捉了两者。

<a id="t1-3--native-heap-result-abires-carrier2026-08-02"></a>

## T1-3——native heap-Result ABI（Res carrier，2026-08-02）

裸形式（`let r = fan.bounded(...)` / race，不带 `??`）打通 native v1。副产物是**一般返回 `Result[Int|Bool, String]` 的 plain fn + match 消费也打通 native v1**（台账预告的“整个 rung 前进”）。

设计：共享 lowering 把 Result 降为原始块（`Alloc DynListStr` + Handle + 地址运算 + Store/Load，tag@4/payload@12）；native 没有内存模型，无法逐 op 对应。选择 **native 路径专用识别 pass**（`native_result_rewrite.rs`）：检测固定模式的 materialize/consume 窗口，改写为五个 native-only prim（ResMakeOk/ResMakeErrStr/ResTag/ResOkScalar/ResErrStr），renderer 映射到 Rust `Result<i64, String>`（NTy::Res）。**wasm 路径一个字节也不变**（pass 仅在 native 流水线运行）。

- 验证协调：保留 CallFn Ptr result / DropListStr 的平衡（born/release 不变）；verifier 为 ResErrStr 增加与 Handle 相同的 alias 规则；被移除 result 块的 Alloc/Consume 成对消失。dead-op sweep 通过完整 read walker 排除误删。
- 裸形式将“整个 region+wrap 按自由变量 outline”（复用 T2-1）转为可追踪的 `let r = <直接 CallFn>` 形式；把 Block-tail call 作为 match subject，双路径均未追踪而碰壁（首次尝试已实证）。
- 裸形式也具有相同3006/3005ns边界 pin，即 wrap 不向 region 收取哪怕1 unit。
- 未支持（保留有意 wall）：Ok payload 为 String 的 Result（tag/payload 窗口与 Err 歧义）、Result 类型参数、带 mono 后缀的泛型 Result 函数。

<a id="fan-並列--budget-の裁定t3-82026-08-02"></a>

## fan{} 并行 × budget 裁定（T3-8，2026-08-02）

消除“native fan{} 是实际线程，fuel 计数器是 thread-local，两者结合怎样计数”的未定义状态。结论：**类型系统已经排除了这种组合**。

- **metered region 内 fan{}**：不可能。region body 按 pure 上下文检查（can_call_effect=false），`fan {}` 必须 effect 上下文（E007）。因此 metered region 在语法上不会跨线程，thread-local 计数器始终安全。checker pin：`t3_8_fan_parallel_inside_metered_region_is_rejected`。
- **fan{} arm 内 metered region**：已定义且确定。region enter/exit 在自身执行上下文内闭合（native 为自身线程计数器，wasm 为顺序全局），逐 arm verdict 无跨上下文观测。wasm 已实证（arm1=Ok / arm2=exhausted）。native 因 fan{}+effect-arm 在 v1 rung 外而 wall；v0 fallback 因缺少 budget prim 被如实拒绝（T3-1新增），处于“没有通往错误答案的路径”状态。native v1 fan{} rung 打通是分支之外的既有待办。
- 注：probe / --time-report 的 consumed 读取 main 线程计数器，故不含 native 实线程 fan{} arm 消耗（wasm 包含）。语义（verdict）不能观测此差异，因为观测需要 enclosing region，而上述已排除。

<a id="wave-1--fan-v2-表面統一2026-08-02同-branch"></a>

## Wave 1——fan v2 表面统一（2026-08-02，同分支）

race/bounded 成为 block head 后，解决剩余表面不一致（仅 any/settle 仍是 thunk-list）：

- **块形式**：`a() ?? b()`（原 `fan.any { a(); b() }`）/ `fan.settle { a(); b() }`——parser 脱糖为 literal thunk-list AST（内部名 `__any_block`/`__settle_block`），frontend lowering 规范化名称。**checker 及以下完全不变**，双目标立即可用。fmt 将内部名还原为块语法（已确认 roundtrip + 幂等）。
- **thunk-list 拼写 tombstone**（E027签名迁移提示）：`fan.any([...])` / `fan.settle([...])` 已删除。双参数 mapper 形式为“已声明、未实现（Wave 2）”专用诊断。合成节点是 legacy 形式唯一生成者。
- **迁移10个文件**：spec 8（wasm_cross 5 + lang 3）+ diagnostics fixed 2。读取各 pin 后迁移；fan_var_thunk_list（#599 var-bound list pin）随拼写失去对象，改记为“混合 arm 种类的 list 顺序确定性”pin。fan_pure_thunks 的 #514 pin（pure arm Ok-adapter）在块形式存续。新增2个 tombstone fixture。
- **移除 async/await 残骸**：删除 `ExprKind::Await` / `IrExprKind::Await` / `Decl::Fn.r#async` / `IrFunction.is_async` 及全部消费分支（optimize/codegen/interp/fmt/mir，约40文件）。从“语法写不出”提升到“AST/IR 无法表示”。

settle 块返回类型在 v1 仍是 legacy 的 `List[Result]`（fan-v2.md 的 tuple 契约已加入正式实现 PR 的 deviation 项）。

> **后记（2026-08-03）**：上面两点是 Wave 1 当时描述，此后均已解决。settle tuple 契约由 T2-4（真实 FanSettle 节点）实现，`let (ra, rb) = fan.settle { a(); b() }` 可用。mapper 形式 any/settle 由 T2-3 实现，**race 为 T7-1（同日按用户指示实现）**；矩阵全部单元现为 implemented（11）或按设计不存在（bounded/timeout mapper，2），`tests/fan_surface_matrix_test.rs`（6 head × 2 form 全单元门禁）机械固定此状态。误触发 thunk 迁移提示已在 T6-1 替换为逐单元回答的专用诊断。

<a id="tier-7--race-mapper-実装2026-08-03ユーザー指示でセル閉鎖"></a>

## Tier 7——race mapper 实现（2026-08-03，按用户指示关闭单元）

`fan.min(xs, f)` / `fan.min(budget, xs, f)` 已实现，填满 fan v2 matrix 最后一个声明单元。语义是块形式相同法则的动态扩展：arm i = `f(xs[i])`（pure 单参数 lambda，必须 Result；ok 参赛、err 自行失格）；index 为列表顺序；budget 逐元素；胜者为 (spend, index) 字典序最小；全灭为台账常量 Err。

- 实现形态：checker 将 mapper 返回直接固定为 **Result[T, String]**（自由 Err 变量导致 ConcretizeTypes 拒绝，这是实现中遇到并确定的规则）。lowering 为 while + list.len/get + 每元素 outlined metered region 的动态 fold（outliner 拆成 IR-body 版，lambda 参数作为自由变量成为 region 参数）。
- 验证：fixture `fan_race_mapper.almd` 的六探针（lex-min／同值→首索引／err 失格／全灭／1ns耗尽／裸 auto-`?`）在 wasm + interp 一致。native 因 list ops 超出 rung floor 而 honest wall（@xt-allow tracked，corpus 346 equal + 1 tracked / stale 0）。向 C-205 补充 mapper 条款。

<a id="tier-5--oracle-層の完遂2026-08-0203同-branch"></a>

## Tier 5——完成 oracle 层（2026-08-02/03，同分支）

不止于确定性 tier（bounded/race），同分支也落地了最初构想的 oracle 层（台账 T5-1..T5-4f）：

- **fan.timeout（T5-1）**：在 charge site 协作检查墙钟期限（中断点统一原则，直接采用 T1-1 check-and-return 截断）。判定相对 ω（R_Ω 契约类，C-208 + ALS-D5）。确定性的两端（宽裕期限→Ok／不终止+极短期限→Err）双目标字节一致。
- **ω record/replay（T5-2，claim 4 可执行证据）**：ω 为期限到达的 wall check 序数。native 用 `ALMIDE_OMEGA_RECORD=1` 记录，通过 `ALMIDE_OMEGA=<n>` **编译时固化**，使所有路径不读时钟即可重放。常驻门禁检查 record(native) → replay(native×2 + wasm) 字节一致。
- **dojo async 任务库 + 首次实测（T5-4，claim 5 材料）**：向 almide-dojo 增加6个 fan v2／确定性时间任务，用 cli:claude 实测。**6/6通过（含重试，首次2/6）**。初次失败全是诊断缺口或真实 bug，轮次间修复后收敛。

<a id="実測が炙り出して本体側で潰したものdogfood-ループの回収"></a>

### 实测暴露并在本体中修复的问题（收回 dogfood 循环）

1. **三项诊断**（dojo MH-003..005）：thunk 拼写误导“Missing ')'” → 迁移提示；`import compute` 的“创建文件” → 明确 auto-available；已 unwrap 值再 unwrap_or → “nothing to unwrap，将 `?? <default>` 放在 producing call”。
2. **checker 漏洞（T5-4f）**：对 scalar subject 使用 `some(..)/ok(..)/err(..)/none` 模式静默绑定 `Unknown`，直到 backend wall。改为类型错误 + auto-`?` 说明。
3. **wasm validate 破坏（T5-4d）**：let 绑定 budget 的时间类型标签到达 MIR layout，被误作 heap Ptr（值已擦除为 i64）。在 MIR newtype eraser 加入 Compute/Duration → Int。常见形式（把 budget 绑定到变量）整类无法编译；dojo 合格解没有此修复就不能通过。
4. **打通 native 路径（T5-4e）**：通过五块砖（result_fns 对 lift 盲区／NativeSigs 同样问题／err-abort window 的 Handle+Die／定义侧 `Ok(..)` 包装／interp timeout prim 路由）打通跨调用使用 metered effect fn。三方 corpus（native == wasm == interp）75/75绿，新增 fixture fuel_var_budget 为 C-204 evidence。

**Deviation 节为空**（Tier 5 无偏离规范的实现。v1 有意范围限制——ω 编码为首个 hit 的单一序数、settle 的 List[Result]——已明确写入契约和 roadmap，属于“规范侧记载”，不是 deviation）。
