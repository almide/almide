原文 description 元数据：将 Value 引用计数纳入证书覆盖区域
<a id="柱c-extension-bring-value-rc-into-the-certified-region"></a>

# 支柱 C 扩展：将 Value RC 纳入证书覆盖区域

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](value-rc-cert.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](value-rc-cert.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](value-rc-cert.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/value-rc-cert.md)

> 原文注记：CERTIFIED 指建模所有权证书/checker 范围，不是外部认证或普遍内存安全。开头纠正优先于早期“语料库无 handle 来源 RC”说法，两者作为历史保留。Brick 1+2 carrier 工作与 load64 提供的嵌套 child 不同。后续 CoownLoop/CoownCompose 证明更新了旧证明 TODO，但 Rust provenance 及跨函数配对在此快照仍未完成。共享 whitelist 落地更新了早期手工 registry 后续项。计数及 kernel 检查是历史，未重跑；模型定理本身不确立 renderer 集成或所有实际程序的安全。


**状态：Brick 1+2 已落地 — carrier RC 已纳入证明（verify_ownership + ownership_certificate，不可分、一致）。2026-06-24。**

由 prim.handle 提供的 Value RC 类现在已进入证书覆盖区域：不平衡的 `rc_inc(prim.handle(v))` 是可执行 verifier 和已证明 cert 均能捕获的 Leak（`i a d` → rc>0）。corpus-wall ACCEPT 18165（字节一致），mir 502/0，verify_ownership↔cert 一致性成立，unit test 为 `value_rc_carrier_balance_is_certified`。对先前范围的纠正：语料库确有 prim.handle 提供的 rc_dec（`__drop_value` 的 `prim.rc_dec(prim.handle(v))`），但它会 ACCEPT — verifier 对 rc 0 的 RcDec 保守地作无操作处理，加上 cert 的 carrier projection，让借用参数自我消耗保持健全，由已证明的 Coq checker 验证。剩余 brick 为 #3（load64→object slot provenance），它才会真正验证 reduce_value 的 `__copy_value`（load64 提供的 child）；在此之前，load64 提供的 Value RC 仍处于差分测试下限。

<a id="why"></a>

## 原因
Value 引用计数操作（`prim.rc_inc`/`rc_dec`、`Op::DropValue`）位于 ownership checker 视为无操作的 PRIM 区域。因此 Value RC 平衡对 cert 不可见 — 只有粗粒度 leak-test（5000× 中的 OOM/trap）能捕获不平衡。这是阻止 `list.reduce_value` 的结构性 blocker：字节正确、5/7 leak-safe，但 heap payload accumulator return（Str/Array 上的 `(a,b)=>a`）泄漏；精确调整 RC 只能对着粗粒度 leak-test 试错，没有验证反馈。corpus-wall 会 ACCEPT 此类代码（对 prim RC 盲区）— 受理但不安全，仅由 leak-test 防护。

<a id="feasibility-4-agent-scope-validated-tractable-rust-first"></a>

## 可行性（4 agent 定界，已验证）：可着手，先 Rust
- **Coq 证明不是 blocker。** OwnershipChecker.v 的 Op 字母表已有 Inc/Alias（+1）与 Dec/MoveOut（−1）；`exec` 已在 rc≤0 时对 −1 报错（二次释放），结束时 rc>0 的对象为泄漏；RuntimeModel.v/WasmRcDec.v 已证明生成的 `$rc_dec` 实现抽象 RC cell 写入。RC 建模无需新引理，也无需重证 `check_line_unroll_sound`。
- **blocker 完全在不受信任的 Rust 一侧**：`verify_ownership`（lib.rs:844）和 `ownership_certificate`（certificate.rs）把 `Op::Prim{RcInc/RcDec}` 一律视为无操作，故 RC 平衡不可见。
- **可命名情形的 provenance 可恢复**：`prim.handle(v)` lowering 时 source object 在 `args[0]`（calls_p4.rs:790），因此 checker 无需重设 MIR 就能重建 `handle→object`（地址 carrier）。诚实边界：`prim.load64(raw_addr)` 提供的 RC（`__varr_copy`/`__drop_value` 循环体要素 RC，以及 `__copy_value` 的 child）仍不透明，在 slot provenance brick（类型化 slot layout / `Repr::HandleAddr`）之前留在差分测试下限。

<a id="the-bricks-land-atomically-per-the-certificate_p2rs-invariant-cert--verify_ownership-agree-on-every-case"></a>

## 各 brick（按 certificate_p2.rs 的“cert ⟺ verify_ownership 在每种情形一致”不变式不可分地落地）
1. **同时在 verify_ownership 与 ownership_certificate 引入 carrier RC。** verify_ownership（已试作、门禁验证：corpus-wall ACCEPT 18165，mir 501/0）：`Prim{Handle,dst,[src]}` → `object_of[dst]=object_of[src]`（carrier，无 RC/dead）；`RcInc(carrier)` → `rc[o]+=1`；`RcDec(carrier)` → 仅 `rc≥1` 时 release（对借用值 rc 0 的 rc_dec 仍为无操作 — 不会新增拒绝既有语料库的保守逃生口）。ownership_certificate 必须镜像此行为（RcInc(carrier) 生成 `a`，RcDec(carrier,owned) 生成 `d`），使二者一致，并由已理解 RC 的 Coq checker 验证。语料库影响：无 — 语料库 RC 操作全是 load64 提供（无 carrier），故没有新 cert 字节，`plus_one_events_backed` 不破坏，corpus-wall 字节一致 ACCEPT。
2. **plus_one_events_backed 扩展**（classify_corpus.rs:331）— 把 RcInc prim 操作计为 cert `a` 的依据，与 brick 1 的 cert 生成不可分进行（否则 `i==allocs+heap_results && a==dups` assert 会破坏）。
3. **load64→object slot provenance** — 真正的 reduce_value 修复。通过类型化 slot layout 将加载的元素 handle（`prim.load_handle(block+12+i*8)`）追溯到 block 的元素对象，使 `__copy_value`/`__varr_copy` 循环体元素 RC 可验证。这是更大的 brick（需 Repr::HandleAddr 或 slot 类型化 load）。此前 reduce_value（以及任何 load64 提供的 Value RC combinator）仍在 leak-test 下限 / wall 内。

<a id="corpus-risk-mapped"></a>

## 语料库风险（已映射）
- `plus_one_events_backed` 硬断言 `i==allocs+heap_results && a==dups` — RC 一旦为语料库 fn 生成 cert 字节就破坏。因语料库 RC 全由 load64 提供（无 carrier → 无 cert 字节）而避免。Brick 1+2 只为 prim.handle 提供的情况生成，这在语料库中尚未出现。
- `__drop_value:740` 对借用参数（rc 0）执行 `prim.rc_dec(prim.handle(v))`。保守 RcDec 规则（只建模 rc≥1）保持它为无操作 → 不会新增拒绝。（未来精确规则“借用 carrier 上的 RcDec = 获授权的自我消耗”可对其建模，但 brick 1 不需要。）

<a id="prototyped-diff-reverted-librs-verify_ownership-prim-arm"></a>

## 原型差异（已回退，lib.rs verify_ownership 的 Prim arm）
将 `| Op::Prim { .. }` 无操作拆为 `Op::Prim { kind, dst, args } => match kind { Handle => carrier; RcInc => rc+=1 if carrier; RcDec => rc-=1 if carrier && rc≥1; _ => {} }`。已通过门禁：cargo test -p almide-mir 501/0，corpus-wall ACCEPT 18165（字节一致）。对语料库这是严格 refinement，但对 prim.handle 提供的 RC 引入 verify_ownership↔cert 分歧，因此必须与匹配的 ownership_certificate 生成一起落地（brick 1+2 不可分），绝不能单独落地。

<a id="brick-3-load64-fed-nested-element-rc--foundation-landed-the-core-composition-lemma-is-proven-proofscoownloopv-2026-06-25"></a>

## Brick 3（load64 提供的嵌套元素 RC）— 基础已落地：核心组合引理已证明（proofs/CoownLoop.v，2026-06-25）

概念上最难的部分已完成：`proofs/CoownLoop.v` 在 Coq kernel 上证明了 OwnershipLoop.v 明确排除的共同所有 COPY / 递归 DROP 平衡（axiom-clean、coqchk 复核、在 check.sh proof gate 内）— 将 container 直接元素建模为 refcount 向量；producer 共同所有 FILL（每个 +1，即 rc_inc）后接 container 递归 DROP（每个 −1，<=0 时 fault），把每个 source 元素恢复为原 RC（`coown_fill_drop_neutral`），对任意元素数给出 `coown_copy_no_double_free` + `coown_copy_no_leak`。这是按元素数而非逐迭代配对、net +1 被另一个递归 drop 平衡的情况。剩余（engineering）：按 container identity 将具体 producer 的 fill event 与匹配递归 drop 配对的 MIR/cert 集成（类型化嵌套元素模型 + cert section），使 __copy_value/__varr_copy/value.merge 继承已证明平衡，而非依赖 leak 下限。下面保留原范围供集成设计使用：

<a id="original-scope--the-integration-that-consumes-coownloopv-still-fresh-engineering"></a>

### 原范围 — 使用 CoownLoop.v 的集成（仍需新工程）

不存在小而健全的 cert 证明 slice。“结构上归约到已证明 map-fill”想法错误：map-fill 每次迭代体 net 0（rebind：drop 旧 + inc 新），这正是 OwnershipLoop.v 保持 RC 的 Loop 规则能接受它的原因；共同所有 copy loop 每元素 net +1，而 OwnershipLoop.v（header 第19–21行）明确推迟 / 排除嵌套共同所有。平衡 rc_dec 位于另一个 raw-handle 递归 routine（__vdrop_arr/__drop_value），其 cert 为空，因此单函数 fold 无法见证平衡。

真正 Brick 3 需要（全部新增，多周）：
1. 类型化嵌套元素 MIR 模型 — 将 load64 提供的元素 handle 从未追踪 raw i64 提升为已追踪但嵌套的类（带 `ElementOf(container)` 关系的 Op），使元素 e 的 rc_inc 记入 container child account，而非 ghost rc[o]=0。
2. 按 container identity + 元素数，将 producer 累积 child inc 与 recursive-drop child dec 配对的 cert section（跨 producer loop 与单独 drop loop 的全局性质，不是逐迭代）。
3. Coq 开发 `CoownLoop.v`（对 container shape 良基归纳 — List[List[String]] 为2层，leaf 数 ≠ intermediate 数）+ RuntimeModel block 模型（container length field + 正好发出 N 个 child dec 的 rt_recursive_drop）+ 组合引理 SUM(co-own incs) == SUM(recursive-drop decs) + 让新 cert section 贯穿 Extract.v + driver.ml。

不可分：没有共享嵌套元素模型 + 组合引理，就无法仅 cert 证明一个 producer（reduce_value 的 __copy_value）。应作为专门的新形式化 brick 排期。

<a id="containment-hardening-the-tractable-near-term-2026-06-24"></a>

## 围堵强化（近期可着手部分，2026-06-24）
信任下限完好：load64 提供的 RC 是未建模的无操作，由 rc_inc/rc_dec whitelist（calls_p4.rs — 只有具名受信任 producer/consumer 能命名 rc_inc/rc_dec；其他 .almd fn 调用则 lowering 拒绝）+ leak-loop fixtures（value_array、value_as_array、list_set_value、list_sort_str、value_merge，及此处新增 value_object）门禁。可选进一步强化（暂缓、价值低）：将手工 whitelist 注册表化（GOTCHA 3 — 通过手改 match arm 增长）。

<a id="brick-3-integration--the-precise-fresh-engineering-spec-consumes-coownloopv-2026-06-25"></a>

## Brick 3 集成 — 精确的新工程规格（使用 CoownLoop.v，2026-06-25）

平衡跨函数（producer `__varr_copy` 填 N 个 child inc；平衡用 N 个 child dec 在独立的 `__drop_value`），所以任何逐函数 checker 改动都不行 — 只在 `__varr_copy` 内建模 child RC 会误拒绝（dec 在别处）。CoownLoop.v 证明原理；集成必须按 container 配对两侧。设计：

1. **类型化嵌套元素模型（MIR/lib.rs）** — 从已追踪 container 的元素偏移加载 `prim.load64(container + 12 + i*8)`，产生 `ElementOf(container)` handle（不是任意地址的 ghost rc[o]=0）。这就是 carrier RC brick 对 load64 缺失的 provenance。
2. **共同所有 producer 识别** — 在 `0..len(container)` 循环中对每个 `ElementOf(dst)` 做 rc_inc 并存入 `dst` 的 fn，发出一个抽象 `ContainerFill dst` event（N 个 child inc，N = len）。whitelist（calls_p4.rs）成为识别集合。
3. **递归 drop 识别** — 在 `0..len(v)` 循环中对每个 `ElementOf(v)` 做 rc_dec 的 `__drop_value`/`__vdrop_arr`/`__vdrop_obj` 发出 `ContainerDrop v`（N 个 child dec）。
4. **cert section + 配对** — 在调用方，container（value.array/object/copy result）被 alloc（`i`、block）+ fill（`ContainerFill`），随后 drop（`DropValue` = block `d` + `ContainerDrop`）。已证明 checker 按 container identity 配对 `ContainerFill c` 与 `ContainerDrop c`；CoownLoop.v 给出任意 N 的逐元素平衡。
5. **组合定理（Coq）** — BLOCK 平衡（OwnershipChecker.v i/d）且 CHILD 平衡（CoownLoop.v fill/drop）的 container 完全无泄漏 / 二次释放。这是组合两模型的一个新引理，其余全部为 Rust（ElementOf provenance + 两个识别器 + cert section）。有它，__copy_value/__varr_copy/value.merge 即可由 cert 证明，RC whitelist（calls_p4.rs）退役。

此前：whitelist + leak 下限 fixture 是受信任门禁，现在以 CoownLoop.v 为依据（calls_p4.rs comment）— 名称当且仅当遵循已证明共同所有 / 递归 drop 模式时属于该集合。

<a id="brick-3-integration--coq-half-done-rust-half-confirmed-multi-week-2026-06-25"></a>

## Brick 3 集成 — Coq 半边已完成，Rust 半边确认需多周（2026-06-25）

组合半边已落地并经 kernel 检查：proofs/CoownCompose.v 的 `lifecycle_safe`（block-balanced ∧ children-source-owned ⇒ fully safe）组合 OwnershipChecker.v（block）与 CoownLoop.v（child），已在 proof gate 内。

通过对抗性验证为 ELEMENTOF（Rust cert section）半边定界，反驳了所期待的小 slice：
- “严格 refinement 可沿用既有 block i/d 配对（无 corpus 回归）”主张为假：共同所有 producer 的 fill（`__varr_copy`）与其平衡递归 drop（`__vdrop_arr`/`__drop_value`）处于不同函数，所以 child account 真正跨函数，逐函数 cert refinement 无法捕获。
- 它不能捕获 reduce_value 的 str-a/arr-a 泄漏：该泄漏是 load64 提供的 child 释放不足（递归 drop 确实存在并运行；不平衡是跨函数、以元素数为键且由 load64 提供的计数，故对 block account 和 carrier RC brick 均不可见）。捕获它需要完整 load64→ElementOf(container,index) provenance + 两函数间 ContainerFill↔ContainerDrop 配对 — 已确认是需多周的新工程，单独追踪。

当时可做（已落地）：真正的信任锚点 RC whitelist 提升为一个共享 source（crates/almide-mir/src/coown_names.rs：COOWN_PRODUCERS / COOWN_DROPS / COOWN_SET_REPLACE），以 CoownLoop.v + CoownCompose.v 为依据，由 calls_p4.rs 使用，通过 `coown_names_documented` test 固定，避免 producer/consumer 两半静默漂移。corpus-wall ACCEPT（保持行为），mir 503/0。
