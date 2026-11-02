<a id="branch-burn-down--worktree-stage1-charge-probe-の残件台帳"></a>

# 分支燃尽——worktree-stage1-charge-probe 剩余工作台账

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](BURNDOWN.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](BURNDOWN.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](BURNDOWN.zh-CN.md)

> 固定原文的非规范性历史读者快照。未修改原本仍为权威来源；不自动同步，未独立测试、重跑基准或证明当前安全性／性能。8月2–3日分支指示、等待merge GO、实现主张和测量均属历史，不是当前执行授权。旧auto-unwrap、可省clock语法和路径不是当前API指南。保留1000→50→3 ns校准历程及后续更正，不合并成一次测量。Stage 1同时列出不可比较native wall及广泛peephole主张，不能视为新的BCE验证。台账标题23项与后加tier、首次2/6与“初次失败2项”、11+2与6×2矩阵计数均保留为原文不一致。timeout-ω切片不等于通用effect replay；轮次间修改后的自测Dojo结果不是独立固定编译器基准。保留claim4/5全文适用限制。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/spike/charge-probe/BURNDOWN.md)

> 方针：**关闭全部项目**。每项都有完成条件，完成后记 `[x]` + 提交 SHA。新发现的剩余工作必须**先追加到本台账**再处理，不做台账之外工作。用户明确 GO 之前，不合并到 develop。
>
> **当前位置（2026-08-03）：23项，全部完成。** REPORT 的 deviation 节也全部解决。剩余手续仅为确认最终完整测试套件绿，并等待用户 merge GO（明确 GO 前不合并）。

<a id="tier-1--意味論の大物"></a>

## Tier 1——大型语义工作

- [x] **T1-1 strict cap + unwind（trap 可见窗口 + 不终止截断）**（以无需 unwind 的 check-and-return 实现：charge site 为“subtract → fuel<0 → 从该 fn 返回虚拟值”；W1 使连锁 return 收敛到 outlined fn 的 exit，通过常规路径保存 verdict/spend。两条路径 + interp（det_region_depth gate）同构。fixture：`fuel_divergence_cut`（while true 被截断后继续）/ `fuel_trap_cut`（窗口外落败 trap 不触发）/ `fuel_trap_window`（内联后进入窗口的 trap 双路径同样触发），均为 C-204/C-205 evidence。关闭 deviation 4/7。）
  **确定设计（无需 unwind、双路径同构 check-and-return）**：metered fn 的 charge site 设为“subtract → if fuel<0 → 从该 fn 返回虚拟值”。W1（所有循环经过 charge site）保证耗尽后剩余执行有限；region 调用链在各 fn 下一 charge site 连锁返回 outlined fn 本体，exit 沿普通路径保存 verdict=1（fuel<0）和 spend，无需 unwind 或标记。截断点为耗尽后首个 charge site，由 charge 放置保持性保证双路径相同。虚拟值不可观测（verdict=Err 丢弃值）。T1-2 使 budget-only 模式中所有带 charge 的 fn 均 metered，可在 charge render 无条件放置检查；probe 模式只从 fuel=MAX 递减，不会到 <0。
  native：`//__CUT_RET__` marker，确定末尾 ret-NTy 后 replacen 为 `if __almd_fuel_lt0() { return <default>; }`（JOIN marker 先例）。
  wasm：`(if (i64.lt_s (global.get $__fuel) (i64.const 0)) (then (return <typed default>)))`。
  interp：`det_region_depth`（enter+1/exit-1）>0 且 fuel<0 时，eval_while break + call_function 提前 return（虚拟 Value::Int(0)）。fixture：不终止截断（loop_forever 被 budget 截断）+ 两类 race 落败 trap 不可见。
- [x] **T1-2 metered-clone 特化**（de7c2d20——`specialize_metered_clones`：从 roots=`__almd_bounded_*` 出发的 CallFn 传递闭包复制为 `__fuel` 并重定向；仅 roots+clone 计费，probe mode 仍全部 fn 计费，probe 语义不变。native sig 表从 base 复制 clone，修复返回 Result 的 region 被错认 String。门禁机械检查两产物“非 region 路径 charge 为零”，带 vacuity pin。3006/1506ns边界不变，全部 metered fixture 绿。已知剩余：FuncRef 可达时 `__lambda_*` 全域计费，因为 table 不可复制。）
- [x] **T1-3 native heap-Result ABI（打通裸形式）**（1a63629e——native-only 识别 pass `native_result_rewrite.rs` + NTy::Res carrier。裸 bounded/race 在 native v1 绿，`bare_result` fixture 精确边界等同；**一般 Result[Int,String] plain fn + match 也打通**，differential 行 `result_fn_match`。wasm 路径完全不变。有意 wall：String-Ok payload / Result 参数 / 带 mono 的 generic。详见 REPORT。）

<a id="tier-2--意味論の中物"></a>

## Tier 2——中型语义工作

- [x] **T2-1 body/arm 完整块化**（2e26138a——parser 改为 parse_brace_expr；outliner 加入自由变量参数化通用路径，单调用保留原 args-as-params；fmt 支持 Block body。ADR parse→filter→render 示例在双目标+interp一致；bind 语句 charge 0，精确边界不变，3006ns pin 在块形式等同。race block arm 也可用。移除旧 single-call 诊断 fixture。）
- [x] **T2-2 Result arm 跳过 Err（race）**（1a63629e——checker 以 Result arm 的 Ok 类型 unify；fold 加入逐 arm is-ok match + candidate 合成。`race_err_skip` fixture：排除最便宜 Err arm／Ok Result arm 以 spend 胜过 plain arm／全 Err fallback／裸形式混合，均在双目标+interp一致。native 经 Res carrier 也绿。）
- [x] **T2-3 mapper 形式 any/settle（Wave 2 声明部分）**（6aa779fb——any 为 self-host `fan_any.almd`，4类型对 + 结构化早停；以内部名 `any_map` 全面接入 v0/wasm/checker，注册 `is_self_host_result_module_fn`，使 match/auto-unwrap 消费在 wasm EXECUTE。settle mapper 脱糖到 list.map，语义相同且继承既有限制。`spec/lang/fan_mapper_test.almd` 含 WASM 路径均绿。）
- [x] **T2-4 settle block 返回 tuple**（f0c3b900——真实 FanSettle 节点，涵盖 parser/checker/lowering/fmt；值为 arm 顺序 tuple 字面量，元素顺序求值实现顺序确定契约，也消除 v0 native 实线程交错 flake。异构 arm + effect arm Err 捕获（auto-unwrap OFF）+ tuple-pattern match（health_report 形态）双目标一致。5个既有 fixture 迁移到 tuple 契约。）
- [x] **T2-5 S3 运算矩阵**（24236472——checker interceptor 填补 Named 放行 generic numeric、导致 T*T 不警告的漏洞；饱和擦除为 + 向 MAX 饱和／− 向0饱和／×Int 负数 trap + MAX 饱和。`time_ops` fixture 在双目标+interp逐单位固定所有单元；`negative_scale` trap fixture；checker matrix 17单元。）

<a id="tier-3--小物半日圏"></a>

## Tier 3——小项（半天范围）

- [x] **T3-1 饱和运算 + 负值 trap**（5d8619bf——构造器擦除在 IR 合成 §13 abort 负值 guard + MAX overflow 饱和 guard；非负字面量折叠。`negative_trap`/`saturate` fixture 纳入双目标门禁。副产物：native v1 rung 加 `eprintln` shim + `ProcExit` arm，也消除 assert desugar native v1 wall。**发现既有漏洞**：含 budget prim 的 v0 fallback 以 rustc E0425 死亡；改为明确诊断拒绝，检测 `almide_rt_prim_budget_`。differential corpus 增2行。）
- [x] **T3-2 UFCS 歧义诊断**（1a81dc1a——E002 路径检测单位名，指出两种时钟候选；旧提示误导到 `int.abs`。matrix gate 加 S6-3 案例。）
- [x] **T3-3 S6 matrix gates**（e9bf6eef——在 `almide_types::time_units` 单一来源化单位表、时钟表、S4时钟列；checker/lowering/诊断 hint 均读同表。`tests/time_units_matrix_test.rs` 包含 S6-1/4/6 + 裸 Int 门禁，案例由表生成。S6-2/3/5 在 T3-1/T3-2/T4-6 落地。）
- [x] **T3-4 interp 支持 budget prim**（6102bdba——interp 实现确定性计量器：user-fn entry / while n+1 / for-in n+1 / closure 调用的 W1 镜像 + budget 四重奏，经 RuntimeCall arm，因为 fan lowering 直接发出 `almide_rt_prim_budget_*`。含逐单位精确边界扫描的全部 metered fixture，第三票与 backends 一致。CM-1 移至 almide-types/time_units，真正只定义一次。）
- [x] **T3-5 Dyn charge**（`Op::ChargeDyn`——按 `__str_concat` 结果长度计费 `1 + len/16`，result-keyed 使双路径一致为构造性事实。两 renderer + interp mirror + 相同 strict cut/trace 规则。`fuel_dyn_charge` 逐单位精确固定理论252 units消耗，750/760ns翻转、三方一致。）
- [x] **T3-6 CM-1 常量单一来源化**（13135f61——wasm render 用插值、native shim 用模板注入，共用唯一 `CM1_NS_PER_CHARGE` 定义。集成门禁增加 artifact 检查。）
- [x] **T3-7 D5 校准门禁常驻**（9c3d78a7——刚常驻就以 ratio 0.05 检出 v0.2 污染测量。CM-1 v0.3重校准为3ns/unit，边界 fixture 加强为ns精度精确翻转，bounded 3006ns / race 1506ns，双目标相同。）
- [x] **T3-8 fan{} 并行 native × budget 裁定**（3ecf61dd——类型系统已排除：region=pure × fan{}=必须effect（E007），region 无法跨线程。反方向逐 arm 自闭合、确定；wasm 已实证，native 为 honest wall/拒绝。checker pin + REPORT 全文裁定。）
- [x] **T3-9 报告显示 `--time-report`**（90ca1acd——`almide run --time-report` 向 stderr 输出 `time: 0.151ms deterministic (≈38.7ms wall here)`。吞掉 probe 行；门禁固定双目标确定性时间一致。）

<a id="tier-4--merge-準備branch-上で先行pr-は-go-後"></a>

## Tier 4——合并准备（先在分支进行，GO 后 PR）

- [x] **T4-1 更新 CHEATSHEET**（b3dd3e60——“Concurrency & deterministic time (fan)”节 + 时间构造器封闭集合 + 五行 DO NOT：async/await、thunk-list、裸 Int、虚假单位、时间字面量。）
- [x] **T4-2 重写 SPEC.md §13**（63159a24——§13.2 block heads / §13.3确定性时间 / §13.4 bounded / §13.5 race + 13.1/13.2矛盾后记，作为 T9 定理 / §13.6 rules。同步 language.md §5.17。）
- [x] **T4-3 C-NNN 契约正式化**（d81cc2d8——C-202..C-207六契约 + 新 ALS 章 `deterministic-time.md`，ALS-D1..D4。10 fixture 移到 spec/wasm_cross，@contract 双向，check-contracts.sh 绿，207契约/340 fixtures。CM-1 在 C-207 成为版本化对象，D5 gate 为 by-construction evidence。合并时若编号冲突则重新编号。）
- [x] **T4-4 docs/stdlib 的 compute / duration 页面**（6471c8ef——两页面 + CHEATSHEET 表两行。docs-gen counter 和 signature-index 生成器都增加“时钟页面＝checker 表面”的例外，Rust 直接读 TIME_MODULES。）
- [x] **T4-5 roadmap 状态协调**（be4c13d2——在 fan-v2.md / logical-time-implementation.md 增加实现状态块，明确“若有差异，以 BURNDOWN/契约台账为准”。）
- [x] **T4-6 诊断 fixture 全覆盖**（c50134cb——8 fixture：裸 Int／混入 Duration／非 call 本体／未知单位／pure 上下文(E007)／effect arm(E006)／T×T／UFCS 裸单位。同时给 metered-region 上下文 E006 专用措辞，去除“改成 effect fn”的循环误导。）

<a id="tier-5--oracle-層インセプション完遂2026-08-03-にスコープ拡大で編入"></a>

## Tier 5——oracle 层（完成 inception，2026-08-03扩展范围纳入）

- [x] **T5-1 fan.timeout（Stage 4）**（f8d760ba——FanTimeout 节点 + 复用 T1-1/T1-2，charge site wall check 使用相同 check-and-return 截断。min-cap 嵌套、E027签名迁移，legacy 双参数／无块形式通过 AST 重建触发；专用“expected Duration, found Compute”；S6-6首个 Duration 行。`fuel_timeout_ends` 确定性两端，完成 Ok／不终止+极短期限 Err，双目标一致。契约 C-208，R_Ω 类 + ALS-D5；更新 CHEATSHEET/SPEC §13.6/duration.md。）
- [x] **T5-2 B1 record/replay（claim 4）**（f8d760ba——ω **编译时常量固化**，无需 env 管线。native 用 `ALMIDE_OMEGA_RECORD=1` 记录 `__ALMD_OMEGA <ord>`；`ALMIDE_OMEGA=<n>` 使所有路径不读时钟、按序数截断。门禁 `timeout_deterministic_ends_and_replay` 常驻检查 record(native)→replay(native×2 + **wasm**) 字节一致，作为 claim 4 可执行证据。v1 ω 编码为首个 hit 的单一序数，多 region 泛化在契约中明确。）
- [x] **T5-3 effect 表面期限类型 pin**（f8d760ba——ADR S4 行已存在；ALS-D5 + docs/stdlib/duration.md 明确“effect 表面期限保留同一类型，未来实现”。S6-6表机械强制未来增行，未声明的 Duration 消费表面会 panic。）
- [x] **T5-4a test 块 × budget 路径整备**（af8041a5——(a) budget/timeout prim 流入 native test harness 时以“在 WASM test 路径运行”明确诊断拒绝，消除不透明 E0425。(b) bind-first 在 wasm test 路径绿，确认 bounded/race/timeout 三探针。inline call subject assert 的 untracked wall 作为已知形态写入任务记法约定。）
- [x] **T5-4 dojo async 任务库 + 首次实测（claim 5）**（dojo 09fb90d=6任务，21dbf08=实测记录。**6/6通过，含重试 MSR 100%，首次2/6**：bounded-total r0 / settle-health r0 / deadline-guard r1 / any-candidates r1 / race-strategies r2 / budget-units r3。初次失败2项均不是模型原因，而是诊断缺口或真实 bug，在轮次间修复本体，MH-003/004/005 + T5-4d/T5-4f，再测后收敛。budget-units 合格解是无 T5-4d 就无法编译的 let 绑定 budget 形式；dogfood 循环双重达成目的。也修复 harness datestamp bug，原样穿透的“YYYY-MM-DD”字面量改为 %Y-%m-%d。记录：dojo runs/2026-08-02/summary.md + malicious-hints.md MH-003..005。）
- [x] **T5-4b auto-available import 误导提示**（3078f557——实测第2轮发现。`import compute` 被提示“Create compute.almd”，尽管是 auto-available checker surface。resolve.rs 的 TIME_MODULES 分支 + parser Fan token 分支使三路径都提示“'X' is auto-available — Remove the `import X` line”。诊断 fixture：import-auto-available / import-fan-auto-available。）
- [x] **T5-4d let 绑定 budget 破坏 wasm validate**（3078f557——深挖同轮发现而暴露的真 bug。`let budget = compute.ms(ms)` 的变量类型以 `Ty::Named("Compute")` 到达 MIR，layout 误作 heap Ptr，即 i32 slot。值已在 lowering 擦除为 i64，故 validate 报“expected i64, found i32”。修复为**给 MIR newtype eraser 添加 Compute/Duration → Int 种子**。frontend expr_ty 擦除会破坏饱和代数 dispatch，因此不可用，作为已考虑的死路记录。inline head 会 const-fold，故此前隐藏。fixture：spec/wasm_cross/fuel_var_budget.almd，加入 C-204 evidence，并向 statement 添加变量间接条款。）
- [x] **T5-4e 打通 metered effect fn 调用的 native 路径**（3078f557——T5-4d fixture 暴露既有缺口链，通过五块砖打通：(1) result_fns 漏掉 lifted effect fn，声明 ret Int|Bool + is_effect，consumer window 未 rewrite；verifier 误把原始 LoadHandle 判为 UseAfterFree。(2) NativeSigs 同样对 lift 盲区，将 CallFn dst 错定为 I64。(3) err-abort window 的 Handle(msg)+Die 在 rung subset 外；native render 实现 Str-alias Handle + Die，stderr+exit 1，wasm $__die 的双胞胎。(4) lifted fn 定义仍 raw scalar；render_fn return seam 包装 `Ok(..)`，签名也 Result 化。(5) interp RuntimeCall guard 只接受 budget_，不接受 timeout_，使 fuel_timeout_ends interp panic；扩展 guard。结果：native probes / fuel_var_budget 的 native==wasm 字节一致；wasm_runtime_test 75/75，三方 corpus 绿。render wall 也附上 fn 名。）
- [x] **T5-4f scalar subject 的 variant 模式绕过 checker**（fb095282——实测第3轮发现。`let r = fan.bounded(b){...}` 已 auto-`?`，r: Int，对它写 `match r { some(v) => v, _ => -1 }`；`some(v)` 即使 Int subject 也无类型错误，静默 `_ => Ty::Unknown`，加 `_` 后绕过 checker，直到 wasm wall，使诊断退化到最下游。拒绝 some/ok/err/none × 具体 scalar subject，提示“pattern `some(..)` cannot match Int”+ auto-`?` 说明 + `?? <default>`。Named/TypeVar/Unknown subject 保持原状，对 alias/cascade 安全。fixture：variant-pattern-on-scalar。spec 325/325绿。）
- [x] **T5-4c 已 unwrap 值缺少 unwrap_or hint**（3078f557——同轮发现。effect fn 中 `fan.bounded(){...}` 已 auto-`?`，值为普通 T，但模型再套 `option.unwrap_or`/`result.unwrap_or`，在 E005“Fix the argument type”迷失。针对 unwrap 家族 × 期待 Option/Result × 非容器实参，加入“nothing to unwrap，值已为 T；fallback 应在 producing call 用 `?? <default>`”提示。fixture：unwrap-already-plain。）

<a id="tier-6--残滓掃討2026-08-03-スコープ拡大残りも全部つぶしてで編入"></a>

## Tier 6——残留清理（2026-08-03以“剩余也全处理”扩展范围纳入）

- [x] **T6-1 fan.min mapper 单元规范化**（起初写“通过实现关闭”，但**按已批准 fan-v2 设计修订**：race mapper 设计状态为“已声明，等待实际使用”，这是反证条件中明确的 fallback；截至当时实际用途为零。点状实现也违反 API 完备性操作原则，即门禁固定面、不单加点。真 bug 是 **thunk 迁移提示误触发正规 mapper 拼写 `fan.min(xs, (x) => …)`**。修复：检测末尾单参数 lambda 为 mapper 拼写，提示“declared but not implemented (Wave 2 keeps the cell open until a real use appears)”，引导到块形式 / list.find_map(xs, f)。bounded/timeout mapper 也给矩阵答案“has no mapper form”，说明按设计不存在及组合惯用法。未来实现单元时，T6-2门禁强制同 PR 更新。）
- [x] **T6-2 新建 fan head×form matrix gate**——`tests/fan_surface_matrix_test.rs`：6 head × 2 form 的13 fixture，9实现单元必须 type-check，4不存在单元必须给准确矩阵回答文本。fan-v2.md 反证条件注明门禁生效。
- [x] **T6-3 更正 REPORT 过时描述**——向 Wave 1 两点，即 mapper 未实现／settle List[Result]，追加后记，保留历史并明确现状：any/settle mapper 已实现、settle tuple 可用、race mapper 是门禁固定的声明单元。
- [x] **T6-4 消除 inline call subject assert 的 wasm test 路径 wall**（eee33f6a——根因两层：assert desugar 将 call 留在 if 条件；**MIR 阶段 hoist 晚于 IR 阶段 never-err/auto-wrap ABI 分类，错过 rewrap 机制**。修复：IR 预处理阶段新增 `hoist_assert_call_subjects`，与 hoist_block_call_args 同族，**限定 Result 类型且含 call 的 operand**，hoist 为 bind-first。无差别 hoist 会在 Var 路径类型范围比 inline materialize 更窄时使 map_higher_order_test 等退化，这是经验。`assert_eq(pick(5), ok(5))` 绿，spec 325/325、路径分配316/9恢复，corpus 75/75、charge_probe绿。T5-4a 的 bind-first 记法约定由机制实现而不再必要。）
- [x] **T6-5 workspace build warnings 归零**——精查后**分支来源 warning 为零**；blame 全样本在 develop 已有，如 TestWhere=780c8341、gap_blanks=a1860635。develop 既有91条另走独立路径，避免污染本分支合并范围，见下方 Branch 外。
- [x] **T6-6 验证普通 lifted effect fn 到达 native v1**——**已确认打通**：`effect fn f(n: Int) -> Int` + main 调用，never-err lifted、非 metered，无 fallback note，直接 native v1 render，是 T5-4e 五块砖的副产物。can-err 形式，fs.read_text + auto-`?`，在既有 trust-spine brick 即 variant match in tail 如实 fallback；为另一条路径的已知余项 project_v1_mir_trust_spine。

<a id="tier-7--race-mapper-実装2026-08-03ユーザー決定でセルを閉じる"></a>

## Tier 7——race mapper 实现（2026-08-03，用户决定关闭单元）

- [x] **T7-1 实现 fan.min mapper 形式**（e7046fe1——按用户指示以实现关闭单元。`fan.min(xs, f)` / `fan.min(budget, xs, f)`：FanRaceMap AST + checker G3 arm，检查 E007 / budget clock / List[X] unify / mapper **直接固定 Result[T, String]**；从自由 Err 类型变量会留下 Result[Int, Unknown] 到 lowering 并遭 ConcretizeTypes 拒绝的陷阱中学到这一点。动态 fold lowering 为 while + list.len/get + 逐元素 outlined metered region，将 outliner 拆成 IR-body 版 `outline_metered_arm_ir`，lambda 参数按自由变量转为 region 参数。支持裸形式与 `?? fb` 融合，fmt roundtrip 幂等。
  验证：fan_race_mapper.almd fixture，lex-min 300／同值→index 0的7／err失格8／全灭-1／1ns耗尽-1／裸 auto-`?` 40，wasm+interp一致；native 因 list ops 在 rung floor 外 honest wall，@xt-allow跟踪，346 equal + 1 tracked / stale 0。test 块路径绿，含 inline subject／空列表。矩阵门禁已迁移单元：implemented 11 + 设计不存2 + 非 Result mapper 契约错误。C-205 statement 加 mapper 条款及 evidence，347/347双向。更新 fan-v2 matrix／CHEATSHEET／SPEC §13.5。已实测 MSR：dojo `race-cheapest`（cdf8edc）**通过（retry 1**，唯一失败是 `import fan`，T5-4b新诊断立即促成修复）。async任务库7/7。）

- [x] **T7-2 同步 async-inception / async-world-claim 状态**——T4-5范围外两篇仍以“未来”口吻过时。开头追加实现状态块，历史不变：Lane 1全部 + B2 + D(Almide侧) + race mapper完成；**B1仅 timeout-ω 切片**，effect响应带 fs/http/random尚未开始。claim 4全文须 B1完成后才解禁，目前限定为“timeout事件的确定性复现”；claim 5全文仍需同模型 lang-bench(async)比较；C(KPN)按设计仍在远景。也明确“有差异以 BURNDOWN/契约台账为准”。

- [x] **T7-3 修复 develop CI（Trust Spine / CI）红灯**（cf99b52b + b1ca8277——merge后 Trust Spine 首次运行本分支暴露三项：①`fan_any` 未列入 stdlib purity registry，T2-3遗漏，确认 keyword-pure 后加入 PURE_MODULES。②**PCC corpus-wall 拒绝**：scalar if-value merge dst 流出为外层分支值时，变为 released-merge 对象，cert `i{m|}`/`i{|m}`，单侧move；binary verifier 不跟踪scalar而accept，Coq已证明checker却reject。Stage 3潜伏的静态race fold共27对象与新mapper fold均受影响。修复：fold AND/OR 平坦化为 **0/1算术**，乘法 / a+b−ab；**cert模型将released-merge限于heap merge**，merge_dst_i_credits同过滤保持lockstep。ownership 45063对象ACCEPT + Rocq kernel全认证。③spanless-wall ratchet +2来自fn名装饰器两处字面量mention，改为 `Self::Unsupported` 拼写的 `with_fn_context` 方法。副产物真bug修复：**free_vars未登记While/ForIn body内Bind绑定**，loop-local误作自由变量，outliner生成虚幻param。spec 325/325、corpus 75/75、mir 605、charge_probe/matrix均绿。）

<a id="branch-外残り--参照のみ"></a>

## 分支之外（剩余，仅供参考）

Rung 1输出transactional / AARA移交其他路径台账。ADR-0002执行顺序批准需用户讨论，故不列入本台账。

- **develop既有build warnings 91条**（T6-5精查副产物）：全部早于分支，blame已确认，TestWhere=780c8341 / gap_blanks=a1860635 / NTy privacy=17e493ba等，37文件分支未碰。此分支处理会污染合并范围，应在develop单独做卫生清理提交。
- **can-err lifted effect fn的native v1**（T6-6余项）：variant match in tail的trust-spine brick，project_v1_mir_trust_spine已知剩余。如实fallback，行为正确。

<a id="完了の定義全体"></a>

## 总体完成定义

台账Tier 1–4全部`[x]`，REPORT.md deviation节为空或仅“提升为规范”注记，整个workspace cargo test + 全spec + charge_probe集成门禁绿。在此状态请求merge GO。
