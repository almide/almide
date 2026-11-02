说明：动态 Value 模型：自托管加一个受信任的递归释放例程
<a id="v1-dynamic-value-model--the-yaml-keystone-path-a-self-host--one-trusted-recursive-drop-routine"></a>

# v1 动态 Value 模型 — yaml 的关键环节（路径 A：自托管 + 一个受信任的递归释放例程）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-value-model.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-value-model.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-value-model.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-value-model.md)

> 原文历史层次说明：这是叠加的 2026 年 6 月设计/实现记录，不是当前保证。开头的已落地更新与较早的 DESIGN、暂存/回退、剩余障碍及布局提案并存；22→15→13→11 是会话时点统计，不统一成当前总数。后文限定了早先关于证书接受或往返测试证明无泄漏的主张：原始句柄递归例程的证书为空，仍属受信任部分；20000/100000 次有限探针不能证明所有 Value 操作安全。唯一例程的说法与同级释放辅助函数和扩大的白名单并存。顶部 cap-as-tag/as_array 记录与后文独立外壳/@16 损坏讨论，以及自动获取载荷与旧版拒绝返回借用，保留的是不同阶段，而非统一表示。原文将 Result→List[Value]→Value→String 称为 3 层；保留此说法，不暗中重新计数。格式错误的嵌套反引号条目原样保留，并在其外完整翻译。未推断后续实现状态。

**状态：设计（2026-06-19）。CEO 选择了路径 A（“就选 A”）：将受信任基础保持最小，证明 Value 模型（构造器/提取器/序列化器以 .almd 自托管，经证书验证），将递归释放作为唯一的受信任运行时例程（类似已有的 `DropListStr`）。无需 Coq；每个构建块都与 v0 `runtime/rs/src/value.rs` 做字节一致验证。**

<a id="-landed-2026-06-19-the-recursive-value-drop-foundation-valuearray--3-level-recursive-free"></a>

## ★★★★ 已落地 2026-06-19：递归 Value 释放基础（value.array + 3 层递归释放）

路径 A 的关键环节——唯一新增的受信任基础——已经构建并经泄漏验证（不再是暂存状态）：
- **value.array**（value_core.almd）：自包含的 tag-5 Value `[rc@0][tag=5 @4][len@8][elem@12+i*8]`，通过 `__varr_copy`（`prim.rc_inc`）浅拷贝每个元素句柄。已注册。
- **__drop_value**（递归、原始句柄、空证书）：在最后一个引用（rc==1）时按标签分派——tag 5 通过 `__vdrop_arr`→`__drop_value` 释放每个元素 Value，tag 4 释放 String，标量无嵌套内容需要释放。`Op::DropValue` 现在渲染为 `(call $__drop_value …)`（取代会泄漏 Array 元素 Value 的平坦内联释放）。已验证无回归（output-parity 64→65，所有已有标量/Str Value 程序字节一致——它们通过 $__drop_value 以相同方式释放）。
- **__drop_list_value** + **Op::DropListValue** + `value_elem_lists` 集合：`List[Value]` 通过 `$__drop_value` 释放每个元素（平坦的 `DropListStr` 会泄漏各元素 Value 的载荷）。证书将 `DropListValue` 视为一个 `d`（加入 lib.rs + certificate.rs 的分组）。同时接入 `emit_scope_end_drops` 和 `drop_arm_locals`（逐迭代/分支拆除——泄漏循环发现的错误：循环体使用 `drop_arm_locals`，起初仍发出平坦的 `DropListStr`）。
- **List[Value] 实体化**：`try_lower_str_list_literal` 接纳 `[value.int(1), value.str("a")]`（Call 元素），并标记 `value_elem_lists`。
- `prim.rc_dec`/`rc_inc` 白名单 += `__drop_list_value`。

符合 §4.1：没有新增手写 WAT 运行时函数——`$__drop_value`/`$__drop_list_value` 是自托管 Almide（随 value_core 自动链接），复用已证明的 `$rc_dec`/`$rc_inc`。3 层递归释放（Array → 元素 Value → String）由 `spec/wasm_cross/value_array_leak_loop.almd` 验证无泄漏（含 Str 元素的 20000 次创建+释放循环，完成并输出“done”；由于分配器有空闲链表，若有泄漏，会在远早于约 320 次迭代时越过 64KB 并触发越界 trap）。门禁：corpus-wall 接受（14491 个堆对象，无泄漏/重复释放）+ cargo test（包括 §4.1 + 证书）+ output-parity 65/65。这是 CEO 要求“设计优先”的健全性核心——现在经泄漏循环实证验证（证书无法看到 tag-5 泄漏）。8 个 Value 障碍的剩余工作：`value.as_array`（读回，需要具有 Value 感知释放的 Result-of-List[Value] 表示——另一个按标签分派的释放）+ `value.stringify`（递归序列化器）+ value.object。

<a id="-landed-2026-06-19-valueas_array--yaml-1513-is_compound--emit_non_int"></a>

## ★★★★★ 已落地 2026-06-19：value.as_array — yaml 15→13（is_compound + emit_non_int）

Value 数组模型的读取侧。`value.as_array(v)` 以 cap-as-tag 表示返回 `Result[List[Value], String]`（复用字符串结果的 `materialized_results_str` MATCH 机制——tag @16、payload @12——因此 Camp-4 借用绑定 `ok(items) => …` 就是 fab14729 中的 str_heap_bind 借用+随后释放，没有新增 match 代码）。新部分是释放：Value 数组 Result 递归释放其 `List[Value]` 载荷（`Op::DropResultListValue` → 自托管 `$__drop_result_lv`：tag@16 为 0→`$__drop_list_value`，为 1→rc_dec；平坦的 `DropListStr` 会泄漏列表元素 Value）。跟踪由类型驱动（`is_result_listval_ty`：Ok 分支是 `List`），因此在每个字符串结果标记点都健全（try_lower_variant_value_match、lower_branch、seed_variant_param、lower_bind）——Ok 为 List 时使用 `value_result_lists`，为 String 时使用 `heap_elem_lists`。释放操作选择统一到一个 `drop_op_for` 辅助函数（emit_scope_end_drops + drop_arm_locals + 变体 match 的被匹配值释放）。还修复了潜在泄漏：`materialized_call_arg` 现在将 `Value` 调用实参标记为 `value_handles`（→ 递归 `DropValue`），而非平坦的 `Drop`——`f(value.array([…]))` 实参原先会泄漏元素 Value。
已验证：`value_as_array_roundtrip.almd` 与 v0 字节一致（arr_len 3/-1/0、is_compound 1/0——value.array 正确存储 + value.as_array 正确读取 + Ok(items) 借用 + 标签分派）+ `value_as_array_leak_loop.almd`（文中称为 3 层的 Result→List[Value]→Value→String 释放完成并输出“done”）；corpus-wall 接受 + cargo test + output-parity。剩余 Value 障碍（6）：collect_map/collect_seq/seq_item/parse_lines/parse_nested/map_entry（解析时构建 value.array/object）+ value.object/value.stringify。

<a id="-landed-2026-06-19-list-iterator-tco-heap-loop-carried-escape--yaml-1311-oct_rec-bin_rec"></a>

## ★★★★★★ 已落地 2026-06-19：列表迭代器 TCO（堆循环携带值逃逸）— yaml 13→11（oct_rec、bin_rec）

首个在不扩展证书的情况下消除的堆循环携带值递归——通过证书检查的变换（扩展标量 TCO）。对缩小中的列表进行尾自递归的函数（`f(.., cs)` 匹配 `list.first(cs)`，以 `list.drop(cs,1)` 递归）被重写，使 `cs` 成为不变的借用列表，加上一个合成的标量索引 `idx`：`match list.first(cs) { none => BASE, some(ch) => BODY }` → `if idx < list.len(cs) then { let ch = cs[idx]; BODY } else BASE`；每次 `f(list.drop(cs,1), …)` 将 `idx += 1`（在 `tco_rewrite` 中）。`cs` 从不重新赋值 → 循环是已证明可通过证书检查的标量形式；无堆回边合并，无 `verify_ownership` 更改。`try_list_iter_rewrite`（lower/mod.rs）在 `tco_collect`（遇到 `match` 函数体会退出）之前运行。已验证：`spec/wasm_cross/list_iter_tco.almd` 与 v0 字节一致（bin_rec 5/15/0/-1 直接分支 + 块分支 `cnt` 长度 5/0）；corpus-wall 接受（14548 个堆对象）+ cargo test + output-parity 67/67。
途中修复了 2 个错误：
- `max_var_id` 跳过了模式绑定变量（`some(ch)`），因此合成的 `rk`/`idx` 与 `ch` 冲突 → 渲染器将一个局部变量同时用于 i32 句柄和 i64 标志，导致无效 wasm。现在统计模式绑定（也加固了已有的标量 TCO）。
- （提交 1601af5e）位于 BinOp 操作数位置的标量 `??`（`(int.parse(s) ?? 0) - 48`、`(codepoint(ch) ?? 0) - 48`）经过 `lower_scalar_value` 落入 `Const 0`，因此整个 BinOp 静默读作 0。这是一个普遍存在的既有静默误编译（起初误诊为“codepoint 借用”缺口——单独的 codepoint(cs[i]) 没问题；错误在 `?? · - 48`）。在 `lower_scalar_value_inner` 中增加了标量 `??` 分支（`try_lower_option_unwrap_or`，以标量回退值为准入条件）。由此 oct_rec/bin_rec 现在端到端与 v0 字节一致（oct 15/511/.../83、bin 5/15）——`spec/wasm_cross/unwrap_or_operand.almd` + oct/bin 探针。因此 oct_rec 完全正确（lowering + 渲染），而不只是清除了障碍；仅在完整 yaml 程序内渲染时仍需 float.parse。
下一步：追加累加器 TCO 变体（collect_seq/flow_rec 的 `acc + [x]` → 原地 push）+ 相互递归（flow_rec↔flow_step），然后是 value.object/stringify + 元组堆（block_*）。

<a id="historical-yaml-wall-countdown-june-2026-session-tally"></a>

## 历史 yaml 障碍倒计时（2026 年 6 月会话统计）

74 个函数 → 障碍：22（会话开始）→ 21（识别 float.parse）→ **19**（标量实参 TCO，提交 77c91648）→ **17**（嵌套堆结果 `match` 分支，提交 5510dc47）→ **16**（字符串结果堆载荷绑定 = `emit`，提交 fab14729）→ **15**（let 绑定变体调用的读取形状初始化 = `num_signed_base`，见下文）。剩余 15 个：oct_rec、bin_rec（堆循环携带值 + match 叶子）、flow_rec、flow_step（相互递归 + 堆累加器）、block_scalar/block_line/block_nonblank（元组堆 + let 绑定堆 if）、parse_lines、map_entry、parse_nested、collect_map、collect_seq、seq_item、emit_non_int、is_compound（Value 数组模型：value.array/as_array/object/stringify + 递归释放）。

<a id="landed-2026-06-19-let-bound-variant-call-read-shape-seed--yaml-1615-num_signed_base"></a>

### 已落地 2026-06-19：let 绑定变体调用的读取形状初始化 — yaml 16→15（num_signed_base）

`num_signed_base` 的 `let parsed = if kind=="o" then parse_oct(d) else parse_bin(d); match parsed {…}` 受阻，尽管 (a) let 绑定堆 `if` 的尾部复制（`desugar_let_bound_heap_branch`）将 `match` 分配到每个分支，且 (b) 嵌套 `match` 分支构建块可对 match 分支做 lowering。剩余缺口：let 绑定的用户函数 Result/Option 变量没有初始化其变体读取形状，因此 `match parsed` 看到未跟踪的被匹配值而受阻——尽管直接调用实参位置（`lower_call_args` 的 Named 分支，calls.rs:1075）已通过 `seed_variant_param` 初始化。修复：在 `lower_bind` 中对返回变体的 Named 用户调用也执行 `seed_variant_param(dst, ty)`——与直接路径完全镜像。`seed_variant_param` 只增加布局/读取知识（绑定的 `dst` 已经是在作用域结束时释放的拥有所有权的堆值），因此没有所有权/证书更改。已验证：`spec/wasm_cross/letbound_variant_match.almd` 与 v0 字节一致（既有简单的 `let p = mk(b); match p`，也有完整的 `let parsed = if … ; match parsed` num_signed_base 形状）；corpus-wall 接受；output-parity + cargo test 通过。（block_scalar 同样有 let 绑定堆 `if`，但还返回 `(Value, Int)` 堆元组——元组堆构建块——因此仍受阻。）

<a id="landed-2026-06-19-str-result-heap-payload-bind--yaml-1716-emit"></a>

### 已落地 2026-06-19：字符串结果堆载荷绑定 — yaml 17→16（emit）

字符串结果的 Camp-4 边界（来自 `value.as_string` 的 `Result[String, String]`——@12 的 slot-0 拥有唯一的 String，Ok/Err 标签位于 @16，通过 `(i32.load)` 读取 = 仅低 32 位句柄）。`emit`（`match value.as_string(v) { ok(s) => emit_scalar(s), err(_) => emit_non_string(v, ind) }`）绑定 Ok String 并返回堆结果——这是 `try_lower_variant_value_match` 在 936 处拒绝的情况（先于分支释放被匹配值的脱糖无法借用已释放的被匹配值）。解决方案（通过证书检查，无检查器更改）：对于字符串结果堆绑定，将载荷绑定为借用（@12 的 `LoadHandle`，在 `param_values` 中），将拥有所有权的被匹配值的释放推迟到分支汇合之后（使借用在两个分支内都存活），并依赖裸 Var 分支的自动获取（`Op::Dup`）——这样，无论分支借用载荷（调用实参）还是返回它，后置释放都恰好释放 slot-0 一次。也将 Ok 分支绑定从 `scalar_bind` 扩展为 `heap_or_scalar_bind`（以 `heap_elem_lists` 自行把关，因此标量 Result 仍拒绝堆绑定——无回归）。非字符串的堆载荷（列表的堆 Result、Array 元素）尚无单槽借用表示 → 仍是真正的 Camp-4 边界（Value 数组模型）。已验证：`spec/wasm_cross/str_result_heap_bind.almd` 在借用载荷和返回载荷两种形状下均与 v0 字节一致（5/5）；corpus-wall 接受（4234 个函数、14392 个堆对象——所有权证书就是泄漏/重复释放检查，因此接受 ⟹ 后置释放无泄漏）；output-parity 62/62 + 新夹具；3 个非基线 MISMATCH 文件仍是既有问题。

<a id="landed-2026-06-19-nested-heap-result-match-arm--yaml-1917-try_decimal-parse_number"></a>

### 已落地 2026-06-19：嵌套堆结果 `match` 分支 — yaml 19→17（try_decimal、parse_number）

`lower_heap_result_arm`（control.rs）处理 Module 调用 / Named 调用 / 嵌套 `if` / 构造器分支，但对嵌套 `match` 分支落入 `_ => None`，使任何自身分支又是 `match` 的堆结果 `if`/`match` 都受阻——即 `try_decimal` 形状（`match int.parse(c) { ok(n) => value.int(n), err(_) => match
float.parse(c) {…} }`）和 `parse_number` 的 then 分支（`match int.from_hex(..) { ok(n) => value.int(n),
err(_) => value.str(raw) }`）。修复：新增 `Match` 分支情况，递归使用与尾位置相同的已证明机制——变体被匹配值使用 `try_lower_variant_value_match`（对标量载荷在分支之前释放被匹配值 + 堆结果 `if` 骨架），Int 字面量被匹配值使用 `desugar_match_to_if` + `lower_heap_result_if_inner`。递归已对每个叶子执行 `Consume` 并返回合并后的 if 结果 `dst`，因此该分支不额外增加 `Consume`（与嵌套 `If` 分支完全一样）。通过证书检查：组合两个已证明、内部平衡的 lowering——无新 MIR 操作，无检查器更改。已验证：针对性夹具（`spec/wasm_cross/nested_match_heap_arm.almd`，仅含可链接的 int.parse/int.from_hex）与 v0 字节一致（5/5 个用例）；corpus-wall 接受（4231 个函数、14370 个堆对象、3 个性质）；output-parity 基线 61/61（无回归）+ 纳入新夹具以保持不退步；3 个非基线 MISMATCH 文件是既有问题（经 A/B stash 测试确认——与此构建块无关）。注意：try_decimal/parse_number 现在可做 lowering（障碍消除），但使用它们的完整 yaml 程序仍需 float.parse（strtod）才能渲染——lowering 障碍与链接障碍是独立计数。

<a id="why-this-is-the-keystone"></a>

## 为什么这是关键环节

yaml 剩余的 22 个 v1 障碍主要围绕动态 `Value` 模型：`value.array`/`value.object` 构建它（collect_seq/collect_map），`value.as_array` 绑定数组载荷（yaml:249 `ok(items) => emit_seq(items, ind)`），`value.stringify` + yaml `emit` 序列化器读取它。v1 自托管标量 Value（value_core.almd：Null/Bool/Int/Float 标签 0-3、Str 标签 4），但不自托管复合 Value（Array 标签 5、Object 标签 6）。阻碍是递归释放：`List[Value]`/Array 的元素本身也是拥有堆载荷的 Value，因此已有的 `DropListStr`（平坦的逐槽 `rc_dec`）会泄漏它们。

<a id="the-pieces-a-coupled-unit--observable-only-end-to-end-via-stringify-so-build--byte-verify-together"></a>

## 各构建块（耦合单元——只能通过 stringify 端到端观察，因此一起构建并验证字节）

1. **`List[Value]` 实体化**——扩展 `try_lower_str_list_literal`（binds.rs:103），像元组/ResultOk Call 路径一样接纳 Call 元素（`[value.int(1), value.int(2)]`）；并将列表标记为 Value 元素列表（新集合 `value_elem_lists`），使其释放采用递归的 Value 感知版本，而非平坦的 `DropListStr`（后者会泄漏 str/array/object 元素）。调用实参位置也一样（calls.rs，镜像 f89cbfca 的 List[String] 构建块）。

2. **递归 `$__drop_value` 受信任运行时例程**（preamble，render_wasm.rs ~1040——唯一增加的信任，类似 `$rc_dec`/`$alloc`/内联 DropListStr）。在 rc==1（最后一个引用）时：
   - tag < 4（标量）：无嵌套内容 → `rc_dec(p)`。
   - tag == 4（Str）：`rc_dec(payload@12)`（释放 String）→ `rc_dec(p)`。（等同于今天的内联 DropValue。）
   - tag == 5（Array）：payload@12 = 一个 `List[Value]`。`for i in 0..len(list): $__drop_value(elem_addr(list,i))`；然后 `rc_dec(list)`；然后 `rc_dec(p)`。
   - tag == 6（Object）：payload = 键值存储（匹配 v0 的 Object 布局——`Vec<(String, Value)>`）。`for each pair: rc_dec(key String); $__drop_value(value)`；然后 `rc_dec(store)`；然后 `rc_dec(p)`。
   证书看到一个 `d`（一个 `Op::DropValue`，不透明）。递归是受信任例程——由 rt-oracle-registry 差分测试验证（一个 `value.stringify` 往返夹具证明与 v0 相比无泄漏/无重复释放）。`value_elem_lists` 列表通过同级 `$__drop_list_value` 释放（对每个元素循环调用 `$__drop_value` + 释放列表）。

3. **`value.array`/`value.object` 自托管**（value_core.almd）：`alloc_value`、`store32(h+4, 5/6)`、`store64(h+12, <list/store handle>)`——将 items 移入（v0 执行 `Array(items.clone())`，因此是深拷贝：自托管要么复制 items，要么获取所有权；通过 stringify 往返匹配 v0 字节）。在 crates/almide-types/src/self_host_registry.rs（`self_host_runtime()`）中注册。

4. **`value.as_array`/`value.as_object`**——读取标签；tag 5/6 → `Ok(payload list/store)`（绑定堆载荷——yaml:249），否则 `Err`。一个堆 Ok Result（cap-as-tag，类似 value.as_string——复用提交 7b24ef8f 的字符串结果机制）。

5. **`value.stringify`** 自托管——递归：标量 → “null”/“true”/n/float.to_string；Str → `"\"" + escape(s) + "\""`（转义 `\\ " \n \r \t`，与 v0 value.rs:228 完全一致）；Array → `"[" + (items |> list.map(stringify) |> list.join(",")) + "]"`；Object → `"{" + (pairs |> list.map((k,v) => "\"" + escape(k) + "\":" + stringify(v)) |> join(",")) + "}"`。自调用位于 `list.map` 内（由 C1 去函数化，非尾调用），因此自递归守卫不适用——应能做 lowering。float 通过自托管的 float.to_string（#63）。验证：`value.stringify(value.array([value.int(1), value.str("a")]))` 与 v0 字节一致（`[1,"a"]`）。

<a id="41-compliant-recursive-drop--the-design-breakthrough--its-soundness-boundary-2026-06-19"></a>

## 符合 §4.1 的递归释放 — 设计突破及其健全性边界（2026-06-19）

`handwritten_wasm_runtime_does_not_grow` 门禁（§4.1）禁止手写 WAT `$__drop_value`（我试过——已回退）。符合 §4.1 的路径：在 Almide 中自托管 `__drop_value`，操作原始句柄（Int），与 `string_eq`/`__streq_at` 完全一样——`fn __drop_value(v: Int) -> Unit = { let tag
= prim.load32(v + 4); if tag == 5 { <loop: __drop_value(prim.load_handle(elem_addr)) > over the
List[Value]>; prim.rc_dec(list) } else if tag >= 4 { prim.rc_dec(payload) }; prim.rc_dec(v) }`。
由于它操作原始 Int（无堆类型的值），其所有权证书为空（无 i/a/d/m）——像 string_eq 一样通过证书检查。`Op::DropValue` 的渲染变为对它的 `CallFn`；该 Op 保留证书 `d`（因此 Value 的分配仍平衡）。需要一个新 prim：`prim.rc_dec(addr: Int) -> Unit` → 发出 `(call $rc_dec …)` 的 `PrimKind::RcDec`——复用已有 `$rc_dec`，无新 WAT 函数，符合 §4.1。

**健全性边界（问题所在——这属于任务 #30“runtime Almide化”的领域，对健全性至关重要）：**
`verify_ownership` 将 `Op::Prim` 视为无操作（lib.rs:705）。因此 `prim.rc_dec` 是未跟踪的释放——使用它的自托管函数可以重复释放而不被检查器发现（证书漏洞）。这与 `DropListStr` 内联逐元素 rc_dec 的信任级别相同（未跟踪、以 rc==1 把关、受信任），但向所有自托管代码暴露 `prim.rc_dec` 会扩大受信任范围：任何 .almd 函数都可能误用它。遏制选项（设计优先，不要仓促）：(a) 将 `prim.rc_dec` 限制到白名单（只有 `value_core.__drop_value` 可调用），像纯度注册表一样强制执行；(b) 专用 `Op::DropValueRecursive`，lowering 仅为释放例程发出它；(c) 单独证明释放例程全终止/安全。这是 Value 释放自托管中对健全性至关重要的核心——适用 CEO 的“设计优先、不要仓促”纪律。突破是真实的（原始句柄自托管绕过了默认借用/消费约定问题）；rc_dec 暴露是需要把关的步骤。

<a id="-implemented--empirically-validated-2026-06-19-prim-infra-committed-self-host-staged-with-4-confirmed-blockers"></a>

## ★ 已实现并经实证验证 2026-06-19（prim 基础设施已提交；自托管暂存，有 4 个已确认阻碍）

已提交（83176420）：`prim.rc_dec`/`prim.rc_inc`（PrimKind::RcDec/RcInc，复用已证明的 `$rc_dec`/`$rc_inc`，无新 WAT 函数 = 符合 §4.1），在 lower_prim_call 中将 `fn_name ∈ {__drop_value, __varr_copy}` 列入白名单（遏制未跟踪释放——Prim 对证书是无操作，因此不受限的 rc_dec 会让任何函数在不被发现的情况下重复释放）。白名单 + `prim.load_handle`（b50853ef）是基础。

下方自托管代码已编写、构建，其递归释放在 render_program 中正确运行（`let a = value.int(5); match value.as_int(a) {…}` → “5”，`$__drop_value` 已链接并被调用，见所发出 wat 的第 758 行）。它处于暂存状态（已从 value_core.almd 回退，以保持源码树测试通过），等待解决实现中发现的 4 个实证阻碍：

1. **value.stringify 被 TCO 阻挡，且会使 value_core 整体出问题。** 其数组情况 `__vstr_arr(v,i,n,acc) = if i>=n
   then acc else __vstr_arr(v,i+1,n, acc+sep+value_stringify(e))` 是堆结果（String）自递归 → 命中自递归守卫 → 无法 lowering → 出现未链接的 `$__vstr_arr` → 又因为调用任意 value.* 时 value_core 都自动链接其整个源码，所有 Value 程序都会受阻。修复：将 stringify 重构为 Unit 提取至 List[Value]（Unit 自递归不受守卫限制）+ `list.fold(",", value_stringify)`（fold 经 C1 去函数化，因此 value_stringify 的递归是间接的，不受守卫限制）；或先落地 TCO。
2. **测试框架与生产链接分歧。** render_program（corpus-wall/output-parity）自动链接 `__drop_value`（随 value_core）；单元测试的 `lower_source`（tests_part1.rs:195，经 `lower_function_all_with_types`）没有 → json 单元测试报 `unknown func $__drop_value`，而生产运行正常。在接入 DropValue→__drop_value 之前，使 lower_source 的自动链接与 render_program 一致。
3. **首先需要 List[Value] 实体化。** `value.array([value.int(1),…])` 的实参受阻：`try_lower_str_list_literal`（binds.rs:103）接纳 LitStr/Var/Record/Tuple/ConcatStr，但不接纳 Call 元素。增加 Call 元素分支（通过 CallFn 实体化每个元素，Consume 到槽内），以 `List[Value]` 为准入条件。
4. **已有门禁无法验证释放（健全性关键）——但泄漏测试可行。** 错误的 tag-5 释放 = 静默泄漏：输出仍正确（泄漏发生在打印之后），证书将 DropValue 视为一个平衡的 `d`——因此 output-parity 和 corpus-wall 都无法发现。**已解决 2026-06-19：分配器有空闲链表（`$freelist`：`$rc_dec` 在 rc 为 0 时将块压入，`$alloc` 复用同大小块），因此泄漏测试可行——`while i < 100000 { let xs = [..]; … }` 创建+释放循环在正确释放时完成（空闲链表使内存保持在第 1 页），泄漏时触发 trap（存储越过 64KB 越界）。已证明：100000 次 List[String] 创建+释放循环运行至“done” → DropListStr 无泄漏。** 因此 Value 释放可以受信任：泄漏测试 `while i<100000 { let a = value.array([value.int(i)…]); … }`。不能用自递归驱动该循环（无 TCO → 在约 100k 深度栈溢出）；必须使用 `while` 循环。

可用的自托管代码（value_core.almd，待 1-3 消除后可重新粘贴；自包含 tag-5 `[rc][tag=5 @4][len @8][elem@12…]`，通过 rc_inc 浅拷贝，原始句柄递归释放 = 空证书）：
```almide
fn __varr_copy(src: Int, dst: Int, i: Int, n: Int) -> Unit =
  if i >= n then () else { let e = prim.load64(src + 12 + i * 8); prim.rc_inc(e); prim.store64(dst + 12 + i * 8, e); __varr_copy(src, dst, i + 1, n) }
fn value_array(items: List[Value]) -> Value = {
  let ih = prim.handle(items); let n = prim.load32(ih + 4); let v = prim.alloc_value(n); let vh = prim.handle(v)
  prim.store32(vh + 4, 5); prim.store32(vh + 8, n); __varr_copy(ih, vh, 0, n); v }
fn __vdrop_arr(v: Value, i: Int, n: Int) -> Unit =
  if i >= n then () else { let h = prim.handle(v); let e: Value = prim.load_handle(h + 12 + i * 8); __drop_value(e); __vdrop_arr(v, i + 1, n) }
fn __drop_value(v: Value) -> Unit = {
  let h = prim.handle(v)
  if prim.load32(h + 0) == 1 then { let tag = prim.load32(h + 4); if tag == 5 then __vdrop_arr(v, 0, prim.load32(h + 8)) else if tag == 4 then prim.rc_dec(prim.load64(h + 12)) else () } else ()
  prim.rc_dec(h) }
```
渲染接线（已回退）：`Op::DropValue { v } => format!("    (call $__drop_value (local.get {}))\n", local(*v))` + 注册 `("value_array","value.array")`，以及（stringify 重构后）`("value_stringify","value.stringify")`。
已确认的 Almide 注意事项：Unit 字面量是 `()`，不是 `unit`。

<a id="-full-build-attempt-2026-06-19--built-end-to-end-the-one-remaining-gate-is-the-layout-brick-as_array-usage"></a>

## ★★ 完整构建尝试 2026-06-19 — 已端到端构建；唯一剩余门禁是布局构建块（as_array 的使用）

本会话推进了整个耦合批次，通过构建+运行确认各部分，然后重新暂存（无法端到端验证 → 按 ② 不交付）。可用的部分（已验证）：
- `prim.rc_dec`/`rc_inc` + `prim.load_handle`（已提交）+ 白名单（`__drop_value`/`__varr_copy`/`__vfill`）。
- **List[Value] 实体化**——`[value.int(1), …]` 接纳 Call 元素（Module value.* 经 lower_pure_module_value_call，Named 经 CallFn），Consume 到嵌套所有权列表中。已构建（此前受阻，现在可渲染）。
- **value.array**（自包含 tag-5，经 `__varr_copy`+rc_inc 浅拷贝）——检查所发出的 wat 可见其正确（分配 n 个槽，存储 tag@4=5 / len@8=n，逐元素 rc_inc+存储）。
- **__drop_value**（递归原始句柄，Unit 自递归辅助函数 `__vdrop_arr`——不受守卫限制）——此前已在生产中运行。
- **value.as_array**（经 `__vfill`+rc_inc 提取到新的 List[Value]；`prim.alloc_list_str` 是泛型 `[A]`，因此类型为 `List[Value]` 的 `alloc_list_str` 不需要新 prim）——构造正确。
- `Almide gotchas CONFIRMED: Unit literal is `()`; `value.at` is NOT a v0 fn (frontend rejects) — use the real v0 `value.as_array`; the allocator HAS a `$freelist` so the leak-loop gate works.`

上条因原文反引号嵌套形式而原样保留，其全文含义是：已确认的 Almide 注意事项——Unit 字面量是空括号；value.at 不是 v0 函数（前端会拒绝），应使用真正的 v0 value.as_array；分配器具有 $freelist，因此泄漏循环门禁可行。

唯一剩余门禁——**布局构建块（精确到载荷的堆绑定）**——阻挡 as_array 的使用：`match value.as_array(a) { Ok(items) => items[i] / list.len(items) }` 错误绑定。两个耦合原因：(1) `bind_pattern`（control.rs ~150）通过 Dup 整个被匹配值来绑定堆载荷（容器粒度——其注释本身说“精确到载荷的身份需要布局构建块”），因此 `items` = Result 外壳，而非内部列表。(2) 未跟踪列表的堆 Result：`materialized_results_str` 只标记 `Result[String,_]`（cap@16 字符串结果机制，7b24ef8f）；`Result[List[Value],String]` 未被识别，因此 match 线性化/误读。证据：`value.array([7,8,9])` 后 `Ok(items)=>list.len(items)` 打印 1 而非 3（读取外壳，而非列表）。所以下一个构建块是布局构建块：精确到载荷的堆载荷绑定 + 列表堆 Result 跟踪。然后重新粘贴暂存的 value.array/as_array/__drop_value，注册它们，接入 Op::DropValue→$__drop_value，以往返（`value.as_int(items[i])`）+ 泄漏循环验证。yaml 的 emit/collect 随之解阻。

<a id="-the-final-gate-pinned-precisely-2026-06-19-the-camp-4-frontier-heap-payload-bind-over-a-heap-result"></a>

## ★★★ 最终门禁已精确定位 2026-06-19：Camp-4 边界（对堆 Result 的堆载荷绑定）

as_array 使用所需的“布局构建块”，准确说就是 `try_lower_result_match`（control.rs ~916-939）中的 **Camp-4 边界**。`match value.as_array(a) { Ok(items) => … }` 中的 `items: List[Value]` 是对堆 Result 的堆载荷绑定。有两道硬性阻断：
- `Ok(inner)` 使用 `scalar_bind`（第 919 行），仅接纳标量/通配符——堆 `items` → 回滚。
- 即使经过 `heap_or_scalar_bind`，在 `heap_res` 时第 936 行也会回滚任何堆载荷绑定（“Camp-4 frontier: defer”）。
根因（真正的证书设计问题，而非一行修复）：在 cap-as-tag 表示（value.as_string，7b24ef8f）中，Ok 载荷与被匹配值互为别名（同一句柄）。堆 Result match 在分支之前释放被匹配值（第 965 行），以便将分支变为干净的标量条件 `if`——但这样载荷借用（`items` = LoadHandle@12 = 被匹配值）就悬空了。替代方案（将载荷移动到各分支的绑定中，由分支拥有并释放）会让以平坦方式处理 if 分支的 `verify_ownership` 看到同一个被匹配值被释放两次 = 误报重复释放（检查器未建模 then/else 互斥）。因此，今天借用和移动都无法通过证书检查。
真正的修复（二选一）：(a) 让 `verify_ownership` 理解 then/else 互斥（`then` 和 `else` 中对同一对象的释放，净效果是一次释放）——原则性的修复，普遍解除堆载荷绑定的阻碍；或 (b) 不共享的 Result 表示（单独的 Ok 外壳 `[rc][tag][_][payload@12=list]`，使载荷是分支移出的独立拥有所有权对象，而外壳释放只释放外壳）。(a) 长期更优（也解除 Err 堆绑定 + Some 堆绑定的延期）。这对 ② 至关重要（错误选择 = UAF 或重复释放），因此必须设计优先，而不是在会话结束时仓促处理。Value 模型的其他所有部分都已构建并验证（见上方完整构建尝试）；这就是唯一门禁。

<a id="camp-4-depth-fully-investigated-2026-06-19-a-lowering-only-path-exists-but-needs-a-new-heap-result-of-list-rep"></a>

## Camp-4 深入调查完成 2026-06-19：存在仅修改 lowering 的路径，但需要新的列表堆 Result 表示

好消息（无需内核/Coq 更改）：证书已经为困难部分兜底。如果堆载荷 match 将 `items` 绑定为借用（对被匹配值载荷槽的 LoadHandle），并在分支**之后**释放被匹配值（而不是当前的之前），`verify_ownership` 就只看到一次被匹配值释放——通过平坦证书检查。而无别名安全性无需额外代价：返回借用载荷的分支（`ok(items) => items`）是“返回不拥有所有权的值”，证书已经拒绝它（lib.rs:725），因此只有使用而不移动的分支（yaml 的 `ok(items) => emit_seq(items, ind)`）能通过。所以 Camp-4 是对 `try_lower_result_match` 的 lowering 修改（借用载荷 + 后置被匹配值释放），不是检查器/Coq 修改。（我之前过于保守。）

真正剩余工作——新的列表堆 Result 表示：`value.as_array : Result[List[Value],
String]` 不能复用字符串结果 cap@16 表示（value.as_string，is_self_host_result_str_module_fn）。cap-as-tag 将 Ok/Err 标签写在载荷的 @16——对 String 没问题（那是它的字节区域），但对 `List[Value]` 而言 @16 是 `elem0` 的高 32 位（槽位于 @12、@20、…）→ 标签会损坏元素 0。因此列表载荷 Result 需要单独的 Ok 外壳表示 `[rc][tag@4][_][payload@12=list]`（match 读取 tag@4，将 `items` 绑定为 LoadHandle@12）——不同于字符串结果 cap@16 机制。计划：(1) 一个 `value` 堆结果表示 + 跟踪集合（独立外壳，而非 cap@16）；(2) 在 try_lower_result_match 中接纳 Ok(heap) 绑定（Ok 的 scalar_bind→heap_or_scalar_bind），仅对此表示采用借用 + 后置被匹配值释放（不要扰动已证明的字符串结果先释放路径）；(3) 验证往返（`value.as_int(items[i])`）+ corpus-wall + 无字符串结果回归。这是聚焦的表示+lowering 构建块（非内核/Coq）——证书会拒绝误用，因此可验证，只是需要谨慎（错误表示属于静默损坏类，corpus-wall 的证书可能无法发现——需要往返 + 泄漏门禁）。

<a id="prim-floor-layer-found-2026-06-19-the-foundation-below-piece-1"></a>

## 发现 PRIM 基础层 2026-06-19（构建块 1 下方的基础）

prim 基础层（load_str/store_str/load64/store64/handle）没有带类型的 `load_list`，无法从 Value 的 @12 槽读回 `List[Value]` 载荷（load_str 硬编码为返回 String）。因此，`value.array` 存储 `List[Value]` 载荷，以及 `value.as_array`/`stringify` 读回它，首先需要扩充 prim 基础层：一个 `prim.load_list`（返回 `List[Value]` 的带类型 `LoadHandle`，是 `load_str` 的列表同级项），接入 `lower_prim_call`（calls.rs:1968 将 load_str 映射为 LoadHandle）+ prim 标准库类型签名。（另一方案：将 Value 数组表示为自包含的 tag-5 块，内联存放元素 Value 句柄——无独立 List 载荷，无 load_list——但这样 value.array 必须将 items 的元素复制进来，stringify/drop 则迭代内联槽；应与 load_list 方案权衡。）
从基础向上的层次：`prim.load_list` → 递归 `__drop_value` → List[Value] 实体化 → value.array/object/as_array/stringify 自托管 → 注册表。每层都是在实现下层时显现出来的（本会话实证确认）。

<a id="gates-per-brick-the-proven-methodology--9-bricks-this-session"></a>

## 门禁（逐构建块，已验证的方法——本会话 9 个构建块）

记录的门禁是 corpus-wall 接受、Value 模型字节一致探针（构建 + stringify 对比 v0）、cargo 测试和 output-parity 基线。证书接受确立所建模的所有权事件性质；它不检查受信任递归释放例程内部的每项操作。泄漏循环或差分探针可以揭示运行时错误，但有限测试通过并不能证明所有错误释放都会被拒绝。解释这些结果时，应保留前文关于空证书/受信任例程的限定。后续的 TCO/float.parse 工作属于历史 yaml 计划。

<a id="honest-scale"></a>

## 如实说明规模

这是一个耦合单元（约 5 个部分，递归运行时释放 + 序列化器），确实需要聚焦推进多个构建块，而非单轮任务——但每部分都无需 Coq 且可验证字节，递归释放是受信任基础唯一的新增项（一个例程，类似 DropListStr）。路径 A 结合了经检查的所有权事件、经实证测试的操作以及受信任递归释放边界；仅仅通过这些门禁，并不能证明整个 Value 实现正确。
