说明：仅在 lowering 中使 heap-result match 从识别形状改为全域
<a id="camp-4-generalised-make-heap-result-match-total-instead-of-recognised"></a>

# Camp-4 的一般化：使 heap-result `match` 从识别形状改为全域

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](camp4-general-heap-match.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](camp4-general-heap-match.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](camp4-general-heap-match.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/camp4-general-heap-match.md)

> 原文注记：8月17日落地及有限403/403、7/10 corpus、下游运行是历史原文报告，不是此处重跑测试或普遍 match 支持。早期 wall 描述和未开始阶段保留为诊断历史；若开头落地结果已解决相应例子，以其为准。L2 仅在测试处成立。Map/Set typed twin 在记录中是独立未开始范围。RESEARCH.md 与外部参考 tree 是此 snapshot 不含的历史引用，不是独立审计证据。原文称四镜像位置，同时分组列 twin/drop/linkage 工作，保留该分组。未执行所提 gate 移除、删除、命令；比较全域性及删除估量为原主张，不是新验证。


状态：**计划 + 首个已落地阶段。** 2026-08-17 根据新发现的外部 wall（almide/dfa）和四个 RC 参考实现的源码阅读编写。

<a id="landed-2026-08-17-the-1492-arc--all-gates-green-403403-no-cert-change"></a>

## 2026-08-17 已落地（#1492 阶段）——全部门禁绿（403/403，无 cert 变更）

下方 L2 假设在测试范围成立：所有修复**仅在 lowering**，无 checker、kernel、Coq 改动，符合 L2 预测（`CBranch` 时代 checker 从不是 blocker，旧 FLAT 模型下写的 gate 才是）。

- **接受 Heap→heap Result payload 重新 wrap**（`let r = g(n)!` 后的 `ok(r.node)`，#1492 标题问题）：`try_lower_result_variant_ctor` 的 Ok arm 从 ctor builder fallback 到 `lower_result_str_piece`，恰是 record cell 已用的组合。piece 侧已接近全域（`lower_owned_heap_field`）。
- **递归 payload subject 经 RC guard wrapper drop**：`try_lower_result_match_value` 将 Ok payload 需递归 drop 的 subject 路由至 `resrec:`（scalar-Ok/递归-Err 则 `optrec:`），不再走 flat dec；后者在 arm 提取子项而非整体移动 payload 时会泄漏 payload 字段。整体 payload move 效果不变（递归 dec 到1停止）。这是 Lean 的派生值 incs-before-decs 规则，用于唯一缺失的位置。
- **`lower_result_str_piece` 得到其他 Named-call 位置都有的 ctor guard**：variant ctor payload 构造 tagged block，而非 emit 悬空 `CallFn`。审计全类：22处 user-named CallFn、10个 ctor 位置 probe 程序，全 BUILD+PARITY；renderer unlinked-call wall 为其余兜底。
- **接受 module-level heap global 为 heap-result arm 值**（`if c then DIGIT else …`）：arm 的 Var case 在每 arm frame 内用 `value_or_global`，镜像 owned-heap-field Var arm。
- **接受 Scalar-Ok × record-Err**（`Result[Int, {code, msg}]`）：variant-Err cell 的 record 对应版，len-as-tag `optrec:` wrapper。
- **接受 `List[(scalar, …)]` variant ctor 字段**（ADT brick 5，`Cls(List[(Int, Int)])` 形状）：四个镜像点同步扩展——ctor 接纳、两个 `needs_recursive_drop`、generator 字段 free（`__drop_list_str` 扫查，原样沿用 List[flat-variant] 先例）、`LIST_STR_DROP_SRC` linkage gate。

10形状 wall corpus（`w01–w10`）结果：**2 → 6 BUILD+PARITY**，经下述 combinator 阶段后 **10中7**；余下3个是 Map/Set twin。

<a id="landed-2026-08-17-second-arc-the-combinator-leg"></a>

## 2026-08-17 已落地，第二阶段：combinator 路径

没有 linked typed twin 的类型实例上的 `result.map` / `map_err` / `flat_map`，现在 desugar 为等价 `match`（`desugar_result_combinator_to_match`，counted-tree 与 lowering 两条 chain 都有），是参考编译器的 combinator 架构（rustc `?`、Swift 库 `??`、Roc canonicalization）。desugar 询问 `result_call_name` 本身该实例是否走 `_x`，所以 twin 可用性逻辑仅存一处：linked twin 字节不变，lowering 无法表达的 match 仍与 `_x` 一样 wall。用 lambda 和 named-fn `f` 参数、custom record Err 类型验证，403/403。

<a id="the-remaining-brick-mapset-typed-twins-sized-not-started"></a>

## 剩余 brick：Map/Set typed twin（已估规模，未开始）

`Map[List[Int], _]` / `Map[String, <record>]` / `Set[List[Int]]` 在 `_key_wall` / `_x` 路径（mod_p4_e.rs:609）wall。这不是 lowering gap：stdlib self-host 是按类区分的 prim-level hash table（`map*.almd` 3,238行、`set*.almd` 1,191行，每 key/value 类一个文件，`set_str.almd` 是 String 版）。heap-key 类需要元素结构 eq+hash、table 内 owned-block drop routing、registry + router + usage-gate 接线；按 CLAUDE.md family rule，还须在同一 PR 提供表明存在哪些 cell 的可执行 matrix gate。这是独立的大型 stdlib 阶段，不要附加到 lowering 会话。

下游检查：`almide/dfa`（完整递归下降 regex parser，以 `Result[Cursor,_] → Result[Node,_]` 为主干）现在可 build 到 wasm（41KB、v1-verified），50个测试经 wasm 运行，真实输入 native/wasm 字节一致。

此处取代 `v1-value-model.md` §263-308 的框架；后者把 Camp-4 固定为一个实例（`value.as_array : Result[List[Value], String]`）以及代码显示已不存在的 checker 限制。其单例分析仍好，但在其上构建之前，所称**根因**需重新验证。

---

<a id="1-what-walls"></a>

## 1. 什么被 wall

可失败函数不能将 `Result` payload 从一种 heap 类型改成另一种。已上报 almide/almide#1492。

```almide
type N = | L(Int) | C(List[N])
type R = { node: N, pos: Int }

fn g(n: Int) -> Result[R, String] = if n > 0 then ok(R { node: L(n), pos: n }) else err("neg")

fn f(n: Int) -> Result[N, String] = { let r = g(n)!; ok(r.node) }   // R -> N
```

```
heap-result `match` outside the executable subset cannot be faithfully returned
in this brick (would move out an empty deferred heap value)
```

原诊断译文：此 brick 无法忠实返回可执行 subset 之外的 heap-result match（会从空的 deferred heap 值中移出）。首段代码注释 R → N 表示将 payload 类型由 R 改为 N。

`ok(r)`（payload 不变）可 build；`ok(r.pos)`（payload → scalar）可 build；仅 heap → heap wall。以 `match` 代 `!` 同样 wall；`result.map` 根本没有 wasm 定义，因此三种写法全关闭。

这不是奇异形状。source → AST → IR → analysis → output、每步都可失败，是全部分阶段转换的主干。递归下降 parser 在丢弃 cursor 的单行就遇到：

```almide
fn parse_alt(cps: List[Int], pos: Int) -> Result[Cursor, String] = ...

fn parse(pattern: String) -> Result[Node, String] = {
  let c = parse_alt(to_codepoints(pattern), 0)!
  ok(c.node)                                       // the only line that walls
}
```

原代码注释表示：仅末尾这一行被 wall。

<a id="2-where-it-lives"></a>

## 2. 位于何处

`lower_tail_heap_match`（`lower/tail_b.rs:355-440`）按固定顺序尝试手写形状识别器，最终落入 `Unsupported`：

| # | 识别器 | 接受 |
|---|---|---|
| 1 | `tuple_extract_match_index` | 单 arm tuple 解构 |
| 2 | `try_lower_custom_variant_match` | user ADT subject、heap arm |
| 3 | `try_lower_variant_value_match` | Option/Result subject——**仅 scalar payload** |
| 4 | `try_lower_result_match_value` | len-as-tag Result subject |
| 5 | `try_lower_option_match_value` | `Option[heap]` subject |
| 6 | `try_lower_list_match_value` | list subject |
| 7 | `try_lower_tuple_refinement_match` | scalar-tuple subject |
| 8 | `desugar_match_to_if` → `try_lower_heap_result_if` | 可还原为 if |
| — | 其余 | **wall** |

识别形状方案的规模：

- `lower/` 下 **192** 个不同 `try_lower_*` / `lower_heap_*` 识别函数
- 六个承载 Camp-4 gate 的文件共 **3,817** 行（`control_p2.rs` 829、`control_p2_b.rs` 656、`control_p3.rs` 460、`result_match_value.rs` 246、`tail_b.rs` 813、`desugar_unwrap_b.rs` 813）
- `lower/` 总计 **64,930** 行

注释明示这是一形状一形状推进的边界——`tail_b.rs:375`：“heap payload 在此自行 gate 回 None = 真正 Camp-4 frontier”；`control_p2.rs:751`：“尚无 single-slot borrow rep”。

<a id="3-diagnosis-three-layers-and-only-one-of-them-is-real-work"></a>

## 3. 诊断：三层中只有一层是真正工作

<a id="l1--architectural-shape-and-ownership-are-entangled"></a>

### L1——架构：形状与所有权纠缠

每个识别器须手工证明自身所有权逻辑，因此**每个新形状都是新工作，每个未证明形状都是 wall**。`RESEARCH.md` 综合第5点已将其列为所调查八个编译器都没有的 anti-pattern：

> 没有编译器具有 per-VALUE“tracked/untracked”状态。[…] Almide 的 `materialized_results` / `heap_elem_lists` ValueId 集正是该反模式；“对 UNTRACKED subject 做 match”这种 wall，在八者中都无法表达。

Survey 3a 直接说明替代方案：

> 形状与所有权分离：Roc 的 match lowering 按 layout 驱动且全域；borrow-vs-incref 是后续在同一 layout 上的全程序 ARC pass。[…] 两种编译器都没有“Untracked → 拒绝 match”。**每个良类型 scrutinee 按构造可 match。**

<a id="l2--stale-the-recorded-root-cause-names-a-checker-that-was-replaced"></a>

### L2——过时：记录的根因指向已替换 checker

`v1-value-model.md:275` 将 blocker 描述为：

> `verify_ownership` 以 FLAT 处理 if-arm，看到同一 subject 的两个 drop = 假 double-free（checker 不建模 then/else 互斥）。

现在 `lib_c.rs:735-742` 明确反说：

> BRANCH JOIN（镜像已证明 checker 的 `CBranch` 规则）：每个 `IfThen`/`Else`/`EndIf` arm 从同一 entry state 运行，各对象离开时 count 须一致。[…] FLAT fold arm 的**旧模型**计算两 arm 的事件，静默接受跨 arm 补偿。

`OwnershipChecker.v` 有对应规则及测试（`check_line [COp Inc; CBranch [Inc] [Inc]; COp Dec; COp Dec] = true`、`check_line [CBranch [Inc] [Dec]] = false`）。

若此阅读正确，**旧模型下 gate 从未重新审查**，192识别器中一部分在防守已不存在的限制。

> 这正是组织自身教训所警告的主张：“单人深入/Coq 诊断在每个困难 wall 上都错了”。此处将它列为首先要**测量**的内容，而非可据以构建的发现。见 Stage 0。

<a id="l3--real-no-uniform-borrow-representation-for-a-heap-payload"></a>

### L3——真实：heap payload 无统一 borrow 表示

`control_p2.rs:751`：非 `String` heap payload“尚无 single-slot borrow rep”。且 `Result` 当前有**两种**物理 layout——len-as-tag（37文件）与 cap-as-tag（36文件）；因此 `Result[Unit, String]` 根据生成它的 stdlib 函数不同，物理上是两种东西。在 layout 成为类型的全域函数前，不可能有一般 match lowering，因为它不知道正在 match 什么。

这是 `RESEARCH.md:231` 已记录采用计划的 Stage A。

<a id="4-what-the-references-actually-do"></a>

## 4. 参考实现实际怎么做

在 `almide-references/` 下列行阅读，不是从 memo 转引。

<a id="koka--kokasrcbackendcparchs-parcguard"></a>

### Koka——`koka/src/Backend/C/Parc.hs`，`parcGuard`

```haskell
return $ \liveInSomeBranch -> ... do
  let dups = S.intersection ownedPvs liveInThisBranch
  drops <- filterM isOwned (S.toList $ liveInSomeBranch \\ liveInThisBranch)
```

两行。`liveInSomeBranch` 是所有 arm 的并集，每 arm drop 并集拥有但自身没有的项。模式变量通过对 **scrutinee** 的 `isOwned`（`ownedPats`）判定 owned 或 borrowed；`inferShapes` 记录 scrutinee→payload alias map。Almide 当前视作 blocker 的 aliasing，在此是有名称的 first-class 概念。

<a id="lean-4--lean4srcleancompilerlcnfexplicitrclean"></a>

### Lean 4——`lean4/src/Lean/Compiler/LCNF/ExplicitRC.lean`

`addPrologForAlt`（:421）用相同规则，另加 Almide 当前没有的一个顺序限制：

```lean
-- These are derived values who are no longer kept alive by a (potentially transitive)
-- parent value in this alternative and must thus be incremented. It is crucial that
-- these increments happen before the decrements as the decrements might contain an
-- operation that frees a parent.
```

原注释译文：这些派生值在该 alternative 不再由可能具有传递关系的 parent value 保活，因此必须 increment。decrement 可能含释放 parent 的操作，所以这些 increment 必须先于 decrement，这点极为重要。

`cases` 位置（:624-653）显式构造并集：

```lean
let caseLiveVars := alts.foldl (init := {}) fun acc ⟨_, altLive⟩ => acc.union altLive
modifyLive fun _ => caseLiveVars
```

`withCtorAlt cs.discr c` 将 payload bind 注册为**源于 discriminant**，默认 borrowed，仅 bind 比 parent 长寿时 increment。

<a id="roc--rocsrclirarczig"></a>

### Roc——`roc/src/lir/arc.zig`

match lowering 按 layout 驱动且全域；ARC 是同 layout 上独立的全程序 pass。payload read 刻意**不**传播 ownership（borrow + anchor）。每个 exit 发出 `state − keep`。

<a id="grain--graincompilersrccodegengarbage_collectionre719-731"></a>

### Grain——`grain/compiler/src/codegen/garbage_collection.re:719-731`

structured wasm 上 RC，与我方 target 形状最近。`MReturn` 完整清理每个 live managed binding，**跳过返回值**。

<a id="the-law-44"></a>

### 法则，4/4

**每个良类型 scrutinee 可 match。** 随后的 liveness 计算决定 ownership，分支规则为 `union ∖ here`。没有参考实现因 payload 表示而拒绝 match。

<a id="5-the-key-fact-almide-already-wrote-kokas-rule-in-the-wrong-place"></a>

## 5. 关键事实：Almide 已写出 Koka 规则，但放错位置

`lower/result_match_value.rs:227-244`：

```rust
fn emit_arm_release_parity(
    &mut self,
    consumed_by_then: &[ValueId],
    consumed_by_else: &[ValueId],
    else_marker_at: usize,
) {
    for x in consumed_by_then {
        if !consumed_by_else.contains(x) { /* drop after THEN */ }
    }
    for x in consumed_by_else {
        if !consumed_by_then.contains(x) { /* drop at the ELSE marker */ }
    }
}
```

原注释译文：在 THEN 后 drop；在 ELSE marker 处 drop。

这就是 `liveInSomeBranch \\ liveInThisBranch`，原样一致。与参考实现差别全是结构而非概念：

| | 当前 Almide | Koka / Lean |
|---|---|---|
| 范围 | **一个**识别器内 | 一个 pass、全部 branch |
| 元数 | 两 arm | 通过并集 fold 处理 n arm |
| 输入 | 手工收集 `consumed_by_*` 列表 | 每 arm **计算所得** liveness |
| 顺序 | 仅 drop | **inc 先于 dec**（Lean：dec 可释放 parent） |
| borrow | 按形状 gate | `withCtorAlt` / `inferShapes` alias map |

因此不需发明规则，而要把**已有规则从一个 brick 提升出来**，用计算的 liveness 驱动，再删掉它曾补偿的 gate。

<a id="6-plan"></a>

## 6. 计划

<a id="stage-0--measure-before-building-no-production-code"></a>

### Stage 0——先测量再构建（无 production code）

以下全计划以 L2 为条件，先实证确立它。

1. 建 wall corpus：每个当前被 `lower_tail_heap_match`、`try_lower_variant_value_match` 拒绝的形状各有一个最小 `.almd`，带预期运行输出。
2. 对每个引用 FLAT-arm 模型的 gate，**仅移除该 gate**，跑完整 gate（corpus-wall ACCEPT ×3性质、byte-verify、cargo test、output parity），记录哪些形状无其他改动即可通过。
3. 公布计数。若相当部分通过，Stage 3 会大幅缩小，且可部分先于 Stage 1～2。

交付物：`shape → still-blocked? → by what` 表。不合并任何内容。

**不要独自做。** 组织记录的教训是单人深入/Coq 诊断在每个困难 wall 上都错，而独立 fleet map + full-gate 验证每次找到健全答案。用此方式执行 Stage 0。

<a id="stage-1--one-type-one-layout"></a>

### Stage 1——一个类型，一个 layout

把 len-as-tag、cap-as-tag 的 `Result` 表示统一为 canonical layout，使 `layout = f(type)` 成立。对应 `RESEARCH.md:231` Stage A，那里估为~70文件。添加 `control_p2.rs:751` 所称缺失的 single-slot payload borrow：任意 heap payload 共用**一个** borrow rep，而不是每 payload 类型一个。

Gate：接受的程序不变，此阶段纯表示变更。

<a id="stage-2--hoist-the-union-rule-into-a-pass"></a>

### Stage 2——将并集规则提升为 pass

lowering 后、`verify_ownership` 前运行的 MIR pass：

1. 对每 branch arm 计算 live set（Lean `withCollectLiveVars`）。
2. `caseLive = ⋃ altLive`。
3. 每 arm：对 owned 且非 borrowed 的 `caseLive ∖ altLive` 发 `dec`；对 bind 比 parent 长寿的 borrowed 值发 `inc`。**inc 先于 dec emit**。
4. payload bind 默认 borrowed，源于 subject（`withCtorAlt`），因此建模而非绕开当前阻塞 lowering 的 alias。

这是 Almide `CBranch` checker rule 一直等待的 pass：checker 要求 arm 一致，此 pass **使它们一致**。

Gate：全部当前接受程序继续接受；旧路径已 emit parity drop 的地方 byte-identical。`emit_arm_release_parity` 成为死代码并删除。

<a id="stage-3--make-the-lowering-total-delete-the-recognisers"></a>

### Stage 3——lowering 全域化，删除识别器

layout 全域（Stage 1）、ownership 下游处理（Stage 2）后，`lower_tail_heap_match` 成为无 `Unsupported` fallback 的 layout 驱动 dispatch。按依赖序退役识别器，各自经过完整 gate，连同其 Camp-4 注释删除。

预期删除：六 gate 文件3,817行的大部分，加上 Stage 0 证明早已无用的部分。`RESEARCH.md:434` 另外估计九个 `!` 位置 pass 约2,800行，同一 primitive 会使其退役。

<a id="7-relationship-to-survey-4s-opreturn"></a>

## 7. 与 Survey 4 的 `Op::Return` 关系

`RESEARCH.md:422`（Survey 4e）建议 Zig 形的 frame-targeted `Op::Return { val }`。这是**另一个** wall——九个位置专用 `!` desugar。两者互补，不是替代：

- `Op::Return` 修复**值位置的提前退出**。
- 本计划修复**对 heap subject 匹配并产生 heap result**。

两者汇入同一 verifier 工作（branch frame 的 `diverged`、`Return` 的 exit obligation），因此 Stage 2 pass 应从一开始就设计为接受 diverging arm。

<a id="8-what-would-falsify-this-plan"></a>

## 8. 什么会反证此计划

- **Stage 0 表明全部 gate 仍承担关键作用。** 那么 L2 错，六月分析成立，Stage 1 borrow rep 承担全部成本。
- **不改 kernel 就无法在 Almide structured、携带 certificate 的 MIR 上表达并集规则。** Koka、Lean 都输出 unstructured IR；Grain 是 structured-wasm 参考，应在 commit Stage 2 前全文阅读。
- **layout 统一破坏 byte 验证。** Stage 1 改动每个返回 `Result` 的 stdlib 函数 emit 字节，若无法重建 certificate，则阶段顺序必须倒转。

<a id="9-provenance"></a>

## 9. 来源

- 编写 [almide/dfa](https://github.com/almide/dfa)（面向 wasm 的多 pattern DFA matcher）时发现 wall，上报 almide/almide#1492。
- 阅读参考位置：`koka/src/Backend/C/Parc.hs`（parcGuard）、`lean4/src/Lean/Compiler/LCNF/ExplicitRC.lean:421,624`、`roc/src/lir/arc.zig`、`grain/compiler/src/codegen/garbage_collection.re:719`。
- 引用的 `RESEARCH.md` 综合 / Survey 3a、4 不在此仓库 snapshot 中。下方行引用是历史来源，不是可解析 in-tree source anchor。
- 仓库内先例：[v1-value-model.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-value-model.md) 中 ownership 与递归 drop 讨论。
