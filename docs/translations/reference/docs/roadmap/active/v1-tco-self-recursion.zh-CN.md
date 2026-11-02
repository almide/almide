原文说明元数据：将自递归尾调用变成标量状态循环 — yaml parser 的关键支柱
<a id="v1-tco--self-recursive-tail-calls--scalar-state-loop-the-yaml-parser-keystone"></a>

# v1 TCO — 自递归尾调用 → 标量状态循环（yaml parser 的关键支柱）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-tco-self-recursion.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-tco-self-recursion.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-tco-self-recursion.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-tco-self-recursion.md)

> 原文注记：保留带日期的设计/纠正/改进/落地顺序，不表示所有提议递归类别均已交付。早期“必须新增中途带结果 break primitive”被后续从最终标量可重构、循环后产出结果的 subset 限定；更难的 heap-carried、match-leaf 和相互递归仍为独立待办。早期所有 yaml 仅更新标量的预期及“无 heap arg”措辞，与不变 String 参数及后续相反盘点并存，不暗中统一。障碍计数属于不同快照，22→19 包含此前 float.parse 改动。checker 接受及样例字节匹配是有范围的原文观察，不是普遍终止、栈安全或内存安全证明。未独立重复测试、证明或深输入运行。

**状态：设计（2026-06-19）。关乎健全性 → 设计先行，全新实现 + 对抗性验证，交付前对照已证明 checker 验证。绝不交付 checker 拒绝的见证。**

<a id="why"></a>

## 原因

`almide/yaml` 的 v1 障碍（28/74）主要来自直接自递归 parser（`scan_quote`、`find_colon_at`、`oct_rec`、`bin_rec`、`flow_rec`、`esc_rec`、…）。heap-result-`if` arm lowering（`lower_heap_result_arm`，control.rs）对自递归调用 arm 设置 WALL（`if name == self.fn_name { return None }`），因为 v1 没有 TCO：发出真实递归 `CallFn` 会在深输入上溢出 wasm 调用栈（fail-stop trap）— 而 **v0 对这些做 TCO**（2026-06-19 验证：500k 深自递归 `Option[Int]` 在 v0 运行无溢出；`pass_tco` 的 `is_tco_candidate` = `can_default_init(ret) ∧ all self-calls in tail position`）。所以 v1 不做 TCO 就执行递归，会在深输入上偏离 v0（v0 循环，v1 trap），不具字节一致性。guard 正确；修复是给 v1 与 v0 相同的 TCO。

<a id="the-key-simplification"></a>

## 关键简化

每个 yaml 自递归 parser 的自调用都只改变标量参数；堆参数原样传递：

```almide
fn scan_quote(s: String, pos: Int, in_q: Bool) -> Option[Int] =
  if pos >= string.len(s) then none                       // base → result, break
  else { let c = string.get(s, pos) ?? ""
    if in_q then scan_quote(s, pos + 1, not (...))        // recurse: pos,in_q := …  (s unchanged)
    else if c == "#" and … then some(pos)                 // base → result, break
    else scan_quote(s, pos + 1, …) }                      // recurse: pos,in_q := …
```

代码注释翻译：基例产生结果并 break；递归更新 pos、in_q，s 不变。后面的基例同样产生结果并 break；最后的递归同样更新 pos、in_q。

唯一堆参数 `s` 在每次自调用中相同，只有标量 `pos`/`in_q` 变化。因此这里 TCO 是标量状态循环 — 恰好是 `try_lower_scalar_while`（control.rs:1336）已通过 `LoopStart`/`LoopBreakUnless`/`LoopEnd` + `Op::SetLocal`，重新赋值标量 loop-carried 状态、以 cert-clean 方式 lower 的形态。堆参数无需重赋值（无 heap loop-carried 状态 → 无 `in_frame` 堆重赋值障碍）。

<a id="the-shape"></a>

## 形态

```
LoopStart                                  ; markers carry no ownership (verify_ownership no-op)
  <recompute per-iteration locals>         ; e.g. `let c = string.get(s, pos) ?? ""` — a per-iter heap temp
  <if base-case-A> { result := <A>; brk }  ; base arm ALLOCs the heap result into `result` local, breaks
  <else if recurse> { pos := pos'; in_q := in_q'; <continue, no break> }
  <drop per-iteration heap temps>          ; `c` freed before the back-edge (per-iteration balance)
LoopEnd
ret = result                               ; the moved-out function result (rc=1)
```

图中注释翻译：LoopStart marker 不携带 ownership（verify_ownership 为 no-op）。重算每次迭代 local，例如 c 是逐迭代堆临时值。基例 arm 将堆结果 ALLOC 到 result local 后 break。递归 arm 更新标量，继续而不 break。逐迭代堆临时值 c 在 back-edge 前释放，使每次迭代平衡。loop 后 ret = result 是移出的函数结果（rc=1）。

变换：函数本体为 heap-result `if`/`match` 链，且每个自调用 (a) 位于尾部，(b) 只改变标量参数（堆参数位相同）的自递归函数，转换为上述循环。基例 arm（`none`/`some(p)`/`ok`/`err`/literal/concat）设置 `result` local 并 break；递归 arm 用 `SetLocal` 更新标量，进入 back-edge。

<a id="the-cert-the-soundness-critical-part--new-vs-scalar-while"></a>

## 证书（健全性关键部分 — 相比 scalar-while 是新形态）

`try_lower_scalar_while` 要求“没有净 heap handle 逃出逐迭代 frame”。TCO 违反这一点：基例 arm 的 `Alloc`（函数结果）必须逃逸 — 在 `LoopEnd` 后移出。因此这是新 cert 形态：

- 每次迭代：递归路径不为结果分配（只对标量 `SetLocal`），并在 back-edge 前释放临时堆（`c`）→ 平衡的 `i…d`。
- 基例路径分配结果（cert `i`）并退出循环；结果在 `LoopEnd` 后恰好 `Consume`（cert `m`）一次（move-out）→ `im`，正是 checker 已接受的 heap-result-`if` arm 平衡。
- 堆参数 `s` 借用（callee 不 acquire、不 drop）— 与直线路径相同。

须对照已证明 checker（`corpus-wall` ownership fold）验证的主张：对象集合为 {逐迭代临时对象（各自在 loop frame 内 `id`）、结果（`im`，在实际基例分配，循环后移出）}。若 checker REJECT（例如不能理解 heap 结果逃出循环时的 break-once / model-one-iteration），则设计错误 → revert，不得交付；fallback 是保留自递归 guard（这些 yaml 仍被阻塞），直到 loop-result cert 扩展完成。

<a id="blocker-found-2026-06-19-the-loop-primitives-are-top-test-only"></a>

## 2026-06-19 发现的 blocker：loop primitive 仅支持顶部测试

MIR loop marker（`LoopStart` / 顶部 `LoopBreakUnless{cond}` / `LoopEnd` back-edge / `SetLocal`）建模经典 `while cond { body }` — 一个在顶部测试的出口，无值。一般 TCO 需要携带结果的循环中途 break：自递归函数有 ≥2 基例（`none` 对 `some(p)`），与递归 arm 交错，每个产生不同堆结果。不逐函数重写计算（这不算通用变换），就无法表示为单个顶部测试 while + 循环后结果推导。

因此 TCO 需要新 loop primitive：中途**设置函数结果并退出的 break**（如 `Op::LoopBreakWith { val }` 渲染为 `(result.set;
br $outer)`），加上 cert 推理，说明逃出循环的结果 `Alloc`（`i`）在 `LoopEnd` 后恰好被 `Consume`（`m`）一次。这是 MIR + render + **ownership-cert 扩展**（堆值跨循环边界）— 严格大于复用现有顶部测试 marker，也更关乎健全性。设计先行 + 对抗性实现；交付前验证逃逸结果对象的 `im` trace 被已证明 checker ACCEPT。若 checker 不能建模堆结果逃出循环，这就是真正 cert 前沿（如 Camp 4）→ Mob-gate，不交付被拒绝内容。

<a id="scope--gates"></a>

## 范围 / 门禁

- 仅自递归（直接 `name == self.fn_name`）；相互递归是独立且更难的问题，保持拒绝。
- 仅改变标量的自调用（堆参数位相同）。改变堆参数的自调用（重建 list/string）需要 heap loop-carried 状态 → 暂缓（`in_frame` 堆重赋值障碍已拒绝它）。
- 两侧之前脱糖：若实现为 IR 重写，`count_ir_calls` 必须看到同一变换（自调用消失 — 必须保持计数一致）。
- 每步验证：`render_program` 探针字节匹配（浅输入和深输入都测，确认无 trap = TCO 真正应用）+ `corpus-wall` ACCEPT（3个性质 + mir≤ir）+ `cargo test -p almide-mir` + `output-parity` baseline 无回归。

<a id="expected-payoff"></a>

## 预期收益

解锁约6个直接自递归 yaml parser（以及语料中任意自递归返回堆值函数）。结合 (2) 非自递归 heap-arm 形态（after_colon 的含 heap-Value tuple 等）和 (3) Camp-4 heap-payload variant match（4个 `looks_numeric`/`is_compound`/… + float.parse self-host），这就是 yaml 在 v1 上达到0障碍的路径。TCO 是关键支柱。

<a id="cleanest-tco-entry-point-found-at-the-function-level-2026-06-19-scan_quote--find_colon_at"></a>

## 最清晰的 TCO 入口，2026-06-19 在函数层发现：scan_quote / find_colon_at

这两个 yaml 函数是最容易处理的 TCO 目标（直接解除22个障碍中的2个，oct_rec/bin_rec/flow_rec 可能也如此 — 同样形态）：
```almide
fn scan_quote(s: String, pos: Int, in_q: Bool) -> Option[Int] =
  if pos >= string.len(s) then none
  else { let c = string.get(s, pos) ?? ""
    if in_q then scan_quote(s, pos + 1, not (c == "'" or c == "\""))   // TAIL self-call
    else if c == "#" and ... then some(pos)                            // base: scalar-payload Option
    else scan_quote(s, pos + 1, c == "'" or c == "\"") }               // TAIL self-call
```
代码注释翻译：首尾是尾部自调用；中间是标量载荷 Option 基例。

形态 = 教科书 TCO 案例：**尾部自递归、标量 loop-carried 参数（pos、in_q）、标量载荷 `Option[Int]` 结果、基例 `none`/`some(scalar)`**。无 heap arg，无 heap accumulator。

构建（扩展既有循环机制 — control.rs 约1349的 scalar-while + 约2027的 for-range 使用 `LoopStart`/`LoopBreakUnless`/`LoopEnd`/`SetLocal`）：把自递归 guard（control.rs:1641 `if name == self.fn_name { return None }`）换成这一形态的 TCO 变换 — 检测本体 if/match arm 为 (a) 相同函数 + 标量更新参数的直接自调用，或 (b) 基例表达式；发出 `LoopStart` … 逐迭代计算分支条件，基例 arm 设置结果 local（`Op::Alloc` 一个 `none`/`some(pos)` Option — 唯一堆分配，`i`）后 break；自调用 arm 用 `SetLocal` 更新标量参数并循环。`LoopEnd` 后返回结果 local（`m`）。cert 看到一个 `i`（实际 break 的结果 Option）+ 一个 `m`（return）= 平衡；标量 SetLocal 不携带 ownership。验证：`scan_quote`/`find_colon` 输入上与 v0 字节匹配 + corpus-wall + tests。这是聚焦且范围明确的步骤（新递归→循环变换），不是会话末尾的一行修补；但它是下一步最高收益行动（TCO 解除22个障碍中的约6个）。

<a id="correction-implementation-level-2026-06-19-tco-needs-a-new-mir-primitive-not-a-loop-reuse"></a>

### 纠正（实现层，2026-06-19）：TCO 需要新 MIR primitive，而非循环复用

从 op 层检查：scan_quote 基例（`none`、`some(pos)`）位于本体中途（`if in_q … else if # … else …` 链内），不是循环顶部。现有 loop primitive 是顶部测试 `LoopBreakUnless { cond }`（control.rs:1377/2032），只能在迭代开头退出。因此递归→循环变换不能复用它：需要**本体中途携带结果 break** primitive（`Op::LoopBreak { result: ValueId }` 或类似），携带本体任意位置计算的堆结果退出循环。这是新 MIR op + render（在传递结果的 wasm `block`/`loop` 内，设置结果 local 后执行 `(br $loopexit)`）+ cert 处理（break 结果的 `i` 必须与函数 `m` 平衡；cert 须将 loop body 逐迭代 op 视为平衡，将 break-result 视为移出的返回值）。所以本文标题提到的“循环中途带结果 break MIR primitive”确实存在且必需 — TCO 是聚焦的 MIR+render+cert 步骤，不是对现有顶部测试循环的变换。（这正是 control.rs:1641 自递归 guard 保留的原因：没有新 primitive，就无健全 lowering，只有深输入 trap 的递归。）

<a id="refinement-2026-06-19-the-gate-verifiable-cert-clean-structure-no-new-primitive-no-cert-change"></a>

### 改进 2026-06-19：门禁可验证的 cert-clean 结构（无新 primitive，无 cert 修改）

重新分析表明，不需要新 `Op::LoopBreak` primitive 就可实现 TCO（因此不改 cert；改 cert 会无法验证，因为 corpus-wall 使用 cert）。cert-clean 形态是：**仅标量的顶部测试循环 + 循环后的 heap-result-if**。
- 循环本体仅通过 `SetLocal` 更新标量 loop-carried 参数（`pos`、`in_q`）— 本体无 heap，因此逐迭代平衡显然成立（现有 cert 原样通过）。
- heap 结果（`none`/`some(pos)`）在循环后计算，作为最终标量状态上的普通 heap-result-if — 正常平衡（`i`+`m`），无 Alloc 逃出循环本体。
前提：基例结果必须能从最终标量状态重构。scan_quote/find_colon_at 满足：`if pos >= len then none else some(pos)`。（基例结果需要最终标量以外信息的函数需要 result-in-loop 形态 → Alloc 逃出 loop body → cert 标记它 → 该 subset 仍需新 primitive。因此从可重构 subset 开始。）
变换（复杂之处）：从叶子为自调用 `f(invariant, pos', in_q')` 或基例表达式的嵌套 if/else 本体，推导 (a) continue-condition = ¬(到达任一基例)，(b) loop body = 自调用路径上的标量更新，(c) post-loop result = 在最终标量上重新求值的基例表达式链。发出 `LoopStart` / `LoopBreakUnless{continue}` / body `SetLocal` / `LoopEnd`，然后 post-loop heap-result-if。验证：scan_quote 输入上与 v0 字节匹配 + corpus-wall（现有 cert）+ tests。门禁可验证、有界（错误推导导致字节不匹配 → revert），但确实是复杂变换，一个聚焦步骤。先做 scan_quote/find_colon_at（2个障碍、可重构 subset）；oct_rec/bin_rec/flow_rec/flow_step 很可能也是同样形态。

<a id="landed-2026-06-19-commit-77c91648-scalar-arg-tco--scan_quotefind_colon_at-yaml-2219"></a>

### 已落地 2026-06-19（commit 77c91648）：scalar-arg TCO — scan_quote/find_colon_at，yaml 22→19

实现了 `try_tco_rewrite`（lower/mod.rs）：result_kind + scalar-loop + post-loop-dispatch 变换，门控为 heap-result 尾部自递归、标量 loop-carried 参数。验证：scan_quote 与 v0 字节匹配（3/99/99/0），corpus-wall ACCEPT，output-parity 61/61，466 tests。yaml 22→19（scan_quote + find_colon_at 现可 lower；此前 float.parse 识别使22→21，TCO 使21→19）。

剩余自递归障碍全都需要更难的扩展（heap loop-carried 参数 = heap back-edge merge，即 `while_body_heap_accumulator_walls` 类）：
- `oct_rec`/`bin_rec`：`match list.first(cs)` 叶（tco_collect/rewrite 需 MATCH-leaf 支持），且 `cs: List[String]` 为 heap loop-carried 参数（每轮 drop(cs,1)）— 超出标量门禁。
- `flow_rec`/`flow_step`：相互递归（flow_rec↔flow_step，非直接自递归），且 `acc:
  List[String]` 是 heap accumulator（acc+[x]）。
所以下一个 TCO 步骤是 HEAP-LOOP-CARRIED：携带 heap accumulator 的标量循环，需要跨 back-edge 合并 accumulator（每次迭代重绑 heap local，仅释放一次）— 这是 scalar-while 当前明确拒绝的难例。再加 match-leaf 和相互递归支持。各项都是进一步的聚焦扩展；scalar-arg subset（本 commit）是清晰基础。
