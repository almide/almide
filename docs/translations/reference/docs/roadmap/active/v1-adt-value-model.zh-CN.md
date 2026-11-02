原文说明元数据：将自定义 ADT（variant）作为 v1 值模型中的一等值
<a id="v1-①--custom-adt-variant-as-a-first-class-value"></a>

# v1 ① — 将自定义 ADT（variant）作为一等值

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-adt-value-model.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-adt-value-model.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-adt-value-model.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-adt-value-model.md)

> 原文注记：这是逐步实现记录，不是当前的普遍保证。早期构造器不受支持的诊断、单 arm 堆结果障碍与后来的完成/所有位置表述并存，按历史层次保留，不暗中统一。生成的递归 drop 例程受信任，ownership 证书为空，其递归正确性门禁依赖记录的泄漏循环。语料见证被接受和例子字节匹配不证明所有误编译、双重释放或泄漏均不存在。计数、commit 和组织仓库观察均为原文主张，未独立重跑。

<a id="original-diagnosis-before-the-completed-bricks-recorded-below"></a>

## 最初诊断（在下述已完成步骤之前）

最初的跨仓库障碍突破项 #1（约41处障碍：对 `type Expr = Lit(Int) | Add(Expr,Expr) | …` 进行递归 `to_string` / pretty-print）暴露了自定义 variant 渲染的缺失。这是实现前的诊断，不表示目前缺乏 ADT 支持；后面的 STATUS 节记录了步骤1–5已完成。

<a id="evidence-byte-match-first-tmpadtalmd-tmpadt3almd"></a>

## 证据（字节匹配优先，/tmp/adt.almd、/tmp/adt3.almd）

- `classify(e) = match e { Lit(n) => n, Add(_,_) => 100, Neg(_) => 200 }`（非递归、标量）v0 = 7/200，v1 = **WALL** `unlinked stdlib/runtime call(s): Lit, Neg` — **构造器没有 wasm 实现**。
- `eval`（标量 match）被 classify 计为 `in-profile`，但 render 因同样的未链接 ctor 遇到 WALL。因此 **classify 夸大了 ADT 支持**，render（字节匹配）才是实情。（这也意味着，对任何使用 ADT 的仓库，org-trust-status 的 `in-profile` 都偏乐观。）
- MIR 中的根源：`build_record_layouts`（lower/mod.rs:142）**跳过 variant 声明**（“variant / alias 声明不携带扁平 record layout”）。MIR lowering 中没有 `IrPattern::Constructor` 标签分派；现有 `try_lower_variant_*` 仅适用于 Option/Result。

<a id="target-repr--v1s-own-uniform-slot-block-only-the-output-byte-matches-v0"></a>

## 目标表示 — v1 自己的统一槽位块（只有输出与 v0 字节匹配）

纠正（构建步骤1时验证）：v1 **不**复制 v0 的内部字节；只有可观察输出（打印到 stdout 的 `classify` / `to_string` 结果）必须与 v0 字节匹配。v0 wasm（emit_wasm：`variant_alloc_size`、`collections.rs::emit_record`、通过 `i32_store(0)` 将标签置于 @0，字段从 @4 开始放入按类型大小紧密排列的槽位，填充至 `4 + max payload`）按字节紧密排列。但 v1 record 已使用不同的内部模型 — 每字段统一 i64 槽位（`layout::slot_offset`、`Store{width:8}`、块 `[rc][len][cap]` + 槽位），并从写入的同一槽位读回，所以无需匹配 v0 的内部字节也能匹配输出。variant 加入的是这个模型：

- variant 值是类似 record 的块，**槽位0 = 标签**，**槽位1.. = 当前 ctor 的字段**（每个统一为 i64，使用与 record 相同的 `slot_offset` 机制）；
- `slot_count = 1 + max arity over all ctors`，所以该类型的每个 ctor 都具有同一块大小（统一分配 + 健全的整块 `==`）— 这是 v1 对应于 v0 最大载荷填充的做法，不是其字节副本。

标签 = 声明索引，元组字段命名为 `_0.._n`（与 v0 注册一致），所以即使字节布局不同，两个后端的标签与字段身份仍一致。

<a id="brick-sequence-each-ends-in-a-v0v1-byte-match-protected-by-proofsdiff-fuzzsh"></a>

## 步骤顺序（每步以 v0==v1 字节匹配结束，由 `proofs/diff-fuzz.sh` 保护）

1. **Variant layout registry** — ✅ 完成（commit `c81b0878`）。`build_variant_layouts` 构建 `VariantLayouts` 注册表（类型 → `VariantLayout { generics, tag-indexed cases, slot_count }` + ctor名 → 类型的反向索引 + `lookup_ctor`），通过新的 `lower_function_all_with_layouts` 传入 lowering（仅处理 record 的 `_with_types` 用空 variant 注册表委托；render_program + classify_corpus 构建并传入它）。仅是基础设施 — 尚无消费者，因此**输出变化为零**（diff-fuzz mismatch=0，完整 mir 测试套件不变）。单元测试锁定标签顺序、`_N` 元组合成、具名 record 字段、`slot_count = 1 + widest arity`、`lookup_ctor`。（已确认缺口：ctor 是 `IrExprKind::Call { target: Named(ctor) }` → 当前作为“unlinked stdlib/runtime call”遇到 WALL；oracle 的 `classify(Lit/Add/Neg)` v0 = 7/100/200。）

   首个字节匹配切片（步骤2+3，仅标量 — 与步骤5的递归 drop 隔离，类似 #51/#54 从标量开始）：
   ```
   type Tok = Num(Int) | Sym(Int) | Eof
   fn val(t: Tok) -> Int = match t { Num(n) => n, Sym(s) => s * 10, Eof => -1 }
   ```
   无堆字段 ⇒ 无递归 drop ⇒ 步骤2+3可单独验证。递归 / 堆字段 `Expr`（#1 的 to_string 突破项）需要步骤5，随后处理。

2. **Ctor 构建（标量字段先行）** — ✅ 完成（commit `62a4a862`）。`try_lower_variant_ctor`（binds.rs）拦截名字在 `variant_layouts.ctor_to_type` 中的 `Call{Named(ctor)}`：`Alloc` 一个 `slot_count` 宽的块，将标签 `Store` 到槽位0，把各参数 lower 到槽位 `1+i`（通过 `try_lower_scalar_record_construct` 槽位机制做标量复制）。已接入调用参数路径（`val(Num(7))`、calls.rs）和 LET 绑定路径（`let t = Num(9)`、binds.rs）。堆 / 递归 ctor 字段会遇到障碍（ADT 步骤5），绝不生成字节错误的块。
3. **标签分派 match（标量结果）** — ✅ 完成（commit `62a4a862`）。`try_lower_custom_variant_match`（control.rs）：实化 / 借用被匹配对象 → `handle → tag@slot0`，发出右嵌套的 `if tag==t_i { bind scalar fields from slots 1+i; arm } else …` 链（最后一个 arm / 任意 wildcard 是无条件 else — 穷尽性保证这一点）。接入尾部、let 绑定和标量操作数 match 位置（不受仅用于 Option/Result 的 `is_variant_ty` 限制）。这是 `try_lower_variant_value_match` 向 N 构造器的推广。在乱序 arm、wildcard、多字段 ctor 和 bind/let 位置上与 v0 字节匹配；corpus-wall 的已证明 checker 接受所有见证；新增 diff-fuzz 生成 variant 模板 + 确定性 wasmtime cargo test 保护它。仅标量结果 + 标量 ctor 字段绑定（堆结果 arm = 步骤4，堆 / 嵌套 ctor 字段 = 步骤5）。
4. **堆结果 match（借用对象）** — ✅ 完成（commit `f37a7314`）。对借用参数 / 变量对象进行、具有堆结果的 `match t { Ctor(..) =>
   <String> }` 在 tag@slot0 上分派，并通过 `lower_heap_result_arm` lower 各 arm（arm 移出新的堆值；绑定的标量字段读取仍存活的借用对象槽位）。`emit_variant_arm_chain` 接收 `result_ty`，选择标量或堆 arm lowering；接入堆结果尾部。具有堆结果的所有权临时对象会遇到 WALL（需要在 arm 之前 drop 对象，步骤4b），而不是发出证书失败的 MIR。corpus-wall ownership 15904→16046（现在能 lower 堆结果 variant match，已证明 checker 全部接受）。
5. **堆 / 递归 ctor 字段 + 递归 drop** — 剩余前沿（解锁递归 `Expr` to_string，即 #1 突破项）。分成3个子步骤，每步均以 corpus-wall ACCEPT 为准入条件：

   - **5a — 叶 String 构建** ✅ 完成（commit `f2c32c90`）。`try_lower_variant_ctor` 将 `String` ctor 字段移入其槽位（cert `m`）并追踪 `record_masks`，因此块在作用域结束时的 drop，通过与 String 字段 record 相同的带掩码 DropListStr 释放它。corpus-wall ACCEPT（ownership 16046→16085）；wasmtime 上1000×构建+drop 泄漏循环 cargo test。（仅构建：字段仍通过 WILDCARD 匹配 — 堆字段绑定是5c。）
   - **5c — 堆字段 match 绑定** ✅ 完成（commit `b50e95a3`）。绑定叶 String ctor 字段的多 arm match 可被 lower：**借用**槽位句柄（`LoadHandle`+`param_values`）— 移出 arm 在 `lower_heap_result_arm` 自动 `Dup`，消耗式重用在 `lower_owned_heap_field` 中 `Dup`，因此拥有该槽位的对象绝不在 rc 0 时被释放。**单 arm 堆结果 match**（1 ctor newtype `unbox`、`B(x)=>x`）被 WALL：没有 IfThen 分支合并 `dst`，arm 值直接 return，与 arm 的 `Consume` 发生双重 move（`amm`/`aamdm`，净值 −1 — checker REJECT）。证书驱动诊断：精确复现例是 `spec/wasm_cross/generic_fn_in_inferred_lambda.almd` 中的 `unbox[String]`；`Op::Consume`→`m` 加上堆 `func.ret`→`m` 重复计数（certificate.rs）。corpus-wall ACCEPT（ownership 16085→16161）、1000×泄漏循环、wasmtime cargo test + diff-fuzz 堆字段模板。（`emit_cert_from_source
     <f> <fn> mir` 现在转储 MIR ops + cert — 这正是发现问题的调试辅助。）
   - **5b — 嵌套 variant ctor 字段 + 递归 drop** ✅ 完成（commit `c98486be`）— #1 突破项打通。`try_lower_variant_ctor` 递归构建嵌套 variant ctor 字段（`Add(Lit(1),
     Neg(Lit(2)))` — ctor 调用参数 → 递归，变量 → `Dup`）并移入它，追踪该块以供 `Op::DropVariant` 使用。match 堆字段绑定扩展到 variant 类型字段（递归 `tos(l)`/`tos(r)` 借用传递）。递归释放是按类型生成的 Almide 函数 `$__drop_<T>`（`generate_variant_drop_sources` — `$__drop_value` 形状：`prim.handle`/`load*`/`load_handle`/`rc_dec` + 自递归），通过 render_program + lower_source 中的两遍 `source_to_ir` 自动链接；它只使用 `prim`，所以 ownership cert 为空（受信任例程 — `rc_dec` 白名单允许 `__drop_*`）。`Op::DropVariant` 是一个受信任的 `d`（如同 `DropValue`）。验证：`tos(Add(Lit(1), Neg(Lit(2)))) == "(1 + -2)"` 与 v0 字节匹配 + 一棵深树；2000× build+tos+drop 泄漏循环无问题（递归唯一的正确性门禁）；corpus-wall ACCEPT（ownership 16161→16300，已证明 checker 接受语料中的递归 variant，双重释放 / 泄漏为零）；确定性 wasmtime cargo test + diff-fuzz 递归 variant 模板。

   长期教训（本轮再次验证）：堆字段 variant lowering 即使在 wasmtime 上字节匹配，也可能潜藏双重释放 — 只有 kernel 证明的 `[ownership]` checker 能捕获它（5c）；对受信任 prim 例程，则由泄漏循环捕获（5b）。借用绑定和 Dup 后的所有权绑定都曾被 REJECT，直到证书驱动诊断定位 *单 arm 直接 return* 的双重 move；修复是限制这种情况，而不是改绑定模型。绝不交付被 checker 拒绝的见证。

<a id="status-①-adt-value-model-complete-bricks-15"></a>

## 状态：① ADT 值模型完成（步骤1–5）

自定义 ADT 在 v1 信任主干中已是一等值：构建 + N arm 标签分派 match（标量 / 堆结果，所有位置，包括 Unit 语句）+ 标量 / String / 嵌套 variant ctor 字段 + 递归 drop。记录的例子与 v0 字节匹配，语料 ownership 见证被接受。这些有范围限制的测试 / 检查并不确立“所有可能的静默误编译或运行时双重释放均为零”，并由每 PR diff-fuzz + 确定性 cargo 测试守护。跨仓库障碍突破项 #1（对 `type Expr = Lit | Add | Neg` 递归 `to_string` / pretty-print）已打通。

<a id="org-repo-verification-the-authoritative-byte-match"></a>

### 组织仓库验证（具有权威性的字节匹配）

实际 `github.com/almide` 组织的 variant 类型现在在 v1 信任主干上实现 v0==v1 字节匹配（②门禁，不只是障碍计数）：`porta` 的 `InstanceState = Created | … | Exited(Int) | Failed(String)`（unit + 标量 + String 载荷 ctor）、`RestartPolicy`/`ValType`（enum）、`FdKind`（String 载荷）— 全部 `almide run`（native）== `render_program | wasmtime`，且 String 载荷 `FdKind` 的2000× ctor+match+drop 泄漏循环无问题。porta 的 variant ctor 不再因“unlinked call”遇到障碍。

注意（测量，而非正确性）：`scripts/org-trust-status.sh` 只扫描各仓库的入口模块，所以看不到子模块里的 ADT 代码（porta 入口是 barrel — subset 内函数0个）。因此 ADT 工作没有改变仪表盘总数；上面的字节匹配才是真正证据。组织范围内下一突破项（仪表盘排名首位的原因，约40处障碍）现在是**堆结果表达式系列**（在尾部或调用参数中返回的 `match`/`if`/`Range`/`??`/`Try`/`List`/`BinOp`）— 阻塞 toml/svg/aes/base64/csv，而不是 ADT。

<a id="why-this-is-①-value-model-unification-not-a-one-off"></a>

## 为什么这是①（值模型统一），而不是一次性补丁

record / tuple / enum / list 都是“带标签？ / 带掩码的堆块”。步骤1-5使 variant 成为加入这一模型的最后成员；完成后，各表面形状的步骤（List[(String,Value)] 手动 drop、Option-tuple-match、堆 accumulator 特例）都是同一种带掩码 / 标签块的构建+match+drop 实例 — 这是消除偶然复杂性的路径。始终字节匹配优先，每 PR 的 diff-fuzz 门禁（commit 33e4237f）在生成程序触及偏差时立即捕获它。
