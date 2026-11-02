<!-- description: 历史ADT brick-5目标prompt、已完成recursive drop及保留的失败尝试 -->
<a id="goal-prompt--finish-adt-brick-5-heap-field-bind--recursive-drop-to-the-1-lever"></a>

# 目标PROMPT — 完成ADT brick 5（heap-field bind + recursive drop），打通首要突破点

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-adt-brick5-goal.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-adt-brick5-goal.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-adt-brick5-goal.zh-CN.md)

> 非规范性历史读者资料，不是可执行任务或执行命令授权。固定原文为权威且不变，不自动同步、无新测试或证明。开头brick完成记录优先于旧剩余任务prompt。所有权trace接受不验证生成raw-memory drop本体；保留原方案断言及失败尝试，不升格为当前普遍安全保证。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-adt-brick5-goal.md)

<a id="historical-goal-prompt--completed-brick"></a>

## 历史目标prompt — brick已完成

权威[ADT value-model记录](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-adt-value-model.md)记载5b的recursive drop在5c之后于`c98486be`完成。以下prompt记录更早的分支内调查，不能作为开放实现任务重新启动。被拒绝的尝试及trusted-drop边界仍是有用证据；所有权trace获接受不证明生成的raw-memory drop routine本体正确。

继续在 **`develop-v1`** 分支进行v1 trust-spine ADT value-model工作。Brick 1–4 + 5a已完成并push（见`git log`）。任务是**落地5c（heap-field match bind）及5b（recursive drop），使递归`Expr` to_string与v0逐字节一致，这是跨repo wall的首要突破点（约41处wall）。** 先读`docs/roadmap/active/v1-adt-value-model.md`，它是设计及状态的权威记录。

<a id="the-goal-concretely"></a>

## 具体目标

以下程序在v1 trust spine（`render_program` → wat → `wasmtime`）必须与v0（`almide run`）逐字节一致：

```almide
type Expr = Lit(Int) | Add(Expr, Expr) | Neg(Expr)
fn tos(e: Expr) -> String = match e {
  Lit(n)    => int.to_string(n),
  Add(l, r) => "(" + tos(l) + " + " + tos(r) + ")",
  Neg(x)    => "-" + tos(x),
}
fn main() -> Unit = { println(tos(Add(Lit(1), Neg(Lit(2))))) }   // v0: (1 + -2)
```

当时它诚实地wall。完成意味着：本例及`Expr` create+drop leak loop的v0==v1 byte一致；`bash proofs/corpus-wall.sh`三种属性全ACCEPT；`cargo test -p
almide-mir`没有新失败（既有两项`self_hosted_json_*`的`$__drop_value`失败无关）。

<a id="non-negotiable-discipline-the-②-cardinal-rule--already-re-proven-twice-this-arc"></a>

## 不可协商的纪律（②根本规则，本arc已两次重新证明）

**绝不交付被内核证明的`[ownership]`检查器拒绝的MIR。** heap-field variant lowering可在`wasmtime`逐字节一致，却潜藏double-free；只有`corpus-wall.sh`的已证明检查器捕捉它。两次天真5c尝试（borrow `LoadHandle`+`param_values`及`lower_heap_result_arm`内Option式move-out auto-Dup）均得到`[ownership] REJECT`（ownership会从16085升至16162）。每步gate都是corpus-wall ACCEPT；若REJECT则revert、不交付，不能靠推理绕过。小步推进，每步运行corpus-wall。

<a id="where-the-code-is"></a>

## 代码位置

- 构造：`crates/almide-mir/src/lower/binds.rs`内`try_lower_variant_ctor`（5a scalar + leaf `String` field已落地；`String` field被移入、经`record_masks` mask，由已有DropListStr释放）。heap非String / nested-variant field会wall。
- Match：`crates/almide-mir/src/lower/control.rs` — `try_lower_custom_variant_match`（value）、`lower_custom_variant_unit_match`（unit-stmt）、`parse_variant_arms`（共用arm parser，当时拒绝heap-field bind）、`bind_variant_arm`（仅scalar bind）、`emit_variant_arm_chain` / `emit_variant_unit_chain`。连接点：tail.rs（scalar+heap）、binds.rs（let-bind）、calls.rs（operand/arg及arg位置ctor构造）、control.rs:约122（unit statement）。
- v0参考布局（#782删除，从git历史读取）：`emit_wasm/collections.rs::emit_record`、`equality.rs::{variant_alloc_size,find_variant_tag_by_ctor}`、`mod.rs` variant注册。（v1使用自己的uniform-i64-slot block：tag@slot0、fields@slot1..、填充到slot_count；不是v0 byte-packed布局。只有可观测stdout须与v0一致。）

<a id="step-5c--heap-field-match-bind-the-blocker-use-aware-binding-is-the-answer"></a>

## Step 5c — heap-field match bind（blocker，答案是按使用方式绑定）

本session诊断已完成，已知精确corpus再现及两种失败模式：

- 拒绝的corpus函数是 **`spec/wasm_cross/generic_fn_in_inferred_lambda.almd`**：`fn unbox[T](b: Box[T]) -> T = match b { B(x) => x }`在`Box[String]`上单态化；从borrowed param绑定String field并移出（`B(x) => x`）。单个ownership.cert生命周期定位bug：
  - **borrow bind**（`LoadHandle`+`param_values`，加`lower_heap_result_arm`的Var auto-Dup）：move-out `B(x)=>x`正常（`am`），但消耗式复用`Ctor(s) => Other(s)`在rc 0 borrow上emit `Consume`/`m`，导致REJECT。
  - **Dup后的owned bind**（`LoadHandle`+`Op::Dup`，push到`live_heap_handles`，由arm frame drop）：消耗式复用正常，但move-out `B(x)=>x`双重取得：bind `a` + `lower_heap_result_arm` auto-Dup `a` + 其`m` + arm-frame `d` + return `m` = `aamdm`，净−1过度释放，REJECT。
  再现：先`cargo run -q -p almide-mir --example classify_corpus -- --out /tmp/cw
  spec/wasm_cross/generic_fn_in_inferred_lambda.almd`，再`cat /tmp/cw/ownership.cert`（Dup为`aamdm`，borrow为平衡`am`）。

修复是**按使用方式绑定**。根据arm本体如何使用heap field决定bind mode：
  - 只读 / borrow传给借用callee（`string.len(x)`、`tos(l)`）：**borrow**（`LoadHandle`+`param_values`，不进`live_heap_handles`、不drop），无`ad`且平衡。
  - 作为arm结果移出（`B(x) => x`）：**borrow + 既有auto-Dup**，位于`lower_heap_result_arm`的Var case（`am`）。不能预先Dup，否则双重取得。
  - 消耗进新ctor / 消耗call（`Ctor(s) => Other(s)`）：**先`Dup`后move**（`am`），预先Dup使被move的是owned ref而非borrow。
  用小型use-visitor分类arm本体对每个绑定field的使用：borrow传递、作为结果移出、消耗；据此绑定。已有逐arm-frame helper `lower_variant_arm_value` / `lower_variant_unit_arm`，使用bind前取得的mark，经`drop_arm_locals`在arm末尾drop owned bind，对条件分支安全。注意：不能在条件arm内把owned local从`live_heap_handles`移除。lhh在compile时，而branch在runtime；另一个arm仍需scope drop。像helper一样将drop留在各arm的IfThen/Else区域内。

验证：corpus-wall ACCEPT（gate）+ `unbox`式、`tos`式byte一致 + leak loop。

<a id="step-5c--status--done-commit-b50e95a3"></a>

## Step 5c — 状态：✅ 完成（commit `b50e95a3`）

多arm match的heap-field（String）绑定已落地：borrow + move-out auto-Dup + consume时Dup。实际blocker不是bind model，而是**单arm direct-ret双重move**（`unbox` newtype `B(x)=>x`：`Op::Consume`→`m` + heap `func.ret`→`m`）。单arm heap case当时被wall，多arm继续。corpus-wall ACCEPT。递归`Expr` to_string突破点只剩5b。

<a id="step-5b--recursive-drop-the-last-piece--concrete-recipe-designed-this-session"></a>

## Step 5b — recursive drop（最后部分）— 具体方案（本session设计）

关键洞察：`$__drop_value`（stdlib/value_core.almd）是由`prim.*` op构建的self-hosted Almide：`prim.handle`、`prim.load32`、`prim.load_handle`、`prim.rc_dec`加Unit自递归。`prim`在检查器中为no-op，因此drop fn的ownership cert为空；它是trusted routine，leak/double-free正确性的检验由leak loop承担而非cert。故custom-variant recursive drop不是render/runtime变更，而是以同样形态生成每种类型的Almide fn，像value_core一样auto-link，按构造cert-clean。

方案：每种带nested-variant heap ctor field的variant类型生成如下形态（v1布局为`[rc@0][len@4][cap@8][tag=slot0@12][field slot i @ 12+(1+i)*8]`，tag存储宽8）：

```almide
fn __drop_Expr(e: Expr) -> Unit = {
  let h = prim.handle(e)
  if prim.load32(h + 0) == 1 then {        // last ref
    let tag = prim.load64(h + 12)          // slot 0 = tag
    if tag == 1 then {                      // Add: fields slot1@20, slot2@28
      __drop_Expr(prim.load_handle(h + 20))
      __drop_Expr(prim.load_handle(h + 28))
    } else if tag == 2 then {               // Neg: field slot1@20
      __drop_Expr(prim.load_handle(h + 20))
    } else ()                               // Lit (tag 0): scalar, nothing to free
  } else ()
  prim.rc_dec(h)
}
```

（仅variant/heap field递归；leaf String field为`prim.rc_dec(prim.load64(h+off))`，scalar field跳过。若ctor需loop，仿照`__drop_value` tag-5 array case和`__vdrop_arr` helper形态。variant各ctor固定arity，不需loop。）

接线：
1. MIR内`Op::DropVariant { v, ty }`，cert为单个`d`（lib.rs + certificate.rs，和其他Drop* op并列）；render为`(call $__drop_<ty> (local.get v))`。
2. program组装时为每个递归variant类型生成`__drop_<ty>` Almide source并auto-link。最自然hook在almide-types/src/self_host_registry.rs的`self_host_runtime()`旁；但registry是固定file的`include_str!`，需增加动态generated-source路径：由`VariantLayouts`产生source string，通过`lower_source`使用的同一frontend feeder，将fn改名`__drop_<ty>`并加入program functions。render_program（trust spine）及render_wasm的`lower_source` test feeder都须链接它。
3. 构造：`try_lower_variant_ctor`允许nested-variant heap field；用`lower_owned_heap_field`移入child handle，其Var case已有`Dup`，再mask，完全仿String路径。追踪构造值，使scope-end drop为`Op::DropVariant{ty}`，以类似`value_handles` → `DropValue`的新set追踪。
4. Match-bind：把`parse_variant_arms`内5c `Ty::String` heap-field bind扩展到variant类型field。borrow即可：`tos`的`l`/`r`向递归call借用传递，不移出，多arm；单arm gate已有退化newtype覆盖。
5. 每步验证：corpus-wall ACCEPT、`Expr` create+drop leak loop（freelist让leak成为OOB trap）、`tos(Add(Lit(1), Neg(Lit(2)))) == "(1 + -2)"`逐字节匹配。

<a id="step-5b--original-notes"></a>

## Step 5b — 原始笔记

nested-variant ctor field（`Add(Expr, Expr)`）不能仅对子slot平坦`rc_dec`，会泄漏孙节点。需要**tag驱动递归free**，类似已有`Op::DropValue` / `$__drop_value`（dynamic-Value递归drop）。可能设计：

- 在`try_lower_variant_ctor`允许nested-variant heap ctor field，移入child handle并mask。
- 每个递归variant类型emit `$__drop_<Type>(ptr)` runtime fn：读取tag@slot0并switch；逐ctor对各variant field递归`$__drop_<FieldType>`、对各leaf field `rc_dec`，最后释放block。或者用emit的每类型layout table驱动通用`$__drop_variant`。
- 新`Op::DropVariant { v, type }`，或复用DropValue机制，由render lower为该fn call；已证明检查器如对`$__drop_value`一样，将其接受为单个trusted `d`。
- 追踪构造的递归variant值，使scope-end drop emit `Op::DropVariant`。
- 验证：`Expr` to_string byte一致、create+drop leak loop、corpus-wall ACCEPT（ownership）；检查器须将递归`d`接受为无leak/double-free。

<a id="verification-commands-run-after-every-increment"></a>

## 验证命令（每个增量后运行）

```
cargo build -q -p almide-mir --example render_program
# byte-match: almide run <f>  vs  render_program <f> | wasmtime
bash proofs/corpus-wall.sh        # MUST stay ACCEPT (all 3) — the soundness gate
bash proofs/diff-fuzz.sh 40       # generative v0==v1 (variant templates already present)
cargo test -q -p almide-mir       # no NEW failures (2 pre-existing json fails are unrelated)
```

当`Expr`突破点byte一致且corpus-wall ACCEPT，将其加入确定性wasmtime cargo test（`crates/almide-mir/src/render_wasm/tests_part3.rs`，`custom_variant_*`附近）及diff-fuzz递归variant template；在`docs/roadmap/active/v1-adt-value-model.md`标5b/5c完成，commit（英文单行、`Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`），仅在被要求时push `develop-v1`。

<a id="guardrails"></a>

## 护栏

- 分支`develop-v1`。绝不对非本人修改file使用`git checkout`/`restore`/`stash`，其他agent同时工作；改用手动Edit revert。
- `?? p03*.*`及其他untracked file不属于你，勿碰。
- 不扩张§4.1手写WAT runtime。
- 若某步无法cert-sound，就诚实wall并记录；wall胜过潜在double-free。部分但sound（如5a构造）是真进展，检查器拒绝则不是。

代码注释译文：首例v0输出为(1 + -2)。drop例仅在最后一个引用时读取tag（slot 0）；Add的field为slot1@20及slot2@28，Neg为slot1@20，递归释放；Lit（tag 0）是scalar，无需释放。验证命令注释指v0与render_program的byte比较、保持三属性全ACCEPT的健全性gate、已具variant template的生成式v0==v1比较，以及无新失败（原有两项json失败无关）。仅解释历史指示，未执行。
