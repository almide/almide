Original description metadata: Proposed counted-loop witnesses and allocation-count bounds; target-specific physical WCET remains a separate obligation
<a id="flight-keystone-あ--proposed-counted-loops-and-allocation-bounds"></a>

# Flight Keystone (あ) — proposed counted loops and allocation bounds

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](flight-wcet-loops.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](flight-wcet-loops.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](flight-wcet-loops.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md)

> Source caveat: separate counted-loop witnesses and named new proof/checker files are proposals, not delivered features or checked proofs. Existing bounded admission does not establish this proposal, witness-to-execution correspondence, byte high-water marks or physical WCET. Coq sketches contain placeholders and undefined helpers; they were not compiled. The text calls an emitter third while listing three existing emitters; preserve that wording rather than infer implementation. Its claim that matching witness n to emitted bound catches inflated B is not established by that equality alone: checking actual ≤ B permits a larger B, and all correspondence obligations remain. Likewise abstract count bounds do not automatically bound recursive drops, runtime allocation or physical time. Keep the distinction between count and heap traffic (Dup), exact trips versus early exits, unbounded recursion, static byte sizes and DynStr length bounds. Historical DO-178C descriptions are source proposals, not independently verified regulatory requirements or qualification. No code, proof, fixture, gate or measurement was executed.

Current bounded-profile admission is specified in [ALS §B](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md).
The separate counted-loop witness and proof sketches below are proposals, not
a claim that their named files/checker already ship. An event-count bound is
not by itself a bound on elapsed time on a target machine.

> **Goal**: concrete implementation design for [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) ladder gates **G-F1** (flight-subset definition + enforcement) and **G-F2** (lift loops into Coq and prove allocation bounds). Core: investigate correspondence between static loop iteration counts and allocation-event counts. Allocator asymptotic complexity alone does not determine target physical WCET. The Coq model referenced by the original design is currently a loop-free fragment (`proofs/Termination.v:16-18`). Lift it as **a separate structural witness** (without breaking RC flat-fold invariants), proving “allocation count is statically bounded” through 2 properties isomorphic to `Subset.v`.
> **Related**: [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §3 / [certificate-format-v1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certificate-format-v1.md).

---

<a id="1-設計核一行"></a>

## 1. Design core (one line)

> **Lift counted loops (statically constant iteration counts) into a structural witness separate from the RC stream. With 2 properties isomorphic to `Subset.v`, `NoAllocInLoop` / `AllocBound`, prove “no alloc in loop bodies ⟹ total allocation count is a loop-independent constant.” `try_lower_scalar_for_range` already knows iteration counts (currently discards them). The smallest brick proves the central claim in pure Coq.**

Why a separate witness? Adding a `Loop` constructor to `OwnershipChecker.Op` (`proofs/OwnershipChecker.v:48-53`) forces every proof consuming `exec`'s flat fold (`:63`) — `RuntimeModel.mrun_tracks_exec`, `Termination.fuel_exec` — to handle Loop, breaking the invariant “v0 i/d is the degenerate case, zero new proof obligations” (`OwnershipChecker.v:34-36`). Like `Subset.v`, a separate function independent of `check`, counted loops use **an independent witness + independent checking function** (existing 3 properties/3 witnesses pattern, `gate.sh`).

---

<a id="2-g-f1--飛行サブセットの定義と強制"></a>

## 2. G-F1 — flight-subset definition and enforcement

<a id="21-counted-loop-の形rust-側-op-拡張"></a>

### 2.1 Counted-loop shape (Rust-side `Op` extension)

Current loop `LoopStart … LoopBreakUnless { cond: ValueId } … LoopEnd` (`crates/almide-mir/src/lib.rs:248-252`) breaks on **an arbitrary runtime Bool**, so iteration count is unknown. A counted loop **puts the iteration bound on its marker as a static constant**. Keep existing markers intact (preserve general while) and add a parallel restricted family:

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

Full translation of the retained Rust documentation comments: a flight-subset COUNTED loop executes the statically known number bound, derived by lowering from literal-bounded for i in a..b. Unlike LoopBreakUnless{cond} (arbitrary runtime exit), carrying the constant trip count here allows WCET/total-allocation bounds to be derived. The body through CountedLoopEnd is 1 iteration, balanced per iteration like existing markers. CountedLoopEnd closes the loop and creates the back-edge. There is no LoopBreakUnless: exit occurs when the counter reaches bound; the renderer emits down-counter br_if, not a data-dependent test. These are the original proposal's comments, not a physical-WCET proof.

Purely additive: `verify_ownership` (`lib.rs:548-551`), `ownership_certificate` (`certificate.rs:377`) and `wasm_pattern` (`translation_validation.rs:92-95`) only gain 2 no-op arms beside existing loop markers — **existing proofs/witnesses unchanged**.

<a id="22-3つのルール"></a>

### 2.2 Three rules

1. **No-alloc-in-loop** (decidable predicate over op sequences). `is_allocating(op)` = ops producing a newly owned heap: `Op::Alloc` and heap-returning `Call`/`CallFn`. **Exactly the set for which cert already emits `'i'`** (`certificate.rs:343-345,368-374`), so G-F2 is provable without new abstraction. Flight rule = no `is_allocating` op between `CountedLoopStart` and `CountedLoopEnd`.
2. **Bounded-static-alloc** (WCET-related aggregation). `total_allocs ≤ (ループ外 alloc) +
   Σ_loops (bound_L × 本体 alloc_L)`. In words: allocations outside loops plus the sum of each loop's bound times its body's allocations. If rule 1 gives body alloc=0, `total = ループ外 alloc` (total equals allocations outside loops), a constant independent of runtime values.
3. **Allocator side**. Fixed 64KiB/single-probe was an old renderer assumption; for the current structural allocator, see size classes/free lists in `crates/almide-wasm/src/alloc_inline.rs`. Using `total_allocs × max_block` as a byte bound also requires conditions on maximum block size, reuse and temporary allocations; allocation count alone is insufficient. Physical WCET requires separate timing assessment for target CPU/host/runtime.

<a id="23-強制の二層ラダー表-flight-profileflight-profilemd-72-と整合"></a>
<a id="23-強制の二層ラダー表-flight-profile-72-と整合"></a>

### 2.3 Two enforcement layers (consistent with [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §7.2 ladder table)

1. **Lowering gate (compiler-side, unproved, cheapest wall)**: in `try_lower_scalar_for_range` (`crates/almide-mir/src/lower/control.rs:766`), if `end` is also literal, compute static bound and emit `CountedLoopStart { bound }`. While lowering body under `scalar_loop_depth > 0` (`control.rs:816`), reject `is_allocating` ops, a sibling of the existing heap-reassignment wall (`lower/mod.rs:278-292`). On rejection, rollback to the sound model-one-iteration form (nonflight programs still compile; simply outside flight subset).
2. **Checker property (proved, revalidated every build)**: G-F2's `NoAllocInLoop`. The lowering gate advises; **binding enforcement is the proved checker**, a backstop even if the gate has a bug.

“Subset = strength, not weakness” ([flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §3.5): counted-range + no-alloc-in-loop are precisely DO-178C C rules (no recursion, no dynamic allocation, bounded loops); novelty is **machine enforcement by certificates rather than review**.

---

<a id="3-g-f2--ループを-coq-に持ち上げ上限を証明"></a>

## 3. G-F2 — lift loops into Coq and prove bounds

<a id="31-新しい構造-inductiveownershipcheckerop-は触らない"></a>

### 3.1 New structural inductive (leave `OwnershipChecker.Op` untouched)

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

Full translation of the retained Coq comments: proofs/CountedLoop.v models the structure of the flight-loop fragment. LAlloc is a new allocation (cert 'i'); LOther a nonallocating op; LLoop a counted loop with bound and body. The sum interpreter uses structural recursion; the comment says bound is a well-founded measure, so fuel is unnecessary. The sketch is retained, including its helper references.

`LLoop n body` = “body exactly n times.” Being structural, it can be proved via structural recursion or fuel.

<a id="32-停止性拡張terminationv-のアナログ"></a>

### 3.2 Termination extension (analogue of `Termination.v`)

`Termination.v:45-56` proves `length ops` fuel sufficient for the flat fragment. With `LOp`, termination follows immediately from Gallina totality through **structural recursion over the `LOp` tree + a per-loop trip counter**. The key is not “does it stop?” but **the step bound is the static trip count**:

```coq
(* step_bound prog = 非ループ op + Σ_loops n * step_bound(body)。
   この step 数で必ず足りる = どの LLoop も発散しない。*)
Theorem counted_loop_steps_bounded :
  forall prog, fuel_lrun (step_bound prog) prog <> None.
```

Translation of the retained comment: step_bound prog = nonloop ops + Σ_loops n × step_bound(body). This step count always suffices, so no LLoop diverges.

**Proof strategy: structural induction on `prog`; the `LLoop` arm is well-founded on trip count `n`.** `n` is finite, and each iteration consumes 1 from `n`. **This `n` being a ranking function is why the counted restriction makes termination provable** (arbitrary `LoopBreakUnless{cond}` has no measure).

<a id="33-2つの新性質ファイルsubsetv-同型定理文"></a>

### 3.3 Two new property files (isomorphic to `Subset.v`, theorem statements)

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

Full translation of the retained Coq comments: proofs/NoAllocInLoop.v states there are no allocation ops in loop bodies. For each LLoop body, use forallb not_alloc plus nested recursion. Strategy: structural induction on prog + forallb_forall (the Subset.v:35 technique), mechanically isomorphic to subset_check_sound. proofs/AllocBound.v states total allocation count ≤ static bound. The keystone corollary composes NoAllocInLoop: when loops are allocation-free, total count equals allocation count outside loops, a constant independent of trip count. This is the WCET-related claim “allocation bounded BY CONSTRUCTION.” Strategy: induction on prog; in the LLoop n body arm, rewrite loop_body_allocs body = 0 and n*0=0. The two files compose here. The code and its placeholders are retained unchanged, not newly checked proofs.

`alloc_bounded` is trivial `leb`; soundness is 1 line of `Nat.leb_le` (isomorphic to `subset_check_sound`). **The substance is the corollary**: `NoAllocInLoop` ⟹ sum is loop-independent ⟹ static bound is exact.

<a id="34-証明書-witness-拡張certificaters"></a>

### 3.4 Certificate-witness extension (`certificate.rs`)

A third witness emitter (parallel to `ownership_certificate` / `name_witness` / `cap_witness`):

```rust
/// counted-loop / alloc-bound witness: flatten した LOp 木 + 主張する静的 alloc 上限 B。
/// 形式(Subset 風・内部化可能): "<B>|<flattened LOp>"
///   'A'=LAlloc, 'o'=LOther, 'L<n>{ … }'=LLoop(trip n + 括弧本体)
pub fn alloc_bound_witness_string(func: &MirFunction) -> String { ... }
```

Full translation of retained Rust comments: counted-loop / alloc-bound witness: a flattened LOp tree + claimed static allocation bound B. Format (Subset-like, internalizable): "&lt;B&gt;|&lt;flattened LOp&gt;". 'A'=LAlloc, 'o'=LOther, 'L&lt;n&gt;{ … }'=LLoop (trip n + braced body).

Walk `func.ops`, open `L<bound>{` at `CountedLoopStart{bound}`, emit body through `CountedLoopEnd` (`is_allocating`→`A`, others→`o`), close it. Claimed `B` is the compiler's static sum. Checker rederives the actual sum via `lrun` and verifies `actual ≤ B`. Add driver modes `noalloc`/`allocbound`; `Extract.v` extracts 2 `check_*_cert` functions (like caps).

<a id="35-サイズ不変条件最重要規律"></a>

### 3.5 Size invariant (most important discipline)

Discipline = **checker size ∝ #event characters + #subset properties + #op→patterns, independent of program complexity**:

- `NoAllocInLoop`/`AllocBound` have `Subset.v` shape (`forallb` + `leb`, one-step soundness). New alphabet of 3 tokens (`LAlloc`/`LOther`/`LLoop`) + 2 subset-like properties, each **constant-size** inside checker.
- **Compiler does the work; checker only subsets**. Static bound `B` and trip count `n` are **computed by untrusted lowering** (already held by `try_lower_scalar_for_range`) and carried in witness. Checker only does `lrun` fold + `leb`, deriving no bound and opening no callee (same lever as transitive-caps design).
- **No CFG walk**. RC `exec` fold remains flat and unchanged; loop structure lives in separate `LOp` witness.

→ 2 properties become 2 small `Subset.v`-sized files (~60-90 lines each, mostly examples + `Print Assumptions`). **Discipline preserved.**

<a id="36-正直なサイズギャップcount-先byte-後"></a>

### 3.6 Honest size gap (count first, bytes later)

RC alphabet discards byte size (`lib.rs:130`: “Alloc is one 'i' regardless of contents”):

- **Count-based proposal** (§3.3): `total_allocs ≤ B` (allocation-event count). Only after establishing witness/execution correspondence may this be used as an execution bound. Alone it proves neither byte high-water mark, recursive-drop time nor target physical WCET.
- **Byte-based bounds need more**: witness carries each `Alloc`'s byte size (from `Repr`/`LayoutId` + `Init`); refine `LOp` to `LAlloc (size:nat)` and fold `Σ size`. Hard part: `Init::DynStr { len: ValueId }` (`lib.rs:137`) = **runtime length**; byte bound requires bounding `len` itself (a second counted quantity). **Count first; bytes only for static `Init` sizes; `DynStr` walls until `len` proved bounded.**

---

<a id="4-de-risking-順最小ブリック先"></a>

## 4. De-risking order (smallest brick first)

**Brick 0 (smallest slice proving central claim)** — *“Counted loops with no-alloc bodies have bounded allocation count.”* Pure Coq, no compiler changes: write `proofs/CountedLoop.v` (`LOp` + `lrun` + `count_top_level_allocs`) + `AllocBound.v` (corollary `alloc_free_loops_bound_is_constant`) + `NoAllocInLoop.v` (`no_alloc_in_loops_sound`), add to `_CoqProject` (after `CowSafety.v`). Prove properties with handwritten `LOp` examples (`LLoop 1000 [LOther]` has `total_allocs` = allocations outside loops), `Print Assumptions` closed. **De-risk largest unknown: can structural `LOp` support `Subset.v`-shaped soundness without CFG walk?** If yes (1 `Nat.leb_le` + 1 induction), proof complete in principle. Termination half too (`counted_loop_steps_bounded`).

**Brick 1 (bind to compiler)** — add `Op::CountedLoopStart/End` + no-op arms in 4 folds; emit from `try_lower_scalar_for_range` when `end` literal + no-alloc-body wall; `alloc_bound_witness_string` + driver modes + `Extract.v` + `gate.sh` row on a real `.almd` fixture (`for i in 0..1000 { println(...) }` accepts, body with `Alloc` rejects). **Real end-to-end slice.**

**Brick 2 (general bounds)** — allow body allocation, prove `total ≤ Σ bound × per-iter`, relax `end` to const-foldable.

**Brick 3 (bytes)** — refine to `LAlloc (size:nat)`, static `Init` sizes only, `DynStr` wall.

---

<a id="5-正直な難所--open-question"></a>

## 5. Honest hard parts / open questions

1. **Nested loops**: `lrun` already handles `LLoop` recursively (model permits nesting). But lowering's `scalar_loop_depth` is a flat counter; composing nested `CountedLoopStart` bounds (outer × inner) and the one-range-at-a-time handling of `try_lower_scalar_for_range` is **emission-side work**. Coq side ready.
2. **`Dup` inside loops (`'a'` issue)**: rule 1 targets new allocations (`'i'`). Body `Dup` increments existing object refcount; it does not increase heap **object count**, but causes per-iteration RC traffic. Strict “no heap traffic in loops” should include `Dup` in `is_allocating`; “allocation-count bound” should not. **Which is the flight rule is a choice to document deliberately** (ladder says no-alloc-in-loop = count, so minimum brick excludes it).
3. **`break`/`continue` versus exact bound**: counted subset bans early exits and has exact `bound`. Allowing conditional early exit makes the static bound **only an upper bound** (sufficient for WCET, preserves `AllocBound`'s `≤ B`). **Exact (no break) versus upper bound (break allowed, weaken to `≤`) is open**. Proof can handle both; minimum is exact.
4. **Recursion versus loops**: this design closes loops; **recursion remains unbounded, out of scope**. Self-host runtime recursion (`CallFn` cycles detected by `reachable_caps` with `visited`) has no trip bound. Flight profile **walls all recursion** (DO-178C C rule), a separate simple gate rejecting flight-function `CallFn` back-edges, unresolved here.
5. **Byte-size gap** (§3.6 again): count honestly provable now, bytes limited to static `Init` size + `DynStr` wall. Even in bounded loops, dynamic-length allocation has unbounded byte footprint without a bound on `len`: real remaining work.
6. **Size-invariant tripwire**: if future bricks make checker **derive bounds** rather than recheck B, or reconstruct CFG from flat markers to fold `LOp`, discipline breaks (checker grows with program complexity). **Guard: witness carries both structure (`L<n>{…}`) and claimed `B`; checker only folds + compares**. Test `bound_in_witness_matches_emitted_loop` pins “witness `n` = `CountedLoopStart`'s `bound`” (catches a compiler inflating B).

---

<a id="6-接合点まとめfileline"></a>

## 6. Connection-point summary (file:line)

| Concern | file:line | Change |
|---|---|---|
| Counted-loop `Op` | `crates/almide-mir/src/lib.rs:248` | Add `CountedLoopStart{bound:u64}`/`CountedLoopEnd` |
| No-op fold arms | `lib.rs:548-551`, `certificate.rs:377`, `translation_validation.rs:92-95` | 2 arms each |
| Bound derivation + no-alloc wall | `lower/control.rs:766,816`, `lower/mod.rs:278` | Emit counted markers, reject body `is_allocating` |
| New witness emitter | `certificate.rs` (after `cap_witness_string`) | `alloc_bound_witness_string` |
| Coq termination | Model on `Termination.v:27-56` | New `proofs/CountedLoop.v` `lrun` + `counted_loop_steps_bounded` |
| Coq property 1 | Model on `Subset.v:22-38` | New `proofs/NoAllocInLoop.v` |
| Coq property 2 | `Subset.v` + `Nat.leb_le` | New `proofs/AllocBound.v` + corollary |
| Build/extract/gate | `_CoqProject:17`, `Extract.v:17`, `driver.ml:20-26`, `gate.sh:78` | Registration, extraction, modes, real-source row |

**Keystone summary**: *add a structural counted-loop witness separate from the flat RC stream, prove 2 `Subset.v`-shaped properties over it, and have `try_lower_scalar_for_range`, which already knows trip counts, carry the bound. Smallest brick (Brick 0) proves the central claim in pure Coq before compiler changes.*
