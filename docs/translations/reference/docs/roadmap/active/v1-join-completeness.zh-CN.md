历史描述：通过 bind 位置 join 消除 continuation 复制，并使已证明 checker 对 witness 大小呈线性。
<a id="v1-join-completeness--linear-checkers"></a>

# v1 join 完备性与线性 checker

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-join-completeness.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-join-completeness.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-join-completeness.zh-CN.md)

> 未改动固定原本的非规范性历史读者快照。不自动同步，未独立重跑benchmark或验证实现。日期、测量corpus/机器条件、已落地slice、未完步骤、失败尝试均保留原范围；形式模型主张不把有限corpus检查提升为普遍实现安全证明。保留certificate ACCEPT与所述覆盖/泄漏并存，不改成所有生成代码的保证。区分排序fast path与二次fallback、被gate限制的witness与无约束输入、信任前提不变与扩展准入。历史命令、CI gate、评审人数、commit顺序只是记录，不是新执行指令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-join-completeness.md)

状态：**Track C 已落地 C1/C2/C3/C6；Track J 已落地 J0/J2a/J3（2026-07-28）。
J1（非 variant 扩展）、J4、J5 和 C4/C5 未完成。**
三项独立代码调查完成（join 机制图、复制清单、cert 文法及 checker 复杂度）。
源于 2026-07-27 trust-spine 事故。

J 实测：事故 fixture 的8链 `main` 恢复为一个函数，线性 lowering。
names witness 231,114B → **4,002B**（约58倍），每个链式 fan.any 增长+480B
（原为×3.5乘法增长）。全部 fan fixture native/wasm 字节相同（#900 pins），corpus walls 0，
ratchet clean，kernel oracle 74s，`cargo test -p almide-mir` 597项green。

C-track 实测（本地 M-series、完整 corpus）：
- 事故类231KB names witness：**超过10分钟（未完成）→7–9s**
  （sorted merge 路径；未排序 FALLBACK 设计上仍二次，64KB/行 gate 将此类挡在CI外）。
- kernel oracle：**254s→74s**；corpus-wall.sh 总计：**5m20s→2m16s**。
- 全corpus witness bytes：names.cert −30%（1,351KB→939KB），最大行−33%，caps_graph −24%
  （sort+dedup，集合语义健全）。
- proof spine + coqchk + claim-drift + gate.sh + `cargo test -p almide-mir`（597）全部green；
  `merge_subset_sound` / `subset_check_fast_sound` 加入公理台账，已内核检查。

<a id="the-incident-that-named-the-problem"></a>

## 为问题命名的事故

`457273df` 加入 `fan_any_early_winner.almd`（同一 `main` 中8次连续 `fan.any`）。
fan inliner 每链一层复制 match 的 ok 主体——函数整个剩余 continuation；连续调用叠加增长：
尾部约1,700副本、6,820个heap对象、**231KB names witness**（corpus中位88B）。
已证明checker在单个witness行上超线性，所以[names]从13s→53min，kernel oracle停在4h之后；
后续push整天全部CANCELLED（从未red）。临时措施落地 `30ea58de`
（fixture每函数一个case、corpus-wall.sh中 `MAX_WITNESS_LINE=65536` gate、
`timeout-minutes: 100`）。本轮去除两个根因：

1. **lowering复制continuation**——8种rewrite这样做（最坏实测约3.5倍/链式调用），
   因为绑定到let的heap分支没有join方案。其中两种在 `MAX_DESUGARED_NODES` fixpoint之外运行，无上限。
2. **已证明checker对witness bytes超线性**——Θ(n²)解析、单进nat上的Θ(k²·V̄)子集检查，
   以及一个真正指数级的可达性fold；单个大而合法的witness就能杀死CI。

得出结论前测过所有无join重编码：arm-inlining约2–3倍/call，
`let $r = <chain>; match $r` 约3.5倍/call（经 `desugar_let_bound_heap_branch`），
带guard的var重赋值语句约2倍/层（`desugar_unit_if_heap_reassign` 流入同一尾部复制）。
复制无法通过desugar消除，必须通过扩大准入消除。

<a id="the-key-finding--the-join-already-exists-the-wall-is-admission-surface"></a>

## 关键发现 — join 已存在，墙在准入表面

调查中三个关键事实：

- **cert模型已支持join。** `Op::IfThen{cond, dst}` 是产值merge；各arm的 `im` 平衡已证明
  （brick 5a，`check_bc_unroll_sound`，“Closed under the global context”）。
  更好的是 `certificate_b.rs` 已为RELEASED merge dst记一个 `i`
  （`ownership_certificate_released_merge_dsts` / `merge_dst_i_credits`），
  所以“绑定merge、scope结束drop”现在就能认证为 `im`/`id`。
- **bind位置join已对variant类型生效。** `lower_bind_heap_if` 允许 `is_variant_ty(ty)`
  （Option/Result绑定会join并获scope跟踪）。墙提示“merged result在flat certificate中没有健全scope-end drop”
  对写下之后落地的机制已过时。String/List绑定受阻在准入，不在cert模型。
- **renderer领先MIR。** wasm把 `IfThen dst` render为 `(if (result i32/i64) …)`，
  已有产值N-way `br_table` join（`render_wasm_switch.rs`，纯render rewrite）；
  native有if-as-value backpatch join。lowering以下无需改变。

所以不是“引入join point”，而是“扩大bind位置join准入，直到每个复制desugar的trigger集合为空，再删除它”，
每步由corpus ratchet约束。

<a id="constraints-violating-any-of-these-is-a-design-bug"></a>

## 约束（违反任何一条都是设计bug）

- **`mir == ir` / 两者之前desugar。** `desugar_heap_branches` 为 `lower_body_into`、
  `count_ir_calls` caps gate和pre-TCO共享。任何trigger收窄须在三调用点同样应用。
- **TCO使用尾部复制。** `mod_c.rs` pre-TCO依赖将continuation（含递归调用）推入arm，
  形成 `tco_collect` 处理的分支递归。J5之前，含自身递归tail call的continuation必须保留复制路径。
- **CBranch二元，arm主体平坦。** 不平衡arm内嵌套region分隔符会污染为始终拒绝的 `{i|}`（设计如此）。
  join扩展须让每对象各arm事件平坦且平衡，否则延期。
  （另外 `parse_bc` 将 `{}` 内第二个 `|` 幂等处理，伪造 `{A|B|C}` 解析成
  `CBranch A (B++C)`。emitter不会产生；既然触碰parser，就加卫生性拒绝。）
- **不加新公理。** `Print Assumptions` = “Closed under the global context”有claim-drift gate。
  `PrimString`（O(1)字符串prim）从 `PrimStringAxioms.v` 导入真正 `Axiom`，本轮拒绝。
  记录在此，免得有人再次把它当快速成果。
- **每次cert表面变化都对抗评审**（#49 / elided-call前例）：
  任何触及certificate生成的J阶段commit前，至少3名独立agent尝试构造accept-but-unsafe。

<a id="track-j--join-admission-kill-the-duplication"></a>

## Track J — join准入（消除复制）

**J0 — 安全brick（无行为变化）。上限已完成2026-07-28。**
`desugar_tuple_empty_list_match` 和 `desugar_list_pattern_match`
（都在200k fixpoint cap外运行，是两项无上限复制器）现有增长上限：
输入基础上+50k nodes，超过则丢弃rewrite并诚实wall该match，仿desugar_heap_branches。
`parse_bc` 第二条bar卫生拒绝仍未做（外观问题，emitter不产生，健全性定理对解析结果量化）。

**J1 — 非variant heap类型的bind位置join。** 用released-merge-dst记账机制解除
`lower_bind_heap_if` / `lower_bind_heap_match` 对String/List/typed merge的墙
（variant类型前例的一般化）。绑定merge以 `IfThen dst` join，scope跟踪，scope结束drop
→ cert行 `im`/`id`。必须对抗评审。gate：corpus walls保持0；所有权计数可变
（arm变为各自平衡），只要ACCEPT保持就健全。部分slice随J3在2026-07-28落地：
VARIANT match-bind join，`lower_bind_heap_match` 获得 `if` 对应项已有的variant路径
（tail位置信任的同一match-VALUE router：custom-variant / variant-value / Result / Option merge），
bind + scope跟踪 + `seed_variant_param`。无新cert表面（既有tail-trusted router）。

> **尝试1被反驳（2026-07-28）——没有以下两个修复，不要重试。**
> 实现全面扩展后被强制对抗评审否决：3名独立审计者中的2名因不同缺陷返回REFUTED，
> 固定复制路径的5个lowering单测失败。具体尝试是删除两bind点 `is_variant_ty` gate
> （`lower_bind_heap_if` 对任意heap类型运行一般merge join；`lower_bind_heap_match` 获得
> `try_lower_list_match_value` + `desugar_match_to_if` → `try_lower_heap_result_if` fallback），
> 再在 `desugar_branch_b.rs` 加 `bind_join_lowerable` 谓词，允许scalar cond下
> {LitStr, heap Var, ConcatStr, 此类嵌套if}的arm，接入
> `desugar_let_bound_heap_branch` trigger和 `rest_branch_binds` counter以停止复制。
> 没有任何内容commit。根据本段及下述两修复重新推导。
>
> **缺陷1 — 活变量覆盖（潜在，被改路由暴露）。**
> 允许bare-Var else arm后，非遮蔽绑定（`let b = …; let j = if c
> then "A" else b; b + "/" + j`）可到达identity-else accumulator slot fold
> （`try_lower_line_cond_acc`），其guard只检查else arm是owned、受scope跟踪的Var，
> 不检查binder是否SHADOW它。fold原地重绑 `b` 槽，后续读 `b` 返回错误block：
> native `sB/A`、wasm `A/A`，两者exit 0，所有权cert ACCEPT。
> 已验证develop不可达（复制desugar先截获），故出货内容没坏，但任何未来改路由都可能触雷。
> 现于fold点加routing不变量注释和常设cross-target pin
> （`spec/wasm_cross/heap_result_if_bind_chain.almd::clobber`，两链；
> 四链以上dense branch-lift将各bind outline而隐藏问题）。
> 已修复2026-07-28（J1前提）：fold现有LAST-READER gate。
> `LowerCtx::var_read_counts` 统计最终完整主体内每变量读取次数
> （在 `cell_vars` 旁、能见全体的位置计算；逐语句lowering自身看不到后续读取），
> `try_lower_line_cond_acc` 仅当accumulator全主体read数等于本次bind值内read数时fold，
> 即此bind是最后reader。正是源码shadow rebind所给条件，所以堆叠shadow accumulator
> （porta `serialize_opts`）仍原地fold，其他binder辞退到普通路径。
> 前提现在被检查，而非偶然由 `desugar_let_bound_heap_branch` 先截获这些bind维持——
> 那正是J1会改变的事情。
>
> **缺陷2 — 嵌套heap类型缺少merge-dst drop class（新）。**
> 扩展把merge dst推入 `live_heap_handles`，却不注册drop CLASS；只有variant获 `seed_variant_param`。
> 所以 `List[String]` merge得到flat `Op::Drop`，释放外block却泄漏所有元素String。
> 两审计者独立复现（N=2M时native有界105MB，对wasm无界2.88GB；固定
> `--max-memory-size` 下trap）。所有权cert ACCEPT，因为其 `d` 无法分辨flat和递归drop。
> 改前此形状WALLED，因此扩展把诚实拒绝转为certified leak。
> 修复：两join点按结果类型注册merge dst drop class
> （仿 `seed_call_module_heap_drop_route` / `register_owned_heap_eq_drop`），
> 或在落地前将扩展限于flat-drop-exact类型（`String`）。
> 不可分离——随J1一起落地，不先做（实测2026-07-28）。
> 已有variant join没有此缺口：`Result[List[String],
> String]` merge bind在4MB wasm cap内churn 100k次并得正确值，
> 因 `seed_variant_param` 已路由递归drop；更深的
> `Result[List[List[String]], String]` merge则WALL而不lowering。
> 所以当时不存在drop class缺失且可达的形状；此刻注册会是不可测试的dead code。
> 诚实顺序是与允许需要它的类型同一变更中加入调用，再由这些类型的churn probe验证。
>
> **过程注记（刻意保留）：** corpus-wall GREEN、walled-real ratchet CLEAN、
> 所有既有cross-target fixture通过、已证明checker对两缺陷都ACCEPT。
> 只有对抗评审抓到它们。这就是把它保留为gate而非建议的全部理由。

**J2 — 收窄 `desugar_let_bound_heap_branch`。J2a完成2026-07-28。**
`is_first_ok_chain`（与chain builder同在desugar_fan.rs）仅识别 `first_ok_chain` 发出的形状。
尾部复制trigger和rest-counter都排除它，于是chain bind到达variant bind位置join而非复制
（辞退前实测×3.5/call）。谓词刻意严格，手写形状不会触发，仅随J1准入同步扩展。
一般收窄（普通branch bind共享join-admissibility谓词）仍待J1。
TCO例外（continuation含自身递归tail call则保留复制）适用于那一步，不适用于chain形状
（fan chain的arm不含外函数递归，仅 `ok($x)`/嵌套chain）。
ratchet：corpus上的duplication-fire counter像walled-real baseline一样入库，计数只能缩小。

**J3 — fan.any线性重落地。完成2026-07-28。** 删除 `inline_match_over_any` 和#900逐层fresh-binder机制；
match-over-any形状现通过 `first_ok_chain`（与value位置共享）+ 一个bind + 原match进行fold。
拆开的fixture重组为原8链 `main`，作为常设线性回归；witness-size gate将任何退回复制的回归
变成快速red并指出函数。witness：231,114B→4,002B，每链式调用+480B。

**J4 — 迁移或限制剩余复制器。** 按影响范围清单顺序：`desugar_stmt_control_unwrap`
（continuation进入各arm尾部，node层面无上限）、`group_option_result_arms` catch-all副本、
`desugar_tuple_variant_match(_deep)` fall-through行、`desugar_unit_if_heap_reassign`
（重新定向：J1后可SSA化成JOINING bind而非复制bind，生成形状免费线性化）。
每项要么由join形覆盖，要么局部cap + 辞退到wall。

**J5 — join上的TCO，然后删除。** 让 `tco_collect` 理解join形（或从join重新推导分支递归）。
随后删复制路径和 `MAX_DESUGARED_NODES`。终局是无continuation复制的lowering，
200k cap成为dead code，因为没有东西会接近它。

<a id="track-c--linear-checkers-independent-start-immediately"></a>

## Track C — 线性checker（独立、立即开始）

231KB墙实测组成（n=bytes、k=ids、V̄=id平均值、F=函数）：
Θ(n²) `split_bar` accumulator-append（两backend）+
Θ(n²) 抽取的 `String.sub` destructor（仅binary）+ Θ(k²) `pnats` 的 `acc ++ [n]` +
Θ(k·V̄) 单进nat数字累积 + Θ(k²·V̄) `subset_check`。
而 `check_prog_cert` 的 `reaches` 没有memoization，最坏Θ(F·b^F)，
主干最陡潜在悬崖（目前被每file很小的graph掩盖）。

**C1 — emitter排序+去重（零证明变更）。完成2026-07-28。**
`subset_prop` 是集合成员关系；`certificate.rs` 自身说明“duplicates are harmless”。
`sorted_dedup_ids` 现在序列化 `name_witness_string` / `cap_witness_string` /
`transitive_cap_witness_string` / `program_cap_graph_witness`
（callee边也去重，它们会放大未memoize的 `reaches` fold）。corpus：names.cert −30%、最大行−33%。

**C2 — Gallina中的Θ(n)解析。完成2026-07-28。** `pnats` / `split_bar` / `split_semi`
保留名字和签名，但遍历 `list ascii`（cons累积，边界做一次 `rev_append`；
`list_ascii_of_string` 抽取成一次O(n) `List.init`）。四项解析二次复杂度在两backend都消除。
由file内Examples + 3-way corpus gate固定行为。

**C3 — 排序fast path。完成2026-07-28。** Subset.v中 `sortedb` + `drop_lt` + `merge_subset`；
`subset_check_fast` 分派，`check_names` / `check_caps` 经此路由。
merge健全性（`merge_subset_sound`）不需排序假设——被接受元素确实在 `sup` 后缀中找到，
所以信任定理无条件，排序只gate完备性
（fallback到 `subset_check`，每个既有witness验证bit完全相同）。
`CapabilityReach.prog_within` 刻意保留 `subset_check`（C5之前reach列表无序）。

**C4 — id类型 `nat` → `N`。** 去除两backend单进nat数字/eqb成本。
witness bytes不变，proof侧重构 `NameWitness`/`CapWitness`/`Fn` + parser。

**C5 — `reaches` memoization。** Visited-set BFS（`MSetPositive`，二进制 `positive` key，
适合vm_compute，已在stdlib closure）替代fuel有界重复展开。
在任一corpus file长出dense call graph之前消除指数悬崖。

**C6 — kernel-oracle卫生。完成2026-07-28。** goal分chunk
（每names/caps/tcaps goal 500 witness、每ownership goal 4,000行；健全：
`check_bc` 逐行fold，`forallb` 对++可分配），限制peak term并让coqc在goal之间GC。
逐行gate旁新增 `MAX_CERT_TOTAL=8388608` 聚合gate（当前全corpus总量约1.1MB）。
kernel oracle：254s→74s。

<a id="order-and-gates"></a>

## 顺序与gate

C1→C6→J0是独立快速成果（C1+C6立即缩小kernel oracle term，J0闭合无上限复制器类别）。
随后C2/C3作为一个proof PR。J1是枢纽：对抗评审后，J2 ratchet使进展单调可见。
J3是成果演示（事故形状、线性）。C4/C5和J4/J5随后。

C-track剩余工作按理由延期（诚实前例）：**C4**（nat→N）去掉残留单进nat算术——
事故类witness仍花7–9s；但该类被64KB gate挡在CI外，重构跨CallModes/CapabilityReach证明表面，
应做独立专注PR。**C5**（`reaches` memoization）闭合真正指数悬崖，
当前因各file图小被隐藏；若触发，会由100分钟workflow timeout变RED而非僵死。
caps graph变密集时立刻紧迫，而不是之前。

每阶段gate：`make verify-trust` 约17分钟green
（事故教会看时钟而非仅判定；慢3倍的green也是回归）、corpus walls=0、
walled-real和duplication-fire ratchet只能缩小，`almide test` + cross-target parity不变。

<a id="honesty"></a>

## 诚实

- 这里不改变程序哪些内容被PROVEN，而改变lowering准入哪些程序（更多）以及checker速度（大幅）。
  信任基础不增长；C-track明确拒绝会扩大它的唯一杠杆 `PrimString`。
- J1触碰certificate生成，正是历史accept-but-unsafe缺口所在表面。
  所以对抗评审是gate而非建议，也所以J1单独落地，不与J2融合。
- 实测倍率（2–3.5倍/call）与事故数字来自2026-07-27/28 session；
  almide-mir文件拆分稳定后，依赖这些数字前须重测。
