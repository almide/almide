原文 description 元数据：拟议 counted-loop witness 与分配次数上限；target 特定的物理 WCET 仍是独立义务
<a id="flight-keystone-あ--proposed-counted-loops-and-allocation-bounds"></a>

# 飞行基石（あ）— 拟议 counted loop 与分配上限

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](flight-wcet-loops.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](flight-wcet-loops.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](flight-wcet-loops.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md)

> 原文注记：独立 counted-loop witness 及新证明/checker 文件为提案，不是已交付功能或已检查证明。当前 bounded 接纳不确立该提案、witness 与执行对应、byte high-water mark 或物理 WCET。Coq sketch 含占位符、未定义 helper，未编译。文本列三个已有 emitter 却称新增为第三，保留而不推断实现。声称 witness n 等于 emitted bound 能捕捉夸大 B，不能仅由该等式确立：actual ≤ B 允许更大 B，所有对应义务仍在。抽象 count 界也不自动界定递归 drop、runtime 分配或物理时间。区分 count 与 heap traffic（Dup）、精确迭代与早退、无界递归、静态 byte 大小和 DynStr 长度界。历史 DO-178C 表述是原提案，不是独立验证监管要求或资格化。未执行代码、证明、fixture、gate、测量。

当前 bounded-profile 接纳由 [ALS §B](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md) 规定。下方独立 counted-loop witness 与证明 sketch 是提案，不主张其命名文件/checker 已交付。事件次数上限本身不是目标机器经过时间的上限。

> **目标**：[flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) 阶梯 gate **G-F1**（飞行子集定义+强制）及 **G-F2**（将循环提升到 Coq、证明分配上限）的具体实现设计。核心是研究循环静态迭代数与分配事件数对应。仅 allocator 渐近复杂度不能确定 target 物理 WCET。原设计引用的 Coq 模型当前为 loop-free 片段（`proofs/Termination.v:16-18`），把它提升为**独立结构 witness**（不破坏 RC flat-fold 不变量），用与 `Subset.v` 同构的2个性质证明“分配 count 静态有界”。
> **相关**：[flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §3 / [certificate-format-v1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certificate-format-v1.md)。

---

<a id="1-設計核一行"></a>

## 1. 设计核心（一行）

> **将 counted loop（迭代数为静态常数）提升为 RC stream 之外的结构 witness，以与 `Subset.v` 同构的 `NoAllocInLoop` / `AllocBound` 两性质证明“循环体无 alloc ⟹ 总分配 count 为与循环无关的常数”。`try_lower_scalar_for_range` 已知迭代数（当前丢弃）。最小 brick 用纯 Coq 证明核心主张。**

为何独立 witness？给 `OwnershipChecker.Op`（`proofs/OwnershipChecker.v:48-53`）加 `Loop` 构造器，会使消费 `exec` flat-fold（`:63`）的全部证明（`RuntimeModel.mrun_tracks_exec`、`Termination.fuel_exec`）必须处理 Loop，破坏“v0 i/d 是退化情况、零新增证明义务”的不变量（`OwnershipChecker.v:34-36`）。如 `Subset.v` 为独立于 `check` 的函数，counted loop 采用**独立 witness + 独立检查函数**（现有3性质3 witness 模式，`gate.sh`）。

---

<a id="2-g-f1--飛行サブセットの定義と強制"></a>

## 2. G-F1 — 飞行子集定义与强制

<a id="21-counted-loop-の形rust-側-op-拡張"></a>

### 2.1 counted-loop 形态（Rust 侧 `Op` 扩展）

当前循环 `LoopStart … LoopBreakUnless { cond: ValueId } … LoopEnd`（`crates/almide-mir/src/lib.rs:248-252`）以**任意 runtime Bool**决定 break，迭代数未知。counted loop **把迭代上限作为静态常数载于 marker**。不破坏现有 marker（保留通用 while），另添并列受限家族：

```rust
/// 飛行サブセットの COUNTED ループ: 静的既知回数 `bound` だけ回る(リテラル境界の
/// `for i in a..b` から lowering が導出)。LoopBreakUnless{cond}(任意ランタイム exit)
/// と違い trip count をここに定数で持つので WCET・総 alloc 上限が導ける。本体は
/// CountedLoopEnd まで = 1 イテレーション、既存マーカーと同じ per-iteration balanced。
CountedLoopStart { bound: u64 },
/// counted ループを閉じ back-edge を張る。LoopBreakUnless は持たない ── exit は
/// カウンタが bound に達することによる(renderer が down-counter br_if を出す。
/// データ依存テストではない)。
CountedLoopEnd,
```

保留 Rust 文档注释的全文译文：飞行子集 COUNTED 循环运行静态已知 bound 次，由 lowering 从 literal 边界的 for i in a..b 推导。不同于 LoopBreakUnless{cond} 的任意 runtime exit，此处持有常数 trip count，故可导出 WCET/总 alloc 上限。本体直至 CountedLoopEnd 为1次迭代，与既有 marker 一样每次迭代 balanced。CountedLoopEnd 关闭循环并连 back-edge，不含 LoopBreakUnless；counter 达到 bound 时退出，renderer 发出 down-counter br_if，不是数据依赖测试。这是原提案注释，不是物理 WCET 证明。

纯添加，所以 `verify_ownership`（`lib.rs:548-551`）、`ownership_certificate`（`certificate.rs:377`）、`wasm_pattern`（`translation_validation.rs:92-95`）仅在既有循环 marker 旁增加2个 no-op arm，**既有证明/witness 不变**。

<a id="22-3つのルール"></a>

### 2.2 三条规则

1. **no-alloc-in-loop**（op 序列上可判定谓词）。`is_allocating(op)` = 产生新拥有 heap 的 op：`Op::Alloc` 及返回 heap 的 `Call`/`CallFn`。**恰好是 cert 已输出 `'i'` 的集合**（`certificate.rs:343-345,368-374`），所以 G-F2 无需新抽象即可证明。飞行规则：`CountedLoopStart`～`CountedLoopEnd` 间没有 `is_allocating` op。
2. **bounded-static-alloc**（WCET 相关汇总）。`total_allocs ≤ (ループ外 alloc) +
   Σ_loops (bound_L × 本体 alloc_L)`，即循环外分配加各循环 bound × 本体分配之和。按规则1，本体 alloc=0 时 `total = ループ外 alloc`（总数等于循环外分配），是与 runtime 值无关的常数。
3. **allocator 侧**：固定64KiB/单探测是旧 renderer 前提；当前 structural allocator 参见 `crates/almide-wasm/src/alloc_inline.rs` 的 size-class/free-list。要将 `total_allocs × max_block` 用作 byte 上限，还需最大 block 大小、复用、临时分配条件，次数本身不够。实机 WCET 需另行评估目标 CPU/host/runtime 时间。

<a id="23-強制の二層ラダー表-flight-profileflight-profilemd-72-と整合"></a>
<a id="23-強制の二層ラダー表-flight-profile-72-と整合"></a>

### 2.3 两层强制（与 [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §7.2 阶梯表一致）

1. **lowering gate（compiler 侧、未证明、最廉价 wall）**：`try_lower_scalar_for_range`（`crates/almide-mir/src/lower/control.rs:766`）中若 `end` 也是 literal，则计算静态 bound，发出 `CountedLoopStart { bound }`。在 `scalar_loop_depth > 0`（`control.rs:816`）下 lower 本体时拒绝 `is_allocating` op，是既有 heap 重赋值 wall（`lower/mod.rs:278-292`）的兄弟。拒绝时 rollback 至健全 model-one-iteration 形态（非飞行程序仍可编译，只在飞行子集外）。
2. **checker 性质（已证明、每次构建复验）**：G-F2 的 `NoAllocInLoop`。lowering gate 只是建议，**约束性强制来自已证明 checker**，即使 gate 有 bug 也兜底。

“子集不是弱点，而是强项”（[flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §3.5）：counted-range + no-alloc-in-loop 就是 DO-178C 的 C 规则（禁递归、禁动态分配、循环有界），新意是**用证书而非 review 机器强制**。

---

<a id="3-g-f2--ループを-coq-に持ち上げ上限を証明"></a>

## 3. G-F2 — 将循环提升到 Coq、证明上限

<a id="31-新しい構造-inductiveownershipcheckerop-は触らない"></a>

### 3.1 新结构 inductive（不动 `OwnershipChecker.Op`）

```coq
(* proofs/CountedLoop.v — 飛行ループ断片の構造モデル *)
Inductive LOp : Type :=
  | LAlloc : LOp                       (* 新規確保(cert 'i') *)
  | LOther : LOp                       (* 非確保 op *)
  | LLoop  : nat -> list LOp -> LOp.   (* counted loop: bound, body *)

(* 総和インタプリタ: 構造再帰。bound が整礎測度なので fuel 不要。*)
Fixpoint lrun (prog : list LOp) (acc : nat) : nat :=
  match prog with
  | [] => acc
  | LAlloc :: rest => lrun rest (S acc)
  | LOther :: rest => lrun rest acc
  | LLoop n body :: rest => lrun rest (acc + n * loop_body_allocs body)
  end.
```

保留 Coq 注释的全文说明：proofs/CountedLoop.v 是飞行循环片段的结构模型。LAlloc 为新分配（cert 'i'），LOther 为非分配 op，LLoop 为 counted loop（bound、body）。求和解释器为结构递归；注释称 bound 是良基度量，故无需 fuel。保留 sketch 及其 helper 引用。

`LLoop n body` =“严格执行 body n次”。结构化，所以可用结构递归或 fuel 证明。

<a id="32-停止性拡張terminationv-のアナログ"></a>

### 3.2 终止性扩展（`Termination.v` 对应物）

`Termination.v:45-56` 证明 flat 片段只需 `length ops` fuel。`LOp` 的终止性由 **`LOp` 树结构递归 + 每循环 trip-counter**、Gallina 全域性立即得到。核心不是“停不停”，而是 **step 上限是静态 trip count**：

```coq
(* step_bound prog = 非ループ op + Σ_loops n * step_bound(body)。
   この step 数で必ず足りる = どの LLoop も発散しない。*)
Theorem counted_loop_steps_bounded :
  forall prog, fuel_lrun (step_bound prog) prog <> None.
```

保留注释的译文：step_bound prog = 非循环 op + Σ_loops n × step_bound(body)。该步数必定足够，即任何 LLoop 都不发散。

**证明策略：对 `prog` 结构归纳；`LLoop` arm 以 trip-count `n` 良基。** `n` 有限，每迭代消耗 `n` 中的1。**`n` 是 ranking 函数，正是 counted 限制使终止性可证的原因**（任意 `LoopBreakUnless{cond}` 无此度量）。

<a id="33-2つの新性質ファイルsubsetv-同型定理文"></a>

### 3.3 两个新性质文件（与 `Subset.v` 同构，定理陈述）

```coq
(* proofs/NoAllocInLoop.v — ループ本体に確保 op 無し *)
Definition not_alloc (o : LOp) : bool := match o with LAlloc => false | _ => true end.
Fixpoint no_alloc_in_loops (prog : list LOp) : bool := (* 各 LLoop body に forallb not_alloc + 入れ子再帰 *)
Theorem no_alloc_in_loops_sound :
  forall prog, no_alloc_in_loops prog = true -> AllocFreeLoops prog.
(* 戦略: prog 構造帰納 + forallb_forall(Subset.v:35 の手) ── subset_check_sound と機械的に同型 *)

(* proofs/AllocBound.v — 総確保 count ≤ 静的上限 *)
Definition total_allocs (prog : list LOp) : nat := lrun prog 0.
Definition alloc_bounded (prog : list LOp) (B : nat) : bool := Nat.leb (total_allocs prog) B.
Theorem alloc_bounded_sound :
  forall prog B, alloc_bounded prog B = true -> total_allocs prog <= B.
Proof. intros prog B H. apply Nat.leb_le. exact H. Qed.

(* キーストーン系: NoAllocInLoop を合成 ── ループ alloc-free なら総和はループ外 alloc
   count = trip count 非依存の定数。これが WCET 関連の主張「確保有界 BY CONSTRUCTION」。*)
Corollary alloc_free_loops_bound_is_constant :
  forall prog, no_alloc_in_loops prog = true ->
    total_allocs prog = count_top_level_allocs prog.
(* 戦略: prog 帰納、LLoop n body アームで loop_body_allocs body = 0 を書き換え n*0=0。
   ここで2ファイルが合成する。*)
```

保留 Coq 注释的全文说明：proofs/NoAllocInLoop.v 表示循环体无分配 op；对各 LLoop body 做 forallb not_alloc 与嵌套递归。策略是 prog 结构归纳 + forallb_forall（Subset.v:35 做法），与 subset_check_sound 机械同构。proofs/AllocBound.v 表示总分配 count ≤ 静态上限。基石推论组合 NoAllocInLoop：循环 alloc-free，则总和等于循环外 alloc count，为与 trip count 无关的常数。这是 WCET 相关主张“分配按构造有界”。策略为 prog 归纳，在 LLoop n body arm 改写 loop_body_allocs body = 0，于是 n*0=0；两个文件在此组合。代码及占位符原样保留，不是新检查的证明。

`alloc_bounded` 是自明 `leb`，健全性为1行 `Nat.leb_le`（与 `subset_check_sound` 同构）。**实质在推论**：`NoAllocInLoop` ⟹ 总和与循环无关 ⟹ 静态上限精确。

<a id="34-証明書-witness-拡張certificaters"></a>

### 3.4 证书 witness 扩展（`certificate.rs`）

第三个 witness emitter（与 `ownership_certificate` / `name_witness` / `cap_witness` 并列）：

```rust
/// counted-loop / alloc-bound witness: flatten した LOp 木 + 主張する静的 alloc 上限 B。
/// 形式(Subset 風・内部化可能): "<B>|<flattened LOp>"
///   'A'=LAlloc, 'o'=LOther, 'L<n>{ … }'=LLoop(trip n + 括弧本体)
pub fn alloc_bound_witness_string(func: &MirFunction) -> String { ... }
```

保留 Rust 注释的全文译文：counted-loop / alloc-bound witness，为扁平化 LOp 树 + 所主张静态 alloc 上限 B。格式（Subset 式、可内部化）："&lt;B&gt;|&lt;flattened LOp&gt;"。'A'=LAlloc，'o'=LOther，'L&lt;n&gt;{ … }'=LLoop（trip n + 花括号本体）。

遍历 `func.ops`，在 `CountedLoopStart{bound}` 打开 `L<bound>{`，直到 `CountedLoopEnd` 写入本体（`is_allocating`→`A`，其余→`o`）再闭合。主张 `B` = compiler 静态总和。checker 用 `lrun` 重导实际总和并验证 `actual ≤ B`。driver 添 `noalloc`/`allocbound` mode，`Extract.v` 抽取2个 `check_*_cert`（与 caps 同法）。

<a id="35-サイズ不変条件最重要規律"></a>

### 3.5 大小不变量（最重要纪律）

纪律 = **checker 规模 ∝ #event字符 + #subset性质 + #op→pattern，与程序复杂度无关**：

- `NoAllocInLoop`/`AllocBound` 为 `Subset.v` 形（`forallb` + `leb`，单步健全性）。新字母表3 token（`LAlloc`/`LOther`/`LLoop`）+ subset 式2性质，在 checker 内均为**常数大小**。
- **compiler 做工作，checker 只做 subset**。静态上限 `B`、trip count `n` 由**不受信任 lowering 计算**（`try_lower_scalar_for_range` 已持有）并放入 witness。checker 仅 `lrun` fold + `leb`，不推导上限，不打开 callee（与 transitive-caps 设计同一手段）。
- **无 CFG walk**。RC `exec` fold 保持 flat 不变，循环结构位于独立 `LOp` witness。

→ 两性质为2个 `Subset.v` 规模小文件（每个约~60-90行，大半是例子+ `Print Assumptions`）。**纪律得以保持。**

<a id="36-正直なサイズギャップcount-先byte-後"></a>

### 3.6 诚实的大小缺口（先 count，后 byte）

RC 字母表丢弃 byte 大小（`lib.rs:130`：“Alloc 不论内容只有一个 'i'”）：

- **按次数提案**（§3.3）：`total_allocs ≤ B`（分配事件数）。只有确立 witness 与执行对应后，才可作为执行上限。本身不证明 byte high-water-mark、递归 drop 时间或目标机器物理 WCET。
- **按 byte 还需额外内容**：witness 携带每个 `Alloc` byte 大小（来自 `Repr`/`LayoutId` + `Init`），将 `LOp` 精化为 `LAlloc (size:nat)`，fold `Σ size`。难点为 `Init::DynStr { len: ValueId }`（`lib.rs:137`）= **runtime 长度**，byte 上限需给 `len` 本身设界（第二个 counted 量）。**先 count；byte 仅静态 `Init` 大小；`DynStr` 在 `len` 证明有界前 wall。**

---

<a id="4-de-risking-順最小ブリック先"></a>

## 4. 降低风险顺序（最小 brick 先）

**Brick 0（证明核心主张的最小切片）**：*“no-alloc 本体的 counted loop 分配 count 有界。”* 纯 Coq，不改 compiler：编写 `proofs/CountedLoop.v`（`LOp` + `lrun` + `count_top_level_allocs`）+ `AllocBound.v`（推论 `alloc_free_loops_bound_is_constant`）+ `NoAllocInLoop.v`（`no_alloc_in_loops_sound`），加入 `_CoqProject`（`CowSafety.v` 后）。用手写 `LOp` 例（`LLoop 1000 [LOther]` 的 `total_allocs` = 循环外 alloc）证明性质，`Print Assumptions` closed。**降低最大未知风险：结构 `LOp` 模型能否在无 CFG walk 下允许 `Subset.v` 式健全性？** 若可以（1 `Nat.leb_le` + 1归纳），原则上证明完成。终止性半边也同时完成（`counted_loop_steps_bounded`）。

**Brick 1（绑定 compiler）**：添 `Op::CountedLoopStart/End` + 4个 fold 的 no-op arm；在 `try_lower_scalar_for_range` 的 `end` 为 literal 时 emit + no-alloc 本体 wall；`alloc_bound_witness_string` + driver mode + `Extract.v` + `gate.sh` 行，用真实 `.almd` fixture（`for i in 0..1000 { println(...) }` accept，本体有 `Alloc` 则 reject）。**端到端真实切片。**

**Brick 2（一般上限）**：允许本体 alloc，证明 `total ≤ Σ bound × per-iter`，把 `end` 放宽为 const-foldable。

**Brick 3（按 byte）**：精化 `LAlloc (size:nat)`，仅静态 `Init` 大小，`DynStr` wall。

---

<a id="5-正直な難所--open-question"></a>

## 5. 诚实难点 / 开放问题

1. **嵌套循环**：`lrun` 已递归处理 `LLoop`（模型允许嵌套）。但 lowering 的 `scalar_loop_depth` 是 flat counter，嵌套 `CountedLoopStart` bound 合成（外 × 内）及 `try_lower_scalar_for_range` 一次一范围处理是 **emission 侧工作**。Coq 侧就绪。
2. **循环内 `Dup`（`'a'` 问题）**：规则1针对新分配（`'i'`）。本体 `Dup` inc 已有对象 refcount，不增 heap **对象数**，但有每次迭代 RC traffic。若严格“循环内无 heap traffic”，应在 `is_allocating` 纳入 `Dup`；若“alloc-count 上限”，则不纳入。**哪个是飞行规则，是须刻意记录的选择**（阶梯为 no-alloc-in-loop = count，最小 brick 排除）。
3. **`break`/`continue` vs 精确上限**：counted 子集禁止早退，`bound` 精确。允许条件早退，则静态上限**仅为上界**（足以用于 WCET，保持 `AllocBound` 的 `≤ B`）。**精确（禁 break）还是上界（允许 break、弱化为 `≤`）未定**。证明可支持两者，最小为精确。
4. **递归 vs 循环**：本设计关闭循环，**递归仍无界且超范围**。自托管 runtime fn 递归（`CallFn` 环，由 `reachable_caps` 的 `visited` 检测）无 trip 上限。飞行 profile **全面 wall 递归**（DO-178C C 规则），需独立简单 gate（拒绝飞行函数 `CallFn` back-edge），此处未解决。
5. **byte 大小缺口**（重述 §3.6）：count 现可诚实证明，byte 为静态 `Init` 大小 + `DynStr` wall。有界循环内，动态长度分配若无 `len` 界，byte footprint 仍无界，是真正余项。
6. **破坏大小不变量的 tripwire**：若未来 brick 让 checker **推导上限**而非复检 B，或从 flat marker 重建 CFG 后 fold `LOp`，就破坏纪律（checker 随程序复杂度增长）。**guard = witness 同携结构（`L<n>{…}`）和主张 `B`，checker 仅 fold + 比较**。用 `bound_in_witness_matches_emitted_loop` 测试固定“witness 的 `n` = `CountedLoopStart` 的 `bound`”（捕捉夸大 B 的 compiler）。

---

<a id="6-接合点まとめfileline"></a>

## 6. 接合点汇总（file:line）

| 关注点 | file:line | 变化 |
|---|---|---|
| counted-loop `Op` | `crates/almide-mir/src/lib.rs:248` | 添 `CountedLoopStart{bound:u64}`/`CountedLoopEnd` |
| no-op fold arm | `lib.rs:548-551`, `certificate.rs:377`, `translation_validation.rs:92-95` | 各2 arm |
| bound 推导 + no-alloc wall | `lower/control.rs:766,816`, `lower/mod.rs:278` | emit counted marker，拒绝本体 `is_allocating` |
| 新 witness emitter | `certificate.rs`（`cap_witness_string` 后） | `alloc_bound_witness_string` |
| Coq 终止性 | 以 `Termination.v:27-56` 为模型 | 新 `proofs/CountedLoop.v` 的 `lrun` + `counted_loop_steps_bounded` |
| Coq 性质1 | 以 `Subset.v:22-38` 为模型 | 新 `proofs/NoAllocInLoop.v` |
| Coq 性质2 | `Subset.v` + `Nat.leb_le` | 新 `proofs/AllocBound.v` + 推论 |
| build/extract/gate | `_CoqProject:17`, `Extract.v:17`, `driver.ml:20-26`, `gate.sh:78` | 注册、抽取、mode、真实 source 行 |

**基石总结**：*添加 flat RC stream 之外的结构 counted-loop witness，在其上证明2个 `Subset.v` 式性质，让已知 trip count 的 `try_lower_scalar_for_range` 携带 bound。最小 brick（Brick 0）用纯 Coq 在改 compiler 前证明核心主张。*
