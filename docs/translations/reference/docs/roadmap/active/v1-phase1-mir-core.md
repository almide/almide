Description: Historical MIR Phase 1 implementation design and the distinction between IR and MIR oracles.
<a id="almide-v1-phase-1-mir-コア--二レンダラ--実装設計"></a>

# Almide v1 Phase 1: MIR core + two renderers — implementation design

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](v1-phase1-mir-core.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-phase1-mir-core.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-phase1-mir-core.zh-CN.md)

> Non-normative reader snapshot of the pinned source; original remains authoritative. No automatic synchronization, independent implementation validation or execution of quoted instructions. Historical status, plans and comparative claims retain their source scope.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-phase1-mir-core.md)

> Source caveat: historical design and staged progress, not an unstarted current migration or universal proof. Original six-component/six-decision wording coexists with seven rows in section 1.1; section 3.4 does not exist, and “second row” in 5.1 names Core→MIR although that is the first data row. These are preserved, not repaired by guessing. The opening IR/MIR interpreter distinction limits later interpreter-blindness claims. Finite Phase 0, 32+ cases and 149/270/200k fixture results are historical observations, not new runs or full compiler faithfulness; structural-impossibility and flight-grade wording describe source goals/claims, not external certification. Borrow/clone wording, flat-drop and shared-mutation design are retained without claiming their final implementation. The source has an unmatched backtick in the box-detection coordinate; preserved verbatim. No quoted CI changes or instructions executed.


<a id="historical-mir-design-and-oracle-boundary"></a>

## Historical MIR design and oracle boundary

This is the implementation design following the June 2026 Phase 0 spike,
not an unstarted plan for the current compiler. Its v0 Perceus/layout/emit
source coordinates belong to that design snapshot. The present shipping
Wasm route is [structural](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/wasm_route.rs).

In §5.1, the claim that an interpreter cannot observe a Core→MIR bug applies
to an interpreter of that same MIR, not to every interpreter. The repository's
[IR interpreter](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-interp/src/lib.rs) evaluates the earlier
IR and provides a separate differential observation; it too has coverage and
trust limits. Neither agreement nor the Phase 0 examples prove the entire
compiler faithful.

Status: active — design-first. Main design following the Phase 0 decision gate pass (5/5); subject to review before implementation.
Owner: compiler
Parent: [v1-mir-architecture.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md) (constitution, canonical form, phased plan)
Grounding: measured mapping of the existing compiler's ownership/layout decisions through close reading of six components (all file:line locations in §1 are verified anchors).

---

<a id="0-このドキュメントの位置づけ"></a>

## 0. Position of this document

[v1-mir-architecture.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md) defines **what and why** (constitution: “ownership and layout decisions happen once in MIR; renderers only reproduce them”). This document is Phase 1's implementation design for **where, in what order, and how to verify**, grounded in measured mapping of existing code.

**Established in Phase 0** (§8.1, `research/spike/v1-mir/GATE.md`): all five ownership-difficult shapes achieved “one Perceus decision → both idioms agree by construction,” with 0 escape hatches. Four canonical-form boundaries also emerged: per-binding polarity, syntax versus semantics, value-equivalence verification, and the shared-mutation edge. **Phase 1 scales this thesis to the whole language while demonstrating it with existing oracles and retiring debt.**

---

<a id="1-現状マップ--意味論の実装が2つあるの実体"></a>

## 1. Current-state map — what “two implementations of semantics” actually means

The most important fact established by close reading: **high IR is already half MIR**.

- `IrStmtKind::RcInc/RcDec` are **already IR nodes** (crates/almide-ir/src/lib.rs:723). Perceus places dup/drop at IR level. The “dup→__rc_inc / drop→__rc_dec” line of §2.2 is literally v0 `emit_wasm/statements.rs:329/358` (deleted in #782; see git history).
- `LayoutRegistry` (:77) in `engine/layout.rs` **already is the §2.1 Repr table**: STRING/LIST/SWISS_MAP/SET/VARIANT/OPTION/RESULT with field offset (Fixed/AfterDynamic), MemType width and header_size; **zero magic numbers outside this module** (`list_layout.rs` is a thin accessor).

→ Phase 1 is not building a new IR from zero. It **promotes already centralized facts to MIR values and folds scattered decisions into one pass**.

<a id="11-5つの所有権パス--レンダラ再決定--1つの事実"></a>

### 1.1 Five ownership passes + renderer re-decisions → one fact

| Current pass/site | Target | Fact decided | MIR carrier | Source (verified) |
|---|---|---|---|---|
| **PerceusPass** | wasm only | Fresh-versus-alias polarity → RcInc/RcDec placement | `Dup`/`Drop` nodes (already IR) + per-binding polarity | pass_perceus.rs:728 `yields_borrowed_alias` (exhaustive, no wildcard), :612 `is_heap_type` |
| **BorrowInferencePass** | Rust only | Param polarity {Own/Ref/RefStr/RefSlice/RefMut} + insertion of caller Borrow | Function-boundary per-param polarity {own/borrow/borrow_mut} (Ref forms are Repr-derived **spelling**) | pass_borrow_inference.rs:497 is_heap_type, :516 check_needs_ownership, :371 is_derive_fn(@derived) |
| **CloneInsertionPass** | Rust only | Last-use → move, otherwise Clone (=Rust dup) | Per-use polarity `Dup` (non-last) / `Consume` (last=move) | pass_clone.rs:141 split_clone_ids, :223 last-use move gate |
| **CaptureClonePass** | Rust only | Capture polarity: Dup-into-env / value-copy / shared cell | Polarity per closure env field + shared-mutation node | pass_capture_clone.rs:100 detect_shared_mut, :448 wrap_lambda_with_clones |
| **AliasCowPass** | wasm only | needs_cow (mutable alias + mutation) → `__cow_check` | `MakeUnique` node | pass_alias_cow.rs, annotations.rs:85 needs_cow |
| **BoxDerefPass** | Rust only | Which recursive-type fields are boxed → insert Deref | `Repr::Boxed{layout}` (boxing is MIR fact, `&**deref` is spelling) | pass_box_deref.rs:132 find_recursive_enums, :18 Rust-only |
| **emit_typed_rc_dec** ⚠ | wasm only, **inside renderer** | Recursive compound-type drop schedule (owned child pointers and release order) | **Flattened `Drop` node sequence** (§3.4) | emit_wasm/statements.rs:734-939, :724 is_heap_type (`Mirrors pass_perceus::is_heap_type`) |

<a id="12-二実装病が具体的に見える-drift-座標phase-1-が殺す対象"></a>

### 1.2 Concrete drift coordinates exposing the “two implementations” disease (targets for Phase 1)

**Duplicate implementations of the same facts** found by close reading—the concrete constitutional violations:

1. **Two copies of `is_heap_type`**: `pass_perceus.rs:612` and `emit_wasm/statements.rs:724` (the latter explicitly comments “Mirrors pass_perceus::is_heap_type”). Synchronized by hand.
2. **Two shared-mutation detectors**: `detect_shared_mut` (Rust, capture time) and `detect_mutated_captures` (wasm, ClosureConversion). Agreement depends on construction.
3. **Variant tags enumerated in two places**: `mod.rs:1338` and `equality.rs:179` (both `enumerate()` index). Order drift silently breaks equality.
4. **Recursive-type detection in three places**: find_recursive_enums / walker annotation / wasm's natural pointer treatment.
5. **emit_typed_rc_dec rewalks Ty**, **re-deciding drop schedules inside the renderer**—the heart of the §643 class. **Invisible** both to `PerceusVerify` (sees only IR RcInc/RcDec) and the byte gate (`verify_rc_balance` checks only `wasm_dec ≥ ir_dec`, cannot detect over-count; mod.rs:1858).

> It is no accident that §643 escaped detection as a heisenbug. **Some ownership decisions are outside both verifiers' reach in hand-coded renderer logic.** Phase 1 lifts them into MIR nodes, within reach.

---

<a id="2-mir-コアの定義"></a>

## 2. Definition of the MIR core

<a id="21-値モデル-repr"></a>

### 2.1 Value model: `Repr`

Attach `Repr` to every MIR value/binding: promote `LayoutRegistry` + `byte_size` (values.rs:58) + `find_recursive_enums` into **value properties**:

```
Repr =
  | Scalar { width }                  // Int=8, narrow=1/2/4, Bool=4, Float=8 — RC 不要、Dup/Drop 無
  | Ptr    { layout: LayoutId }       // String/List/Map/Set/Record — RC 管理、layout は offset/header/stride を持つ
  | Boxed  { layout: LayoutId }       // 再帰サイクル member field — Rust:Box<T> / wasm:bare ptr
```

Code-comment translation: Scalar requires no RC and has no Dup/Drop; Ptr is RC-managed, its layout carries offset/header/stride; Boxed represents a recursive-cycle member field, spelled Rust Box<T> or a bare wasm pointer. Numeric widths are retained in the original code.

- `layout: LayoutId` points to `LayoutRegistry`'s `MemLayout` (header_size, ordered MemField's FieldOffset::Fixed/AfterDynamic, MemType, elem_stride). **Field offsets become MIR lookups**, removing the walker's accumulated `byte_size` and wasm's literal `4+foff`.
- A variant's layout has `tag_value` (case declaration-order index, decided in **one place**, eliminating duplicate computation at mod.rs:1338 and equality.rs:179), `tag_offset` (currently fixed at 0), and `payload_layout`. Future niche/tagless optimization needs a change in **one place**.
- `clone_free` (top_let_storage.rs:67) ⇔ `Repr::Scalar`. #531's four projection tables fold into one `Repr`.

**Hard precondition**: Repr must be fully concrete. `is_heap_type`'s `Ty::Unknown ⇒ heap` (pass_perceus.rs:620) and `byte_size`'s Unknown→4 fallback (values.rs:75, #525) are **breeding grounds for silent miscompilation**. → Move the `AllTypesConcrete` gate (currently post-pipeline `assert_types_concretized`) **before Core→MIR** (§4).

<a id="22-所有権ノード集合"></a>

### 2.2 Ownership node set

Extend the Phase 0 spike's node set to the whole language:

```
Alloc { repr }                 // ヒープ確保(+ field store)
Store / Load { offset }        // layout 経由のフィールド読み書き
Dup    v                       // +1 owned ref(Perceus dup) — wasm:__rc_inc / Rust:.clone()
Drop   v                       // -1 ref(以後未使用) — wasm:__rc_dec / Rust:スコープ末 Drop
Borrow v  (mut?)               // 消費せず参照 — wasm:ptr透過 / Rust:&v / &mut v
Consume v                      // 最後の使用=所有権移動 — wasm:ptr転送(inc無) / Rust:move
MakeUnique v                   // 共有され得る可変への変更前(§2.5)
Call ...
```

Code-comment translation: Alloc allocates the heap and stores fields; Store/Load read/write fields through layout; Dup adds one owned reference (Perceus dup), via wasm __rc_inc or Rust .clone(); Drop removes one reference, with no later use, via wasm __rc_dec or Rust end-of-scope Drop; Borrow references without consuming, transparent pointer on wasm and &v / &mut v on Rust; Consume is the final use transferring ownership, pointer forwarding without increment on wasm and move on Rust; MakeUnique precedes mutation of potentially shared mutable data (§2.5).

**Invariant**: `Dup`/`Drop`/`Borrow`/`Consume`/`MakeUnique`/`Repr` are **semantic facts MIR decides once**. Changing their **counts or polarities** in a renderer is an ownership re-decision, hence a bug (lint in §3).

<a id="23-per-binding--per-parameter-極性gate-finding-1"></a>

### 2.3 Per-binding / per-parameter polarity (gate finding #1)

Ownership is decided per binding/argument:

- **Function boundary**: each param has `own | borrow | borrow_mut`. Ref/RefStr/RefSlice of `ParamBorrow` are **not MIR facts**—the **spelling follows** borrow × param Repr (`Ptr{String-layout}` borrow → `&str`; `Ptr{List-layout}` borrow → `&[T]`). Fixed-point signature propagation (caller polarity depends on callee; pass_borrow_inference.rs:95) runs once as **dataflow inside Core→MIR**.
- **Each use site**: `Dup` (clone count >1) / `Consume` (last use=move) / `Borrow`. pass_clone's `eligible` last-use==0 move means “count statically 1 → Perceus affine special case”; `always` classes (top-let/static/loop-bump) mean “count statically >1, Dup required.”

> Even for the same source binding, consume means move and borrow means clone/dup. This is **not a renderer re-decision: MIR decides once per binding/argument**.

<a id="24-平坦化された-drop-schedule最大の獲物"></a>

### 2.4 Flattened drop schedule (the biggest prize)

`emit_typed_rc_dec` (statements.rs:734-939) rewalks Ty during emission, **re-deciding inside the renderer** recursive drops of List/Set elements, separate Map keys/values, Result/Option payloads, record fields and closure environments. Replace this with **Core→MIR deciding the complete ownership tree once per drop, lowering to a flat `Drop` sequence** (one Drop per owned sub-pointer, including release order).

Results:
- wasm's RcDec arm becomes **one line** (`call __rc_dec`), Rust uses scope-end Drop. **Neither rewalks Ty**.
- The duplicated `is_heap_type` (coordinate ①) **disappears**.
- The §643 class of renderer drop re-decisions becomes **structurally impossible**.

⚠ **Highest value and highest risk**: recursive drop (Map-entry/closure-env/nested-Named) has **the thinnest corpus coverage** (add dedicated fixtures first in §5).

<a id="25-共有可変ノード-makeuniquegate-finding-4-の縁"></a>

### 2.5 Shared-mutation node `MakeUnique` (gate finding #4's edge)

**Important measurement**: a shared-mutation escape hatch **already exists in production**—the non-Copy branch of `detect_shared_mut` emits `SharedMut<T> = Rc<RefCell<T>>` (lib.rs:481), and its Copy branch emits `Rc<Cell<T>>`. Phase 0's “read-only capture has zero Rc” concerns **a different shape**, not a contradiction.

→ MIR `MakeUnique` / shared mutation needs a **Repr fact distinguishing Cell (Copy, no mutation through a borrow) from RefCell (non-Copy, may be borrowed in-place as `&mut` by `list.push` etc.)**. `needs_cow` (wasm cow_check) and Rust clone-at-mutation are **unified in this one node**; alias_cow.rs already demonstrated “one decision.”

---

<a id="3-単一-coremir-パスと忠実なレンダラ契約"></a>

## 3. One Core→MIR pass and a faithful-renderer contract

<a id="31-1つのパスが全部決める"></a>

### 3.1 One pass decides everything

Unify §1.1's six rows (five passes + emit_typed_rc_dec) into **one Core→MIR ownership+layout pass**. Output MIR: Repr-bearing values, §2.2 nodes, per-param/use polarities. PerceusPass (is_heap_type × yields_borrowed_alias) is already the canonical fresh/alias classifier, so **start there**, deriving Rust decisions (clone/borrow) from the same facts.

<a id="32-忠実性契約を操作可能な-lintにする"></a>

### 3.2 Make the faithfulness contract an actionable lint

Enforce §3's constitution in implementation:

> **Neither renderer may independently calculate `is_heap_type` / `yields_borrowed_alias` / last-use / recursive-type detection. Read MIR facts only. Computing them is the #643 re-decision bug.**

- **SYNTACTIC differences may be absorbed** (gate finding #2): absence of box-pattern → tag guard + `&**deref` (BoxDeref is a **renderer responsibility** in v1, not a Core→MIR pass), `&str` versus `&[T]`, `__hoist` temporary, ANF lifting, match expansion, IterChain fusion.
- **SEMANTIC decisions are forbidden**: changing counts or polarities of `Dup/Drop/Borrow/Consume/Repr/MakeUnique` is a bug.
- Implementation: **remove calls** to these predicates from renderers and replace them with MIR field reads. CI grep forbids remaining calls (faithfulness lint).

---

<a id="4-パイプライン挿入点canonical-cut"></a>

## 4. Pipeline insertion point (canonical cut)

**Asymmetry** found by close reading: the Rust path (main.rs:484) **does not call** `optimize_program`/DCE (ConstFold only), whereas wasm (build.rs:294) calls it **before** mono. Five ownership passes run **after** mono/DCE/ir_link inside the nanopass Pipeline.

→ Put Core→MIR at **one cut shared by both targets**: after mono + ir_link + DCE, before any ownership pass. Prerequisites:
1. **Move `AllTypesConcrete` earlier** (Repr requires concrete types, §2.1); currently a post-pipeline assert.
2. Resolve compile-order asymmetry (both paths supply the same Core IR to Core→MIR).
3. Reproduce hard ordering constraints (BorrowInsertion→CloneInsertion / ANF→Perceus / StackBalance→Perceus / terminal Canonicalize) as **stages within one pass** (ANF/heap visibility is necessary for correct Drop placement).

---

<a id="5-移行戦略--オラクル駆動の脱リスク順序"></a>

## 5. Migration strategy — oracle-driven risk-reduction order

**Do not rewrite. Use the end state as the north star, converge with existing byte gates and corpus as oracles, and peel away the old implementation** (§6). Order by dependencies and testing the bet early and cheaply:

| Stage | Content | Risk | Oracle | Reason for order |
|---|---|---|---|---|
| **1. Repr-on-values** | Promote LayoutRegistry+byte_size to MIR values; move AllTypesConcrete earlier; behavior unchanged, Repr merely rides along | Low (already centralized, zero magic numbers) | Byte equality (no behavior change) | Prerequisite to drop flattening; most mechanical |
| **2. Flatten drop schedule** | emit_typed_rc_dec → flat Drop nodes; one-line wasm RcDec; remove duplicate is_heap_type | **High** (thinnest corpus, heart of §643) | **Add dedicated compound-drop fixtures first** (Map-entry/closure-env/nested-Named) + byte gate + corpus replay | The bet is truly tested here; falsify early |
| **3. Unify polarity** | One pass generates per-use (Dup/Consume) and per-param (own/borrow) for both targets; refactor pass_clone/borrow_inference/perceus to read MIR | Medium (different conservatism in two implementations) | **Value equivalence required** (byte gate), with type mismatches caught separately as compile errors | Core of making renderers dumb |
| **4. Unify capture+MakeUnique** | detect_shared_mut/detect_mutated_captures → one analysis; needs_cow + clone-at-mutation → one MakeUnique distinguishing Cell/RefCell Repr | Medium (edge, two-detector drift) | Value equivalence + host-determinism (BTreeSet order) | Turn finding #4's edge into an explicit node |
| **5. Rust renderer uses MIR** | Switch Rust to consuming MIR; both renderers now dumb | Medium | Full-corpus byte/value equality | §7's “switch Rust renderer next” |

<a id="51-検証-oracle-の二層構造完全性の核心--必読"></a>

### 5.1 Two-layer verification oracle (the heart of completeness—must read)

**Unification gives up the independence differential testing relies on**. Byte gates (native↔wasm agreement) work because two **independent** implementations are unlikely to agree by chance. In v1 both renderers start from **one MIR**; **wrong Core→MIR decisions make all targets identically wrong and blind the native↔wasm agreement gate** (both agree). This is not hypothetical: Phase 0 alias_cow (§8.1-③) demonstrated that omitting MakeUnique makes both idioms **identically** corrupt `a` while RC remains balanced. Shared-decision bugs are invisible to differential testing.

→ Completeness **can never rest on the byte gate alone**. Each reduction step needs **an oracle independent of the thing being checked**:

| Step | Property preserved | Independent oracle | Covered by byte gate? |
|---|---|---|---|
| **Core→MIR (decision)** | Source semantics | **Semantic-law property tests + translation validation (#570)** | ❌ **No (the current frontier)** |
| MIR→wasm | Faithfulness to MIR small steps | Interpreter agreement + wasm-spec faithfulness proof | △ (old emit is independent during migration) |
| MIR→Rust | Faithfulness to MIR small steps | During migration: old emit differential; afterward: interpreter | △ |

**The second row's oracle (Core→MIR correctness) is load-bearing new work in this phase**. Byte gate/interpreter serve renderer faithfulness and are powerless against Core→MIR bugs. Concrete measures:
- **Semantic-law property tests** (proptest **laws**, not fixtures): value semantics—`b=a; mutate(b)` leaves `a` unchanged; ownership invariants—dup/drop balance, no use-after-free after drop, no reference to the original variable after Consume. Test these as **source-level truths**. Catch shared-decision bugs **from above** without a second implementation.
- **An interpreter catches renderer bugs, not Core→MIR bugs** (faithfully executing wrong MIR **agrees** on the wrong result). Thus use it for MIR→target; Core→MIR needs properties+proofs.
- **Translation-validation proof (#570)**: prove Core→MIR ownership decisions preserve semantics. Perceus soundness Lean already exists in core; restate it over MIR small steps (full work in Phase 2, property tests lead in Phase 1).

<a id="52-renderer-忠実性ゲートbyte-gate-群--既存資産"></a>

### 5.2 Renderer faithfulness gates (byte-gate family—existing assets)

- **Value equivalence (byte gate) is load-bearing** at the renderer layer. Leak/RC-balance alone is **insufficient**: AliasCow regression gives wrong output with balanced RC (demonstrated in alias_cow.rs). Adding Perceus-belt RC checkers alone lets value corruption through.
- Compare **`new MIR-wasm == old emit_wasm`** using `wasm_cross_target_spec` (tests/wasm_runtime_cross_target.rs, compares (exit,stdout,stderr) triples across both targets). During migration replay broader corpus against **frozen old emit_wasm** (old emit is **independent only during migration**, §5.3). `@xt-allow` ratchet is down-only.
- **Host-determinism** (check-host-determinism.sh): putting HashMap iteration into drop/cow order in the MIR pass can pass the value gate but break the host gate. Preserve BTreeSet order.
- **Heisenbug defense**: corpus replay is necessary but historically insufficient (§643); combine with targeted drop-order fixtures.
- Use **identical `ALMIDE_WASM_FREES`** for new and old (do not compare a leaking body with an RC body and create phantom divergence).

<a id="53-退役順序の不変条件危険な落とし穴"></a>

### 5.3 Retirement-order invariant (dangerous trap)

Parent §6 says “retire byte gates once faithfulness is established,” but **order is crucial**. Remove differential oracles (old emit differential / native↔wasm agreement) **only after higher independent oracles—interpreter + property laws + wasm-spec faithfulness—have replaced them**. Otherwise retirement opens a blind spot for Core→MIR decision bugs. Make **“establish the higher independent oracle before removing the differential”** a hard migration invariant.

---

<a id="6-リスク台帳接地済み"></a>

## 6. Grounded risk ledger

| # | Risk | Source | Gate/mitigation |
|---|---|---|---|
| R1 | emit_typed_rc_dec Map-entry/closure-env/nested-Named drop has thinnest corpus coverage; can pass all 149 wasm_cross fixtures but drift on untested compound drops | statements.rs:734-939; wasm_cross=149 (270 is broader corpus) | Dedicated fixtures before §5 stage 2; MIR brings it within reach |
| R2 | Name-string ownership channels `__tco_`/`__br_`/`__perceus_*` (donate-versus-share) silently invert after renaming (historical 7→55MB churn regression) | pass_perceus.rs:309-315 | MIR stores move-versus-dup as an **explicit fact**, independent of names |
| R3 | TCO **strips** param borrow annotations after BorrowInsertion (owned loop variables) | pass_tco.rs:1142 | Loop transformation=owned happens **before** polarity decisions; byte gate catches E0308 (value equivalence alone insufficient) |
| R4 | @intrinsic/@inline_rust templates spell borrows, conflicting with MIR-decided borrow and producing `&*&*` | pass_borrow_inference.rs:308 | Fragile seam until runtime self-host (Phase 3); isolate templates as fixed Own during migration |
| R5 | Dropping is_derive_fn/mono exclusions (all Own) causes E0308 in cross-module owned-arg calls | pass_borrow_inference.rs:371 (@derived, #647) | MIR “fixed-own boundary” attribute |
| R6 | Desynchronizing manually maintained in-place-mutation-counts-as-use symmetry causes use-after-move (E0382) | pass_clone.rs:116 | MIR drop/dup placement reproduces identical accounting |
| R7 | Module top-let is static (cannot move, E0507) → always-clone | pass_clone.rs:156 module_origin | Repr/polarity marks borrow-only/Dup-always |
| R8 | Over-inclusive recursion detection is harmless in Rust (extra Box), but mis-sizes wasm alloc/RC if it drives them | walker/helpers.rs:39 | Match Boxed semantics in both renderers; byte gate |
| R9 | Asymmetry between variant_alloc_size's 4+max_payload padding (wasm) and rustc enum layout (native); naive Repr breaks mem_eq | equality.rs:1272 | Value-equivalence gate |
| R10 | mutable_captures use plain rc_dec (walking a cell pointer as an object decrefs garbage → wasm trap) | statements.rs:360-368 | MIR stores “cell, plain drop” fact |
| R11 | During stages 2–5 cutover, old emit_typed_rc_dec and new MIR Drop coexist; two live is_heap_type copies drift | statements.rs:724 | Atomic cutover per fixture; always assert byte equality of both paths |
| R12 | One polarity error fans out through corpus (verification tail is §7's greatest cost) | — | Re-green entire corpus at every stage; evaluate bet early at stage 2 |
| **R13** | **Unification removes differential independence → Core→MIR decision bugs make every target identically wrong, blinding native↔wasm agreement gate** (alias_cow §8.1-③ is the canary) | §5.1 | **First-class property-law oracle (value semantics / ownership invariants)**; retirement-order invariant (§5.3); treat byte gate as renderer-faithfulness only |

---

<a id="7-phase-1-完了の定義exit-criteria"></a>

## 7. Phase 1 completion definition (exit criteria)

1. One Core→MIR pass produces all six §1.1 decisions; **both renderers no longer call `is_heap_type`/`yields_borrowed_alias`/last-use/recursive detection** (faithfulness lint green in CI).
2. Delete emit_typed_rc_dec; wasm drop is one-line rendering of flat Drop nodes.
3. Unify duplicated implementations at coordinates ①–⑤ into single facts.
4. Full native↔wasm corpus is green with **value equivalence**, host-determinism green, no increase in `@xt-allow`.
5. **Core→MIR decision property-law oracle is green** (value semantics / ownership invariants in proptest, §5.1), functioning as a first-class gate catching shared-decision bugs from above where the byte gate cannot.
6. `PerceusVerify` (Lean proof checking) remains effective on new MIR Dup/Drop nodes, keeping flight-grade Lean assets attached.
7. Resolve §643 / #591 / #610 **structurally**, not as isolated fixes, and lock with regression fixtures.
8. Differential oracles (old emit differential / byte gate) **are not retired yet**. Retirement is Phase 2 or later, only after higher independent oracles (normative interpreter + property laws + wasm-spec faithfulness) replace them under §5.3's invariant.

→ Achieving these leads to Phase 2 (formalize MIR, make interpreter normative, move Lean to MIR semantics). If not, **cheap early falsification at stage 2** can select §8's failure branches: revise canonical form, allow explicit Rust Rc, or limit to qualification-only use.

---

<a id="8-実装状況と次の一手"></a>

## 8. Implementation status and next step

<a id="80-段0完了-意味法則-property-law-oracle-"></a>

### 8.0 Stage 0 (complete): semantic-law property oracle ✅

The §5.1 independent oracle was installed, verified and committed **before surgery** (`tests/semantic_laws_test.rs`). The generator independently computes correct value semantics in plain Rust and runs equivalent Almide operations on native+wasm, checking **both against the independent model** (`native==expected AND wasm==expected`, strictly stronger than `native==wasm` and survives unification). The model's own correctness is self-checked against the real compiler using known alias_cow (C-033) shapes. 32+ cases and three tests are green against the compiler at that time. Deepen with `ALMIDE_SEMLAW_CASES`. This net watches all ownership changes from stage 1 onward from above.

<a id="81-段1repr-on-valuesの-grounding実測"></a>

### 8.1 Grounding stage 1 (Repr-on-values) through measurement

- **The seed of `Repr` exists**: `WasmRepr{I32/I64/F64/Void}` in `crates/almide-ir/src/wasm_repr.rs` is used for mono ABI-compatibility checks. But it is **four coarse categories without layout**; §2.1's `Repr{Scalar{width}/Ptr{LayoutId}/Boxed{LayoutId}}` is richer.
- **Repr spans crates**: the enum itself (scalar width) can live in almide-ir, but `LayoutRegistry` (`emit_wasm/engine/layout.rs:77`) referenced by `LayoutId`, and boxing (`pass_box_deref.rs:132
  find_recursive_enums)は codegen 側。→ 段1 は almide-ir(Repr 型)+ codegen(layout 解決・
  stamp)に跨る一体変更。LayoutRegistry の所属(codegen→共有クレートへ昇格?)が最初の設計判断。
- **再設計衝突なし**: `codegen-ideal-form.md` is **done** (commit 8ea2d234; the active/ reference in crates/CLAUDE.md is stale). Var-table/dispatch unification is also done. MIR work builds on existing unification.

Source malformed inline-span translation: find_recursive_enums is in codegen. Stage 1 is an integrated change spanning almide-ir (Repr type) and codegen (layout resolution/stamping). LayoutRegistry placement—promote codegen to shared crate?—is the first design decision. The following bullet means “No redesign conflict.”

<a id="82-段1-の着手手順"></a>

### 8.2 Starting stage 1

1. PoC to move `AllTypesConcrete` earlier and resolve compile-order asymmetry (§4), verifying unchanged behavior and byte equality. ← **Next step, the starting point of surgery**.
2. Add MIR value model (`Repr` enum + LayoutId), stamp Repr after mono; absorb `WasmRepr`.
3. ~~Dedicated stage-2 compound-drop fixtures~~ ✅ **Complete**: `spec/churn/{map_entry,closure_env,nested_named}_churn.almd` lock emit_typed_rc_dec's three branches (Map-entry/closure-env/nested-Named drop) with 200k churn. `check-frees-churn.sh` gives native==wasm + exit0 + no trap, all six fixtures green. Avoid `??` (UnwrapOr) to separate from #643. A net is already stretched over surgery's thinnest region.

Stage 1 is an integrated almide-ir+codegen implementation across multiple sessions. **Both pre-surgery nets are installed and green**: §8.0's semantic-law oracle (value semantics from above) and §8.2-3's compound-drop churn (drop schedules in the thinnest region). Proceed while verifying both remain green at every increment.
