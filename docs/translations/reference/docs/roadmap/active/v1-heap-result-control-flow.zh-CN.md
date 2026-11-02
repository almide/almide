<a id="v1-heap-result-ifmatch-execution--design-de-risked-no-coq-change"></a>

# v1 堆结果 `if`/`match` 执行 — 设计（已降低风险：无需修改 Coq）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-heap-result-control-flow.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-heap-result-control-flow.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-heap-result-control-flow.zh-CN.md)

> 这是固定于 35297a612fb29e3f78f6587aba253aa91d51f466 的[正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-heap-result-control-flow.md)的非规范性阅读译本，不会自动同步，原本仍具权威性。历史状态、实现、证明、审计和测量主张仍是固定原本的陈述，此处未经独立执行或验证。待完成的提案不是当前保证。代码块与注释原样保留，原本元数据作为已停用的注记显示。

> 原本说明（已停用元数据）: 历史上首个堆结果控制流范围，以及后续 lowering 背景。

<a id="historical-first-slice-later-lowering-exists"></a>

## 历史上的首个范围；后续 lowering 已存在

下方的字面量分支结果与“剩余”列表描述 MIR 堆结果控制流的最初范围。后续 match 和非字面量分支工作已存在于 [MIR lowering](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-mir/src/lower/)，并记录在 [Camp 4](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/camp4-general-heap-match.md)。不要将这份最初供下一会话使用的步骤当作这些形状仍未实现的证明。当前发布的 Wasm 路径是[结构化路径](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/wasm_route.rs)；原始证书推理保留在其建模范围内，不提升为整个产物的内存安全定理。

状态：**字面量分支的 `if` 已实现（commit 126921e6），经对抗性验证为健全。** `fn label(c) -> String = if c then "yes" else "no"` 现在执行时与 v0 逐字节匹配。下方设计被证明恰好正确 — 无需修改 Coq，现有 checker 原样接受逐分支的 `"im"` 证书。三次独立对抗性审计未发现“接受但不安全”的漏洞。corpus-wall：ownership 13121→13129，全部三项性质 ACCEPT。

此范围内剩余：堆结果 `match`（经 `desugar_match_to_if` 脱糖），以及非字面量分支（直接调用 — 下方步骤 1）。审计暴露的另一个既存缺陷**此后已修复**（render_wasm_p2.rs）：String/Bytes 块现在按与 LIST 兼容的方式计算大小（`cap = ceil(bytes/ELEM_SIZE)` 个元素，分配 = `LIST_HEADER + cap*ELEM_SIZE` — 恰好是 `$alloc` 重用检查重新计算的内容），因此已释放 String 块会被回收重用，反复创建销毁 String 的循环在有界内存中运行（由 render_wasm/tests_part1_b.rs / tests_part2.rs 中的 churn 测试固定）。

<a id="the-problem"></a>

## 问题

堆结果分支从各分支返回 String/数据：

```almide
fn label(c: Bool) -> String = if c then "yes" else "no"
```

标量 `if`/`match`/`while`/`for` 已通过平坦标记 `Op::IfThen`/`Else`/`EndIf`（循环还使用 `LoopStart`/…）执行（commits a2a9f656 / 0a6db87b / 547b5efc）。健全性论证是**逐分支平衡**：每个分支内部平衡，因此 cert 处理平坦标记流的方式与经过语料验证的线性化完全相同。标量分支不携带堆 → 平凡地平衡。

堆结果分支打破了这一点：每个分支 `Alloc` 其字符串，而值**逃逸**（它是返回值），因此分支内部不平衡。平坦所有权证书于是会看到两个分支分配和一次移出：

```
objA: i          (Alloc "yes")
objB: i          (Alloc "no")
ret  : m  on dst (one move-out)
```

`objA`/`objB` 获得 `i`，却没有匹配的 `m`/`d` → checker FAULT（泄漏）→ REJECT。这就是当前 lowering 将堆结果分支推迟为单个 `Alloc{Opaque}` 的原因（健全，但结果为 Opaque = 不可执行）。

<a id="the-key-insight--no-coq-checker-change-is-needed"></a>

## 关键洞见 — 无需修改 Coq checker

经内核证明的 checker 只折叠逐对象事件流（`i`/`a`/`d`/`m`），并检查每个对象是否平衡（`certificate.rs::ownership_certificate` 发出事件流；Coq checker 重新验证）。它已接受 `"im"`（先分配再移出）。因此，只要**编译器**为每个分支发出移出操作，恢复逐分支平衡，现有 checker 就会接受，无需修改任何证明：

```
objA: im         (Alloc "yes"; Consume objA)
objB: im         (Alloc "no" ; Consume objB)
dst : (none)     dst is the IfThen RESULT, never an Alloc → not in `of`
                 → func.ret = dst emits NO second `m` (no double-move)
```

现在每个分支都是 `"im"` = 内部平衡。两个平衡分支的平坦线性化仍然平衡。现有逐分支平衡的健全性论证原样成立。运行时：wasm `if` 恰好运行一个分支 → 一次 `Alloc` + 返回（rc=1，调用者拥有）；未选择的分支从不分配 → 无泄漏、无重复释放。

这把该范围从“修改并重新证明 Coq checker”（早先会话的设想）降为“编译器侧 cert 生成与 render，checker 不动”。成本低得多，但仍属于健全性核心。

<a id="implementation-plan-scoped-literal--direct-call-arms-first"></a>

## 实现计划（范围：先做字面量与直接调用分支）

1. 在 `lower/control.rs` 中实现 `try_lower_heap_result_if(cond, then, else_, ty)`（仿照 `try_lower_scalar_if`）：以 `is_heap_ty(ty)`、可进行标量 lowering 的 `cond`，以及作为**简单拥有型堆值生产者**的分支为准入条件 — `LitStr`（→ `Op::Alloc{Init::Str}`）或返回堆值的直接 `Call`/`CallFn`（→ 调用 op，cert 已按 `i` 计入）。其他分支类型（堆 `Var`、嵌套分支、含局部变量的块）→ 返回 `None` → 回退到当前健全的 Opaque 形式。`match` 变体与标量路径完全一样，通过 `desugar_match_to_if` 脱糖。
2. 每个分支：将生产者 lower 为 `objX`，然后加入 `Op::Consume{v: objX}`，并从 `live_heap_handles` 中移除 `objX`（它作为结果移出，而不是逐分支 drop）。发出 `Else{val: Some(objX)}` / `EndIf{val: Some(objX)}`，使 wasm 将句柄留在栈上供 `dst` 使用。
3. `ownership_certificate`（`certificate.rs`）：无需修改 — `Op::Consume` 已发出 `m`，`Op::Alloc`/堆 `Call` 发出 `i`，而 `IfThen.dst` 从不插入 `of`，所以 `func.ret` 不会再加第二个 `m`。通过对抗性验证确认这一点（危险在于生产者并非新鲜拥有值的分支 — 例如借用参数 — 必须 REJECT，不能默默配平；借用参数的 Consume 在 rc 0 发出 `m` → checker 已会 FAULT，这是正确的阻挡，但仍须确认准入条件绝不会放行它）。
4. `render_wasm.rs`：
   - `render_wasm_fn` 的 `IfThen` 分支当前硬编码 `(result i64)`。让结果类型跟随 `dst` 的 repr：**`dst` 为堆时用 i32**，标量用 i64。
   - dst-repr 难点：`value_reprs_wasm` 必须将 `IfThen` 的 `dst` 分类为堆。可从 `EndIf`/`Else` 的 `val` repr 推断（分支句柄为 i32），或在标记上传递 repr。务必做对 — 将堆 dst render 为 i64 局部变量，最好情况是类型错误，最坏情况是静默误编译。
5. 测试（`render_wasm.rs`）：`label(true)="yes"`、`label(false)="no"` 与 v0 逐字节匹配；调用分支变体；字面量模式、堆结果的 `match`。
6. 门禁：`cargo test -p almide-mir`、`proofs/corpus-wall.sh` 必须保持 WALL OK / 三项性质 ACCEPT（注意 ownership 计数 — 堆结果 if 语料函数从 Opaque 的 1 个对象变为逐分支 2 个对象，因此 ownership 可能上升；只要仍 ACCEPT 且各自为 `"im"`，就是健全的）。
7. **对抗性健全性验证**（必需 — #49 类“接受但不安全”漏洞就在这里）：启动 ≥3 个独立 agent，尝试构造一个被新 lowering 使 checker ACCEPT、但运行时泄漏或重复释放的堆结果分支。只有它们未能攻破时才 commit。

<a id="why-deferred-honesty"></a>

## 为何推迟（坦诚说明）

设计健全且 checker 不动，但步骤 2–4 修改所有权证书生成和 wasm render — 恰是历史上发现两个“接受但不安全”漏洞（#49 的传递性 caps、被省略调用的 taint）的区域。这要求以全新注意力开展对抗性验证，而不是在漫长会话末尾实现。在此记录已降低风险的设计就是进展；实现与对抗性验证是下一会话的首要工作。

## 代码说明与注释译文

第一个事件示例分别分配 “yes” 与 “no”，随后在 dst 上移出一次。平衡示例分别分配并消费每个对象；dst 是 IfThen 结果而非分配，因此不在 of 中，返回 dst 不会发出第二次移出（无重复移动）。

## 原本注意事项

原本同时保留历史上的已实现/健全报告、剩余工作列表、此后已修复的分配问题及下一会话计划，并明确承认后续 lowering 已存在。保留的局部证书推理及对抗性审计报告不是整个产物的内存安全定理，也不是本译本开展的新审计。原本关于测试、agent 和 commit 的指示属于历史计划内容，而非此处执行的操作。
