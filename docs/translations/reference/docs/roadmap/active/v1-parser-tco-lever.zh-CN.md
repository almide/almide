描述：解析器 TCO 的关键能力——跨仓库解除堆结果表达式阻碍
<a id="v1--the-parser-tco-lever-the-real-heap-result-expr-cross-repo-lever"></a>

# v1 — 解析器 TCO 的关键能力（真正跨仓库的“heap-result-expr”能力）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-parser-tco-lever.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-parser-tco-lever.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-parser-tco-lever.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-parser-tco-lever.md)

> 原文历史说明：本文是非时间顺序的调查记录，不是当前实现或安全保证。虽然后置的第4轮记录报告了返回值的 Camp-4 match 支持，开头第5轮的计划仍待完成。旧障碍、撤回的原型、门禁修改方案及诊断与后续订正并存，包括 value.str 移动与复制、元组结果分流和元组元素追加准入，以及内联 value.null 而非为合成调用增加门禁计数的解决办法。旧 csv/Value TODO 和 AES/base64 障碍数没有按后续报告统一。yaml 能运行不等于反复解析无泄漏：原文明确保留最终 OOM 与生产加固工作。字节比较、有限泄漏循环、测试套件／语料数量以及 FIPS 197 示例输出，只是原文报告的检查，不是普遍内存安全、密码安全保证、资格认定或独立复测。ASCII 小写转换和浮点解析的范围保持原样；序列化绕行方案没有解决一般 heap-result-if 降低。native v0 oracle 失败以及 v1 MIR 路径与旧 wasm 代码生成的区别仍须注意。证书接受不涵盖每个受信任 runtime／renderer 操作，也不能发现每种字节误编译。历史上的下次会话指令不执行。未解析的 wiki 式引用保留为原文文本，不虚构链接。

<a id="-decode_chunks-onion--fully-mapped-2026-06-22-turn-5-effortmax-the-remaining-base64-decode--toml-frontier-is-one-coherent-slice-tco-over-a-match-whose-result-is-a-result-with-an-ownership-critical-core-the-unwrap--desugar-turns-let-x--fp-recurse-accx-into-match-fp--okx--recurse-accx-erre--erre---recursion-inside-a-match-arm-returning-resultheap-string-to-lower-it-verified-repro-umr--a-1-unwrap-version-build-it-first-it-is-the-unit-test-for-the-whole-slice"></a>

## ⭐ decode_chunks 的层层问题 — 已全部定位（2026-06-22，第 5 轮，effort=max）。剩余“base64 decode / toml”前沿是一个连贯切片：**对结果为 `Result` 的 `match` 做 TCO**，核心涉及所有权。unwrap-`!` 脱糖将 `let x = f(p)!; recurse(.., acc+[x])` 变为 `match f(p) { ok(x) => recurse(.., acc+[x]), err(e) => err(e) }`——递归位于 match 分支内，返回 `Result[heap, String]`。要降低它（已验证，复现 `umr` 为单次 unwrap 版本；先构建它，它是整个切片的单元测试）：

1. **跨 match 的 TCO** — `tco_collect` + `tco_rewrite` 必须递归处理 `match` 分支（目前仅 `if`/`Block`）。改动小，与 `if` 分支对称。（已做原型并回退——本身健全，但单独不足。）
2. **`tco_empty_for(Result[_,String])` → `err("")`** — 结果累加器的占位值（基例会覆盖它）。改动小。**这才使 TCO 触发**（否则 `tco_empty_for` 返回 None → TCO 拒绝 → “heap-result if”阻碍）。原型中阻碍移到了第 4 层，证明 1+2 有效。
3. **循环体 match 的执行** — TCO 循环体变为 `match f(p) { ok(x)=>{acc=acc+[x]; p=..}, err(e)=>{rk=..; result=err(e)} }`；此用户 `Result` 的语句 match 必须执行而非线性化 → lower_branch 的被匹配值跟踪（control.rs:104）需要用户命名调用的 `Result` 分支（scalar-Ok→materialized_results+heap_elem_lists；heap-Ok→materialized_results_str cap-tag）。同时修复另一个潜在线性化误编译（hok2：用户 heap-Ok `Result` 在 Unit 语句 match 中目前会运行两个分支）。
4. **⚠ 标量循环 Result 槽 Assign（所有权关键层——严重违例所在层）** — 循环中的 `result = ok(acc)` / `result = err(e)`（mod.rs:1532 的标量循环 Assign 仅处理 `acc=acc+[x]` 追加及 `[]`/`""` 重置；`ResultOk`/`ResultErr` 右值被延后 → try_lower_scalar_while 拒绝 → “while body heap-accumulator reassignment”阻碍）。必须经 `materialize_result_str` 物化，并在移入前**对携带的载荷 `acc` 做 `Dup`**——否则循环结束时 drop `acc` 会重复释放现由 `result` 拥有的列表。证书必须检查这一点（所以这是需谨慎处理的切片，而非强行放行）。
5. **真实 decode_chunks** 还增加：4 层嵌套 unwrap-match（跨 match 的 TCO 通过递归处理嵌套）+ 最内层 ok 分支里的 `new_acc` 嵌套 if 累加器（已落地的 flatten-let-block + inline-tail-accumulator + let-bound-if 预脱糖必须与 match 嵌套组合——在提取出的函数上验证）。

在碰 corpus 之前，将 1–4 一起完成，并对 `umr` 做 v0==v1 字节测试（corpus-wall 不会捕获字节误编译）。这一个切片解除 base64 decode（3 个阻碍）+ toml 大部分（22 个）的阻碍。本轮做了 1+2+3 原型（TCO 触发）；因第 4 项的所有权 Dup 与第 3 项的耦合需要在证书下谨慎联合实现，而非零碎处理，已回退。


<a id="cross-repo-conquest-scoreboard-real-githubcomalmide-repos-on-the-v1-spine"></a>

## 跨仓库完成记分板（v1 主干上的真实 `github.com/almide` 仓库）

- ✅ **csv** — 4/4 个公开函数字节匹配（见下文）。
- ✅ **svg** — 全部完成（2026-06-21）。基于 record 的渲染器（rect/text/group/doc/嵌套 children + `map.entries` 属性）与 v0 输出逐字节相同，10⁴ 次无泄漏。record 语言功能（构造／字段读取／spread／递归嵌套所有权 drop／List[Record] 字面量+拼接／通过新 `(String,String)` 元组列表提供 `Map[String,String]` entries）在 v1 上已完整；`almide test` 15/0（mod.almd 经 WASM）。完整设计与提交轨迹：[[v1-records-svg]]（STATUS 6）。同时帮助所有仓库的横切修复：`not <bool-call>` let 分支（lower_bind UnOp）、defunc-map 自递归准入（`in_defunc_body`）、`List[(String,String)]` 的 `DropListStrStr`。
- ✅ **yaml — 在 V1 上运行（2026-06-22）。** `parse` 编译为约 13.7k 行 WAT（0 个阻碍），并在 wasmtime 上按正确字节解析真实 YAML——map、sequence、嵌套、字符串／整数／浮点／布尔均经 `parse`+`stringify` 往返（`a: 1 / b: hello / c:\n  - x\n  - y` → 相同）。完整链执行：list.enumerate + 带闭包的 find-over-(Int,String)、list.set_str、effect-fn (Value,Int) 元组结果 TCO、heap-result-if Match 分支临时 frame、heap-Value 尾部／绑定 drop 修复。剩余（不阻止“运行”）：大量解析的泄漏长尾（map 到 1000× 左右尚可，sequence 路径更早泄漏）——深层解析器每次调用的小量少释放；见下方 STATUS（2026-06-22, runs）。
- 🟢 **bigint / rsa / sha1 / porta** — 0 个降低阻碍（所有函数均可降低）；`sha1` 开箱即与测试向量字节匹配（2026-06-21 探针）。只需字节匹配审计和泄漏循环即可确认成果。
- 🟡 **base64 (9/13) / aes (17/30) / almide-sqlite (20/28) / toml (22/48)** — 仍有机制阻碍。

<a id="yaml-status--the-1-wall-is-the-heap-result-tuple-tco-option-c-engineering-integration"></a>

## yaml 状态 — 唯一阻碍是堆结果元组 TCO（方案 C 的工程集成）

`collect_map` 被阻挡（`heap-result if … would move out an empty deferred heap value`）。根因：它是互递归解析器（`collect_map ↔ map_entry ↔ after_colon`），返回 `(Value, Int)` 元组，并累积 `pairs: List[(String, Value)]`。其尾部自调用（`collect_map(…)!`，跳过空行）被堆结果分支的自调用门禁拒绝——这是正确的，因为 v1 尚无堆结果尾递归 TCO，深层 yaml 会溢出 wasm 栈。因此放行自调用不健全；正确修复是**方案 C 的追加累加器 TCO**（将同层 entry 循环变为真正循环；Coq 循环所有权健全性证明已落地，提交 7f673b4c——剩下的是工程集成）。

进展（2026-06-21，随后为保持树健全而回退）：将列表字面量构建器和 `lower_owned_heap_field` Tuple 分支扩展到 `(String,Value)` 元组（csv 已有 `DropListStrValue`/`str_value_elem_lists`），可使 `[(k, value.str(…))]` 累加器字面量物化——最小 `cm2`（字面量 key 的 `(Value,Int)` TCO 累加器）从阻碍变为 **TRAP**。TRAP 是关键，也是回退原因（②纪律——不交付可达的误编译）：

- TCO 触发（递归变为真正 `while`），`pairs = pairs + [(k, value.str("x"))]` 经追加累加器 Assign 降低。但循环体中的 `value.str(arg)` 将 String 参数移动到 Value，再逃逸到累加器（Value → 元组 → 列表 → 循环携带的 `pairs`）——然而每轮清理（`drop_arm_locals`）仍对该 String 做 `rc_dec`（它仍在 `live_heap_handles`），导致重复释放（`rc_dec(v13)` trap）。这是**循环体逃逸的 drop 平衡错误**：逃逸到循环累加器的值构造器参数，没有从每轮 frame 中标记为 Consumed。（`sv1`/`sv2`——循环外相同拼接／元组——无泄漏，因此问题特指与循环逃逸的交互。）
- 另一个问题：真实 `collect_map`（key 为 `list.get(lines, pos) ?? ""`，而非字面量）仍被阻挡——第二个问题（`??` key 的元素／将 `map_entry` 互递归内联成自递归形状）。

根因（2026-06-21 已充分厘清风险）：`tco_empty_for`（lower/mod.rs:2952——结果累加器的初始空值）仅处理 `String`/`List`；对 `(Value, Int)` 元组返回 `None`，因此 TCO 拒绝循环内结果累加器路径（csv parse 修复 646aa233），回退到旧的循环后分派。后者在循环后重新计算基例 `(value.object(pairs), pos)`，读取陈旧值 → `pos` 读到入口值（`p=0`，而非 3）。因此修复是让元组结果也可使用结果累加器路径。确定的 5 部件计划（按此计划重新实现，每步加门禁）：

1. **`tco_empty_for` → Value + Tuple。** `Value → value.null()`（干净的空 Value）；标量 → `0`/`0.0`/`false`；`Tuple[a,b] → (tco_empty_for(a), tco_empty_for(b))` 递归处理。将 `(Value,Int)` 结果送到循环内结果累加器（不再有读取陈旧值的循环后分派）。
2. **元组结果累加器 DROP。** 累加器槽持有 `(Value, Int)` 元组；每次基例重新赋值（以及初始空值）都通过新的 `Op::DropTuple` 式操作释放旧值（slot-0 的 `Value` 由 `$__drop_value` 释放，slot-1 标量无操作，再释放元组块）。将现有 `record_masks`／逐槽递归 drop 泛化到含 Value 槽的 2 槽元组。（每次运行只命中一个基例，因此空值只释放一次——但对多基例解析器也必须正确。）
3. **重新落地 `(String,Value)` / `(String,String)` 列表字面量物化**（已回退的 `try_lower_record_list_literal` StrValue + `lower_owned_heap_field` Tuple 分支），使 `pairs +
   [(k, value.str(...))]` 累加器字面量能构建。本身健全（sv1/sv2 已验证）。
4. **循环体逃逸 Consume。** 循环体中 `value.str(arg)` 复制 `arg`（runtime `string.repeat(s,1)`），所以 `arg` 仍属调用者，在每轮清理时被 `rc_dec` 是正确的——但 trap（`rc_dec(v13)`）显示它被释放两次：确定重复 `rc_dec` 来自每轮 frame + 陈旧 carry，还是参数与累加器副本被错误共享。（装入部件 1–3 后，从新的 `cm2.wat` 重新推导；此前 trap 发生在结果累加器路径引入之前，所以部件 1 正确路由后可能消失。）
5. **真实 `collect_map` 的互递归内联。** `inline_mutual_tail_recursion` 必须将 `map_entry`（及 `after_colon` 链的同层尾调用）内联进 `collect_map`，使同一嵌套层仅为自递归（更深的 `parse_nested` 仍是有界普通调用）。随后部件 1–4 生效。`??` key 元素（`list.get(lines,pos) ?? ""`）也须能在循环体降低。

随后：每个部件都做 `parse`/`stringify` 字节匹配审计 + 10⁴ 泄漏循环 + corpus-wall + mir 套件。
门禁注记：部件 1 单独可能已修复 `cm2`（将其路由到结果累加器）——先验证，不要预设需要部件 2/4。Coq 循环所有权健全性（方案 C，7f673b4c）支撑整体；这里只涉及提取／降低集成。

<a id="corrected-diagnosis-2026-06-21-sharper--supersedes-the-all-tuple-results-broken-framing"></a>

### 纠正后的诊断（2026-06-21，更精确——取代“所有元组结果都坏了”的表述）

两个用于消歧的探针精确缩小了问题：
- `cm4`（自递归解析器，返回 `List[(String,Value)]`，累积 `acc + [(k, value.str(…))]`）**与 v0 字节匹配**（len 3）。因此 str_value 列表字面量物化、追加累加器 TCO、`value.str` 的参数复制都健全。（部件 3 正确，可重新落地。）
- `cm2`（相同但返回 `(Value, Int)` 元组 `(value.object(pairs), pos)`）产生垃圾（`pos=0`、value 垃圾）+ 清理 trap。
- 现有 `parse_rows_rec` 测试（csv——返回元组的自递归解析器）通过。因此**并非所有元组结果 TCO 都坏了**。区别是 `parse_rows_rec` 的基例读取循环体局部变量（`let
  (field,np)=…`）→ `base_reads_loop_local=TRUE` → 循环内结果累加器路径（正常）。`cm2` / `collect_map` / `collect_seq` 的基例只读携带参数（`pairs`,`pos`）→ `base_reads_loop_local=FALSE` → 循环后分派，重新计算元组基例并读取陈旧值 → 垃圾。

真正问题是**循环后分派重新计算元组基例**（它不像 `cm4` 的 `acc` 这种纯 Var 基例那样，能看到循环最终携带参数值）。宽泛“拒绝元组结果”的守卫是错的——会使 `parse_rows_rec` 回归（已验证：加守卫后测试失败）。

修订后的更窄修复：让仅依赖携带参数的元组基例使用循环内结果累加器（元组结果也路由 `result_var`，需要 `tco_empty_for(Value/Tuple)` + `(Value,Int)` 元组 drop）——或修正循环后分派，使其降低元组基例时读取循环最终携带局部值。无论哪种，`parse_rows_rec`（基例读循环局部值）必须继续工作。最小复现是 `cm2`；同时以 `cm2`（必须修好）和 `parse_rows_rec`（必须保持正确）为门禁。

<a id="csv-full-conquest-status-4-public-fns-v0-vs-v1-byte-match-audit"></a>

## csv 全部完成状态（4 个公开函数，v0 与 v1 字节匹配审计）

逐个端到端审计 `almide/csv` 的公开函数（内联源码，v0 `almide run` 对比 v1 render_program+wasmtime）。**✅ 全部完成——4/4 字节匹配**（已验证：字节匹配 + 100000× 泄漏循环干净 + corpus-wall ACCEPT + mir 套件）。三种不同机制，在根部修复：
- ✅ **stringify**（提交 6fb48108——需要非捕获 heap-map 内联，关闭提升路径中嵌套 map 静默误编译、只返回 `,` 的问题）。
- ✅ **stringify_records**（提交 b129ad45——捕获式 heap-map／map-closure-over-Value）。
- ✅ **parse**（提交 646aa233——TCO 结果累加器修复：parse_rows_rec 的 `paf(…, cur+[field])` 基例读取循环体局部变量 `field`，现在经结果累加器在循环内计算，而非在 `field` 已死亡的循环后分派计算。drop 放置问题，在根部修复）。
- ✅ **parse_records**（提交 fba2e960 + af8dcdf7——被 parse trap 掩盖的嵌套堆列表元素操作：`List[List[String]]` 上的 `list.get(rows,0)` / `list.drop(rows,1)`。`_str` 变体经 `string.repeat` 深复制内部列表（将其长度字当作字节数）——get 静默误编译，drop 重复释放 trap。新增 handle-SHARE `list.get_liststr` / `list.{take,drop}_liststr` + `option.liststr_unwrap_or`：内部列表各通过 rc_inc + 原始 store64 共同拥有（`__ldls_share`，与 `__varr_copy` 一样列白名单），最后一个引用时只释放一次）。
- ❌ **parse_records** — map 机制（enumerate+map 融合 + 元组元素 map，提交 a9aecee5）和 parse_rows_rec 重复释放（TCO 修复 646aa233）均已完成；剩下第三个独立显露的问题，此前被 TCO trap 掩盖：**嵌套堆列表 `List[List[String]]` 上的 `list.get` / `list.drop`**（parse_rows 返回的 `rows`）。缩减 parse_records 主体后隔离（`va*` 复现）：
  - `let header = list.get(rows, 0) ?? []` → **静默误编译**（`va2`：v0 `[2]`，v1 `[0]`）。分派（lower/mod.rs:2354）将任何堆元素送往 `list.get_str`，后者经 `string.repeat` 深复制元素——对叶子 `String` 正确，但复合堆元素（`List[String]`、`List[_]`）必须按句柄共享（类似 :2350 的 `is_value_ty` → `list.{f}_value` 分支），并跟踪其真实 drop 类型（`List[String]` → DropListStr，而非 `_value` 的 DropValue）。需要复合堆访问器 + 元素 drop 跟踪，不只是改分派那一行。
  - `let data = list.drop(rows, 1)` → **TRAP**（`va3`）。子列表操作不了解 `List[List[String]]` 的堆元素（移动的元素句柄 rc 被错误处理）。
  因此 parse_records 达到 4/4 需要嵌套堆列表元素访问器（`list.get`/`first`/`last` 的句柄共享 + 复合元素 drop 类型跟踪），以及了解堆元素的 `list.drop`。`va2` 静默误编译属于②纪律问题（误分派到 `_str`）；最小健全步骤是在访问器落地前，阻挡复合堆元素的 `list.get`，而非安静地做 `_str` 深复制。
- ❌ **parse**（及 parse_records 其余部分）— 共享的 `parse_rows_rec`/`parse_after_field` 重复释放（prr↔paf 互递归内 rc_dec → `unreachable`）。原因是交互，通过三个最小复现缩小范围（诚实纠正此前单一原因猜测）：
  - **dd**（直接自递归 `prr(…, cur + [field])`，含 `let (field,np)=pf(…)`，无 paf）→ ✅ 正常。因此仅解构 + `cur + [field]` 本身健全（此前“解构在使用字段前释放元组”的判断是错的——dd 恰好如此却正常）。
  - **B**（prr↔paf 互递归，无解构）→ 被阻挡（不在子集中，而非 trap）。
  - **prrep**（两者都有：prr↔paf 互递归 + `let (field,np)=pf(…)` 解构 + `cur +
    [field]`）→ TRAP（重复释放）。
  最终触发条件（回归测试 `parse_rows_rec_destructure_mutual_recursion_double_free`，#[ignore]）：精确组合为**自递归同级分支 + 嵌套 else 内解构**。prr 的内层 `if c == "," then prr(…) else { let (field,np)=pf(…); paf(…, cur+[field]) }` 中 THEN 自递归（`prr(…)`），ELSE 解构拥有所有权的元组后使用借用的 `field`——拥有所有权的 pf 元组在 `cur+[field]` 拼接读取它之前被释放（对 String 槽 rc_dec）（wat：drop 在 479 < concat 在 551）→ 复制已释放 String → 重复释放。已确认触发：`ns` 复现——代码相同，但内层 THEN 改为 `paf(…)` 而非自递归 `prr(…)`——不 trap（整个 prr 随后按可执行 heap-result-if 降低）。因此自递归同级分支将解构 else 推向错误放置 drop 的路径。dd（平坦的 `else { destructure;
  prr(…) }`，自递归但没有嵌套 if）也正常（drop 在 374，晚于 concat 347）。给 field 做 Dup 无法修复（Dup 在同一错误位置被释放）。需针对解构 × 自递归同级分支 × 嵌套 if 的交互深入修正 drop 放置，回归风险高。map 机制已完成（a9aecee5）；parse + parse_records 都只在这里 trap。不在 corpus-wall 中（csv ∉ corpus）。

<a id="the-layout-brick-read-side--open-commits-5b7efec7-e43db65f"></a>

## 布局部件的读取侧 — 已打通（提交 5b7efec7、e43db65f）

堆 Result-of-X 的读取／`??` 就是 value 子系统指出的布局部件。现在完整 Result 族已落地——每项都与 v0 字节匹配、泄漏循环干净、corpus-wall ACCEPT、套件绿：
- **value.get** → `Result[Value,String]`（自托管：线性扫描 `__vobj_find` 索引，然后非递归包装 `Ok(@12 borrow)`/`Err("missing field '<k>'")`）。`match` 读 tag@16 并绑定 @12 Value（归类到 `value_result_results` → 递归 `DropResultValue`）；此前乱码为 ok:0|err:0。
- **value.as_array** → `Result[List[Value],String]`；**value.as_string** → `Result[String,String]`。
- `??` 将每类 Ok 载荷送往复用正常 match 读取的自托管 helper：`result.value_unwrap_or` / `result.list_value_unwrap_or` / `result.str_unwrap_or`（Ok 分支 Dup @12；精确的 `is_result_str_str_ty` 区分 str helper 与 result.zip 的 tuple-Ok）。count_ir_calls 计入合成调用，使 mir==ir。handle else-if + Var-case 允许 Value/List 操作数。

- ✅ **Option-of-Value 读取** — 完成（提交 cab0924b）。List[Value] 上的 `list.get` 分派到自托管 `list.get_value`（不是 `_str` 变体；后者用 `string.repeat` 复制元素，把 Object 损坏为 `{}`）；它经 `Some(@i)` 共享元素（`Some(Value)` 构造器对借用 Value 做 Dup，与 value.get 的 Ok 一样），`??` 将 Option[Value] 送往基于 prim 的 `option.value_unwrap_or`（value-match Some 分支的 scalar_bind 拒绝堆载荷，因此 helper 直接读取 len-tag@4 + @12）。另修复泄漏：`value.as_array ?? []` 操作数拥有内部列表 → 在 materialized_call_arg 中重分类到 value_result_lists（递归 `DropResultListValue`）（平坦 drop 泄漏元素 Value，循环 OOM）；Result[Value,String] 的 Ok 仍用平坦 drop（共同拥有）。已验证含 Object 元素的字节匹配、2000x 无泄漏、corpus ACCEPT。

- ✅ **map-closure-over-Value** — 完成（提交 b129ad45/264976e8）。csv **stringify_records 与 v0 字节匹配**（包括 CSV 引号 `LA, CA` → `"LA, CA"`）。C1 内联 `list.map` 扩展到源和结果都是堆元素：List[Value]/List[String] 上的捕获式闭包（无可提升环境）内联为专用循环——通过 `LoadHandle` 读取元素（借用的 i32，供主体使用，如 `value.get(row, h)`）；逐元素主体通过 `lower_heap_result_arm` 降低（一般堆表达式：调用／拼接／嵌套 `list.map … list.join`／新 `??` 分支，经 unwrap helper 路由 `value.as_string ?? ""`）；新拥有的结果扩展为 Handle，存入以 `heap_elem_lists` 跟踪的 DynListStr 槽（递归 drop）。嵌套单元格投影（外层对 rows 的 map 捕获 `header`，内层对 header 的 map 捕获 `row`）两层均内联。仅允许捕获式闭包（非捕获 heap map 仍走已验证的 `list.map_str` 提升路径）+ String 元素结果。已验证：所有 map-closure-over-Value 形状字节匹配、2000x 无泄漏、corpus-wall ACCEPT（16514 个堆对象，caps-transitive）、套件 487/0、diff-fuzz 绿。**完整布局部件已关闭：heap-Result/Option 读取族 + Option[Value] + map-closure-over-Value 均落地，实现 csv stringify_records。**


org-trust 看板最大的阻碍原因（约 40 个，阻碍 toml/svg/aes/base64/csv）看起来是“heap-result-expr 族”（`heap-result if`/`match` … “would move out an empty deferred heap value”）。以 csv 为目标（v0 oracle 正常，而 toml 不同，见下文）揭示了真实原因。

<a id="finding-it-is-not-the-heap-result-arm-shapes--those-already-lower"></a>

## 发现：不是堆结果分支形状——那些已经能够降低

`lower_heap_result_arm`（control.rs）已处理元组构造分支、Named/Module 调用分支、拼接分支、嵌套 if/match、block、Option/Result 构造器。csv 的 `heap-result if` 阻碍来自一个有意设置的守卫：**自递归调用分支被阻挡**（control.rs 约 2162）：

```
if name.as_str() == self.fn_name { return None; }  // v1 has NO TCO → deep recursion traps
```

保留代码注释译文：v1 没有 TCO，因此深递归会 trap。

csv 解析器全是尾部自递归：`parse_unquoted_field(text, pos+1, acc+c)`、`parse_quoted_field`、`parse_rows_rec`、`parse_after_field`——每个都递归，所以每个堆结果 `if` 都触及自递归守卫。因此关键能力是**自递归堆结果解析函数的 TCO**，而非分支形状。

<a id="what-tco-already-covers-try_tco_rewrite-modrs2734-vs-the-gap"></a>

## TCO 已覆盖什么（`try_tco_rewrite`，mod.rs:2734）以及缺口

已覆盖：(1) 列表迭代器向前扫描（携带 `list.drop(cs,1)`）；(2) 追加累加器（`acc + [x]`、`ConcatList`）→ 拥有所有权的循环携带槽（方案 C，证书 `check_cert_lc`）。yaml（经字节验证）能降低，因为解析器符合这些形状。

缺口（csv/toml 解析器组合子的形状）：
- **String 累加器** `acc + c`（`ConcatStr`，而非 `ConcatList`）——将追加累加器扩展到 String 槽（同样每轮 drop-old/alloc-new，证书 `i(id)m`）。
- **元组结果基例** `(acc, pos)`——基例返回 `(String, Int)`，含累加器 + 标量位置，而非直接返回携带类型。
- **多累加器 + 解构元组的自调用**（`parse_rows_rec`：携带 `rows`、`current_row` 两个 `List`，某自调用参数为 `current_row + [field]`，其中 `let (field, np)
  = parse_quoted_field(...)`）。

<a id="plan-byte-match-first-csv-has-a-working-v0-oracle"></a>

## 计划（字节匹配优先；csv 有正常的 v0 oracle）

Oracle：`parse("a,b,c\n1,2,3\n\"x,y\",4,5\n")` → v0 native = `[["a","b","c"],["1","2","3"],["x,y","4","5"]]`（已确认）。Driver = csv/src/mod.almd + 调用 `parse` 的 `effect fn main`（单文件 → render_program）。目标：v1 == 此结果。

1. 将 `try_tco_rewrite` 中的追加累加器扩展为 **ConcatStr（String）累加器** + 最小元组结果基例——解除 `parse_unquoted_field`/`parse_quoted_field` 阻碍。门禁：corpus-wall ACCEPT（循环携带证书 `check_cert_lc`）+ String 累加器泄漏循环 + 字节匹配。
2. 多累加器 + 解构元组的自调用——`parse_rows_rec`/`parse_after_field`。
3. 随后 `parse`（`ok(value.array(...))` ResultOk）+ `parse_records`（`list.map` 闭包）级联降低。csv → 字节匹配 `[["a"…]]`。

每步均以 corpus-wall ACCEPT（TCO 容易出现正确性／泄漏问题，循环携带槽证书就是门禁）和 csv v0==v1 字节匹配为门禁。此能力可跨 toml/svg/aes/base64 清除同一类问题（均呈解析器形状）。

<a id="progress-commit-63a7a1a6--step-1-done--a-pre-existing-miscompile-fixed"></a>

## 进展（提交 63a7a1a6）— 第 1 步完成，并修复一个既有误编译

接入 ConcatStr 累加器时，字节匹配暴露了既有静默误编译（②严重违例）：TCO 循环体为 `{ if base then … else step }`，所以基例检查是 block 尾部 `if`；该尾部直接落到 `lower_branch`（运行两个分支，cond 只记录不执行）——将 `if done then {rk:=k} else {step}` 变为无条件 `rk:=k`，循环恰好只运行一次。主体含堆 `let c = peek(...)` 的任何递归解析器都会遇到（v0 `hello`，v1 `h`）。**修复**：block 尾部 if/match 优先经过 `try_lower_unit_if`（真正分支），仅在不能执行时才落到 `lower_branch`。既消除误编译，也让标量索引的追加累加器解析循环真正执行。

本提交已完成：
- ✅ block 尾部基例检查现在分支执行（修复只运行一次的误编译，list 和 string 均适用）。
- ✅ ConcatStr（String）累加器 + 元组结果基例 `(String, Int)`——`is_self_append` 匹配 ConcatStr，预先槽复制理解 String（`acc + ""`）。泄漏循环已验证（2000×）。
- ✅ corpus-wall ACCEPT（ownership 16303）、diff-fuzz 绿、4 个 `*_loop_reclaims` 测试仍通过，并新增 wasmtime cargo 测试（`string_accumulator_parser_tco_executes_on_wasmtime`）。

<a id="progress-commit-1d8bdd92--step-2-partial-multi-accumulator-reset--cross-read"></a>

## 进展（提交 1d8bdd92）— 第 2 步部分完成：多累加器重置 + 交叉读取

多累加器缺口分为四个子缺口（各有最小复现）。其中两个已完成：
- ✅ 将堆累加器重置为新的空值（`cur = []` / `acc = ""`）——允许作为循环携带槽更新（解析器在分隔符后重置当前行累加器）。
- ✅ **heap-acc-reads-heap-acc**（`out = out + cur`，同时 `cur = ""`）——每轮堆赋值按读取依赖的拓扑顺序发出（读取者先于被读取者）；只有环会被阻挡。双 String 累加器解析器现与 v0 字节匹配（泄漏循环已验证，cargo 测试 `multi_accumulator_reset_and_cross_read_tco_executes_on_wasmtime`）。

完成（提交 cd8ad5e6）：✅ **标量变量列表字面量** `[pos]`——`lower_call_args` 经 `try_lower_scalar_list_construct` 物化它（平坦 `DynList` + store64）。

完成（提交 fc4d8425）——最大难点：✅ **嵌套堆元素列表** `List[List[String]]`。新增 `Op::DropListListStr` 渲染嵌套 wasm 循环（释放每行单元格、每行、再外层块）；`try_lower_concat_list` 允许 `List[String]` 元素（`rows + [cur]`、`__list_concat_rc`）；`try_lower_str_list_literal` 构建 `[cur]` 单元素列表；循环内赋值处理重置（`cur = []`）；此类型的每个值都进入新 `list_list_str_lists` 集合（经 `is_list_list_str_ty`，在 `is_heap_elem_list_ty` 之前检查），使其使用嵌套 drop。泄漏循环最初 OOM-trap（调用结果临时值路由到平坦 drop）→ 通过在所有跟踪位置正确路由而修复。csvcore 与 v0 字节匹配、2000× 泄漏循环干净、corpus-wall ACCEPT、csv 分类 **5/6 → 7/4**（parse_rows_rec + parse_after_field 现可降低）。

完成（提交 b871b73d）：✅ **`[]` 堆结果 if 分支**——`lower_heap_result_arm` 物化空列表分支（`if is_empty(t) then [] else parse_rows_rec(...)`）。csv 7/4 → 8/3（parse_rows 可降低）。

发现（探针）：看板建议的 **`list.map` 闭包**能力大部分早已完成——scalar / String / Value / block-body / nested-map / map|>join 闭包均字节匹配。真正剩余 csv 阻碍更窄（特定值构造），不是一般闭包缺口。

完成（提交 47301322）：✅ **`Result[Value, String]` ok/err 包装**（csv `parse` 的 `ok(value.array(...))`）。新增 `Op::DropResultValue` → 自托管 `$__drop_result_value`（tag 分派：Ok → `$__drop_value`，Err → `rc_dec`）；`try_lower_result_value_ctor`（在 lower_tail + if 分支中）经 `lower_owned_heap_field` 物化 Ok（处理 `value.*` + 嵌套 `list.map`），路由到新 `value_result_results` 集合（`is_value_result_ty`）。ok/err + match 读取往返字节匹配；corpus-wall ACCEPT；2000× v1 无 OOM。**csv 分类 8/3 → 11/2（parse 可降低）。**

<a id="valuejson-subsystem-the-csv-records-dependency--phase-1-done"></a>

## Value/JSON 子系统（csv-records 的依赖）— 第 1 阶段完成

最后两个 csv 函数需要自托管的动态 Value OBJECT + JSON 操作（此前均不在 v1 信任主干中）。这是多函数子系统，拆分如下：

完成（提交 a0dcf31d）：✅ **value.object + json.keys**（tag-6 Object）。每对 2 槽的块（key String + value Value，移入时 rc_inc），通过 `__drop_value` 新 tag-6 分支的递归 `__vdrop_obj`。捕获的关键错误：`@8` 必须存槽数量（2·pairs = freelist 回收的分配大小）——存 pair 数会每轮泄漏 2 槽（2-pair OOM 被泄漏循环捕获）。与 v0 往返字节匹配、2000× 多 pair 泄漏循环干净、corpus-wall ACCEPT。

仍待完成（更深 Value 部件——每项都是实际子缺口，不是快速加法）：
- ❌ **value.get** — 已构建扫描（标量自递归 → 索引）+ Ok/Err 包装，但 (a) 自递归 `Result[Value,String]` 主体产生无效 wasm（TCO×Result 交互），(b) `?? value.null()` 解包和 Value-Ok Result 的 `match ok(v)` 读取错误绑定／被阻挡（Value-Ok Result 的 unwrap_or + match-read 未接线，只有 String-Ok 已接），(c) v0 Err 消息为 `missing field
  '<k>'`，而非固定字符串（字节匹配需要插值）。已回退（保持未构建，不提交损坏版本）。
- ✅ **value.stringify** — 完成（提交 85e05369）。完整递归 JSON 序列化器，在 value_core 中自托管，与 v0 字节相同（null/bool/int/float/带引号转义字符串/`[…]`/`{…}`）。数组／对象递归是 String 累加器，分隔符为 `string.repeat(",", k)`，其中 `k` 为标量 if——这绕开了首次尝试被阻挡的循环体 heap-result-if（真实缺口为标量 while 主体中的 `let sep = if i==0 then "" else ","`；此绕行无需扩展 TCO）。两个使能修复：(1) 自动链接（render_program + lower_source）现在将内部 impl-name 调用改写为重命名后的 call_name（因此 `__vstr_arr` 经 `value_stringify` 递归时，解析到重命名后的 `value.stringify` 定义）；(2) 用作 Module 调用参数的 `prim.load_str` 借用按 Handle 传递，无作用域末尾 drop（否则会重复释放 Str Value 的 tag-4 载荷）。另修复既有问题：任何 Value-drop 操作都会强制链接 value_core（json.* 构造的 Value 的 `__drop_value` 曾悬空——修复 json_scalar/json_string）。全部 7 种 value + 嵌套 + 转义与 v0 字节匹配；2000x 泄漏循环干净；corpus-wall ACCEPT；套件 484/0。注意：循环体 heap-result-if 仍是一般缺口（TCO 扩展，路径 (a)）——这里只是绕过，并未关闭。
- ❌ **parse_records 闭包** — `data |> list.map((row) => { … nested list.map → (key, value.str) …
  value.object(pairs) })`：block 主体闭包构建 `List[(String,Value)]` 后构建 Value object——闭包被阻挡（“unliftable closure”），与 value.object 现已存在无关。

<a id="valueget--stringify_records--blocked-by-the-layout-brick-heap-result-of-valuelist"></a>

## value.get + stringify_records — 被布局部件阻挡（heap-Result-of-Value/List）

尝试 value.get 后回退（阻碍 + 乱码；从未交付损坏版本）。精确发现：
- v0 规范：`value.get(o,k)` → `Ok(v)` / `Err("missing field '<k>'")`（插值消息）。stringify_records 中使用 `value.get(row,h) ?? value.null()`。
- value.get 构造器被阻挡：主体 `if idx<0 then Err(msg) else { rc_inc; let val; Ok(val) }` 是返回 `Result[Value,String]` 的堆结果 `if`，报“heap-result if outside the executable subset”——`Ok(<rc_inc'd borrowed Value>)` + `Err(<concat/let message>)` 分支不在 `try_lower_result_value_ctor` 子集中（它处理新的 `value.*` / list.map 上的 Ok，不处理借用后共同拥有的 Object 槽）。
- value.get 读取乱码：`match value.get(…){ ok(v)=>…, err(e)=>… }` 返回 `ok:0|err:0`，而非 `ok:7|err:'missing field'`——**Result[Value,String] 读取**（match + `??`）未接线（`materialized_results_str` 只跟踪 `Result[String,_]`，Value-Ok 载荷读取错误绑定）。
- stringify_records 首行是 `value.as_array(v) ?? []`，即 `Result[List[Value],String]` 解包——同一布局部件阻碍（as_array 错误绑定，value_core 第 101–112 行），在 value.get 之前就遇到。

因此 value.get 和 stringify_records 都需要布局部件：载荷精确的堆载荷绑定 + heap-Result-of-Value/List 构造与读取（match/`??`）。这是实质性、已记录的机制部件，不是快速添加。（value.object/json.keys/value.stringify 仅需 Value 构造 + 借用读取，均已落地；缺口特指 heap-Result-of-Value/List 往返。）

仍被阻挡（csv 全量字节匹配最后两个，都是构建 value.object 的闭包）：
- ❌ `parse_records` — `data |> list.map((row) => { … value.object(pairs) … })`：从 `header`/`row` 构建 `value.object` 的 block 主体闭包（`list.zip`/pairs 形状）——比 `value.array(value.str)` map 更复杂的 Value 构造。
- ❌ `stringify_records` — 分支使用 `json.keys` + `value.object` + Value object 上嵌套 `list.map(... ) |> list.join` 的堆结果 `if`。
注意：字节匹配 driver 也需要自托管 `value.stringify`（当前未链接）——与降低正交的独立标准库 runtime 能力。

<a id="separate-blocker-tomls-v0-oracle-is-broken"></a>

## 独立阻碍：toml 的 v0 oracle 已损坏

toml 最初被提议为目标，但 `almide run`（native v0）为其发出无效 Rust（`error[E0308]: expected String, found &str`，2×）——因此修复该 v0 Rust-codegen 错误前，toml 没有字节匹配 oracle。故改选 csv（v0 测试通过）。toml 的 v0 问题须作为独立 v0 后端问题修复后，才能对 toml 验证字节。

<a id="status-2026-06-22--the-5-brick-tco-works-in-isolation-blocked-on-the-corpus-caps-gate"></a>

## 状态（2026-06-22）— 5 部件 TCO 单独正常，阻于 corpus caps 门禁

实现完整 5 部件 effect-fn 元组结果 TCO，并在隔离条件下证明其工作，随后回退（②纪律——违反 corpus-wall）。正常工作的部分（验证后为保持树健全而回退）：
- `cm2`（effect fn `(Value,Int)` 自递归解析器，累加器 `pairs + [(k, value.str(x))]`）→ **与 v0 字节匹配** + `cm2leak` 10000× **无泄漏**。
- `ftup`（effect fn 返回元组 + `let (v,p)=f()!` 解构）→ ✅；`cm2p`（纯）✅；`cm4`（List 结果）✅；`parse_rows_rec`（csv，List 元组）✅（无回归）；mir 套件绿。
- **yaml 模块 → 0 个降低阻碍**（collect_map/collect_seq 已降低）。

5 个部件（全部必须一起存在）：
1. `tco_empty_for` → Value（`value.null()`）／标量（`0`）／Tuple（递归）——结果累加器的空值。
2. 对**含 Value 元组**结果路由 `result_var`（`use_result_acc = base_reads_loop_local
   || tuple-with-Value`）——当基例有 `value.object(..)` 调用，循环后分派读取元组基例同级标量 carry 的陈旧值。（无 Value 的元组，如 csv `pf` 的 `(acc,pos)`，不能这样路由——它通过分派正常工作，改路由会使 parse_rows_rec 回归。）
3. `lower_owned_heap_field` 中的 `(Value,scalar)` / `(String,Value)` 元组（通过 try_lower_tuple_construct）。
4. `__drop_value_tuple`（value_core）——单个 `(Value,Int)` 元组的递归 drop（@12 的 Value 槽 DropValue + 块）；通过 `variant_drop_handles="value_tuple"` 路由（平坦 record_masks 会泄漏 Value 载荷 → 10⁴ OOM）。
5. 解构种子（`lower_destructure`）：当 `value.kind = Unwrap|Call` 时，从模式而非 `value.ty` 推导堆槽 mask（effect-fn `f()!` / 去掉 never-err 后的 `f()`，其 `.ty` 是 effect Result，而非 Ty::Tuple）——否则解构按容器整体粒度处理（`p` 读到 0）。对于 Tuple 值保留 value.ty 路径，对于普通 Var 保留无种子的容器粒度路径（`tuple_destructure_aliases_components` 单元测试）。

**回退原因——corpus caps 门禁（剩余工作，风险已厘清）：**
- **mir > ir caps 违例（1 个函数）**：`tco_empty_for` 的 `value.null()` 是 TCO 插入的合成调用，IR 没有，因此 `count_ir_calls`（IR）< mir CallFn 数量 → `mir<=ir` 门禁失败（[[project_v1_gate_count_vs_lower]] 类）。修复：使 `count_ir_calls`（或门禁）计入 TCO 的合成 `value.null`（两者计数前都先脱糖，或计入空初始化调用），类似现有 TCO 合成操作的计数方式。
- 变更下**某 corpus 函数 PANIC（全定义性违例）**——找出它（corpus-wall caps 步骤会命名），并针对其形状加固新元组路径（可能是 (Value,scalar) 分支或 seed 误处理某种元组／聚合形状；以干净拒绝代替 panic）。
- 另有 yaml 完整 `parse` 仍需自托管 **float.parse / list.enumerate / string.to_lower**（解析路径引用的标准库缺口，与 TCO 独立）。

下一会话（已确定）：重新应用 5 部件（本节为精确方案），在重测 corpus-wall 之前修复两个 corpus 门禁问题（合成调用计数 + panic），然后补 3 个标准库缺口，再跑 yaml `almide test`。每步以 cm2（必须字节匹配）+ parse_rows_rec + tuple_destructure + corpus-wall（4 ACCEPT）为门禁。

<a id="status-2026-06-22-later--mirir-breach-solved-better-than-planned-suite-breaking-recursion-fixed"></a>

## 状态（2026-06-22，稍后）— mir>ir 违例已解决（优于原计划）；破坏套件的递归已修复

两个基础提交已落到 develop-v1（门禁：mir 套件单线程 + 并行均 501/0，corpus-wall 4/4 ACCEPT）：

1. **mir>ir caps 违例消失——内联 `value.null()`，没有改门禁**（提交 6ca50e85，`lower/calls.rs`）。不是教 `count_ir_calls` 计入合成调用（那会给信任门禁添加依赖降低的逻辑——保全类 KPI 禁止的 [[project_v1_gate_count_vs_lower]] 反模式），而是现在将 `value.null()` 直接内联降低为 tag-0 Value 块（Alloc + store32），与 String/List 空值一样。因此没有 CallFn：TCO 合成空值增加 0 个 mir 调用（门禁中性）；显式 `value.null()` 源节点仍计入 IR（mir < ir，允许）。corpus-wall ACCEPT 不变。**这完全消除了 corpus 门禁阻碍 (a)，未触碰门禁。** 重新应用部件 1 时，`tco_empty_for` 的 `Value → value.null()` 现已不增加调用。

2. **已修复此前静默破坏整个 mir 套件的既有无限递归**（提交 4536f33c，`render_wasm/tests_part1.rs`）。`lower_source` 测试 helper 递归链接自托管源码；Value-drop 强制链接（a40c1332）在降低 value_core 自身时将 value_core 的 `any_called` 置位（其 drop helper 发出 DropValue 操作），但 `any_defined` 只匹配调用名（`value.null`），不匹配实现名（`value_null`），于是永远重新降低 value_core → 栈溢出、中止测试进程（遮蔽整个套件及任何 cargo-test 门禁工作）。修复：`any_defined` 现在匹配调用名或实现名任一。`capturing_heap_map_over_value` 测试（及套件）重新变绿。注意：不是 value.null 内联造成——禁用内联也能复现；这是正交的测试 helper 基础设施问题。

yaml 剩余：重新应用部件 1–5（mir>ir 现已无代价），定位并加固 corpus panic（阻碍 b，仍未解决，corpus-wall caps 步骤会命名），然后补 3 个标准库缺口（float.parse / list.enumerate / string.to_lower），再做 yaml 字节匹配 + 泄漏循环。

<a id="brick-re-application-probe-2026-06-22--bricks-12-drafted-then-reverted-sharper-blocker-map"></a>

### 重新应用部件的探针（2026-06-22）— 部件 1+2 已起草后回退（更精确的阻碍图）

重新应用部件 1（`tco_empty_for` → Value/scalar/Tuple）+ 2（`tuple_with_value` 路由到 result_var），探测 `cm2`。发现如下（全部回退以保持树健全——没有 3–5 会回归）：

- **仅部件 1+2 会使 corpus-wall 覆盖回归**（仍 ACCEPT/0-panic，但计数下降：ownership 16556→16549，names 3850→3849）。此前经循环后分派降低的某返回 Value 元组的 corpus 函数，现改走 result_var 后被阻挡（缺少部件 3–5 的元组构造／drop／解构）。因此 1–5 必须原子化，绝不能只提交 1+2。
- **部件 1+2 后，`cm2` 真正阻碍不是元组构造**——`try_lower_tuple_construct`（binds.rs:1121）已能降低堆类型元组元素（每个堆槽调用 `lower_owned_heap_field`，记录 `record_masks`）。阻碍在 `lower_while`（control.rs:3338，`body_reassigns_heap`）：TCO 产生的 `while` 主体执行 `pairs = pairs + [("k", value.str("x"))]`——这是元素为 `List[(String,Value)]` 的堆追加累加器。`try_lower_scalar_while`（control.rs:1940）拒绝（其追加累加器路径尚不允许 `(String,Value)` 元组元素），故只建模一轮的回退路径阻挡。**因此部件 3 实际是“允许 `try_lower_scalar_while` 堆追加累加器中的 (String,Value)/(Value,scalar) 元组元素”，而不是独立 `try_lower_tuple_construct`（它已正常）。** 应据此调整部件 3 范围。
- `cm2` 的 v0 oracle（`local effect fn cm2(n,pos,pairs)->(Value,Int)`）：`{"k":"x","k":"x","k":"x"}`，然后 `3`。仅通过 v1 MIR 主干测试（`examples/render_program` → WAT → wasmtime，或 `lower_source`），不要用 `almide run --target wasm`（这是旧 almide-codegen 管线，另一个后端；它会更早在 ANF 阶段对 `List[(String,Value)]` 参数 panic，与 MIR 主干无关）。

<a id="status-2026-06-22-yaml--tco-wall-gone-yaml-lowers-to-14128-line-wat-1-stdlib--1-render-bug-left"></a>

## 状态（2026-06-22，yaml）— TCO 阻碍消失；yaml 降低为 14128 行 WAT，剩 1 个标准库和 1 个渲染错误

effect-fn 元组结果 TCO（5 部件）已落地并通过门禁（提交 141f6c09；cm2 字节匹配 + 10⁴ 无泄漏、corpus-wall 4/4 ACCEPT 0 panic、mir 套件 501/0）。加上本会话两个自托管叶子标准库函数，**`almide/yaml` 的 `parse` 在 v1 MIR 主干降低时 TCO 阻碍为零**——整个路线图针对的结构阻碍已解决。

本会话自托管（已提交、隔离字节匹配、列入 PURE_MODULES）：
- **string.to_lower**（提交，ASCII 折叠）——`stdlib/string_to_lower.almd`。
- **float.parse**（提交，sign/int/.frac/e-exp，单次精确 pow10 缩放）——`stdlib/float_parse.almd`。无元组（单 Int/Float helper）；对 yaml 语法与 `s.trim().parse::<f64>()` 字节匹配。

yaml driver = `mod.almd` + 调用 `parse("a: 1\nb: hello\nc:\n  - x\n  - y\n")` 的 `main`（通过 `examples/render_program` → WAT → wasmtime 测试；不是 `almide run --target wasm`，后者为旧 codegen）。之后 `render_program` 仅报告 `unlinked: list.enumerate`；使用已回退的 list.enumerate 原型可达 0 阻碍（14128 行 WAT），但运行时失败——剩两个错误：

1. **`list.enumerate`（最后一个标准库缺口）。** (Int,String) 元组列表构建机制正常（无元组的 `__enum_str_rec` 在 `List[(Int,String)]` 上做追加累加器；将 `(Int,String)` 元素加入 `try_lower_concat_list`/`try_lower_str_list_literal`/`lower_owned_heap_field`；`list_enumerate.almd` 中的 `$__drop_list_int_str` 递归 drop；`enumerate→enumerate_str` 分派；rc_dec 白名单中的 `__isdrop_list`；PURE_MODULES 中的 `list_enumerate`——全通过门禁，corpus-wall 4/4，覆盖 +50）。因消费者误编译而回退：yaml 使用 `lines |> list.enumerate |> list.find((e) => not is_blank(e.1))`；在 `List[(Int,String)]` 上带读取元组 `.1` 闭包的 `list.find` 未做 C1 defunc（find ∉ map/filter/fold defunc 集合），误分派到 `find_str`（把每个元组当成 String → 垃圾，输出 `none`/`0:`）。真正剩余工作是带闭包的堆元组元素 find（[[v1-selfhost-machinery]] Machinery-3 闭包表面），不是构建器。先重新落地构建器（它健全），再解决 closure-find。
2. **`seq_map` 的 WASM 类型不匹配（i64 与 i32）** 只在完整 yaml 降低中出现（`local effect fn seq_map -> (Value,Int)` = 返回元组的尾调用 `parse_mapping(list.set(…))!`）。不是自递归（与已提交 TCO 无关）；可能是 `list.set`／元组结果尾调用路径的渲染错误。用 seq_map 形状 fixture 隔离。

注意——yaml 的 v0 native oracle 已损坏：当前 native Rust 后端错误借用 `find_colon`（`find_colon_at(s.to_string())` E0308），所以 `almide test`/`almide run` 不能在 v0 编译 yaml。使用 yaml_test.almd 预期值（或修复 native 借用错误）作 oracle。v1 主干不受影响。

<a id="status-2026-06-22-late--listenumerate--closure-find-done-yaml-emits-valid-wasm--runs-the-parser"></a>

## 状态（2026-06-22，晚些时候）— list.enumerate + closure-find 完成；yaml 发出有效 wasm，并运行解析器

用户要求的“重新落地构建器，再解决 closure-find”已完成、提交、通过门禁：
- **`60deaf24` list.enumerate + 带闭包的 find-over-(Int,String)。** (Int,String) 元组列表构建器（追加累加器 TCO + try_lower_concat_list / try_lower_str_list_literal / lower_owned_heap_field 中的 (Int,String) 元素 + 递归 `$__drop_list_int_str`），`list_find_int_str`（加载每个元组 HANDLE，交给读取 `e.1` 的谓词闭包），堆载荷 `Option[(Int,String)]` 构造（`materialize_opt_int_str_some`，共同拥有 Dup）+ 物化识别 + value 结果 match 的 `some((idx,line))` 解构，以及 `List[(Int,String)]` 调用结果 drop 路由（`is_list_int_str_ty` → `variant_drop_handles`）。`enumerate |> find((e)=>…e.1…)` 形状与 v0 字节匹配（`1:b`）+ **10⁴ 无泄漏**；corpus-wall 4/4 ACCEPT（覆盖 16620）；mir 套件 501/0。关键：yaml 的 match 分支返回 Value（`value.null()`/`v`）→ `try_lower_variant_value_match`（有元组载荷脱糖），而非 Unit 主体的 `try_lower_variant_match`；Unit 主体测试会线性化，但 yaml 不是该形状。
- **`96e94de7` List[String] 上的 list.set（`list.set_str`）。** 通用 `list.set` 的 i64 val 参数与 String 元素（i32 handle）不匹配 → `seq_map` 中 wasm 无效。set_str 对每槽做 rc 复制（共同拥有），替换槽 i（旧值 rc_dec，新值共同拥有）；结果用 `DropListStr` 释放。与 v0 字节匹配。

**yaml 现在编译为有效 WASM（约 13.7k 行 WAT，0 阻碍），并执行完整解析器**——`parse → parse_lines → dispatch → parse_mapping → collect_map → map_entry → after_colon → parse_inline
→ parse_scalar → scalar_typed → scalar_value → scalar_numeric → parse_number`。它在 map-entry 值的 `parse_number` 中 trap（`rc_dec`，重复释放）（`parse("a: hello")` trap；裸标量 `parse("hello")` 不 trap，返回 `{}`）。忠实复现无法重现此 trap（`match int.parse(c){ok=>value.int,err=>value.str(raw)}` 形状、带 parse_number 值的 collect_map (String,Value) 累加器、通过 effect-fn `(Value,Int)` 的 `!` 传递循环局部 `after`——隔离时全部字节匹配）。因此是完整数值解析链中的微妙交互。怀疑（据 WAT）：某 `Result[Int,String]` drop（如 `int.from_hex` / `int.parse`）逐元素 `rc_dec` 载荷槽——对 `Err(String)` 正常，对 `Ok(Int)` 则重复释放／将标量当 handle；或借用 `raw`/`after` 在值逃逸到累加器时重复 drop。下一步：在 `parse("a: hello")` WAT 中导出 `$parse_number` 精确触发 trap 的 `rc_dec` 局部变量，追溯源 drop，修复 Result／借用 drop 路由——然后 yaml 端到端运行（注意 v0 native oracle 仍被 `find_colon` 借用 E0308 破坏，因此对照 yaml_test.almd 预期值做字节匹配）。

<a id="status-2026-06-22-runs---almideyaml-runs-on-v1"></a>

## 状态（2026-06-22，运行）— 🎉 almide/yaml 在 V1 上运行

`almide/yaml` 的 `parse` 能降低（0 阻碍），并在 v1 MIR→WAT→wasmtime 主干上按正确字节执行。已验证往返（parse → yaml 自身 stringify，与输入相同）：`a: 1\nb: hello\nc:\n  - x\n  - y`、`name: Alice\nport: 8080\npi: 3.14`、`items:\n  - one\n  - two\n  - three`、`nested:\n  a: 1\n  b: 2\nflag: true`。
“在 almide v1 上运行 almide/yaml”的目标已达成。本次推送提交（全部门禁绿、corpus-wall 4/4、mir 套件 501/0）：
- `60deaf24` list.enumerate + 带闭包的 find-over-(Int,String)（堆载荷 Option 机制）。
- `96e94de7` list.set_str（List[String] set；i64-val 通用 list.set 产生无效 wasm）。
- `6f2128f3` heap-result-if **Match 分支**的逐分支 frame（求被匹配值的临时值 `string.drop(c,2)` 泄漏到函数作用域末尾 → 其他分支运行时发生无条件 `rc_dec(0)` trap，即 parse_number 的 0x 崩溃）；另有 `drop_arm_locals` 截限。
- `59a5164a` 两个 heap-Value 泄漏：(1) 堆结果尾部 `f(string.replace(s,..),s)->Value` 绕过作用域末尾 drop，泄漏其拥有所有权的临时参数——CallFn 后释放；(2) let 绑定的 Named 调用 Value 结果没有 `value_handles`/DropValue 标记（Module 路径有）。

剩余——泄漏长尾（生产加固，而非“是否运行”）：反复解析最终 OOM（map 输入到约 1000× 尚可，sequence 路径——collect_seq/seq_item/dash_item/dash_after——更早泄漏）。OOM 在下一次分配落点显现（随着修复落地，从 parse_number 移至 find_colon_at），因此是每次解析少释放的累积，而非单一位置。模式：迄今每个泄漏都是某堆值（临时值／let 局部值／调用结果），因堆结果尾部／分支绕过函数作用域末尾 drop 而未被释放——审计 sequence 路径（dash_after 的 `(parse_inline(after), pos+1)` 元组 + collect_seq 的 List[Value] 累加器 + `string.get(s,pos) ?? ""` 的 `??` 操作数 Option 临时值）是否存在同类问题。单次／少量解析正确且平衡；这是深层循环加固。（v0 native oracle 仍被 `find_colon` 借用 E0308 破坏——使用往返与 yaml_test 预期值。）

<a id="status-2026-06-22-base64tomlaes--aes-conquered-base64-94-the-two-deep-frontiers"></a>

## 状态（2026-06-22，base64/toml/aes）— aes 完成，base64 9→4，两个深层前沿

此次推送针对三个仓库特有阻碍（base64 / toml / aes）。已落地，全门禁绿（corpus-wall 4/4 ACCEPT、mir 501/0），每项均为可复用机制：

- ✅ **aes 运行**（FIPS 197 `encrypt_block` 与 v0 字节匹配）。关键能力 = **const 模块全局值物化**：带 const 初始化器的模块级堆全局值（`let SBOX = bytes.from_list
  ([…])`、字符串字面量、整数列表字面量）现在直接降低为 `Alloc`——新 `Init::Bytes`（任意字节块，不同于 `Init::Str`，因为 S-box 含 0x00–0xFF）。计算型初始化仍被阻挡（物化会注入门禁 IR 侧 `count_ir_calls` 看不到的 `CallFn` → mir>ir）。`global_inits` 穿过 render/gate/cert，使三者一致。（提交 ac7fae5c）
- ✅ base64 `char_to_val`——堆结果 `Result` 的 **Err 分支消息为 `${}` 插值**（`err("bad '${ch}'")`）：ResultErr 分支折叠插值，并逐分支释放中间值。（fec44e1a）
- ✅ base64 `encode_chunks` 自追加——**多重拼接累加器** `acc + c0 + c1 + …`：TCO 的 is_self_append 沿 ConcatStr 左主链递归到最左叶。（4f2bfdeb）

base64 现有阻碍 9→4；aes 为 0。**剩余 4 个 base64 阻碍 + toml 22 个中的大多数，瓶颈是两个深层前沿——设计切片，而非增量部件：**

<a id="frontier-a--the-let-bound-heap-result-if-in-a-looprecursive-body-highest-leverage"></a>

### 前沿 A — 循环／递归体内 let 绑定的堆结果 `if`（最高影响）
标量循环体内的 `let c2 = if cond then A else B`（base64 encode_chunks/decode_chunks；toml parse_val/read_basic/…）。binds.rs 阻挡一般 let 绑定 heap-result `if`：let 绑定值保留到作用域末尾释放，但合并后的 `IfThen` dst 在平坦证书中没有健全的作用域末尾 drop（将一个 drop 归给两个分支分配中恰好一个，需要 checker/Coq 改动）。函数尾部已通过 `desugar_heap_branches` 正常处理（尾部复制到各分支）。**设计（不改 Coq）：把尾部复制扩展到 continuation 为递归的 let 绑定 `if`**——将 `{ let c = if k then A else B; recurse(acc+…+c) }` 改写为 `if k then recurse(acc+…+A) else recurse(acc+…+B)`，把 continuation（递归调用 + 累加器更新）复制到各分支，在降低前消除 let 绑定 `if`。随后每分支有字面量 `c` 和自己的自调用；`tco_collect` 必须允许分支式自调用（每分支一个，各为条件式累加器 SetLocal——循环体 Assign 已支持）。风险：TCO 与分支调用的组合；由已建模的所有权检查加运行时／差分探针保护（证书不涵盖每个受信任 runtime 或 renderer 操作）。此单一能力解除 base64 ×2 + toml 很大部分阻碍。

<a id="frontier-b--the-heap-payload-resultvariant-match-camp-4"></a>

### 前沿 B — 堆载荷 `Result`／variant match（Camp-4）
`Result[Bytes, String]`（base64 decode/decode_url）上的 `match bs { ok(bytes) => …, err(e) => … }`。`try_lower_variant_value_match` 允许标量载荷 variant（在分支前 drop 被匹配值），但拒绝堆载荷（分支借用被匹配值的堆槽——`Option[String]`/`Result[Bytes,_]`）。这是已记录的 Camp-4 前沿（[[v1-selfhost-machinery]] Machinery 2 扩展至 match-read）。需要先提取堆载荷、再执行分支的机制（分支把 slot-0 作为借用读取，被匹配值随后 drop）。

<a id="toml--also-unwrap--in-a-call-arg-position--while-body-heap-accumulator-on-top-of-frontier-a"></a>

### toml — 除前沿 A 外，还包括调用参数位置的 `unwrap !` + while 主体堆累加器。

<a id="status-2026-06-22-turn-2--base64-encode-conquered-camp-4-is-the-last-frontier"></a>

## 状态（2026-06-22，第 2 轮——base64 编码完成，Camp-4 是最后前沿）

本轮落地三个可复用降低能力（全门禁绿、corpus-wall 4/4、mir 501/0）：
- ✅ **多重拼接自追加 TCO**（`acc + c0 + c1 + …`）——is_self_append 沿 ConcatStr 左主链递归。（4f2bfdeb）
- ✅ **TCO 前 let 绑定 heap-result-`if` 预脱糖**——在 `try_tco_rewrite` 前对 func.body 应用 `desugar_heap_branches`，使 `{ let c = if k then A else B; recurse(acc+c) }` 变为分支递归 `if k then recurse(acc+A) else recurse(acc+B)`（tco_collect 递归处理两分支）。（8cf460e5）→ **base64 `encode_chunks` 可降低；base64 编码运行**（`encode("hello")=aGVsbG8=`，规范字节匹配，standard + url）。（1a871eed + fixture spec/wasm_cross/base64_encode.almd）
- ✅ **let 绑定 unwrap-`!` 脱糖**——`{ …; let v = e!; rest }` → `{ …; match e { ok(v) =>
  {rest}, err($x) => err($x) } }`（`!` 即遇 Err 提前返回，match 成为尾部）。corpus 所有权覆盖 16744→**16877（+133）**——最大的跨仓库阻碍（unwrap-bound-to-let）对 scalar/str-result 形状已解除；str-result unwrap 与 v0 字节匹配。（a564a12e）

**base64 = 9→3 个阻碍（编码完成）；最后 3 个（decode/decode_url/decode_chunks）+ toml 全部 22 个，现在共同阻于一个前沿：Camp-4——堆载荷 `Result`／variant match。**

<a id="camp-4-design-the-last-frontier--precise-mechanism-not-yet-implemented"></a>

### Camp-4 设计（最后前沿——精确机制，尚未实现）
`match e { ok(v) => …, err(x) => … }` 中绑定的载荷（`v` 或 `x`）为堆值（`String`/`List`）。两个子情况，难度递增：
1. **scalar-Ok / heap-Err**（decode_chunks 的 `let v0 = char_to_val(c)!`，作用于 `Result[Int,String]`）：err 分支绑定 slot-0 String，unwrap 脱糖的 `err($x)` 再包装它。需要：也将 `Result[scalar, String]` 被匹配值跟踪为 `heap_elem_lists`（使 `heap_or_scalar_bind` 允许 String Err 绑定，且 `drop_op_for` 使用 `DropListStr`——正确：Ok=len0 不释放任何元素，Err=len1 释放 slot-0），同时保留 `materialized_results` len@4 tag 读取；err 分支重新包装前必须对借用的 slot-0 String 做 `Op::Dup`（否则随后 drop 被匹配值会重复释放）。验证 tag 偏移（len@4）与 drop（DropListStr）的组合——风险为偏移读取错误或重复释放，两者都会被门禁捕获。
2. **heap-Ok / heap-Err**（decode 在 `Result[List[Int],String]` 上的 `match decode_with(..) { ok(bytes) => ok(string.from_bytes(bytes)) }`）：完整 Camp-4——Ok 分支借用并消费堆 List。这是已记录的借用槽移出（[[v1-selfhost-machinery]] Machinery 2 的 match-read）。

Camp-4 涉及重复释放风险（记忆中明确的疲劳警告区域）：重新实现，以所有权事件检查器加运行时和差分探针为安全网。仅证书接受不能排除每种 runtime 重复释放。这是完成 base64（解码）并解除 toml 大部分阻碍的单一关键能力。

<a id="status-2026-06-22-turn-3--7-levers-total-289-corpus-coverage-heap-ok-result-is-the-last-frontier"></a>

## 状态（2026-06-22，第 3 轮）— 总计 7 项能力，corpus 覆盖 +289；heap-Ok Result 是最后前沿

本轮新增（全门禁绿、corpus-wall 4/4、mir 501/0）：
- ✅ **let 绑定 unwrap-`!` 脱糖**（`let v=e!` → `match e {ok(v)=>rest, err(x)=>err(x)}`，+133）。
- ✅ **Camp-4 子情况 1**（scalar-Ok / heap-Err `Result[Int,String]` match：Err-String 绑定的 heap_elem_lists 跟踪 + DropListStr + ResultErr 分支对借用载荷做 Dup，+137）。
- ✅ **if 分支内 unwrap 脱糖**（`let v=if c then e! else d` → `let $r=if c then e else ok(d); $r!`）。

会话合计：corpus 所有权覆盖 16744→17033（+289），7 个可复用降低能力（module-global Init::Bytes、Err-interp 分支、多重拼接 TCO、let-bound-if 预脱糖、unwrap-in-let、Camp-4 子情况 1、unwrap-in-if-arm）。**aes 运行；base64 编码运行。**

<a id="the-last-frontier--heap-ok-result-camp-4-sub-case-2-resultlistint-string"></a>

### 最后前沿 — heap-Ok Result（Camp-4 子情况 2）：`Result[List[Int], String]`
base64 `decode_chunks` 被 `ok(acc)` 阻挡，其中 `acc: List[Int]`——heap-Ok Result。base64 `decode`/`decode_with` + toml 的 heap-Ok match 需要同样能力。这是新布局：现有 heap-Ok Result 处理仅适用于 str-result（`Result[String,String]`/`Result[Value,String]`/`Result[List[Value],String]`，经 materialize_result_str + cap-tag@16）。`Result[List[Int], String]` 的 Ok 载荷为 `List[Int]`，不是 String/Value，所以需要：(a) List-Ok Result 的布局 + tag，(b) heap-result-if 分支里的 `ok(list)` 构造器物化，(c) `match r { ok(bytes) => … }` 的堆 List-Ok 绑定 + drop。这是完成 base64 解码和 toml heap-Ok match 的唯一剩余能力——真正的新布局切片（重新实施，用门禁作重复释放安全网）。

<a id="status-2026-06-22-turn-4-effortmax--camp-4-landed-decode_chunks-needs-tco-over-match"></a>

## 状态（2026-06-22，第 4 轮，effort=max）— Camp-4 已落地；decode_chunks 需要跨 match 的 TCO

本轮落地（全门禁绿、corpus-wall 4/4、mir 501/0；会话覆盖 16744→17088，+344）：
- ✅ **if 分支内 unwrap 脱糖**（`let v = if c then e! else d` → 提升 `!`）。
- ✅ **flatten-let-block**（`let v = { s..; tail }` → `s..; let v = tail`）+ **inline-tail-accumulator**（单次使用、let 绑定的 `acc + …` 内联到递归参数，让 is_self_append 接受它）——嵌套 let 绑定 `if` 的 List 累加器现在能降低（decode_chunks 的 `new_acc` 形状，隔离测试）。
- ✅ **Camp-4 子情况 1**（scalar-Ok / heap-Err `Result[Int,String]` match——err 分支 String 绑定 + ResultErr Dup，覆盖 +137）。
- ✅ **Camp-4 子情况 2（值 match）**——heap-Ok `Result[List[Int],String]` 上的值 match 读取 cap-tag @16（匹配 materialize_result_str 构造），而非 len-tag @4；修复布局不匹配（match 曾读取错误 tag，是潜在静默误编译来源），现可字节匹配。

<a id="decode_chunkss-last-blocker--tco-over-a-match-recursion-in-a-match-arm"></a>

### decode_chunks 的最后阻碍 — 跨 `match` 的 TCO（match 分支里的递归）
base64 decode_chunks 仍被阻挡。本轮精确定位根因（复现 `umr`）：unwrap-`!` 脱糖将 `let x = f(pos)!; recurse(.., acc+[x])` 变为 `match f(pos) { ok(x) => recurse(..,
acc+[x]), err(e) => err(e) }`——递归在 match 分支中。但 TCO 的 `tco_collect`/`tco_rewrite` 仅递归处理 `if`/`Block` 尾部，不处理 `match` 分支，因此自调用被视为埋在内部的非尾叶 → TCO 拒绝 → 阻挡。为 tco_collect/tco_rewrite 添加 `Match`（已尝试，因未验证而回退）必要但不充分：还需要 (a) 结果累加器／循环后分派处理两个 Result 基例 `ok(acc)`/`err(e)`（err 基例读 match 绑定的 `e`），(b) TCO 循环体中用户 Result（char_to_val）的 `match` 必须执行而非线性化——即语句 match 被匹配值跟踪须覆盖用户命名调用 Result（另一个潜在线性化误编译 hok2）。三者互相依赖；应重新一起实现并做字节测试（corpus-wall 门禁不捕获字节误编译，因此每项都需要 v0==v1 fixture）。这是完成 base64 解码 + toml 大部分 heap-Ok／unwrap-in-call-arg 阻碍的单一能力。
