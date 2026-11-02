描述：将循环携带的堆所有权作为证书主干扩展——修复完整性，而不是增加特例
<a id="v1-heap-loop-carried-ownership--option-c-cert-spine-extension-the-completeness-fix"></a>

# v1 堆的循环携带所有权——方案 C（证书主干扩展），完整性修复

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-loop-ownership-cert.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-loop-ownership-cert.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-loop-ownership-cert.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-loop-ownership-cert.md)

> 原文历史说明：这份不断演变的 2026 年 6 月调查记录保留早期计划、失败试验、诊断及其后续撤回，不是当前实现、资格认定或安全保证。反复声称已证明或绝对的 3 个阻碍下限、必需的 ABI／Coq 重设计、嵌套循环阻碍以及已验证的提升方向，后来被限定或推翻。后续记录报告：list_instances 无需证书／Coq 改动即清除（仅 for，不含 while），load_porta_config 清除，read_message 低层化已解决，但 io.read_n_bytes 仍缺 runtime 实现。这些完成边界不同。本文未独立重跑历史修复、测试、证明或命令。关于已建模所有权事件流及引用计数平衡的定理，不等于证明每个 renderer／runtime 操作忠实、分配器回收正常或任意程序无泄漏；原文本身报告证书接受下仍有运行时误编译／泄漏、受信任递归 drop、平坦循环限制及验证器控制流缺口。门禁数量、有限字节／泄漏探针及 NIST 向量检查，仍是原文报告的限定证据，不是密码安全保证或外部认证。开头堆循环主张、后续条件循环集成及旧待办配方不被暗中统一；暂时计数矛盾和“6 workflows”列表原样保留。纯前缀恒等改写和拥有 String 错误副本的限制均保留。本阅读版不授权历史自主 agent 或仓库编辑指令。

**CEO 选择了 C（“只选 C”，2026-06-20）：从根本上补齐证明主干的完整性缺口——让已证明的检查器能够推理循环携带的堆累加器，而非绕开它（A：原地 push）或修改表示来取巧（B）。C 允许用户书写自然的递归 `acc + [x]`，并对其给出证明。**

<a id="the-gap-why-the-11-remaining-yaml-walls-need-this"></a>

## 缺口（为什么 yaml 剩余 11 个阻碍需要它）

基础所有权证书（OwnershipChecker.v）是逐对象的平坦事件流，没有循环概念。循环携带的堆累加器（每轮 `acc = acc + [x]`：释放旧对象、分配新对象、重新绑定槽）无法表示：对象的 `i` 在第 K 轮，`d` 在 K+1 轮，不同对象共享一个槽。`verify_ownership`（平坦、单次遍历）看到不平衡的 `d`/`i`，错误地拒绝安全代码。这是**完整性**缺口（健全性从未受威胁）。11 个阻碍（collect_*/parse_*/seq_item/map_entry、flow_rec↔flow_step、block_*）都依赖于此（追加累加器 + 互递归）。

<a id="-landed-2026-06-20-commit-7f673b4c-the-soundness-proof--the-②-critical-core"></a>

## ✅ 已落地 2026-06-20（提交 7f673b4c）：健全性证明——涉及②的关键核心

`proofs/OwnershipLoop.v`（位于证明门禁：`_CoqProject` + `check.sh` 的 coqc **及** coqchk + claim-drift；“PROOF SPINE OK”，无额外公理的“Closed under the global context”）。新增 `Loop : list FlatOp -> Op` 构造并证明：
- `exec_list`（检查器 fold）的 Loop 规则：当且仅当循环体从入口计数出发保持 rc 且不出错，才接受循环。
- `Unrolls`：抽象证书展开为具体平坦运行（每个 `Loop body` → n 份循环体）。
- **`check_unroll_sound`**：`check ops = true → ∀ unrolling, no_double_free ∧ no_leak`。即保持 rc 的循环体在任意迭代次数下都无泄漏、无重复释放（经 `exec_flat_repeat_preserve` 归纳）。
累加器槽证书为 `[Inc; Loop [FDec; FInc]; MoveOut]`（获取一次；每轮释放旧值 + 获取新值 = 净 0；最后移出）。循环体是平坦的（没有嵌套循环），足以处理 v1 解析器阻碍（每轮一次 drop+alloc）；嵌套循环是将来可组合的扩展。C 中困难且不可削减的部分（Coq 重证——“C 需要 Coq，不能由 corpus-wall 验证”）已完成并经内核验证。其余是可由门禁验证的工程集成。

<a id="remaining-c-integration-each-gate-verifiable--corpus-wall--byte-match--the-proof-gate"></a>

## C 剩余集成（每项均可由 corpus-wall + 字节匹配 + 证明门禁验证）

1. **✅ 完成 2026-06-20（提交 c05fc209）：生产检查器 + 证书格式。** `OwnershipChecker.v` 新增 `CertItem`（`COp`/`CLoop`）、`exec_line`、`check_line`，以及完整 Inc/Alias/Dec/MoveOut/Reuse 字母表上的健全性重推导（`exec_app`、`exec_repeat_preserve`、`UnrollsL`、`exec_line_unroll`、**`check_line_unroll_sound`**、**`check_cert_lc_sound`**），无额外公理，纳入证明门禁。证书格式 v2：循环分隔符 `(`…`)`（`parse_lc`），向后兼容（无括号证书的 fold 与平坦 `check` 完全相同）。`Extract.v` 提取 `check_cert_lc`；`driver.ml` 将所有权分派给它。`build-checker.sh` 往返真实字节：`I(DI)M` ACCEPT（累加器槽），`I(I)M`/`I(D)M` REJECT（泄漏／耗尽）。corpus-wall（14564 个堆对象）经 `check_cert_lc` 仍 ACCEPT，0 回归。
2. **✅ 完成 2026-06-20（提交 291a1f35）：Rust 循环感知证书输出 + 验证。** `lib.rs
   verify_ownership`——`Op::SetLocal { local, src }` 现在重新绑定堆槽（`object_of[local] ←
   object_of[src]`，槽重新存活）；旧对象已由循环体前面的 `Drop` 释放，因此逐轮不变量成立（标量 SetLocal 仍无操作）。`certificate.rs ownership_certificate`——`loop_carried_slots()` 预扫描 `LoopStart`…`LoopEnd` 内 `SetLocal` 的供给值；槽折叠为单个流 `i(id)m`（Alloc/Call 供给的 `i` 路由至槽，循环体外加 `(`/`)`）。单元测试：`loop_carried_accumulator_folds_to_one_slot_stream`（`i(id)m`，verify_ownership Ok），泄漏的循环体 `i(i)m` 被拒绝。已证明的提取检查器 ACCEPT 输出的 `i(id)m`（经 `./checker
   ownership` 验证）。corpus-wall（14564 对象）通过，向后兼容。（既有 2 个 render_wasm json wasm 执行失败无关，已仅 stash 这两个文件验证；属于另一 agent 的 list-cap 工作。）
3. **低层化**（`lower/mod.rs`）：发出循环携带堆累加器 MIR，即追加累加器 TCO。改动点：(a) `try_tco_rewrite` 约 2184 行——当携带的堆参数在所有自调用中的值均为 `acc + [x]`（`left = Var(acc)` 的 `BinOp::ConcatList`），去掉 `carried[i] && is_heap_ty` 的退出条件；(b) `tco_rewrite` 已将该携带参数输出为 `Assign { acc, acc + [x] }`（无需改动）；(c) 循环内 `Assign` 低层化（mod.rs 约 690 行，目前堆重新赋值报 `Err`）——允许 `acc = acc + [x]` → `new = __list_concat(acc, [x]); Drop acc; SetLocal acc, new`（`try_lower_concat_list`；证书支持的槽已由步骤 2 接受）。另需互递归内联（flow_step→flow_rec）。

   **设计已确定——方案 3（新槽预先复制）最干净，不改约定。** 关键：干净的 `i(id)m` 要求槽拥有所有权，且 `of[slot] = slot`（证书以 ValueId 为槽流的键）。借用的 `acc` 参数没有 `i`（证书 `(id)m` → 正确拒绝；第 1 轮释放它会重复释放调用者的对象）。经 Dup／拥有所有权的参数重新绑定，会使 `of[slot] ≠ slot`（参数对象偏离槽键）→ 证书复杂。修复：引入新槽变量 `acc_slot`，以 `acc_slot = __list_concat(acc, [])`（拥有所有权的副本）初始化。Call 的堆结果自动设定 `of[acc_slot] = acc_slot`（证书 `i`），因此槽键 == 对象，干净。再在整个循环体与基例中替换 `acc → acc_slot`；循环携带 `acc_slot`（释放旧值／分配新值），基例返回它（移出）。证书 = `i(id)m`，恰好是步骤 2 接受的形式。借用参数 `acc` 保持借用（调用者拥有），仅为预先复制读取它。字节匹配保持（复制 + 逐轮追加构造出与 v0 递归相同的最终列表）。实现部件：(i) 检测堆追加累加器（carried[ai] 为堆 + 所有自调用值为 `ConcatList{left:Var(acc)}`）；(ii) IR 变量替换 helper（Var(acc)→Var(acc_slot)）；(iii) 发出预先 `let acc_slot = acc + []` 绑定；(iv) 循环内 Assign 接线 (c)。`try_lower_concat_list` 仅支持标量元素，所以先用合成 `List[Int]` 追加验证机制；yaml 的 `List[Value]`/`List[(k,v)]` 随后需要堆元素拼接（+ value.object/stringify）。
4. **✅ 完成 2026-06-20（提交 f3ce5401）：追加累加器 TCO 生成端。** `try_tco_rewrite` 现在检测每个自调用值均为 `acc + [x]` 的堆携带参数，引入新拥有所有权的槽（`let slot = acc + []`，替换 `acc → slot`），循环内 `Assign` 将 `slot = slot + [x]` 低层化为 `new = __list_concat(slot,[x]); Drop slot; SetLocal slot,new`。在 `spec/wasm_cross/append_accumulator.almd`（List[Int]）上端到端验证：在 profile 内（此前被阻挡），所有权证书 `i(id)m` ×2，**与 v0 字节匹配**（output-parity baseline，match 69→70），corpus-wall 通过，cargo-test 干净（2 个 json wasm 失败为另一 agent 的既有问题）。渲染的循环每轮发出 `rc_dec(old)`（wat 中确认释放）。内存说明：`__list_concat` 会复制（O(n²)，与 v0 深递归相同）；大 n 在 wasm 的 64KB 上于 n≈110 时 OOB（sum(1..n)·8B），这是分配器回收／效率限制，不是 rc 泄漏（证书证明 rc 平衡，释放也已发出）。将来的原地 push 会变为 O(n)。fixture 保持较小 n。

**🎯 整条方案 C 链已端到端运行**（证明 → 提取检查器 → 证书序列化器 → verify_ownership → 生成端低层化），全部已验证：提交 7f673b4c、c05fc209、291a1f35、f3ce5401。循环携带的堆追加累加器从 `.almd` 编译，在 v1 信任主干上低层化，携带已证明的 `i(id)m` 证书，并与 v0 字节匹配。标量元素追加累加器的完整性缺口已从根本上关闭。

<a id="remaining-toward-yaml0-the-producer-extensions--the-chain-is-proven-these-widen-its-element-domain"></a>

## 朝向 yaml=0 的剩余工作（生成端扩展——链已证明，这些扩展其元素领域）

- **✅ 完成 2026-06-20（提交 7074579d）：堆元素拼接。** `__list_concat_rc`（自托管，通过白名单中的 `__lc_copy_rc` 对每元素 rc_inc）+ `try_lower_concat_list` 允许 String/Value 元素 + 标记 `heap_elem_lists`/`value_elem_lists`（因此 `drop_op_for` = DropListStr/DropListValue）+ 门禁 `count_ir_calls` 计入堆元素 ConcatList（mir≤ir 成立）。在 `spec/wasm_cross/append_accumulator_heap.almd`（`List[String]` build_s + extend_s）上验证：与 v0 字节匹配、corpus-wall 通过（清除 2 个 spec 阻碍，866→864）、output-parity 70→71、cargo-test 干净。因此 `acc + [x]` 现在对标量（Int/…）及堆（String/Value）元素累加器均可在已证明的 `i(id)m` 槽上低层化。
- **互递归内联——已做原型 + 关键发现（2026-06-20，已回退，未提交）。** yaml 全部 11 个阻碍均为“heap-result if/match”，因为每个追加函数都互递归（`flow_rec↔flow_step`、`collect_seq↔seq_item`、`collect_map↔map_entry`、`collect_block↔block_line↔block_nonblank`），所以 `try_tco_rewrite`（自调用检测）从不触发。`inline_mutual_tail_recursion` 原型（经每参数 `substitute_var_in_expr` 将仅调用一次的同级 G 内联进调用者 F，再删除 G；用 `IrMutVisitor` 重建）+ 检测放宽（自调用可传 `acc` 或 `acc+[x]`）+ `tco_rewrite` 跳过恒等赋值，已用合成 `frec⇄fstep`（List[String]，与 v0 字节匹配）验证。yaml 从 11→9，但 **`esc_rec` + `collect_block` 回归（profile 内 → 被阻挡）**：内联使 F 自递归 → TCO 触发 → TCO 将本来不做 TCO 时能低层化的函数挡住。②禁止这种完整性回归，因此已回退。
  **✅ 完成 2026-06-20（提交 8c9a5c07）：带守卫的互递归内联。** `inline_mutual_tail_recursion`（lower/mod.rs，传递 `globals`+`record_layouts`）：仅当 **F 当前被阻挡且内联后的 F 可低层化**时（两者都试低层化），才将仅调用一次的互递归同级 G 内联至 F（`IrMutVisitor` + 每参数 `substitute_var_in_expr`）并删除 G，因而构造上无回归。另加检测放宽（自调用传 `acc` 或 `acc+[x]`）+ `tco_rewrite` 跳过恒等赋值。接入 render_program + classify_corpus。已验证：`spec/wasm_cross/mutual_append.almd`（`frec⇄fstep`，List[String]）与 v0 字节匹配；**清除 6 个 spec corpus 阻碍（profile 内 3712→3718）**；corpus-wall 通过；cargo-test 干净；yaml 仍为 11（无回归，esc_rec/collect_block 保持 profile 内，守卫拒绝修改它们）。

  **✅ 完成 2026-06-20（提交 f6199af9）：`[call_result]` 元素物化 + 差一位的守卫 → yaml 11→9。** `try_lower_str_list_literal` 现在对 `elem_str` 允许返回 String 的 Module/Named 调用元素（`[string.slice(s,0,1)]`），不再仅支持 elem_value 的 Value 调用：将调用低层化为新拥有所有权的 String（已注册的 `string.slice` runtime；`lower_pure_module_value_call` 已处理一般 module 调用，不限于 value），再移入槽。字节验证：`spec/wasm_cross/list_call_element.almd`（`xs + [string.slice(s,0,1)]`）与 v0 一致。
  **🚨 同时发现并修复静默误编译：** `[string.slice]` 元素揭示 TCO 顺序赋值携带参数，因此 `acc + [string.slice(s, i, …)]` 读取循环下标 `i`（重新赋值为 `i=i+1`）时看见新 `i` → 偏一位（`chars("abc")` → `b-c-` 而非 `a-b-c`）。修复是在 `try_tco_rewrite` 中阻挡交叉依赖 TCO（自调用参数读取另一携带参数），满足②安全要求（阻挡，绝不误编译）；常见情况（每个参数只读自身）不受影响。yaml 11→9：flow_step + 另一函数现在正确低层化；交叉依赖函数（flow_rec、chars）被阻挡而不是误编译。

  **✅ 完成 2026-06-20（提交 89664c68）：同时更新 TCO。** `tco_rewrite` 先把每个携带标量的新值存入新临时变量（读取旧参数），执行堆追加赋值（标量仍旧），再提交临时值。因此交叉依赖追加（读取循环下标的 `acc + [string.slice(s, i, …)]`、`flow_rec` 的 `acc + [slice(s, start, pos)]`）不再偏一位。字节验证：`spec/wasm_cross/cross_dep_accumulator.almd`（chars `a-b-c`、win `ab|bc|cd`）。交叉依赖阻碍现缩小至 (a) 堆累加器读取另一堆累加器，(b) 纯 Var 别名参数（`start = pos`，复制临时量无法暂存）；两者都少见，并按②安全阻挡。清除 +3 个 corpus 阻碍（profile 内 3719→3722）；output-parity OK。

  - **✅ flow_rec 基例完成（提交 af2a5695）：`lower_heap_result_arm` 中的 ConcatList 分支。** heap-result-if 返回 `if string.is_empty(last) then acc else acc + [last]`（Var 移出分支 + ConcatList 分支）现在可低层化（逐分支 `"im"` 平衡）。flow_rec 端到端可低层化 → yaml 9→8。`spec/wasm_cross/heap_result_if_append.almd` 字节验证；清除 +3 个 corpus 阻碍（3722→3725）。

  **⚠ 剩余（yaml 8——Value 解析器核心；每个函数叠加多个缺口，不是单一能力）：** `collect_seq`/`seq_item`、`collect_map`/`map_entry`、`block_*`、`parse_lines`/`parse_nested`。代表性的 `collect_seq` 返回 `(Value, Int)`，具有以下特点：
  - **元组返回**：基例为 `(value.array(items), pos)`，是堆结果元组返回（Value + Int），不是裸 List。需要堆结果元组返回低层化（在基例中用累加器构造 Value）。
  - **基例中的 value.array**（作为操作已完成）折入元组。
  - **互递归 + 额外被调用者**：`seq_item` 是互递归同级，但还调用 `dash_item`；带守卫的内联仍适用（seq_item 仅被 collect_seq 调用），只是内联体保留 dash_item 调用。
  - **effect 调用的元组解构**：`let (val, next) = dash_item(...)!`——从 effect-fn Result 绑定 `(Value, Int)`，再追加 `items + [val]`。
  - 返回元组的 **effect fn**（`!` Result 传播）。
  因此 `collect_seq` 需要堆结果元组返回 + effect 元组解构（以及已完成的追加／内联／TCO）。`collect_map` 再增加 **value.object** + `List[(String,Value)]`（元组元素）追加；`block_*` 增加 **tuple-heap drop**；`parse_*` 是 heap-result match 根。这是几个实质性部件（Value 解析器机制），不是单一能力。追加累加器基础已完成，剩余是 value 聚合构造 + 元组接线。

  **🎯 已识别根本阻碍（2026-06-20，经合成探针）：effect-fn `!` 的提前返回传播。** 剩余 8 个按关键点分为两组：
  - **6 个 effect 函数**（collect_seq、seq_item、collect_map、map_entry、parse_lines、parse_nested）——全为 `effect fn`，绑定 `let (val,next) = dash_item(...)!` 或尾部 `parse_nested(...)!`。合成 `let (v,next) = make(n)!`（make 为 `effect fn -> (Value,Int)`）被阻挡，消息为**“调用参数位置的 unwrap `!` 无法忠实计算（需要提前返回传播）”**。因此全 6 个的关键是 `!` 传播，即 v1 MIR Result／错误机制：将 `f()!` 低层化为 `match f() { Ok(v) => <continue>, Err(e) => return Err(e) }`（Err 时提前返回）。这是基础功能（Result 表示 + 函数中途提前返回／其脱糖），不是逐函数部件；落地后，6 个 effect 函数的元组解构和元组返回（两者已支持）便能组合。
  - **block_scalar/block_line + collect_block**（非 effect 的 `local fn`，没有 `!`）——关键为 3 节点环的迭代内联（collect_block↔block_line↔block_nonblank，不是二元对）+ `(List[String],Int)` 元组返回 + tuple-heap drop。与 effect `!` 工作独立。
  下一关键能力 = effect-fn `!` 提前返回传播（解除 8 个中的 6 个）。这关系健全性（错误 Err 传播 = 静默误编译），所以必须与提前返回脱糖 + 字节匹配一同落地，而非采用 Const/Opaque 捷径（当前阻碍明确拒绝这种捷径）。

  **纠正（2026-06-20，更深入探针）：该组大部分已可低层化，约 74 个中仅 8 个被阻挡，且阻碍异质（无单一关键点）。** dispatcher（dash_item、dash_after、nested_dispatch、…）在 profile 内：effect fn 尾部 `f()!` 是透传（Result 原样返回 → 直接 `f()`），已处理。只有这 8 个被阻挡，各因不同功能：
  - **collect_seq、seq_item**——let 绑定 `!`：`let (val,next) = dash_item(...)!; <rest>` 需要提前返回脱糖 `match dash_item(...) { ok((val,next)) => <rest>, err(e) => err(e) }`（Ok 分支 continuation + 元组载荷解构 + Err 传播）。[最近的 2 个——追加／内联／TCO／元组全已完成，只剩 let 绑定 `!`]
  - **collect_map、map_entry**——let 绑定 `!` + **value.object**（从 `List[(String,Value)]` 构造 Value 对象）。
  - **parse_lines、parse_nested**——`lines |> … |> list.find((e) => not is_blank(e.1))`（list.find + lambda + pipeline）+ `match next { some((offset,line)) => … }`（Option-of-Tuple match 载荷）。
  - **block_scalar、block_line**（+ collect_block）——`(List[String],Int)` **tuple-heap drop** + 3 节点环内联（collect_block↔block_line↔block_nonblank）。
  因而路径是几个不同部件（let 绑定 `!` 提前返回 ⇒ 先 collect_seq/seq_item，再 value.object、list.find+lambda+Option-tuple-match、tuple-heap+3-cycle），各自关系健全性，各自需要字节匹配。不是单一能力。追加累加器 + 方案 C 基础已完成。

  **✅ block_scalar 完成 2026-06-20（提交 b31096e8）：yaml 8→7——且阻碍并非上文所界定的内容。** block_scalar 真正的阻碍是返回 `(value.str(if string.ends_with(ind,"-") then
  joined else joined+"\n"), end)`：heap-result `if` 嵌在元组元素里的 `value.str(..)` 中，前面还有 `let joined = if…`（两个堆 let 绑定 if），不是 tuple-heap-drop，也不是 3 节点环。三个组合修复：(1) `extract_first_callarg_branch` 递归进入元组元素（将 `value.str(if…)` 参数 ANF 提升）；(2) 有界复制门禁允许 ≤2 个剩余分支绑定（此前任意一个都拒绝）；(3) `desugar_heap_branches` 递归进入 if/match 分支及 block 尾部（`desugar_nested_branch_arms`），使复制分支中嵌套 let 绑定 if 被消解；全在共享脱糖中（lower == count，无 mir>ir 违例）。**此过程暴露的 control_flow（C-044）两个既有静默误编译也已修复：** (a) `x |> (n) => body` 被脱糖为 Computed callee 调用，v1 MIR 错降为 0；frontend 现内联为 `{let n=x; body}`（`lower_pipe`）；(b) block 值的标量绑定 `let a = {…; tail}` 也错降为 0；`lower_bind` 现先运行 block 的语句，再绑定尾部。验证：corpus-wall profile 内 3725→3733（+8），ownership 14984 ACCEPT，cargo-test 466，output-parity baseline 无回归 + control_flow 新增 wasm 字节匹配（fixtures heap_result_tuple_return、pipe_lambda_block_value）。（调用参数位置的 pipe-lambda 现被阻挡而非错降，安全；之后再 ANF 提升。）**剩余 yaml 7：block_line（block_scalar 同级，可能最近）、collect_map、collect_seq、map_entry、parse_lines、parse_nested、seq_item**——6 个 effect 函数仍需 effect-monad let 绑定 `!`（⛔ 见上文注记）+ value.object。

  **✅✅ block_line 完成（提交 5518fff3）：yaml 7→6，与 v0 字节匹配。** 按②纪律进行了 6 轮二分调查后（下方长注记：替换／借用参数／fresh-let 内联／简单 ID 重用全经测试并否定），wat 显示 else 分支的 `string.drop(line, 0)` 读取 `$v19`（then 分支的 `__list_concat` 结果）而非 `line`。根因：`desugar_callarg_heap_if` 的裸调用参数提升以 `max_var_id(THIS arm)` 决定新 `tmp`，但该分支不含 `line`（仅同级 else 使用），因此 `tmp` 与 `line` 别名，renderer 全局 VarId→local 映射发生碰撞。修复：在 `desugar_heap_branches`/`desugar_callarg_heap_if`/`desugar_nested_branch_arms` 中传递函数范围的 `next_var` 计数器（`desugar_heap_branches_inner(body, &mut u32)`；公开 wrapper 用 `max_var_id(whole_body)+1` 初始化）。验证：spec/wasm_cross/block_line_collect.almd 字节匹配（`a||bb|c`），corpus-wall profile 内 3733→3734 + ownership 14988 ACCEPT，cargo-test 466，output-parity 无回归 + 新增匹配，全 worktree 扫描 = 仅既有 3–4 个不匹配（string_ops/fan_map/nested_named/list_string，全在 HEAD~1 确认）。**诊断纪律很重要：corpus-wall（所有权）和阻碍数都接受了有 bug 的低层化，只有字节匹配捕获它，6 轮中守住了②。**

  **（历史诊断——为保留方法而留存）**
  **⚠ block_line——捕获的②陷阱（2026-06-20）：指标上可低层化，但运行时误编译，保持阻挡。** block_line 主体为 `if is_blank then collect_block(.., if list.is_empty(acc) then acc else acc+[""])
  else block_nonblank(..)`——heap-result `if` 的 then 分支调用携带调用参数堆 `if`。扩展 `desugar_callarg_heap_if` 以提升裸调用／元组主体（让 `desugar_nested_branch_arms` 到达每分支调用），确实清除了 block_line 阻碍（yaml 7→6），但随后使带守卫的互内联将 collect_block↔block_line 折叠为 TCO，其追加累加器静默地将每个元素错降为 ""（`["a","","bb","c"]` → `["","","",""]`，字节不匹配，而非阻碍）。互内联守卫只检查内联 F 可否低层化，不检查字节匹配，所以可低层化却错误的内联会通过。按②已回退（mod.rs 注记）：交付误编译的虚假阻碍数下降，比诚实阻挡更坏。真正阻碍是 collect_block↔block_line TCO 追加将所有元素读为 ""；2 节点环 `collect_block`/`block_line` 合成已复现。先修它，调用参数提升才安全。

  **🎯 根因已定位（2026-06-20，暂时重新开启提升后进行二分）：互内联的 `substitute_var_in_expr` 生成无法物化的列表元素 → ""。** 对 2 节点环 `collect_block`/`block_line` 合成，在 block_line 主体直接使用常量元素（`acc + ["Z"]`）字节匹配（`Z|E|Z`），TCO + 槽 + 内联机制正确。但 `acc + [line]` 中 `line` 为 block_line 参数时，即使 collect_block 传常量 `"X"`，每元素仍为 ""（`X|X|X` → `||`）。而 `line`/`x`/`y` 为普通局部变量的 `["a"] + [line]` / `[x,y]` 字节匹配。
  **⚠ 纠正（2026-06-20，替换假说经测试被否定）。** 将 `inline_sibling_calls` 的 `substitute_var_in_expr` 换为 `let fresh = arg;` + 将参数重命名为 `fresh` 的内联（使列表元素成为局部变量引用 `[Var(fresh)]`）未修复，block_line 仍将所有元素输出为 ""（flow_rec/chars 保持字节正确，所以 let-inline 无回归但不是解法）。已回退。更深二分：`acc + [string.drop(line, 0)]` 的调用元素也是 ""，因此不是元素物化。新的 `let line = list.get(lines, pos) ?? ""` 本身在 TCO 循环内读到空值：对借用列表参数 `lines` 的 `list.get(lines, pos)` 返回空。chars 能工作是因为循环读取借用字符串参数（`string.slice(s, …)`）；互内联→TCO 循环读取借用列表参数返回空——参数在循环体读取前被释放／清零，或循环未能携带它。

  **🎯🎯 真根因，wat 确认（2026-06-20）：裸调用参数提升 + 脱糖复制中的 VarId 碰撞。** 转储忠实于 yaml 的 `line_at` helper 合成的 wat（提升重新开启）：
  ```
  (local.set $v15 (call $line_at  (local.get $v0) (local.get $v1)))   ;; line = line_at(lines,pos)
  (local.set $v19 (call $__list_concat_rc (local.get $v2) (local.get $v13)))   ;; THEN arm: acc + [""]
  (local.set $v28 (call $string.drop (local.get $v19) (i64.const 0)))   ;; ELSE arm: string.drop(line,0)
  ```
  代码注释说明：第一行 line = line_at(lines,pos)；下一行是 then 分支的 acc + [""]；最后一行是 else 分支的 string.drop(line,0)。
  else 分支的 `string.drop(line, 0)` 读取 **`$v19`——then 分支的 `__list_concat` 结果（List）**，而不是 `$v15`（line_at 的 String）。因此 `line` 的 VarId 与 then 分支拼接槽别名：提升（`tmp = max_var_id(body)+1`）+ 尾部复制（`desugar_let_bound_heap_branch` 把 continuation 克隆到两个分支）+ 放宽有界复制，在两个分支复用 VarId，全局 VarId→wasm-local 映射碰撞；string.drop 对列表指针运行 → 垃圾／""。不是替换，也不是借用参数（上面两者均否定），之前理论皆误导。
  **修复：** 在 `desugar_heap_branches` / `desugar_callarg_heap_if` / `desugar_let_bound_heap_branch` 中传递单个单调增长的新 VarId 计数器，而非每次调用重新计算 `max_var_id(body)+1`（先前重写已消耗 ID 后就会碰撞），使每个提升的 `tmp` 和每个复制的 continuation 都有全局唯一 ID。之后裸调用参数提升才健全 → block_line 正确低层化 → yaml 7→6。不要在新 ID 传递前重新加入提升。

  **⚠ 更新：简单 ID 重用也已排除。** `max_var_id`（mod.rs:2065）确实计入 `IrStmtKind::Bind` 变量（visit_stmt，2107 行）+ Match 模式绑定，所以提升的 `let tmp` 被计数，下次 `max_var_id+1` 更大，并无简单 ID 重用。因此 v19 而非 v15 的断裂不是 IR-VarId 碰撞，而是 MIR 低层化层的 `value_of`／物化断裂：内联将调用 `line_at(lines,pos)` 替换进 block_line 的条件（`string.is_empty(line)`）和元素（`string.drop(line, 0)`）后，低层化把 line_at 物化一次（`v15`），但元素的 `string.drop` 绑定到 `v19`（同级分支拼接）。四个假说现已否定（替换、借用参数、fresh-let 内联、简单 ID 重用）。下一步：转储 `line_at` 合成的 MIR Op 流（不只是 wat），追踪哪个 Op 将 `string.drop` 参数设为 v19；bug 在提升／脱糖为替换到多处的调用传递 `value_of` 的方式。需要一次专注 MIR Op 的调查。

  **⚠ 一概去掉 let 绑定 `!` 不健全——捕获的②陷阱（2026-06-20）。** 在 `lower_destructure` 中去掉 `let (a,b)=f()!` → `let (a,b)=f()`（尾部 `!` 透传），并让 `tco_collect`/`tco_rewrite` 看穿 `!`，确实清除了 seq_item→collect_seq→collect_map（yaml 6→3），但完整 v0/v1 spec 扫描捕获它误编译会报错的函数：`safe_div_chain`、`grade_classify`、`sum_of_squares`、`closure_env_churn`、`map_entry_churn` 全字节不匹配（去除操作丢掉这些函数传播的真实 `err(…)`）。corpus-wall（所有权）通过，只有字节匹配捕获。按②已回退。
  让健全版本可行的关键区别：**yaml 解析器组从不返回 `err(…)`**（grep 验证：唯一 `err("…")` 位于纯整数解析器 `oct_rec`/`bin_rec`，用 `match` 而非 effect `!` 处理）。因此限定在 never-err 范围，仅当被调用者可证明从不报错时才去掉 let 绑定 `!`，便会健全并达到 yaml=0，同时让 `safe_div` 等保持阻挡（调用图不动点：函数有 `err(…)` 或 `!` 调用 can-err 函数时且仅此时可报错；yaml 组没有）。这种逐被调用者分析（传入低层化），或完整 effect-monad（return-wrap），才是道路；一概去掉永远排除。

  **✅✅ never-err 限定去除完成（提交 b154a270）：yaml 6→3。** `mod.rs` 中实现逐被调用者 can-err 分析：`compute_can_err(fns)` 用 `has_result_err`（主体含 `IrExprKind::
  ResultErr`）作为种子，运行 `!` 传播不动点（`unwrap_named_callees` = `Unwrap{Call{Named
  g}}` 中的 `g`；函数若 `!` 传播 can-err 函数，则也 can-err）。`strip_never_err_unwraps` 随后仅对 never-err `g` 将 `Unwrap{Call{Named g}}` → `Call{Named g}`，作为 `inline_mutual_tail_recursion` 第一阶段运行（在内联守卫尝试低层化前，使内联 F 看见裸调用、追加 TCO 触发，`tco_collect` 无需理解 `!`）。yaml 组全部 never-err（无 `ResultErr`；唯一 `err("…")` 在由 `match` 而非 `!` 到达的纯 `oct_rec`/`bin_rec` 中），因此 seq_item/collect_seq/collect_map 全能 TCO 并清除。②验证：corpus-wall profile 内 3741→3758（+17），ownership 15068 ACCEPT；完整 v0/v1 spec 扫描 = 仅既有不匹配。一概去除时字节不匹配的 `safe_div_chain`、`grade_classify`、`sum_of_squares`、`closure_env_churn`、`map_entry_churn` 现在匹配，因为它们的 `!` 可报错所以保留。cargo-test 466。

  **⚠ 剩余 yaml 3：map_entry、parse_lines、parse_nested**——不同的 value 聚合／match 功能（去除 + TCO 基础已完成）：
  - **parse_lines、parse_nested**：`… |> list.find((e) => not is_blank(e.1))`（list.find + 带元组索引 `.1` 的 lambda），随后 `match opt { none => …, some((idx, line)) => … }`——**具有元组载荷的 Option match**（`some((idx,line))`）。try_lower_variant_value_match 处理标量／单个堆载荷；缺口在元组载荷解构。
  - **map_entry**：`match find_colon(t) { none => (value.object(pairs), pos), some(cp) => { … pairs +
    [(key, val)] … } }`——Option 标量 match（cp）无问题，但累加器追加 `pairs + [(key,
    val)]` 是 **List[(String,Value)] 元组元素追加**（value.object 本身已证明，collect_map 基例可低层化）。缺口在元组元素列表追加。

  **🔧 Option 元组载荷 match 的配方（parse_lines/parse_nested）。** 布局已确认：元组是 `DynList`，元素 `i` 在 `layout::slot_offset(i)`（因此 `(idx,line)` = slot_offset(0) 的标量、slot_offset(1) 的 String 句柄）；Option `Some` 载荷作为元组句柄存于 variant 块 `@12`。扩展 `try_lower_variant_value_match`（control.rs:822）：当 `Some`/`Ok` 内部模式为 `Tuple`，将 `@12`（元组句柄）以借用绑定至新 `$p`（类似 `str_heap_bind`），再在分支前置 `let (idx,line) = $p` 并低层化（已跟踪容器上的 `BindDestructure`——`lower_destructure` 的“已跟踪堆变量为容器别名”路径），在分支之后释放 subject（control.rs:1037 的 `str_heap_bind` 分支）。因为 `parse_lines`/`parse_nested` 仅借用 `line`（传给 `dispatch`/`nested_dispatch`/`indent_of`/`string.trim`），从不移出，subject 后置释放正好释放元组及其 String 一次。最干净实现：函数顶部脱糖 `some((idx,line)) => B` → `some($p) => { let (idx,line)=$p; B }`（新 `$p` 来自 `max_var_id+1`），再扩展 `heap_or_scalar_bind`，允许 Option subject 上的堆元组载荷。于是 yaml 3→1；map_entry 元组元素追加再使 1→0。

  **✅ Option 元组 match 完成（提交 a89bda41）：yaml 3→1**——经 variant-match 元组载荷绑定 + 后置 drop，清除 parse_lines/parse_nested（corpus-wall ACCEPT，全扫描无新不匹配）。

  **🎯 最后阻碍 = map_entry（yaml 1），完整诊断（截至提交 55343d53）。** 将合成二分到 map_entry 精确形状（`match find_colon(t) { none =>
  (value.object(pairs), pos), some(cp) => { … cmap(…, pairs + [(key,val)]) } }`），发现三个嵌套阻碍：
  1. ✅ **用户函数 Option subject**（`find_colon` 是 `Named` 函数，而非自托管）——未跟踪导致 variant-match 不触发。已修复：跟踪返回 Option/Result 的 `Named` 调用为物化 subject（相同 DynListStr len-as-tag 表示）。由 `ufo.almd` 字节匹配验证。
  2. ✅ **借用的 `pairs` 同时用于两分支**（none 中 `value.object(pairs)`，some 中 `pairs + [(k,v)]`）——then 分支 consume 泄入 else 分支的低层化视图，使 else 被阻挡。已修复：在互斥分支之间快照／恢复 param_values+live+materialized_aggregates（分支所有权隔离）。
  3. ✅ **完成（提交 95accd80）：`pairs + [(key,val)]`（List[(String,Value)] 元组元素追加）。yaml 1→0。** 新自托管递归 drop `$__drop_list_str_value`（value_core.almd：每元组对 @12 String 槽 rc_dec，对 @20 Value 槽 `$__drop_value`，再释放元组，最后列表），由新 `Op::DropListStrValue` 支持（单个证书 `d`，像 `DropListValue` 一样受信任的递归）；`try_lower_concat_list` + `try_lower_str_list_literal` 允许堆字段 `(String,Value)` 元组元素（经 `try_lower_tuple_
     construct` + `__list_concat_rc`），在 `str_value_elem_lists` 跟踪；rc_dec 允许列表与 example 侧 linker 拉入 `$__drop_list_str_value`/`$__svdrop_list`。corpus-wall ownership ACCEPT（profile 内 3758→3822，+64，也清除另外 63 个 corpus 函数），与 v0 字节匹配，cargo-test 466，全扫描无新不匹配。**⚠ 发现：既有 TCO 堆循环泄漏——尾循环中的 `let xs = [heap]; loop(...)` 在约 N/每轮对象数时 trap（freelist 未复用），List[Value]（已证明）、List[String] 及此新 drop 均相同（List[Value] 1 元素约 2000、3 元素约 1500、此 4 对象约 1000，同样的每对象速率）。因此不是此 drop 的 bug，而是影响所有分配堆的尾循环的独立 freelist/TCO 问题，值得专门处理。**

  ~~3-旧。剩余：`pairs + [(key,val)]` … 调用参数位置。~~（已由上方完成项取代；为诊断轨迹留存）：`try_lower_concat_list`（calls.rs:534）仅允许 String/Value 元素（548–552 行）；**堆字段聚合元素（内部含堆的元组／record）被延后**，调用参数路径随后被阻挡（calls.rs:887，符合②；let 绑定路径却静默延后至 Opaque 空列表 = 潜在误编译，而非真实低层化，所以确实未解决，不是门禁怪癖）。修复是真正“Camp-4 前沿”：**感知元组的递归列表 drop**（对 `(String,Value)` 元组做 `DropList`，迭代、按掩码释放每个元组，释放内部 String+Value），使 `__list_concat_rc` 能 rc 拥有元组元素，结果得以正确回收。新 runtime drop（List[tuple]），不同于 DropListStr/DropListValue。有了它，map_entry 可低层化 → yaml=0。（独立复现：`/tmp/eff2.almd`、`/tmp/eff5.almd`；let 绑定延后为 `/tmp/sv.almd`。）

  **⚠ 纠正（解构脱糖路径被阻挡——已测试）：** 对元组变量／参数的 `let (idx, line) = pair` 单独也在字节检查时被阻挡（`/tmp/td.almd`：v0 `7:hi`，v1 被阻挡）。`lower_destructure` 的形状是元组字面量值或已跟踪容器，都不覆盖普通元组变量 → 标量+堆拆分。（cs 的 `let (v,n) = cs(...)` 可低层化，因为右值是新调用结果元组，是另一形状。）所以 Option 元组 match 不能脱糖为 `some($p) => { let (idx,line)=$p; … }`，必须在 variant-match 的 `bind_payload` 中直接绑定 idx/line：加载 `@12`（元组句柄），再 `idx = load(handle +
  slot_offset(0))`（标量复制）及 `line = load(handle + slot_offset(1))`（堆句柄借用），在分支之后释放 subject。这意味着每分支绑定需从 `Option<(VarId,bool)>` 改为多绑定（单个或元组），是复杂且所有权关键的改动（错误后置 drop = UAF），但证书 + 字节匹配门禁会捕获任何错误。剩余工作是此多绑定重构（parse_lines/parse_nested）+ List[(String,Value)] 元组元素追加（map_entry）。
  （同轮还发现：提交 75c9100e 因过时工作树意外从 mod.rs 丢掉 block_line 新 VarId 修复；经 `git checkout 5518fff3 -- mod.rs` 恢复并重新验证 block_line 字节匹配；yaml 回到 6，corpus-wall 通过，profile 内 3741、ownership 15035 ACCEPT。）

  **🔧 let 绑定 `!` 的具体配方（2026-06-20，Result 表示现已确认）。** v1 MIR 将 effect-fn `Result[T,String]` 表示为以 len 为 tag 的 DynListStr（见 `materialize_result_ok`，control.rs:2030）：`len @ handle+4` 为 Ok 时是 `0`，Err 时是 `≠0`；Ok 载荷（标量或元组／堆句柄）在 `handle+12`。尾部 `f()!` 已透传 Result（`lower_tail(expr)`，tail.rs:256/624），因为尾值就是函数返回，所以健全。let 绑定 `let pat = f()!; rest`（binds.rs:235，被阻挡）脱糖为 heap-result `if`，无需新 variant-match 扩展：
  ```
  let r = f()                                  // Result (DynListStr); track in materialized_results_str
  if <load(r+4) != 0> then r                   // Err: move the Result out as-is (Dup+Consume, the Var arm)
  else { let pat = <load(r+12)>; rest }         // Ok: extract the payload @ +12, then the continuation
  ```
  代码注释翻译：r 是 Result（DynListStr），在 materialized_results_str 中跟踪。Err 原样移出 Result（Dup+Consume，Var 分支）。Ok 提取 +12 的载荷，再执行 continuation。
  Ok 载荷提取 + 所有权恰好是现有 `value.as_array` str-result 路径（control.rs:907–916：将 @12 句柄借用绑定，随后释放 Result wrapper）；对于元组载荷，在绑定后解构 @12 句柄（读取 .0/.1）。两分支均生成函数的 `Result`，所以现有 heap-result-`if` 机制（包括刚增加的 ConcatList/Call/Block 分支）可以低层化。最难集成为 collect_seq：此 `!` 位于 TCO 循环体内，因此 Err 提前返回变成循环携带的 `if`（then 分支 `return r` 是带值 break）。先做独立非 TCO 合成 `let x = mk(n)!`（字节匹配），再做 TCO 集成。从那里开始。

  **⛔ 定论（2026-06-20，上述配方有未满足的前提——代码确认）。** v1 MIR 不把用户 effect fn 返回值包装为 DynListStr Result 表示：`lower_body_with_globals`（mod.rs:180）把 `lower_body_into(body)` 原样作为 `ret` 返回，没有 Ok 包装。因此用户 effect fn 返回裸值（`(Value,Int)`），没有 tag。尾部 `f()!` 能健全去除，仅因 Result 层面 g ≡ f()（tail.rs:253，恒等：g 原样返回 f() 的值，无论 Ok 或 Err）。但 let 绑定 `let x = f()!; rest` 中 g ≠ f()（f() 后再 rest），所以朴素去除会在 Err 路径用垃圾 x 运行 `rest` = 静默误编译。这正是 binds.rs:235 刻意阻挡而非去除的原因。正确 let 绑定 `!` 需要真正提前返回，需要 runtime Result tag 供分支，而裸值模型没有。前提是建立 v1 effect-Result 规约：将每个 effect fn 返回包装为 Result 表示（materialize_result_ok／Err 构造），并让每个调用位置的 `!`/`?` 感知 tag（尾部 `!` 从恒等去除变成带 tag 检查的透传）。这是重大的横切子系统（每个 effect fn + 每个 effect 调用），不是配方所假定的单一脱糖。②：朴素去除不可接受（Err 路径误编译）。所以 6 个 effect-fn 阻碍依赖先构建 effect-monad；2 个 block_* 阻碍（非 effect）独立（3 节点环内联 + tuple-heap），若避开 effect-monad 子系统，它们是更易着手的下一目标。
  （追加拼接——标量 + String/Value 堆——带守卫互内联、调用元素物化、同时更新 TCO、heap-result-if 追加基例，均已完成并验证；偏一位类别已有守卫。）

C 端到端落地后，11 个阻碍将清除（Value 解析器子集还需 value.object/stringify + tuple-heap），在已证明的主干上推动 yaml → 0，这就是 v1 完整性理想。

<a id="org-wall0--the-remaining-6-non-native-walls-precisely-scoped-2026-06-27"></a>

## ORG wall=0——剩余 6 个非 native 阻碍，精确定界（2026-06-27）

cross-module + str-acc/defunc + ReadDir + correctness-sweep 活动后，org 阻碍面为 **12 个仓库 wall=0；6 个非 native 阻碍 + porta 29/sqlite 20 仅 native 阻碍**。这 6 个分为两种机制，均经阅读实际 `.almd` 与阻挡位置验证。

<a id="mechanism-1--aes-2-walls-cfb8_encrypt-cfb8_decrypt--reuses-the-proven-loop-cert-no-new-coq"></a>

### 机制 1——aes（2 个阻碍：`cfb8_encrypt`、`cfb8_decrypt`）——复用已证明 Loop 证书，无需新 Coq
形状（`aes/src/mod.almd:168-193`）：`var iv = state.iv`（`var` 绑定到借用堆 record 字段），随后 `for i in 0..len { … iv = bytes.concat(bytes.slice(iv,1,16), …) }`（每轮重新赋值为新拥有所有权的 `bytes`），最后移入结果 record（`iv: iv`）。这恰好是已证明循环携带槽 `[Inc; Loop[FDec;FInc]; MoveOut]`（OwnershipLoop.v），每轮净 0（释放旧值 + 获取新值），最终移出。
- **阻挡位置**：`lower/tail.rs:48-64` 刻意阻挡循环重赋值（`loop_reassigned_vars`）的可变堆字段变量。非循环同级（`:65-78`）已对可变字段变量做拥有所有权的 `Dup`。
- **证书机制已处理它**：`certificate.rs loop_carried_slots` 将 `LoopStart…LoopEnd` 内任何 SetLocal 的堆结果 Call 注册为槽，`bytes.concat`（堆结果）→ `iv` 槽、循环 `(id)`、移出 `m`。唯一阻碍是进入槽机制前已停止低层化。
- **修复（方案 3，与追加累加器对应）**：槽初始化必须是干净的 `i`（Alloc／堆结果 Call），不是 `Dup`（会发出证书 `a`，且 `of[slot]≠slot`）。因此把阻挡替换为：将 `var iv` 初始化输出为字段的拥有所有权堆结果副本（`bytes` clone 调用 → `loop_carried_slots` 看见其 `i` → 路由进槽），随后循环 SetLocal + 移出折叠为 `i(id)m`，即已证明证书。**门禁：aes 自带 NIST FIPS-197 测试向量**（`mod.almd:200+`），免费获得字节匹配 oracle；corpus-wall ownership ACCEPT 捕获证书错误。aes 2→0，阻碍 6→4。

<a id="mechanism-2--filterfilter_map-4-walls-wasm-bindgen-generate_witesmdts-dojo-backfill_dir--needs-a-new-coq-construct"></a>

### 机制 2——filter/filter_map（4 个阻碍：wasm-bindgen generate_wit/esm/dts、dojo backfill_dir）——需要新的 Coq 构造
形状：`types |> list.filter((t) => list.contains(used_names, get_str(t,"name")))`（捕获 `used_names` 的闭包）；dojo 的 `list.filter_map((f) => match fs.read_text(dir+"/"+f) {…})` 还捕获 `dir`，并含 effect。被活动中的值位置 HOF 诚实性守卫（`calls.rs`、`last_call_had_unlifted_closure`）阻挡。
- **为何是真正 Coq 前沿**（经验确认：某 agent 的 C1 内联使字节匹配，却被 corpus-wall 拒绝）：filter 逐元素获取是条件性的——`if pred then {Inc x; append to
  out}`。输出列表 `out` 积累 runtime 可变数量的副本（k = true 数量），并非逐轮平衡，而由最后 `out` 的批量 DropList（k 个 Dec）平衡。当前 OwnershipLoop Loop 规则要求循环体严格保持 rc（净 0），条件性 +k 不满足。因此拒绝安全程序：比净 0 累加器更进一层的新完整性缺口。
- **所需扩展**：Coq 构造表达**条件性获取到累加器 + 批量排空**；循环体有条件地增加累加器元素数（单调、不出错），由最终释放全部元素的 `Drain` 平衡。健全性：任意 true 数 k，向 out 条件性 Inc k 次 = len out = DropList out 的 k 个 Dec，不论 k 为何都平衡。随后提取至 OCaml 检查器，为 C1 内联的捕获式 filter 输出证书并路由低层化。dojo 的 filter_map 还需要 effect-monad `!`（#22／let 绑定 `!` 前沿），因其闭包含 effect。这是真正不可削减的 `#31` Coq 工作，多层（Coq → 提取 → Rust 证书 → 低层化），关系健全性。

**通向 wall=0 的计划**：(1) aes（复用已证明主干、NIST 门禁、可着手）→ 6→4；(2) 条件性获取 OwnershipLoop 构造（新 Coq）→ wasm-bindgen 3→0 → 4→1；(3) dojo effectful filter_map 的 effect-monad + 条件性获取 → 1→0。之后只剩 porta/sqlite 仅 native 阻碍（重新分类）。

<a id="-step-1-done-commit-6a38227e-aes-20-a-routing-gap-not-coq--see-the-aes-section-above"></a>

### ✅ 步骤 1 完成（提交 6a38227e）：aes 2→0（路由缺口，不是 Coq——见上方 aes 节）。

<a id="-step-2a-done-commit-5d3b642f-the-conditional-acquire-coq-core--ownershipfilterv"></a>

### ✅ 步骤 2a 完成（提交 5d3b642f）：条件性获取 Coq 核心——OwnershipFilter.v。
`CondLoop thenb elseb` + `ccheck_unroll_sound`：两个分支均保持 rc ⇒ 任意逐轮谓词结果序列均无重复释放／泄漏。内核检查 + coqchk + 无额外公理，纳入证明门禁（PROOF SPINE OK）。filter 槽证书 `[CInc; CondLoop [FDec;FInc] []; CMove]` 被接受；泄漏／耗尽分支拒绝；无条件 Loop 是 thenb=elseb 特例。这是不可削减的 Coq 部分。

<a id="step-2b-remaining-the-integration--precisely-diagnosed-2026-06-27-heap-filter-inline--conditional-acquire-certchecker"></a>

### 步骤 2b 剩余（集成——2026-06-27 精确诊断）：堆 filter 内联 + 条件性获取证书／检查器。
经验发现（经 render_program 对 native 探测各 filter 形状）：
- 不捕获的堆 filter（`xs |> list.filter((s) => string.len(s) > 1)`，List[String]）能工作，经 C2 提升（自托管 `(call $list.filter list funcref)` 组合子，有自身已证明证书），而非内联。
- 标量 filter（`[1,2,3] |> filter((x) => x > 2)`）能工作，经 C1 内联（try_lower_defunc_list_hof）：写游标 + 条件性存储 i64 值（无所有权 ⇒ 无元素证书事件，既有检查器已接受此条件存储）。
- 捕获式堆 filter（任何捕获——标量、字符串、列表——都被阻挡）失败：C2 提升不可行（捕获没有 FuncRef），C1 内联 `try_lower_defunc_list_hof` 在 control_p5.rs:116（`filter` 不在堆来源允许列表）+ :138–139（`filter` 结果必须非堆）拒绝堆 filter。于是落入 funcref-drop + 值位置守卫（calls.rs:163）→ 阻挡。
修复（2026-06-27 通过各元素类型探针细化）：`filter(pred) ≡ filter_map((x) => if pred(x)
then some(x) else none)`；对于 **List[String]**，捕获式 `filter_map` 已经经由已证明 str-acc 条件追加路径低层化并获 corpus-wall ACCEPT（验证：`xs |> filter_map((s) => if
list.contains(names,s) then some(s) else none)` 字节匹配 `bb,ccc`；条件追加／跳过是已被接受的证书，OwnershipFilter.v 现在证明其健全）。所以 List[String] filter 阻碍只是 filter→filter_map 脱糖（路由缺口，不需新检查器代码）。但 wasm-bindgen 阻碍不是 List[String]，而是 **List[Value]**（来自 `get_arr` 的 `fns`/`types` = `json.as_array`）。捕获式 List[Value] `filter_map` 也被阻挡（已验证）：str-acc 路径 `result_str_acc`（control_p5:131–133）仅匹配 `List[String]`，所以 `List[Value]` filter_map 拒绝 → C2 提升因捕获失败 → 阻挡。

所以精确剩余工作分为：
1. **filter→filter_map 脱糖**（List[String]）——路由缺口，已验证可着手，清除任意 List[String] 捕获式 filter（一般能力，本身不清除 wasm-bindgen 阻碍）。
2. **Value 元素捕获式 filter/filter_map 内联**（3 个 wasm-bindgen 阻碍）——将 str-acc 条件追加路径从 String 泛化至 **Value 元素**（`value_elem_lists` + Value 的 `__list_concat_rc` 已在上方堆元素拼接部件中用于无条件 Value 追加，现在扩展至条件 filter_map 追加）。证书与 String 路径发出的条件追加相同（OwnershipFilter.v 的 CondLoop，String 已获 corpus-wall 接受），因此很可能不需新检查器代码，只扩展 Value 元素路由。门禁：字节匹配 + corpus-wall ACCEPT。
3. **dojo filter_map**（1 个阻碍）——闭包调用 fs.read_text（含 effect），所以在 Value／record 元素条件追加之上还需 effect-monad let 绑定 `!`（#22 前沿）。最困难。
贯穿门禁：字节匹配（捕获式 filter 的真实输出）+ corpus-wall ownership ACCEPT + backend-split 0。已提交 OwnershipFilter.v Coq 核心是三者的健全性基础。

<a id="-step-2b-partial-commit-5a0a9efb-capturing-heap-filter-lowered--wasm-bindgen-32"></a>

### ✅ 步骤 2b 部分完成（提交 5a0a9efb）：捕获式堆 filter 可低层化 → wasm-bindgen 3→2。
捕获式堆 `list.filter`（List[String] + List[Value]）现通过写游标 + 局部平衡的条件获取低层化：在谓词为真的 then 分支内，复制来源元素（Dup `a`），再移入输出列表（Consume `m`）以保留它。因为 `a..m` 在 then 分支内部平衡（else 无操作；输出列表只 alloc 一次，不是 SetLocal 重绑槽），现有平坦检查器不需要循环携带 CondLoop 就可接受。OwnershipFilter.v 的 CondLoop 证明更一般的循环携带形式；此局部平衡形只需基础检查器。control_p5 lower_defunc_list_hof_inner：允许 filter 堆来源+结果，本体仍为 Bool，then 分支 clone+consume，输出在 heap_elem_lists/value_elem_lists 跟踪。List[String]+List[Value] 字节匹配，corpus-wall 18795 ACCEPT，backend-split 0。`generate_wit` 完全清除。

<a id="remaining-org-non-native--3-the-deep-frontiers"></a>

### 剩余（org 非 native = 3）：深层前沿。
- **generate_dts / generate_esm（wasm-bindgen 2）**——不是简单 heap-result-if（所有独立形状——flat_map 条件调用、let 绑定大列表 if、中间拼接 heap-result-if `A + throws_line + B`、顶层 let 绑定 lambda HOF 参数——全可低层化并字节匹配，最后一项于提交 9757d56a 修复）。残留阻碍是 `sigs = supported |> list.flat_map((f) => { … let param_ty = (p: Value) => {…};
  let params_str = params |> list.map((p) => "…${param_ty(p)}…") |> list.join(", "); … })`：let 绑定 lambda 在 flat_map 主体内定义，由内层 map 内联 lambda 调用（`param_ty(p)`），即嵌套 defunc 主体的 let-lambda。顶层形式现在内联（9757d56a，经 lambda_bindings）；flat_map Block 分支也经 lower_stmt 低层化内部语句（所以 `let param_ty` 注册），但合成 `fs |> flat_map((f) => { let tag = (p)=>p+":"+f; ["a","b"] |> map(tag) })`（tag 捕获外层 flat_map 参数 `f`）仍阻挡外层 flat_map。这是外层 str-acc 主体遍历与内层 map(captured-let-lambda) 的更深交互，内层捕获来自外层 HOF 参数。进一步缩小（2026-06-27）为精确的位置依赖 bug：当 `tag` 是 let 绑定 lambda（`let tag = (p) => …`）时，`xs |> list.map(tag)` 在尾部／值位置可低层化（9757d56a 修复；n2/n1d 字节匹配），却在绑定位置被阻挡（`let parts = xs |> list.map(tag); parts`，n1e/n1f/n1h 都报“list.map with an unliftable/closure-list higher-order argument”）。堆（List[String]）和标量（List[Int]）结果、捕获或不捕获都如此。两位置调用相同 lower_pure_module_value_call → try_lower_defunc_list_hof（calls.rs:89/139），且 `tag` 在使用前已无条件注册到 lambda_bindings（binds_p2:280），但绑定位置的 try_lower_defunc_list_hof 返回 None（→ funcref-drop → last_call_had_unlifted_closure=true → binds_p2:642 守卫阻挡），尾部返回 Some（211 清除）。内联 lambda 绑定（`let parts = xs |> map((p) => …)`）在绑定位置可用，所以特指 let 绑定 lambda（Var）的解析因位置而异。defunc 路径不读取 binding_var/binding_is_mutable，因此差异更细微（绑定右值与尾部中 `tag` 的 IR 参数结构，或 lower_bind 在 calls.rs:609 前设置的状态）。下一步：转储 n1d（正常）与 n1e（阻挡）的 MIR ops/args，比较 try_lower_defunc_list_hof 调用处 `tag` 参数节点及 lambda_bindings 状态，专注 IR 级调查。盲修可能掩盖真正不忠实 HOF 阻碍（静默误编译），必须随 ops 差分 + 字节匹配落地，不能猜测。generate_dts/esm 的 `sigs` 恰好触及此形（`param_ty` 为 let lambda 的内层 `params |> list.map((p) => …${param_ty(p)}…)`，以及该 map 的 `let params_str =` 绑定）。
  **✅ 绑定与尾部差异已修复（提交 1dee4752）：let 绑定 lambda HOF 参数在绑定位置也可低层化**（binds_p2 的 data_arg_has_fn 将 let-lambda Var 错当成函数类型数据参数而阻挡；现在认作闭包参数）。n1e/n1f/n1h 字节匹配。但 generate_dts/esm 仍阻挡，let-lambda 并不是它们第一个阻碍。
  **generate_esm 真阻碍（2026-06-27 完整刻画）：(List[String], List[String]) 两堆列表元组累加器 fold**——`tuple_helpers`：`all_tuples |> list.fold(([], []), (state, ty) => { let
  (lines, seen) = state; let sig = …; if list.contains(seen, sig) then (lines, seen) else (lines +
  emit_tuple_helpers(ty), seen + [sig]) })`（去重模式）。累加器是两个堆 List 的元组，本体是返回此元组的 heap-result-`if`。现有 tuple-acc fold（#69）只处理 (List[T], Int)，一个堆列表 + 一个标量；两个堆列表槽（各有 DropListStr，条件性复用一个或构造另一个）是实质性扩展。generate_dts 的 `matrix_iface_block` + `sigs` 是类似深层形状；dojo `backfill_dir` 是 effect-monad（#22）。这些是真正深入、涉及多功能的低层化（非单一路由缺口能力），各需新一轮独立处理。
- **dojo backfill_dir（1）**——闭包调用 fs.read_text 的捕获式 `filter_map`，在 Value／record 元素条件追加之上需要 effect-monad let 绑定 `!`（#22）。
- **porta 29 + sqlite 20 仅 native**——`@extern(rust)` host stub 无 wasm 形式（物理上无法低层化，仅重新分类，是用户记账决策，不是低层化任务）。

<a id="the-5-real-lowering-walls-user-approved-2026-06-27-両方-分離--5-実装--precise-diagnoses"></a>

## 5 个真实低层化阻碍（用户批准 2026-06-27“两个都做：分离 + 实现 5 个”）——精确诊断

@extern(rust) 重分类（39 个 native-FFI host import → 单独类别）已在 org-trust-status.md 完成。要实现的 5 个真实低层化阻碍：

1. **wasm-bindgen generate_dts/esm（2）**——List[Value] 上捕获式 list.filter/filter_map，需要 OwnershipFilter.v CondLoop 集成（条件性获取）。OwnershipFilter.v（Coq 核心）已证明并在门禁中（5d3b642f）；捕获式 String/Value filter 写游标 clone+consume 已低层化（5a0a9efb），残留为 (List,List) 元组 fold 去重 + 嵌套 defunc 形状。
2. **dojo backfill_dir（1）**——闭包调用 fs.read_text 的捕获式 filter_map：effect-monad let 绑定 `!`（#22）+ Value／record 元素条件追加。
3. **porta proxy.start（1）**——heap-result `match` + `validate(c)!`（effect-monad let 绑定 `!`，#22）。
4. **porta observability.format_tool_log（1）**——`"…${if success then "ok" else "error"}"`：嵌入 heap-result-`if` 部分的 StringInterp。精确诊断：手动 ConcatStr `("" + "a=") + (if c
   then "x" else "y")` 可低层化并字节匹配（脱糖形式无问题），但 StringInterp 路径被阻挡。try_lower_string_interp（calls_p2.rs:188–189）脱糖为相同树，再经 `try_lower_concat_str` 低层化（→ lower_call_args 拒绝 heap-result-if 操作数），手动 ConcatStr 返回却经 lower_tail 的 ConcatStr 处理（会物化 if）。相同 IR 树，不同低层化入口。修复：将 try_lower_string_interp 脱糖树路由至 lower_tail 对 ConcatStr 所用的 if 物化路径（而非 try_lower_concat_str），或教 try_lower_concat_str / lower_call_args 物化 heap-result-if 操作数。需 nest（正常）与 sc2（阻挡）的 MIR Op 转储，锁定精确入口差异。属于 #60（StringInterp 脱糖）领域。

#22（effect-monad let 绑定 `!`）是阻碍 2 + 3 的关键。OwnershipLoop 条件获取 + (List,List) 元组 fold 是阻碍 1 的关键。阻碍 4 是上述 StringInterp-if 路径修复。

<a id="portastart--dojo-effect-monad-22--precise-statement--finding-2026-06-27"></a>

## porta.start / dojo effect-monad #22——语句位置 `!` 的精确发现（2026-06-27）

将 porta.start 关键点追踪至对可报错、真实 Result 被调用者的语句位置 effect `!`（`validate(c)!`，其中 `fn validate(c) -> Result[Unit,String]` 构造 ok/err）：
- `lower_effect_call`（calls.rs:438，从 lower_stmt:576 进入的语句路径）去掉 Try/Unwrap（第 10 行），低层化内部调用。此去除是仅尾部适用的捷径（调用就是函数返回时才健全）。
- 对主体中途语句 `validate(c)!`：去掉后 `validate(c)` 生成堆 Result 句柄，(a) 不释放而留在栈上 → 无效 WAT（已验证：`validate(n)!; ok(42)` →“type mismatch at end of function, expected [] but got [i32]”），或 (b) 若释放则丢失 err 传播 → 静默误编译。所以健全低层化需要真正提前返回：`match validate(c) { ok(_) => <drop result,
  continue>, err(e) => return err(e) }`。（标量 Ok 的真实 Result 被调用者上的 let 绑定 `!`——`let x =
  mk(n)!`——已可低层化，lb1；缺口为语句 `!` 与 heap-Ok let 绑定 `!`。）
- 两个子交付：(1) 语句 `!` 提前返回（tag 检查 + 返回 err + 释放 Ok 句柄）；(2) 先让主体中途 can-err heap-Result 语句的去除变成阻挡，而非无效 WAT，作为“绝不发出无效 wasm”的临时修复。完整修复 = effect `!` 提前返回机制（v0 已经通过 emit_early_return_decs 支持；v1 语句／let 绑定路径需要 tag 分支 + Ok drop）。这是 #22 关键，局限于真实 Result 被调用者（不是整个裸值 effect-fn 子系统——validate/json.parse 返回真实 Result，所以分支 tag 存在）。dojo.backfill_dir 的 `dash_item(...)!` let 绑定属于同一家族，另加捕获式 filter_map。

<a id="-correction-2026-06-27-portastartdojo-need-nested-match-desugar-not-a-v1-mir-return-op"></a>

## ⭐ 纠正（2026-06-27）：porta.start/dojo 需要嵌套 match 脱糖，而非 v1 MIR Return 操作

我先前“v1 MIR 没有函数中途提前返回（Return 操作），所以 porta.start + dojo 需要重大结构子系统”的说法错误（要验证，不要断言；通过测试目标形式纠正）。effect `!` 不需要提前返回 Op，它脱糖为嵌套 match continuation（标准 monadic do 脱糖）：

    { before; let x = f()!; after }   →   { before; match f() { ok(x) => { after }, err(e) => err(e) } }
    { before;     f()!    ; after }   →   { before; match f() { ok(_) => { after }, err(e) => err(e) } }

continuation（`after`）嵌入 Ok 分支；Err 分支重构 `err(e)`（与外层函数相同的 `Result[_, String]` 错误类型），无需提前返回、无需 Return 操作。函数尾部变为嵌套 match，现有 heap-result-match 尾部低层化已能处理。

已验证：显式嵌套 match 目标在 porta.start/dojo 使用的每种 Ok 类型上均字节匹配：
- 标量 Ok：`match validate(n) { ok(_) => ok(42), err(e) => err(e) }` → `42`/`bad` ✓
- 嵌套（两个 `!`）：`match validate(n) { err(e)=>err(e), ok(_)=> match parse(n) {err(e)=>err(e), ok(p)=>ok(p+1)} }` → `1`/`bad` ✓
- String Ok：`match parse(s) { err(e)=>err(e), ok(p)=>ok(p+"?") }` → `a!?`/`E:empty` ✓
- record Ok（porta.start 的 `Result[ProxyHandle, String]`）：`match validate(n){err(e)=>err(e),ok(_)=>if … then err … else ok({handle,port})}` → `5:10`/`E:bad` ✓

因此 porta.start（及 dojo 的 effect 部分）剩余工作是 frontend/IR `desugar_effect_unwrap` Block pass：扫描 effect fn 本体 Block 的第一个 `Unwrap`/`Try`（`let x = f()!` Bind 值或裸 `f()!` Expr 语句），拆分前后，改写为上述嵌套 match（Ok 模式 = let 变量或 `_`；Err 模式绑定新 `e`；递归处理 `after` + 各分支）。入口与 `desugar_heap_branches` 并列（mod.rs:1003），传递函数范围 `next_var`（如 block_line 新 VarId 修复）。结构：`IrExprKind::Match{subject,arms}`、`IrMatchArm{pattern,guard:None,body}`、`IrPattern::{Ok,Err,Bind,Wildcard}`、`IrExprKind::ResultErr{expr:Var(e)}`。目标已验证，可着手，不是我错误声称的重大 Return 操作子系统。dojo.backfill_dir 在此之上增加捕获式 effectful `filter_map`（闭包本体逐元素调用 fs.read_text 并产生 Option[record]），那是闭包作为 effectful 循环体的前沿，真正更难的剩余部分。

<a id="-the-remaining-3-lowering-walls-all-reduce-to-one-frontier-the-unliftable-hof-closure-2026-06-27"></a>

## ⭐ 剩余 3 个低层化阻碍都归为一个前沿：不可提升的 HOF 闭包（2026-06-27）

effect-monad `!` 脱糖（9af74ed5）+ porta.start native-FFI 重分类后，剩余 3 个 org 低层化阻碍（wasm-bindgen generate_dts/esm、dojo backfill_dir）都触发同一拒绝：`list.{map,filter,flat_map,filter_map,fold} with an unliftable/closure-list higher-order argument`（calls.rs:166 / binds_p2:651，`try_lower_defunc_list_hof` 返回 None 且 `last_call_had_unlifted_closure`）。三个独立子案例（各经验证，较简单形状全可低层化，只有这些叠加形式阻挡，所以去函数化器是单一关键）：

1. **闭包本体中的嵌套 lambda + 内层 HOF**（generate_dts `sigs`、generate_esm `record_helpers`）。最小复现（阻挡）：`xs |> list.flat_map((f) => { let tag = (p) => if … then "BIG" else p; ["a","Mb"]
   |> list.map((p) => "x:" + tag(p)) |> list.join(", "); [f + "(" + parts + ")"] })`。flat_map 闭包本体定义 let 绑定 lambda（`tag`/`param_ty`），同时运行调用它的内层 `list.map`。`try_lower_defunc_list_hof` 内联外层闭包本体，却不能去函数化嵌套其中的内层 HOF（嵌套去函数化）。带 let 绑定列表的 block 主体闭包可正常低层化（sig1/sig2 验证），仅嵌套 lambda + 内层 HOF 被拒绝。注意：本体仅为 heap-result-`if` 链（没有内层 HOF）的 flat_map 闭包也能低层化（fmc/fm2 验证），所以缺口不是 if 链，而是内层 HOF。
2. **闭包本体内的 effect 调用**（dojo `backfill_dir`）：`task_files |> list.filter_map((f) =>
   match fs.read_text(dir+"/"+f) { ok(c) => some(parse_task_md(f,c)), err(_) => none })`——闭包捕获 `dir`，逐元素运行 effect 调用，产生 `Option[record]`。block 级 effect `!` 脱糖达不到埋在 HOF 闭包里的 effect 调用；闭包作为 effectful 循环体，需要把 effect 传过去函数化后的逐元素循环。
3. **(List,List) 条件式元组 fold**（generate_esm `tuple_helpers`）：`all_tuples |> list.fold(([],[]),
   (st, ty) => { let (lines, seen) = st; if list.contains(seen, sig(ty)) then (lines, seen) else (lines +
   emit(ty), seen + [sig]) })`——两个循环携带堆列表槽，带条件获取（两分支均保持 rc，即已证明 OwnershipFilter.v CondLoop 模式）。`try_lower_defunc_tuple_acc_fold`（control_p5:75）只处理 (List[T], Int)；扩展到 (List[heap], List[heap]) + 条件本体。

关键是扩展 `try_lower_defunc_list_hof`，处理闭包本体中 (a) 内层 HOF + 嵌套 let 绑定 lambda [#1]，(b) effect 调用 [#2]；并扩展 `try_lower_defunc_tuple_acc_fold` 处理 (List,List) 条件 fold [#3]。攻克 #1（嵌套去函数化）可能清除 generate_dts + generate_esm record_helpers；#3 清除 generate_esm tuple_helpers；#2 清除 dojo。均可由门禁验证（字节匹配 + corpus-wall + 证明门禁）。这是最后 org 低层化前沿。

<a id="critical-refinement-sub-case-1-nested-hof-needs-the-nested-loop-cert-a-coq-frontier"></a>

### 关键细化：子案例 #1（嵌套 HOF）需要嵌套循环证书（Coq 前沿）

更深追踪 nl1 拒绝：包含内层 `list.map` 的 flat_map 闭包意味着内层 map 的逐元素循环嵌入外层 flat_map 循环。但 OwnershipLoop.v 已证明的 `Loop` 是平坦的——“循环体平坦（没有嵌套循环）……嵌套循环是将来可组合的扩展”（2026-06-20 落地注记）。所以 `try_lower_defunc_list_hof` 拒绝包含另一 HOF 的闭包本体是健全的（正确拒绝发出证书无法验证的嵌套循环），不只是工程缺口。generate_dts `sigs` / generate_esm `record_helpers`（可变长度列表上的内层 `params |> list.map(… param_ty …)`，无法展开）因此需要**嵌套循环所有权证书**（扩展 OwnershipLoop.v：Loop 本体包含 Loop，组合 rc 保持），属于 #31 级 Coq 前沿，而非只改去函数化器。剩余三项按可着手程度排序：
- **#3 (List,List) 条件式元组 fold**（generate_esm `tuple_helpers`）——最易着手：CondLoop 已证明（OwnershipFilter.v），两个平坦循环携带槽，无嵌套。扩展 `try_lower_defunc_tuple_acc_fold` 至 (List[heap], List[heap]) + 条件本体。今天即可证明。
- **#2 闭包内 effect**（dojo `backfill_dir`）——中等：filter_map 闭包逐元素本体运行 effect 调用，将 effect（Stdout/FsRead cap）传过 defunc 逐元素循环。平坦循环，但闭包内 effect `!`（不同于已交付 block 级脱糖）是新机制。
- **#1 嵌套 HOF**（generate_dts/esm）——最深：先需嵌套循环 Coq 证书。
所以 generate_esm 两阻碍分为 `tuple_helpers`（#3，可着手）与 `record_helpers`（#1，Coq）；dojo = #2。

<a id="-correction--win-2026-06-27-generate_esm-was-the-bounded-duplication-gate-not-coq-now-cleared"></a>

## ⭐ 纠正 + 胜利（2026-06-27）：generate_esm 原因是有界复制门禁（非 Coq），现已清除

对真实 wasm-bindgen mod.almd 的全模块二分（唯一可靠方法——独立 render_program 不链接 string.starts_with/list.contains，所以孤立复现误导）纠正了我反复的误诊：
- **generate_esm**：不是嵌套 HOF，不是 Coq。阻碍是 `desugar_let_bound_heap_branch` 的有界复制门禁（mod_p6.rs:1238，此前为 `rest_branch_binds > 2`）。generate_esm 叠加 **4 个顶层可选列表 `if`**（matrix_helpers/bytes_helpers/import_shim/shim_noop，每个为 `if cond then [LITERALS]
  else []`）；第一个看到 rest=3 > 2 → 门禁退出 → 合并的 let 绑定 `if` 报“no sound scope-end drop”。门禁提高为 `> 3`（≤ 2^4 = 16 叶；尾部复制已证明逐分支平衡）就清除了 generate_esm。验证：corpus-wall ownership **18854→19338 ACCEPT**（新增 484 个 corpus let 绑定 if 可复制并低层化，全经内核重新验证），output-parity 124/124，corpus-wall 快速（1:52，无爆炸）。**wasm-bindgen 2→1。**
- **generate_dts**：确是真正嵌套循环前沿。二分将阻碍定位至 `sigs` flat_map 闭包的 `param_ty`；`params |> list.map((p) => "${name}: ${param_ty(p)}") |> join` 是嵌在外层 flat_map 循环中的内层 map（去掉 param_ty 即清除 generate_dts）。循环体内的循环在 OwnershipLoop.v（平坦 Loop 本体）不可表示，需**嵌套循环所有权证书（Coq）**。
- **dojo backfill_dir**：闭包内 effect 调用（`filter_map` 本体调用 fs.read_text 并产生 Option[record]），即 defunc 循环内 effect 机制。

净结果：低层化阻碍 **3→2**（generate_esm 清除）。剩余 2 = generate_dts（嵌套循环 Coq）+ dojo（闭包内 effect）。“深层／Coq”元模式只对了一半：generate_esm 是一行门禁（再次适用“深层反复被实证证明是误判——逐壁验证”的教训），但 generate_dts 嵌套循环是真正 Coq 前沿。

<a id="dojo-backfill_dir--precise-the-blocker-is-an-effect-call-as-the-filter_map-closures-match-subject-2026-06-27"></a>

## dojo backfill_dir——精确阻碍：effect 调用作为 filter_map 闭包 match subject（2026-06-27）

对真实 dojo 模块的全模块二分（可靠方法）精确锁定 backfill_dir 阻碍：`task_files |> list.filter_map((f) => match fs.read_text(dir+"/"+f) { ok(c) => some(parse_task_md(f,c)),
err(_) => none })`。两个切分：
- 将 `fs.read_text`（effect）替换为纯调用，但保留 match + Option[record] 结果 + `dir` 捕获 → **backfill_dir 可低层化（0 阻碍）**。因此闭包内 match、`Option[Task]`（record）结果、`List[Task]`（List[record]）filter_map、`dir` 捕获全已可用。
- 唯一阻碍为去函数化 filter_map 循环体内作为 match subject 的 effect 调用 `fs.read_text`。（不是 List[record]，不是 Coq——又一个“深层”误诊经二分纠正。）
fs.read_text 顶层低层化正常（ReadTextFile prim、FsRead cap，已交付 dojo 3→1），但 defunc 本体 match subject 低层化（append_body_to_str_acc / append_variant_match_to_str_acc / 堆元素内部路径）拒绝 effectful Module 调用作为 subject。修复是让 defunc 逐元素本体将已允许的 effectful 标准库调用（fs.read_text → CallFn + FsRead cap-witness）低层化为 match subject，将 cap 传至外层函数 declared_caps，并拥有／释放逐轮 Result。这是 defunc 循环内 effect 机制，实质性工作但不是 Coq（不同于 generate_dts 真正嵌套循环）。因此最后 2 个 org 低层化阻碍为：**dojo = defunc 循环内 effect（工程）**，**generate_dts = 嵌套循环所有权证书（Coq）**。

<a id="dojo-backfill_dir--the-hoist-approach-is-verified-correct-but-my-desugar-impl-non-terminated-reverted"></a>

## dojo backfill_dir——提升方式已验证正确，但我的脱糖实现未停止（已回退）

二分证明修复方向：手动将 `filter_map((f) => match fs.read_text(p) { ok(c) =>
some(parse(f,c)), err(_) => none })` 改写为 `filter_map((f) => { let r = fs.read_text(p); match r { … } })`，会让 backfill_dir 可低层化（defunc 内联器可处理逐元素 let 绑定中的 effect 调用，却不能处理被拒绝的 variant-match subject）。所以 dojo 修复是脱糖：把 HOF 闭包 lambda 本体内非纯／effect variant-match subject 提升为 let 绑定。不是 Coq，也不是 List[record]（纯 subject／提升形式均已验证可用）。

我实现了它（`desugar_hof_match_subject_hoist` + `hoist_hof_call_lambda_subject` + `hoist_variant_subject`，接入 `desugar_heap_branches_inner`），却发生栈溢出（render exit 134、空 wat——我最初看到“dojo total: 0”是崩溃抑制 Unsupported 行，不是真正清除；验证而非断言通过空 wat + h3 合成也溢出发现它）。脱糖纸面似乎幂等（提升后的 `match Var {…}` subject 纯，不会再触发），但 lower_body_into / desugar_heap_branches 不动点未停止，是微妙交互（可能 desugar_nested_branch_arms 递归，或同级脱糖对提升形式再触发），我无法在极长会话尾声安全定位。已干净回退（工作树回到 lowering-walls=2，build 0 错误，dojo 保持诚实阻碍，generate_esm 仍已清除）。新会话 TODO：用停止守卫重新实现提升（如“恰好改变一次”的不动点，或进入 HOF lambda 参数的单次非循环 pass），再以 Result-ok/err + Option[record] filter_map fixture + corpus-wall + oracle 验证字节匹配。方向已证明，仅剩可停止实现。

<a id="dojo-hoist--second-attempt-one-shot-lower_body_into-fixpoint-also-stack-overflowed--it-is-the-lowering-not-the-desugar-loop"></a>

### dojo 提升——第二次尝试（单次 lower_body_into 不动点）也栈溢出 → 是低层化，不是脱糖循环

将提升重新实现为自递归、每 pass 只提升一次的变换，在 lower_body_into 的 `if let Some` 不动点应用（位于 desugar_heap_branches 循环外，避开首次循环交互）。构造上应收敛：每 pass 提升一个 subject，提升后的 `match Var {…}` subject 纯，所以不再提升（幂等 → 第二 pass 返回 None）。纸面上脱糖不动点停止。然而真实 dojo 及 h3 合成的 render 仍栈溢出（exit 134）。因此非停止不是脱糖不动点，而在低层化提升后的形式（defunc 内联 `{ let r = <effect/call>; match r {…} }` lambda 本体时递归，或下游 pass 重入）。定位需要调试器／插桩（递归深度追踪），不能依赖已失败两次的盲目幂等性推理。两次均干净回退（lowering-walls 回到 2，build 0 错误，dojo 诚实阻碍 exit-2）。新会话：插桩提升后 HOF 闭包 `{let r=…; match r{…}}` 的低层化寻找递归，或走另一条路（扩展 defunc 本体低层化以直接接受 effect 调用 variant-match subject，像顶层 variant-match 那样物化，而非提升）。方向（effect match subject 须变为逐元素绑定／物化值）已证明，安全机制仍未决。两次盲试就是停止、下次使用调试器的信号。

<a id="dojo-hoist--sharpened-debugger-pass-direction-is-verified-my-desugar-builds-a-malformed-tree"></a>

### dojo 提升——细化（调试器调查）：方向已验证，我的脱糖构造了畸形树

谨慎且检查退出码的二分（先前“0 阻碍”是崩溃假阳性——render exit 134 发出空 wat + 0 行“Unsupported”，所以 grep Unsupported 读到 0）：
- **真实 dojo 手动预提升**（将 `filter_map((f) => { let rr = fs.read_text(p); match rr {…} })` 编辑进 backfill.almd 源码）→ backfill_dir 清除（无 `backfill_dir:` 阻碍；exit-2 是无关模块级阻碍）。所以提升方向真正验证：树来自 frontend 时，defunc 可低层化 Block 本体 `{let rr=…; match rr{…}}` filter_map 闭包。
- **同一逻辑程序由我的脱糖构造提升** → 栈溢出（exit 134）。同一逻辑形状，但程序构造的 IR 不同于 frontend，导致低层化崩溃。
- （最小合成手动预提升 `m_str`/`m_rec` 被阻挡而非低层化，是不同结果类型路径，不是 dojo 形状。手动真实 dojo 测试验证的正是 dojo 特定形状。）
所以 bug 不是低层化（手动树可低层化），也不是脱糖不动点（幂等、收敛），而是我的脱糖生成了相对 frontend 的畸形树。主要嫌疑：(1) 改写 lambda 保留原 `lambda_id`（`lambda_id: *lambda_id`），旧 ID 的缓存提升形式／捕获不再匹配新本体（试 `lambda_id: None`）；(2) 相对 frontend 正规形的 Block 结构／类型不匹配。新会话：对手动预提升源码做 `--emit-ast`，与脱糖输出 IR 转储比较，锁定畸形点（看到后几乎肯定只需改 1 字段）。提升是 dojo 的正确修复，仅剩树构造细节。已回退（lowering-walls=2，干净）。

<a id="dojo-hoist--third-attempt--backtrace-the-loop-is-the-lower_body_into-fixpoint-re-applying-a-non-idempotent-hoist"></a>

### dojo 提升——第三次尝试 + 回溯：循环是 lower_body_into 不动点重施非幂等提升

插桩（HOIST_DEBUG env）：栈溢出前提升触发 **874 次，全部 tmp=VarId(5)**。第 2 次触发时 panic 的回溯锁定循环：
```
hoist_variant_subject  (self-recursive ×3 — descends nested Blocks)
hoist_hof_call_lambda_subject
desugar_hof_match_subject_hoist (×2)
lower_body_into  (×5 — the `if let Some(r)=hoist(body){return lower_body_into(&r)}` fixpoint)
```
回溯说明：hoist_variant_subject 自递归 ×3，进入嵌套 Block；经 hoist_hof_call_lambda_subject、desugar_hof_match_subject_hoist（×2），到 lower_body_into（×5），其不动点在提升返回 Some 时再次进入 lower_body_into。
所以 lower_body_into 提升不动点永不收敛：每 pass 我的提升返回 Some（对自身输出重触发），lower_body_into 递归 → 溢出。每 pass tmp=VarId(5)（从不增加），尽管所构树正确（转储：`Bind{var:5, ty:Result, value:mk(Var2)}` + `Match{subject:Var(5)…}`，subject 确为纯 Var(5)），且 walk_expr 有 Lambda 分支。非幂等根因（为何 hoist_variant_subject 对纯 subject 的 `{let 5; match Var(5)}` 再触发，为何 nv 保持 5）无法由纯推理 + 转储 + 回溯解决，需逐步执行每 pass 精确树变更（在每层 lower_body_into 前后打印 body）。已干净回退（第 3 次；lowering-walls=2，build 0 错误，dojo 诚实阻碍）。可能修复（新会话）：不要在 lower_body_into 递归 `Some→re-enter` 不动点里应用提升，而是在函数本体入口（mod.rs 约 1003 行，lower_body_into 之前）作为单次全树 pass 仅应用一次，使再次触发在结构上不可能。提升方向仍经验证（手动真实 dojo 预提升清除 backfill_dir）；bug 仅在脱糖不动点位置／幂等性。

<a id="dojo-hoist--fourth-attempt-single-pass-at-function-entry-fixes-the-crash-but-the-tree-still-walls--regresses"></a>

### dojo 提升——第四次尝试（函数入口单 pass）：修复崩溃，但树仍阻挡 + 回归

来自回溯的修复：在函数本体入口（mod.rs 约 1003 行，desugar_heap_branches 前）以自顶向下完整 pass 仅做一次提升，不放在 lower_body_into 不动点。结果：栈溢出消失（exit 2 而非 134），入口单 pass 确为停止修复。但 backfill_dir 仍报“filter_map unliftable”（与未提升相同），且 dojo 总数 1→2（别处回归）。因此即使正确停止，我的脱糖构造 `{let $r = subj; match $r{…}}` 树也不等价于 frontend 手动预提升（已验证能清除 backfill_dir）：defunc 仍拒绝我的树，计数侧（count_ir_calls 看不见仅入口提升）可能引入 +1。不可削减的阻碍已确认：脱糖构造 IR 与 frontend IR 的差异使 (a) defunc 不低层化，(b) 破坏两侧之前脱糖的 caps 不变量。四次尝试（2 次栈溢出、1 次仅插桩、1 次停止但阻挡+回归）均干净回退。确定的新会话计划：(1) 输出／转储手动预提升源码的 frontend IR，与脱糖构造树比较（畸形就在其中）；(2) 在共享的两侧之前脱糖路径应用提升（count_ir_calls 也能看到），仅入口会破坏 caps 不变量。方向保持已验证；未决为脱糖 IR 忠实性 + 门禁位置。lowering-walls=2。

<a id="dojo-hoist--fifth-attempt--ir-diff-correction-my-baseline-was-the-walling-synthetic-not-the-clearing-real-dojo"></a>

### dojo 提升——第五次尝试 + IR 差分纠正：基线是被阻挡的合成，不是已清除的真实 dojo

在函数本体入口（mod.rs）以单次全 pass 仅应用一次提升，确实修复栈溢出（第 5 次：exit 2 而非 134），但 backfill_dir 仍“filter_map unliftable”。IR 转储（DUMP_IR env 探针）确认 backfill_dir 的 filter_map lambda 本体就是 `Match { Call read_text, [ok,err] }`，恰好是提升目标，所以提升确实触发，却仍阻挡。此前暗示“我的树与手动结构相同”的 IR 差分，是对手动合成（纯 `mk` 的 `proc`、Option[String]）做的，但该合成本身就被阻挡（m_str）。只有手动真实 dojo（dojo3，fs.read_text + Option[record]）曾观察到清除 backfill_dir。因此我把输出与被阻挡基线相比，从未转储清除基线（真实 dojo 手动）的 IR。纠正后的新会话计划：(1) 重新验证手动真实 dojo 预提升确实清除 backfill_dir（退出码 + 逐函数阻碍，不仅“没有行”）；(2) 若真如此，转储清除树的 backfill_dir IR，与真实 dojo 上我的提升输出比较（差异即畸形）；(3) str_acc／堆元素 filter_map 路径也许根本不接受任何 `{let r; match r}` 本体闭包（合成 m_str 也阻挡），那么提升是错误机制，修复应扩展 defunc 本体低层化（append_body_to_str_acc / lower_defunc_list_hof_inner），直接接受 effect 调用／非纯 match subject。五次尝试（2 次溢出、1 次插桩、1 次停止但阻挡+回归、1 次入口单次仍阻挡）均干净回退（lowering-walls=2）。“提升方向已验证”现存疑（只有真实 dojo dojo3 清除，而该单次读数本身可能是 render 在首个阻碍停止的假象，必须先重新验证）。

<a id="dojo-backfill_dir---the-precise-fix-point-corrects-5-hoist-attempts-defunc-variant-match-handler-is-option-only"></a>

### dojo backfill_dir——⭐ 精确修复点（纠正 5 次提升尝试）：defunc variant-match handler 仅支持 Option

提升是错误机制（5 次尝试）。阅读 defunc 本体低层化锁定根因：`append_variant_match_to_str_acc`（control_p5.rs:1207）低层化去函数化 filter_map/flat_map 闭包内的 `match subj { … }`，却仅处理 `IrPattern::Some` / `IrPattern::None`（约 50–58 行扫描 Some/None 分支），不处理 `IrPattern::Ok` / `IrPattern::Err`。dojo backfill_dir 闭包为 `match fs.read_text(p) { ok(content) => some(…), err(_) => none }`，是 Result（ok/err）match，所以 handler 找不到 Some/None → 返回 None → filter_map 报“unliftable”。（合成 m_str 的 `match mk(x) { ok(v)=>some(…), err=>none }` 因同一原因被阻挡，确认是 handler 而非 subject／提升。）修复（新会话，不是提升）：扩展 `append_variant_match_to_str_acc`（及 List[record] 结果的 record／Value-result 同级）同时接受 `Ok`/`Err` 分支，把 `ok(x) => some(e)` 当作 `some(x) => some(e)`（Ok 载荷为保留元素），把 `err(_) => none` 当作 `none`（跳过）。subject 的 effect 调用（fs.read_text）随后作为逐元素循环内 match subject 低层化（handler 已物化 subject，约第 31 行 `CallArg::Handle`）。这是局部 handler 扩展（Option→Option+Result），不是破坏树的提升，也不是 Coq。验证：扩展 handler 后，用 Result-ok/err filter_map fixture + corpus-wall + oracle 做字节匹配。此项取代上方全部提升注记；放弃提升，Ok/Err handler 扩展才是路径。

<a id="dojo--refined-fix-point-the-exact-gates-handler-is-self-host-option-call--somenone-gated"></a>

### dojo——细化修复点（精确门禁）：handler 受自托管 Option 调用 + Some/None 限制
`append_variant_match_to_str_acc`（control_p5:1207）按 (1) `is_self_host_option_call(subject)`（1221）及 (2) Some/None 分支（1256/1278）门控。dojo 的 `match fs.read_text(p) { ok(content)=>some(…), err(_)=>none }` 两项都失败：fs.read_text 返回 Result（非自托管 Option 调用），分支为 Ok/Err。因此新会话修复 = 扩展此 handler（及 dojo 实际使用的 List[record/Value]-result 同级路径，因为 parse_task_md 返回 record／Value 而非 String），允许自托管 Result 调用 subject（增加 `is_self_host_result_call`）、Ok/Err 分支（Ok≈Some 保留，Err≈None 跳过）、反向 tag（Result Ok = tag==0，Option Some = tag!=0；`is_result_ty` 已区分）。subject 物化（lower_call_args → CallArg::Handle，第 1232 行）已处理任意堆调用，所以 effect 调用作为逐元素 subject 低层化，折入 FsRead cap。这是健全、局部的机制，5 次提升尝试是远路；应扩展 handler，不是重写树，也不是 Coq。lowering-walls=2。

<a id="dojo--final-path-confirmation-result-is-listrunresult-record-so-it-is-the-recordvalue-path"></a>

### dojo——最终路径确认：结果为 List[RunResult]（record），所以是 record／Value 路径
`parse_task_md` 返回 `RunResult`（record，dojo types.almd），所以 backfill_dir 的 `filter_map` 构造 List[record]，不是 List[String]。因此修复不在 append_variant_match_to_str_acc（String 累加器路径），而在 record／Value 元素 filter_map handler（lower_defunc_list_hof_inner 的堆元素列表路径／逐元素 variant-match 本体低层化）。同一扩展应用于此：允许自托管 Result 调用 subject + Ok/Err 分支 + 反向 tag（Ok=tag==0），逐元素物化 effect 调用 subject（折入 FsRead cap），Ok 时保留由 Ok 载荷构造的 record，Err 时跳过。因此完整 dojo 新会话计划：(1) 找到 record／Value 元素 filter_map variant-match 本体 handler，(2) 扩展 Result/Ok-Err + Result 调用 subject + 反向 tag（对应现有 Option/Some-None 逻辑），(3) Result-ok/err → List[record] filter_map fixture + corpus-wall + oracle 字节匹配。这是健全、局部机制（5 次提升尝试是远路）。lowering-walls=2。

<a id="-dojo-filter_map-wall-solved-workflow-commit-e44637b0--the-hoist-was-wrong-write-cursor-keepskip-was-right"></a>

### ⭐ dojo filter_map 阻碍已解决（Workflow，提交 e44637b0）——提升错误；写游标保留／跳过正确
5 次提升尝试采用了错误机制。实际修复（由 dojo-record-variant-handler Workflow 找到）：`try_lower_defunc_list_hof` 没有结果为 List[heap-non-String]（record／Value）的 filter_map 分支。新增 `result_filter_map_heap` → `lower_defunc_filter_map_hof`：写游标结果列表（如 `filter`）保留 Ok/Some 分支构造的拥有所有权元素（lower_heap_result_arm 证书 `i` + Consume `m`），跳过 Err/None 分支；按 len(xs) 分配，循环后把长度修为游标值。这就是已证明的捕获式 filter 条件获取（5a0a9efb）。无需提升，无需树重写。验证：corpus-wall 全 ACCEPT（ownership 19338 / caps 3432），parity 124/124，oracle backend-split 0，scalar+str-Result → List[record] 字节匹配。backfill_dir 的 filter_map 阻碍已消失。上方提升注记被取代；答案是 defunc handler 扩展（新组合子分支，而非脱糖），正是第 4 次尝试后指出的“扩展 defunc 本体低层化”替代方案。

<a id="dojo-backfill_dir--now-walls-on-fswrite-the-last-dojo-wall-a-new-capability-effectful-brick"></a>

### dojo backfill_dir——现被 fs.write 阻挡（最后 dojo 阻碍，需新 capability 的 effectful 部件）
filter_map 清除后，backfill_dir 在 `fs.write(path, content)` 被阻挡：“effectful/impure stdlib Module call fs.write needs a declared capability not in this brick”（一直被 filter_map 阻碍掩盖）。fs.write 不在 is_admitted_effectful 中（calls.rs:216，列有 random.int/env.args/fs.read_text/fs.list_dir）。修复对应 ReadDir 部件，但需新 capability FsWrite（不是 FsRead）：(1) 在 is_admitted_effectful 允许 fs.write；(2) 自托管 fs_write.almd → prim.write_text_file；(3) Op::Prim::WriteTextFile（lib.rs）携带 Capability::FsWrite；(4) 低层化 prim.write_text_file（calls_p4.rs，参照 read_text_file:880）；(5) v1 wasm runtime 函数（path_open O_CREAT|O_WRONLY + fd_write）；(6) Rust enum 与 Coq Capability registry（单一来源，#35）中的 Capability::FsWrite + cap_witness + caps 门禁。新 capability + Coq registry 单一来源是健全性关键部分。这是 effectful-27／WASI-write 下限（#61 家族），是真实部件，也是 dojo 达到 0 的最后障碍。

<a id="-final-frontier-2026-06-29-real-lowering-walls--3-all-porta--the-genuine-targeted-irreducible-floor"></a>

### 🎯 最后前沿（2026-06-29）：真实低层化阻碍 = 3（全为 porta）——真正不可由定向方案削减的下限
本次会话把 org 真实低层化阻碍从约 89（分类前混在一起）降至 **3**，清楚分离 native-FFI（porta wasm_rt @extern + sqlite + process/http，排除 44 个），16/17 仓库 wall=0。清除的每个阻碍均通过完整门禁（corpus-wall 全 ACCEPT + Coq PROOF SPINE OK + 三方 oracle backend-split 0 + parity 124/124 + 字节／泄漏检查）；4 个新 effectful WASI prim 以 Coq 通过状态落地（FsWrite=4、Clock=5、Stdin=6、PathExists + fs.mkdir_p/remove_all 复用 FsRead/FsWrite）；HOLE-1（门禁不可见的 record-Ok 平坦 drop 泄漏）被证明并以永久 wasmtime 回归测试关闭。

剩余 **3 个是定向方案不可削减的深层前沿**（各经对抗式设计+批评反复得出“没有健全的仅生成端／仅低层化修复”；需要真正研究，而非物化器分支）：
1. **config.almd::load_porta_config**——A2 非对称合流 + **6 个以上捕获堆的 list.map/filter_map 闭包**被去函数化器阻挡。需要闭包捕获去函数化（环境 record 提升并保持直接调用，不是一等函数值，后者会破坏 caps 证明），属于 C2 领域，深层。
2. **ops.almd::list_instances**——**循环内 effect-monad**：`for d in dirs` 主体内的 `fs.read_text()!`，err 必须终止循环并从函数返回 err（循环携带提前退出）。组合 #76（effect ! 值位置）与循环前沿，深层。
3. **jsonrpc.almd::read_message**——#77/#78 **循环携带累加器生成端**：TCO-while String／缓冲累加器，其形状不能归约为已证明的无条件净 0 循环槽证书（OwnershipLoop.v 已落地，但此特定缓冲形状的生成端低层化没有健全的仅生成端路径）。

这些是定向方法的诚实终点。清除需要多会话研究（闭包捕获去函数化／循环内 effect-monad 提前退出／read_message 循环生成端证书），成功不确定。无论如何，native-FFI 意味着字面意义的全仓库零在结构上不可能；已达到的下限是“真实低层化阻碍 = 3”（全 porta，全已刻画）。

<a id="refined-2026-06-29-the-3-floor-walls--2-genuine-research-themes-all-no_sound_path-empirically-proven"></a>

### 细化（2026-06-29）：3 个下限阻碍 = 2 个真实研究主题（均为 NO_SOUND_PATH，实证证明）
三个专门调查 Workflow 确认剩余 3 个 porta 阻碍没有健全的定向／集成路径，可归为两个研究主题：

A. **循环内 effect-monad（非局部控制）**——2 个阻碍，均实证 NO_SOUND_PATH：
   - jsonrpc.almd::read_message——TCO 合成整个 Result 累加器（rv=ok(...)），在多条条件提前退出基例路径重赋值，加上主体中途 effect ! 传播。不是已证明净 0 追加槽（OwnershipLoop.v 覆盖无条件 buf=buf+chunk；此处是条件性写一次的 Result acc + effect-monad 提前退出）。
   - ops.almd::list_instances——`for d in dirs` 主体内 `fs.read_text()!`，Err 必须结束循环并从函数返回 err。
   两者都需要健全低层化“穿过循环的 effect ! 提前返回”：循环携带 Option[err]／遇 err break + 传播，逐轮释放堆。v1 主干没有非局部 Return，需要新循环内 effect-monad 脱糖及其所有权证书。真正多会话研究（#76 x loop）。

B. **闭包捕获去函数化（C2）**——1 个阻碍：
   - config.almd::load_porta_config——6 个以上捕获堆的 list.map/filter_map 闭包被去函数化器阻挡。需要保持直接调用的环境 record 提升（不能用一等函数值，会破坏 caps 证明）。

结论：真实低层化阻碍 = 3 是定向／集成方法已证明的下限（6 个专门 Workflow：loop-cert producer、CondLoop-extract、conditional-acquire-5、list_instances、read_message 均返回 NO_SOUND_PATH，或被对抗式批评识别为过度设计）。清除 3 个 = 专门多会话研究计划，针对 (A) 循环内 effect-monad 与 (B) 闭包捕获去函数化，成功不确定。native-FFI（44）意味着无论如何，字面意义全仓库零在结构上不可能。

<a id="definitive-2026-06-29-real3-is-the-proven-research-floor--the-path-to-0-is-a-multi-component-research-program"></a>

### 定论（2026-06-29）：real=3 是已证明研究下限——通往 0 是多组件研究计划
主题 A（循环内 effect-monad）设计被对抗式反驳：read_message + list_instances 都有逐轮第三条路径（skip/continue，所有权形状 []），所以循环槽不可避免地条件化（CondLoop：then=[Dec;Inc] / else=[]），不是单一本体 CLoop。对它使用 CLoop 证书会接受但未建模（check_line_unroll_sound 的 UnrollsL 只含 n 份相同复制）= 不健全。折叠成无条件重包装会在 Err 后重新运行 effect（破坏 parity）。因此清除 2 个确实需要：(1) 将 OwnershipFilter.v CondLoop 提取至 checker.ml（ccheck_unroll_sound 已证明，但当前只提取 CLoop）；(2) 将多个逐轮槽（status + 结果累加器）+ 临时 f(x) wrapper／移出的 Err 载荷建模为独立对象，履行 break 路径 drop 义务。这是真正多组件研究计划，不是单一部件。

WALL=0 路径（专门多会话研究计划，成功不确定）：
- CondLoop checker.ml 提取（Coq 移植 OwnershipFilter CondLoop → OwnershipChecker.v → 重新生成 checker.ml + parse_clc），关系健全性，触及信任内核。
- CondLoop 之上的循环内 effect-monad 脱糖（循环携带 Result status + 条件 skip/continue + break 路径 drop），清除 read_message + list_instances。
- 闭包捕获去函数化（环境 record 提升保持直接调用，绝不使用一等函数值），清除 load_porta_config。

定向／集成证据已用尽（本会话）：loop-cert producer = NO_SOUND_PATH；CondLoop-extract = 对 serialize_opts 是过度设计（批评），但条件循环槽确实需要；A2 conditional-acquire-5 = 3 个可着手项清除（fs.exists）+ 2 个 A2-hard；list_instances + read_message 调查 = NO_SOUND_PATH；主题 A 设计 = 被反驳（需 CondLoop + 多槽）。真实低层化阻碍 = 3 是下限。native-FFI（44）使字面意义全仓库零无论如何都结构性不可能。

<a id="step-2-refutation-2026-06-29-the-decisive-blocker-is-step-15--verify_ownership-does-not-model-intra-iteration-control-flow"></a>

### 步骤 2 反驳（2026-06-29）：决定性阻碍是步骤 1.5——verify_ownership 不建模迭代内控制流
循环内 effect-monad 脱糖（步骤 2）被对抗式反驳，决定性发现（H-A）是：可执行镜像 verify_ownership（lib.rs:1117–1119）与证书生成器（certificate.rs:127）都把循环体当作一个平坦迭代遍历，LoopBreakUnless / IfThen / Else 均视为无操作。它们不建模 break 跳过本轮余下部分，也不跨 if 分支快照／恢复 rc。后果：
- !-break 前获取、只在 break 后 continue 路径释放的临时值，被镜像及 Coq 证书都计为平衡，却在 break 轮泄漏。因此忘记 break 路径 drop 的脱糖被不健全地接受（镜像不能确认忠实性，H3 当前无法履行）。
- 反之，在两个 if 分支都释放的临时值，在平坦遍历里减两次，导致假 DoubleFree。
CCondLoop 内核证书（步骤 1，已落地）仅证明单个携带 status/rv 槽的 rc 增量；主要泄漏面（跨多个 !-Err 分支释放的逐轮临时值、TCO 回边 drop、从 Result 移出时的同一性）在证书之外，也在镜像当前可见范围之外。

细化的 wall=0 研究路径（每步均真实、关系健全性，成功不确定）：
- 步骤 1：CondLoop 提取——完成（提交 19474461，任务 #78）。
- 步骤 1.5：扩展 verify_ownership + 证书生成器，建模迭代内控制流（LoopBreakUnless 跳过本轮余下部分；IfThen/Else 逐分支快照+恢复 rc/dead）。这是信任内核可执行镜像的基础性变化，必须保持全部 4517 函数 corpus-wall 全 ACCEPT。决定性阻碍。
- 步骤 2：循环内 effect-monad 脱糖（需 1.5）→ read_message + list_instances。
- 步骤 3：闭包捕获去函数化 → load_porta_config。
真实低层化阻碍 = 3 维持；在步骤 1.5（基础性验证器变化）落地前，它就是下限。

<a id="absolute-floor-2026-06-29-the-effect-monad-in-loop-walls-require-a-foundational-effect-fn-abi-redesign-not-a-research-brick"></a>

### 绝对下限（2026-06-29）：循环内 effect-monad 阻碍需要基础性 effect-fn ABI 重设计（不是一个研究部件）
步骤 1.5+2 组合设计被最深、决定性发现反驳：清除 read_message + list_instances 不是“深层研究”，而是需要改变一个刻意的 v1 设计决定，并扩展多个证明：
1. **v1 MIR 从用户 effect fn 返回裸值，不把返回值包装为 runtime 带 tag Result**（lib.rs lower_body_with_globals；417–431 行 ⛔ 定论注记）。两个阻碍都确实会报错（Err 路径要求字节匹配），因此 never-err 去除（b154a270）不适用；无 runtime Result tag 时 err-break／传播边是静默误编译，恰好是已回退（293–305 行）的一概去除②陷阱。因此循环内 effect-monad 脱糖需 runtime effect-fn Result-tag ABI，而 v1 刻意省略它。
2. **CCondLoop（步骤 1）只证明两个平坦净 0 分支上的仅 continue 循环**（`list FlatOp`，“无嵌套循环”）。脱糖的 `match status { ok => match f(x){ok,err} }` 是 then 分支包含另一个条件的 CondLoop，CCondLoop 无法表达（需嵌套条件 Coq 泛化）；err-break 退出边（部分 live 集合仍在时于迭代中途离开）没有 Coq 对应，等同事实上的新公理。
3. 增强镜像（跨 if 分支快照／恢复）需要栈 + rc-map 规范化，否则会误拒绝平衡的 corpus 函数（4517 门禁违例风险）。

结论：真实低层化阻碍 = 3 是此代码库当前设计的绝对下限。通往 0 是基础性重设计（runtime effect-fn Result-tag ABI + CondLoop 嵌套条件 + break-exit Coq 定理 + 基于栈的镜像），是触及刻意 v1 设计决定的 v2 规模计划，不是自主研究部件。是否开展应由用户决定，而非自主循环。native-FFI（44）使字面意义全仓库零无论如何都结构性不可能。已落地的真实进展：CondLoop 提取（#78，条件循环槽内核基础）仍是首个可复用部件。

<a id="recursiontco-gate-sidestep-also-refuted-2026-06-30--real3-confirmed-across-all-angles"></a>

### 递归／TCO 门禁绕路也被反驳（2026-06-30）——所有尝试角度均确认 real=3
一个创造性的自我批评角度（完全避开循环：门控 try_tco_rewrite，让主体中途含 effect ! 的 effect fn 经已证明尾部 effect-unwrap 低层化为普通递归，即 Almide 以递归代替循环的惯用法）在真实 porta 源码上试验（确认位于 /Users/o6lvl4/workspace/github.com/almide/porta，先前 Workflow 仅查 worktree+cache 而误报不存在）。结果：最小重建递归 effect ! fixture 可干净低层化（Ok+Err 字节匹配），但真实 read_message（jsonrpc.almd:39–65，嵌套 if/else、多处 ok(...) 返回 + `else read_message()` 尾递归 + 主体中途 parse_and_wrap(body)!）不能归为该干净形状；即使用 TCO 门禁，仍报“while body with heap-accumulator reassignment”。list_instances 递归重写使 MIR ACCEPT，却字节匹配失败。递归绕路在真实函数上也被反驳。real=3 现已在所有尝试角度确认为下限：定向低层化（已用尽）、CondLoop 提取（完成/#78）、循环内 effect-monad 脱糖（反驳——基础性 effect-fn Result-tag ABI）、递归／TCO 门禁绕路（反驳——真实 read_message 嵌套结构不能变为干净递归）。wall=0 路径仍是 v2 规模计划（effect-fn Result-tag ABI + CondLoop 嵌套条件／break Coq + 栈镜像，或针对真实嵌套形状的更深递归低层化），不是自主部件。

<a id="load_porta_config-true-root-2026-06-30-direct-main-agent-diagnosis--corrects-3-workflow-misdiagnoses"></a>

### load_porta_config 真根因（2026-06-30，主 agent 直接诊断——纠正 3 个 Workflow 误诊）
3 个 Workflow 误诊了 load_porta_config（a/ 闭包捕获但修错位置；b/“filter_map 闭包返回 heap-match”；c/“多堆字段 record-Ok”）。直接 5 层追踪 + 二分至 3 行最小复现，找到实际根因：

```almide
type P = { a: String, b: String }
fn f(cap: String, keys: List[String]) -> List[P] =
  keys |> list.map((k) => { let e: P = {a: k, b: cap}; e })   // 🔴 WALL "list.map unliftable/closure-list"
```

代码注释翻译：🔴 被“list.map unliftable/closure-list”阻挡。
精确触发条件（实验隔离）：`list.map`/`filter_map` lambda **捕获自由变量且生成 record 元素**。隔离证明：捕获→标量元素可用（#67），捕获→json-scalar 可用，非捕获→record 元素可用（lift_lambda 救回）；仅捕获 + record 元素阻挡。load_porta_config 中是 `env_vars = env_keys |> list.map((k) => { let val=json.get_string(env_obj,k)??""; let
e: dispatch.EnvVar={key:k,val:val}; e })`（捕获 env_obj、构造 record），阻碍向上传播为尾部“heap-result match”（effect ! 脱糖的 Ok 分支 Block 无法低层化此语句）。

原因：C1 defunc 内联特殊化器（经 value_of 解析捕获）在此位置不把 record 元素 map 送入内联路径，回退至 `lift_lambda`（binds.rs:38），其拒绝任何捕获 lambda（free_vars 非空），因为一等 FuncRef 无法携带环境。非捕获 record map 被 lift_lambda 救回；捕获式无可提升形式，故阻挡。注意低层化是多路径（尾部／绑定／值位置对 list.map 路由不同；计装确认 `lower_heap_result_arm` 不是尾部位置 record 元素 map 路径），这使修复不简单。

修复（已定位，未实现）：在三个位置都让捕获式 record 元素 `list.map`/`filter_map` 走 C1 defunc 内联路径（捕获经 value_of 解析，逐轮构造 record 元素并移入结果槽，使用已证明写游标／结果列表递归 drop），而非落入 lift_lambda。可着手但复杂（defunc 多路径 + 健全逐轮 record 所有权 + HOLE-1 递归 drop + 完整门禁）。这是 load_porta_config 阻碍（3 个中 1 个）；read_message/list_instances 仍是循环内 effect-monad 一对。

<a id="load_porta_config--half-landed-2026-06-30-direct-main-agent-implementation"></a>

### load_porta_config——一半落地（2026-06-30，主 agent 直接实现）
真根因（捕获式 list.map/filter_map → record 元素没有 defunc 内联）现部分修复：
- ✅ **list.map / list.filter 捕获式 record 元素**——落地（提交 bcf82dd7）：允许带生成 `$__drop_<R>` 的 record 结果元素，并注册递归 `$__drop_list_<R>`（不是平坦 DropListStr，否则为 HOLE-1 泄漏）。验证：native==wasm 字节匹配，corpus-wall 全 ACCEPT（ownership +2），parity 124/124，发出递归 drop（有效 wat）。清除 load_porta_config 的 `env_vars` map。
- ⏳ **list.filter_map 捕获式 record 元素**（load_porta_config `secrets`）——剩余，更深：filter_map 元素路径（`append_variant_match_to_result_list`）只跟踪调用 subject，但 secrets 为 `let val = json.get_string(sec_obj,k); match val {…}`（subject 是 let 绑定 Var 的 Block）。需要：(a) subject 替换（`{let v=E; match v}` 且 v∉arms → `match E`），让 subject 再为调用；(b) Block 保留分支（`some(v) => { let e=…; some(e) }`）；(c) none 分支嵌套 `let obj=json.get(…); if
  json.get_bool(…) then some({…process.env(k)…}) else none`（effectful process.env 捕获）。多层。
因此 load_porta_config = map 一半完成 + filter_map 一半剩余；两者落地前 real 仍为 3（一个函数，两个 HOF）。read_message/list_instances 仍为循环内 effect-monad 一对。

<a id="sharpened-2026-06-30-all-3-remaining-walls-converge-on-the-effect-monad-in-hofloop-frontier"></a>

### 细化（2026-06-30）：剩余 3 个阻碍全部汇聚于 HOF／循环内 effect-monad 前沿
直接实现已落地 load_porta_config 的 map 一半（bcf82dd7，健全），但深入检查 filter_map 一半（`secrets`）发现它不是清楚的 4 层扩展，而是真正涉及多个前沿：
`sec_keys |> list.filter_map((k) => { let val = json.get_string(sec_obj,k); match val {
  some(v) => some({key:k, val:v}),                                    // capturing + record keep
  none => { let obj=json.get(sec_obj,k)??null; let from_env=json.get_bool(obj,"from-env")??false;
            if from_env then { let env_val = process.env(k) ?? ""; some({key:k, val:env_val}) }  // EFFECT in arm
            else none } } })`                                          // none 分支自身是条件性保留
代码内注释翻译：some 分支捕获并保留 record；from_env 分支内部含 effect；none 分支本身是条件性保留。
所以 none 分支是条件性保留（非简单跳过），且含 effect 调用（process.env）。这与 read_message/list_instances（循环内 effect !）同属 HOF 分支内 effect ! 前沿。结论：3 个剩余阻碍（load_porta_config secrets / read_message / list_instances）都汇聚于一个前沿——“穿过 HOF 分支或循环体的 effect !／条件获取，并提前返回越过它”，没有推迟的 effect-fn Result-tag ABI + 条件循环／HOF 所有权证书，v1 无法表达。real=3 是真实下限；解除全部 3 个的单一深层研究主题是 HOF／循环内 effect-monad 低层化（+ CondLoop/CondHOF 证书）。捕获式 record map/filter 能力（bcf82dd7）健全、已落地，是 load_porta_config 的真实前提部件。

<a id="2026-06-30--list_instances-cleared-real-32-effect--in-for-loop-no-certcoq-change-d3853d52"></a>

## 2026-06-30——list_instances 清除（real 3→2）。`for` 循环内 effect `!`，不改证书／Coq（d3853d52）

“越过循环体内 effect `!` 提前返回”子案例不需要推迟的 Result-tag ABI，也不需要新证书。这是到现有已证明循环槽的纯 IR→IR 脱糖（`desugar_loop_unwrap`，mod_p6.rs）：

  `for x in xs { … e! … }; <post>`  →  `var __ef=false; var __ev=""; for x in xs { if not __ef then { match e
  { ok(v)=>…, err($x)=>{__ef=true; __ev=$x ++ ""} } } else () }; if __ef then err(__ev) else { <post> }`

与提前返回字节相同（一旦 `__ef` 设置，逐轮守卫跳过余下部分；循环通过耗尽 `xs` 终止。**仅 `for`**，排除 `while`，因为跳过其进度更新会不停止）。

**关键健全性洞察（可复用）：** err 累加器必须存放**拥有所有权的副本**（`$x ++ ""`，新 String）。存借用 match 载荷并在循环后移出，会得到不健全证书 `idm`（初始化／drop／移动已死引用 → corpus-wall REJECT，真正重复释放）。`++ ""` 分配槽拥有的新 String，所以循环携带槽证书为已证明 `i(id)m`（全部 4517 上 corpus-wall ownership ACCEPT）。门控于 `String` 错误（覆盖每个 porta 阻碍）。Fixture：spec/wasm_cross/effect_unwrap_in_loop.almd（C-119）。全门禁通过；Ok 路径（完成）与 Err 路径（短路）均 native==wasm。

<a id="refined-next-steps-for-the-remaining-2-each-a-separate-deep-cycle-but-the-owned-copy-insight-transfers"></a>

### 剩余 2 个的细化后续步骤（各自独立深层周期，但拥有副本的洞察可迁移）

- **read_message**（jsonrpc.almd）——**2026-06-30 重新定位根因（不是 TCO；`ok(some(record))` 也非缺口，已能低层化）。** 真根因是缺少 **`ok(<Option-typed Var>)`** 与 **`ok(some(<String>))`** 形状的 `Result[Option[heap], String]` 构造器。`read_message` 基例为 `ok(r)`，其中 `r = parse_and_wrap(body)!` 是拥有所有权的 `Option[JsonRpcRequest]` 局部值，即 `ok(<Option Var>)`。tail.rs 的 heap-result 构造链（`try_lower_result_record_ctor` / `_value_ctor` / `_str_int_ctor` / …）没有 Option 载荷构造器，所以落到 `alloc_init → Opaque → bail`（tail.rs:611“heap-result ResultOk cannot be faithfully returned”）。最小复现（scratchpad），由 classify_corpus 验证：
  - `okrec.almd` 的 `ok(some({id,method}))`（record 字面量 Some）→ **可低层化**（record-Option 路径存在，所以 `parse_and_wrap` 不阻挡，仅 `read_message` 阻挡）。
  - `okvar.almd` 的 `fn pass(o: Option[String]) = ok(o)` → **阻挡**“ResultOk cannot be faithfully returned”，正是 read_message 遇到的 `ok(<Option Var>)` 形状。
  - `okstr.almd` 的 `ok(some("hi"))` → **阻挡**（Option[String] Some）。
  （先前 `wrap`/`scan3` 的“while body”原因，是递归在此构造器缺口之上叠加 TCO→while 回退；根因在构造器缺口。）修复路径：为 `Result[Option[heap],
  String]` 增加 `try_lower_result_option_ctor`，覆盖 `ok(<Option Var>)`（把拥有的 Option 块 Dup 进去）、`ok(some(x))`、`ok(none)`、`err(s)`，对应 `try_lower_result_record_ctor`+`materialize_result_aggregate`。drop——**已确认无需新 Op、无需 Coq 改动即可实现**（2026-06-30）：经 `resrec:opt_<R>` 将 Ok-drop 路由至现有 `Op::DropWrapperRec`（其证书对 `drop_fn` 一致，certificate.rs:76/679 忽略名字，因此已证明检查器 0 改动），生成新 Almide drop helper `fn __drop_opt_<R>(e: Option[R]) ->
  Unit = { match e { some(r) => (), none => () } }`（`match` 消费 `e`、释放 Option 块，`some` 分支经 `$__drop_<R>` 释放 `r`）。helper 唯一依赖——`match Option[record] { some(r)
  => (), none => () }` 可低层化——已验证（scratchpad `dropopt.almd`，0 阻碍）。所以不是深层 Op/Coq 改动，而是：(1) `try_lower_result_option_ctor`（低层化侧）；(2) 仅发现已使用情况的 `collect_result_option_records(&ir)`（仿 `collect_recursive_anon_records`），供给 (3) `generate_record_drop_sources` 的 `$__drop_opt_<R>` 生成循环，并传至 render 调用点。仅生成已使用者（非全部递归 record；不能低层化的 helper 会破坏整个程序；发现限制为实际被包在 `Result[Option[R],String]` 中的 R）。`Result[Option[String],String]` 是较简单同级（内部 Option[String] 是 0 或 1 元素 `DropListStr` 块）。门禁兜底：错 drop → corpus-wall REJECT。okvar/okrec/okstr/dropopt 是确定性 classify_corpus fixture；端到端字节测试需 stdin。剩余是构造器 + 发现／生成接线 + 门禁，一个聚焦周期，不再是信任主干深度阻碍。

  **经验更新（2026-06-30 原型后回退）：read_message 是多层的，构造器必要但不足。** `try_lower_result_option_ctor` 原型确认非递归情形可低层化（`okrecvar.almd` 的 `fn pass(o: Option[Req]) = ok(o)` → wall=0、FORBIDDEN=0），因此构造器 + 已验证 `$__drop_opt_<R>` helper 确是真实健全能力。但 read_message 本身仍被相同“while body heap-accumulator”阻挡：其 `ok(r)` 在 `else read_message()` 重试递归的嵌套 if 分支里，走 TCO（`tco_rewrite`），而非尾部／分支构造器。所以 read_message = 第 1 层（`ok(<Option-Var>)` 构造器，已做原型、可行，因不完整回退；还需接入 `$__drop_opt_<R>` 生成，否则渲染悬空调用）+ 第 2 层（TCO 必须为引用循环体局部变量的递归基例构造 `Result[Option[record]]` 结果，是真正深层，已抵抗三种理解框架）。构造器为避免半接线无效 wasm 状态已回退（未提交）；先带生成重做第一切片，再处理 TCO Option 结果基例。两个剩余目标中，load_porta_config 的 defunc-filter_map 更清晰，宜先尝试。

  **层叠结构（完整原型于 2026-06-30 验证，随后回退到干净树）。** read_message 是最深 porta 函数，阻碍级联穿过 ≥5 个交互层。原型确认：
  (a) `ok(<Option-Var>)` 构造器能低层化非递归情形（okrecvar wall=0）。
  (b) 对 `Result[Option[<nested-heap>], String]` 的 base-reads-loop-local 拒绝 TCO（mod_p5 约 :944，排除 Option[String]），使阻碍从“while body heap-accumulator”变成“heap-result `if`”，即正确离开 TCO，回退至真正递归 heap-result-`if` 路径。
  (c) 将构造器同时接入 tail.rs 与 `lower_heap_result_arm`（control_p4）仍报“heap-result `if`”，因为 read_message 分支含 `let r = parse_and_wrap(body)!`，即 **heap-result-`if` 分支内部的 effect `!`**（第 5 层）；它不同于循环 `!` 部件（Unit `for` 本体），尚不能在返回 Result[Option] 的 if 内低层化。
  因此完全清除 read_message 需构造器 + 拒绝 TCO + 分支接线 + `$__drop_opt_<R>` 生成 + heap-result-`if` 分支内 effect `!` 低层化，是协调的多层周期。所有原型编辑已回退（没有生成时构造器渲染不完整，任何单项都未清除阻碍）。建议先做 load_porta_config（单机制 defunc-filter_map），再做 read_message（5 层）。

  **load_porta_config 清除 2026-06-30（bc37cbe0）：真实低层化阻碍 2 → 1。** 单一机制正如预测：defunc `emit_filter_map_arm`（control_p5）只处理裸 `some(elem)` / `none` 分支本体，阻挡所有其他形状。将它扩展为递归处理 Block 分支本体（低层化前置 let）及 `if` 分支本体（向同一写游标作 Unit 保留／跳过，对应已证明 `append_body_to_str_acc`），就低层化了 secrets 的 `none => { let b = …; if b then some(rec) else none }`。无新 MIR op，无证书／Coq 改动（4520 上 corpus-wall ACCEPT）。Fixture C-120（filter_map_conditional_arm.almd；porta `json.get_string` subject 用用户函数代替，以通过 render_program registry 渲染）。**read_message 现为唯一剩余 porta 阻碍**（上方 5 层），也是 porta 达到 wall=0 的唯一障碍。

  **下一周期 read_message 线索（第 3 层简化器）。** read_message 困难分支是 `{ let r = parse_and_wrap(body)!; ok(r) }`。但 `let r = e!; ok(r)` 对 `e` 是恒等（Ok(x) → x → Ok(x)；Err(s) → 传播 Err(s)），即整个 block ≡ `e`。小型通用脱糖 `{ <pure-prefix>; let r = e!; ok(r) }` → `{ <pure-prefix>; e }`（门控：unwrap 结果在 block 尾部原样重包装），把第 3 层（分支内 effect `!`）折为裸尾调用分支 `parse_and_wrap(body)`，真正递归 heap-result-`if` 分支路径已可将其低层化为 `CallFn`+`Consume`（这些分支无需 Option 构造器、无需 `$__drop_opt`，因为传播调用结果而非构造）。脱糖后剩余为 `ok(none)` 分支（需 Option 构造器 + `$__drop_opt_<R>` 生成，04caf8e0 已做原型）以及拒绝 TCO（mod_p5 约 :944，791807ae 已做原型）。组合三小块应能清除 read_message；用 corpus-wall 验证脱糖健全性（它去掉会提前返回的 effect `!`，因此不能越过任何其他引入 live 堆的语句，须门控为纯前缀）。
- **load_porta_config**（config.almd）——secrets `filter_map`：捕获式 lambda 经 `match`（some 分支 = record／条件 none 分支 `if from_env then some(record) else none`）+ `process.env` 生成 record。这是 defunc-`filter_map` 机制（control_p5 `lower_defunc_filter_map_hof`），不是循环脱糖，是另一个深层周期（Block 保留分支 + 条件 none 分支 + effectful none 分支）。

<a id="2026-06-30--read_message-core-landed-06664cd8-8th-feature-is-the-sole-remaining-blocker"></a>

## 2026-06-30——read_message 核心落地（06664cd8）；第 8 项功能是唯一剩余阻碍

read_message 基础结构已提交、门禁干净（4523 上 corpus-wall ACCEPT，新增 3 个 corpus 函数可低层化，output-parity 126/126，proof-spine OK）：unwrap-重包装恒等脱糖（`desugar_unwrap_rewrap_identity`，`{…; let r=e!; ok(r)}` ≡ `{…; e}`）、`Result[Option[record]]` 构造器（`try_lower_result_option_ctor` + `lower_option_piece` + 分支／尾部接线 + `is_option_record_result_ty`）、Option 嵌套堆拒绝 TCO（mod_p5 约 :944），以及过度生成 `$__drop_opt_<R>` helper（`generate_record_drop_sources`，以后缩至基于发现的生成）。由最小复现（scratchpad rdcore/rdcore2/rdcore3*）验证：read_message 核心可低层化（嵌套 `if` + `ok(none)` + 恒等脱糖的 `ok(parse_w(b)!)` 调用分支 + 无参 `io.read_line` 重试递归 → wall=0）。

**唯一剩余阻碍（rdcore3c）：绑定在嵌套 heap-result-`if` 分支深处（深度 ≥3）的 effect 调用 heap-`let`（`let body_bytes = io.read_n_bytes(len)`）报“heap-result `if`”**，但同一 `io.read_n_bytes` heap-`let` 在函数顶层（iob_top）及深度 1 分支（iob_arm）可低层化，深度 3 的两个串联纯 heap-`let` 也可低层化（rdcore3d）。所以是 (effect 调用堆结果) × (深层分支嵌套) 的狭窄交互，read_message 第 8 且最后功能。下一步：解决 heap-result-`if` 分支 Block 低层化对深处 effect 调用绑定堆局部值的处理 → porta wall=0。read_message 叠加约 8 个难功能（最难 porta 函数），本轮已攻克并提交 7 个。

<a id="definitive-root-of-the-8th-blocker-it-is-not-lowering--it-is-a-missing-runtime-primitive"></a>

### 第 8 阻碍的确定根因（不是低层化，而是缺少 runtime primitive）

二分至最小（scratchpad iob_arm7）：函数顶层 `let b = io.read_n_bytes(n)` 报 **“effectful/impure stdlib Module call io.read_n_bytes needs a declared capability not in this brick”**（calls.rs:241），不是 heap-result-`if` 阻碍。先前“heap-result `if`”消息是 capability 阻碍经分支向上传播。因此 read_message 低层化已完全解决（7 个已提交功能；iob_arm8 = 同一 `ok(some(record))` 加纯 let，wall=0）；唯一剩余阻碍是 **io.read_n_bytes 没有 v1 runtime 基础实现**。`io.read_line` 已允许（calls.rs:240 whitelist + 自托管 `stdlib/io_read_line.almd` = `prim.read_line()` + `PrimKind::ReadLine` 渲染为 WASI fd-0 字节循环，构造 String，携带 Capability::Stdin）。io.read_n_bytes 是其同级（WASI fd-0 读取 N 字节 → `Bytes` 块），确实可以用 WASI 实现，所以是真实阻碍，不是 native-FFI（为从指标移除而标成 native-FFI 属指标作弊，禁止）。要关闭 porta wall=0，按 read_line 同级实现 io.read_n_bytes：(1) whitelist `io.read_n_bytes`（calls.rs:240）；(2) `stdlib/
io_read_n_bytes.almd` = `effect fn io_read_n_bytes(n: Int) -> Bytes = prim.read_n_bytes(n)`；(3) `func == "read_n_bytes"` 分支（calls_p4 约 :1024）发出新 `PrimKind::ReadNBytes`（args [n]）；(4) `PrimKind::ReadNBytes` 渲染（WASI `fd_read` 从 fd 0 读 N 字节到 `Bytes` 块，是真实 runtime 工作，适配 `ReadLine` 渲染）；(5) Capability::Stdin 记账（复用）。这是 effectful-WASI-floor 工作流（任务 #61），与现已解决的 read_message 低层化属于不同工作类型，边界明确。一旦 io.read_n_bytes 有基础实现，read_message 就能低层化（结构由 rdcore3* 证明）。
