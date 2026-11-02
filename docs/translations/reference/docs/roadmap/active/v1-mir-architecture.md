Historical description: The single-semantics architecture: ownership and layout are decided once in MIR, and renderers only replay the decision.
<a id="almide-v1-mir-を唯一の真とする単一意味論アーキテクチャ"></a>

# Almide v1: a single-semantics architecture with MIR as the sole source of truth

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](v1-mir-architecture.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-mir-architecture.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-mir-architecture.zh-CN.md)

> Non-normative fixed-source historical architecture reader. The pinned original remains authoritative and unchanged; no autosync, implementation, test/proof rerun or independent measurement. The June 13 five-shape spike result is a bounded experiment, not proof that every program or current renderer is correct. Structural impossibility, automatic correctness, sole-route, scalability and qualification claims are source design arguments and goals; this translation does not establish them independently or assert aviation certification. Old walker/emit and planned MIR/interp authority are historical descriptions, not overrides of current specifications. Preserve the source’s distinction between same-wrong-output agreement and independent semantic oracles, its mandatory oracle replacement before gate retirement, and its explicitly deferred formalization/self-hosting/qualification work. Source row/section cross-references and design tensions remain unchanged. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md)

Status: active — Phase 0 decision gate passed (5/5, §8.1). Implement as **fully greenfield + dual oracle** (§6), with the language surface decided **mission-first** (§10) (2026-06-13). The two pre-surgery nets (semantic-law oracle / compound-drop churn) are installed and green.
Owner: compiler
Supersedes / subsumes: #529 (WasmIR), #643/#591/#610 (cross-target emit-drift class), and the proposal portion of wasm-ownership-emit-mechanization.md
Relates: flight-grade #563–#586, trust-layer

---

<a id="0-なぜ-v1-か--病巣は意味論の実装が2つあること"></a>

## 0. Why v1 — the disease is “two implementations of semantics”

Almide currently has **two independent lowerings** from high IR:

- `walker/` → idiomatic Rust (native). Delegates ownership to Rust.
- `emit_wasm/` → handwritten wasm (~136 runtime routines + manually placed inc/dec in the emit layer).

Native is the “oracle” because it **delegates** to Rust's mature, verified ownership/memory system. Wasm **reimplements it by hand**. About 72% of cross-target bugs are this “wasm drifting from the native oracle”; #643/#591/#610 are its ownership/memory cases.

The huge validation apparatus (byte gate / rt-oracle registry / churn / interp third judge / Lean) is **entirely machinery for detecting drift between two implementations after the fact**. It has two fundamental limits:

1. **It cannot, in principle, reach correctness by construction.** A promise to keep two handwritten implementations synchronized forever tops out at “correctness through exhaustive detection.” This is why the completeness roadmap turns into fixing issues piecemeal.
2. **Detection itself has holes.** #643 proves it: observation (`__println_int`) calls `__alloc`, perturbing heap reuse and masking corruption — a heisenbug missed by both the byte gate and `almide test`.

→ **v1 aims to move from correctness through detection to correctness by construction.** To do that, make the implementation of semantics **one**.

---

<a id="1-v1-アーキテクチャ"></a>

## 1. v1 architecture

```
.almd
  → Core IR            型付き・脱糖・ターゲット中立(今の高 IR を整理したもの)
  → MIR                所有権(Perceus)と レイアウトを明示。小ステップ意味論を持つ。 ← 唯一の真
      ├ → wasm         主経路・検証済み・主権的(MIR は wasm 整合 → ほぼ自明に下る)
      └ → Rust         perf と Ferrocene 資格化の踏み台(MIR を Rust idiom に描くだけ)
  runtime              Almide 自身で書き、同じ Core→MIR→target を通る(self-host)
```

**Invariant (v1's constitution):**

> **Decide ownership and layout exactly once in MIR. Renderers (Rust/wasm) only replay the decision; they never decide it again.**

This sentence makes the #643 class structurally impossible. The present #643 was caused by emit_wasm manually placing ownership and drifting from MIR (or its equivalent analysis) — it happened because the renderer re-decided. The constitution forbids that.

---

<a id="2-mir-の設計中核"></a>

## 2. MIR design (the core)

<a id="21-明示するもの"></a>

### 2.1 What becomes explicit

1. **Repr (value representation)**: each value is `Scalar{width}`, `Ptr{layout}`, or `Boxed{layout}`. Field offsets, variant-tag placement, boxing recursive types — what the layout registry and box_deref currently decide in scattered places becomes a **property of MIR**.
2. **Ownership operations**: explicit Perceus `dup` (inc) and `drop` (dec) on values. Distinguish **consume (move) / borrow / dup** at each use.
3. **Allocation/construction**: explicit nodes for alloc + field stores.

**Design discipline: quantities in the permanent core are ValueObjects (types make magic numbers impossible).** Scattered raw constants were one cause of v0's pain. In v1's permanent MIR core, represent quantities with **value objects**, not raw numbers, making magic numbers such as `Repr::Scalar { width: 4 }` a **compile error**:
- Width = `ScalarWidth` enum (`Word`/`Double`…, `.bytes()` is the sole conversion to a number, so “Word=4” exists in one place). A raw `4` cannot be written.
- `LayoutId` fields are private. No ad-hoc `LayoutId(0)`; only `PLACEHOLDER_LAYOUT` or registry issuance.
- Future layout offsets likewise use ValueObjects such as `ByteOffset`.

→ **The type system itself is the gate** (always on, zero maintenance, structural impossibility rather than after-the-fact detection). Only string-template bootstrap WAT (§4.1) cannot receive type protection; substitute the “no growth in handwritten routines” machine gate (`handwritten_wasm_runtime_does_not_grow`) there.

<a id="22-正準モデル--perceusこれが鍵"></a>

### 2.2 Canonical model = Perceus (the key)

**Perceus RC is the more general model. Affine/move is merely the special case of RC whose count is statically known to be 1.** Therefore, storing MIR with Perceus ownership (explicit dup/drop/borrow + explicit Repr) makes both targets **mechanical translations**:

| MIR (Perceus) | wasm renderer | Rust renderer |
|---|---|---|
| `dup v` | `__rc_inc(v)` | `v.clone()` |
| Last consume | Pass the pointer | move (ownership transfer) |
| `borrow v` | Pass the pointer | `&v` / `&mut v` |
| `drop v` (unused thereafter) | `__rc_dec(v)` | Drop at scope end |
| Mutable alias + mutation (AliasCow) | COW (`__cow_check`) | `.clone()` before mutation |
| closure capture | dup the capture | clone-in to a move closure |

What **separate passes** currently do — `pass_perceus` (wasm inc/dec), `pass_clone`/CloneInsertion (Rust clone), `pass_borrow_inference` (Rust &), `pass_capture_clone`, `pass_box_deref` — are **different representations of the same ownership facts**. In v1, **one Core→MIR pass decides them once**, and both renderers merely render the table above.

**The unit of ownership decision is “polarity per binding/argument”** (confirmed by the Phase 0 gate, §8.1-①). Even for the same source expression, **consume** at that binding/argument means move (Rust)/pointer transfer (RC), while **borrow** means `.clone()` (Rust)/dup (RC). This is **not a renderer re-decision but a fact MIR decides once per binding and argument**. Thus “move the payload on `alias 返却`” (alias return) and “clone for a borrowed argument” are simply different polarities of the same canonical form.

**Known edge of the canonical form (handled in Phase 1)**: values that are **shared, mutated across multiple calls, and returned** cannot fit the move/borrow binary choice; they require explicit shared mutability (MIR MakeUnique / an `Rc<RefCell>` equivalent). This generalizes the AliasCow row (§8.1-④); read-only captures and simple aliases **do not reach this edge** (the gate rendered them with zero Rc). v1 makes the edge an **explicit MIR node**, so renderers do not introduce it implicitly.

**Alignment with Lean assets**: proofs of Perceus soundness and ClosureRc already exist. Restate them over MIR small-step semantics and “language ownership = formally proven Perceus” applies to **both targets** (currently only wasm).

<a id="23-形式の錨"></a>

### 2.3 Formal anchor

Give MIR small-step operational semantics (possible because it is our own IR). Align it with the **machine-verified wasm formal specification**: MIR→wasm faithfulness can be proved against the wasm spec. Rust has no complete formal semantics, so ensure Rust-renderer faithfulness through **translation validation + byte gates during migration** (phases below).

**The heart of completeness — unification gives up differential independence**: with one implementation, the **very independence** relied on by cross-target differential tests (native↔wasm agreement) disappears. If the Core→MIR decision is wrong, **all targets are wrong in the same way**, and the agreement gate is blind (both agree). Phase 0 alias_cow (§8.1-③) demonstrated this. Therefore, achieve completeness by placing an **oracle independent of the subject under test at every reduction step**. In particular, **the correctness oracle for Core→MIR is not cross-target differential testing**, which makes both agree; it is **property testing of source-level semantic laws (value semantics and ownership invariants) + translation validation (#570)**. Interp “agrees with the wrong answer if it faithfully executes wrong MIR,” so it serves MIR→target faithfulness and is powerless against Core→MIR bugs. Details and the two-tier oracle structure: [v1-phase1-mir-core.md §5.1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-phase1-mir-core.md).

---

<a id="3-忠実なレンダラの契約"></a>

## 3. Contract for faithful renderers

A renderer R is **faithful** when, for every MIR program P, the observable behavior of R(P) agrees with MIR semantics ⟦P⟧. Concrete rules:

- **Renderers do not decide ownership/layout.** They only turn MIR `dup/drop/borrow/Repr` into idioms. Re-derivation is a **bug** (implement as lint/assert forbidding a recurrence of #643).
- **Absorb SYNTACTIC differences, but do not make SEMANTIC decisions** (Phase 0 gate §8.1-②). Example: Rust has no box patterns, so render a boxed field's nested ctor as a tag guard + `&**deref` — this **resolves a target-syntax difference**, not a re-derivation of ownership/layout. The boundary is clear: **changing** `dup/drop/borrow/Repr/MakeUnique` is a semantic re-decision, hence a bug. Other spellings (deref placement, match expansion, temporaries) are **the renderer's freedom**.
- wasm renderer: prove faithfulness against the wasm spec (long term).
- Rust renderer: validate faithfulness through translation validation. During migration, the existing byte gate is the oracle ensuring “new MIR-wasm == old emit_wasm.”
- **The observable signature of a faithfulness violation depends on shape** (§8.1-③). Ownership re-decisions manifest as RC double-free in #643, but **value corruption with RC still balanced** in AliasCow. Thus a migration gate requiring **only** no leak/double-free is insufficient; require **both** that and native↔wasm **value equivalence** (provided by the existing byte gate).

---

<a id="4-self-host-runtime"></a>

## 4. Self-hosted runtime

The duplicate implementations in `runtime/rs` (native) and emit_wasm's ~136 routines (wasm) are the source of non-ownership drift (the bulk of ~72%: `string.lines`/regex/libm/json…). In v1, **write the runtime in Almide** → it reaches every target through the same Core→MIR→target path. Entire drift classes disappear, and it is dogfooding too (the direction of former `research/selfhost/`, now `research/spike/v1-mir/`). Only minimal alloc/RC primitives are MIR builtins; everything else is Almide. Bootstrap in stages (Phase 3).

<a id="41-wasm-emit-の理想形--手書き-wasm-表面を極小化して証明可能にする"></a>

### 4.1 Ideal wasm emit — minimize the handwritten wasm surface to make it provable

**The root cause of v0's wasm-emitter hell was two huge handwritten surfaces**: ① the emit layer manually decides ownership/layout per operation (drift, #643, duplicate is_heap, tags in two places); ② ~136 handwritten wasm routines are maintained separately from native `runtime/rs`. Too much to keep correct.

**Ideal-form principle: the only way not to suffer again is to make the emitter small.**

- **MIR holds every decision → renderers never decide** (§3) → drift and #643 disappear structurally.
- **Handwritten wasm = only a mapping of the MIR primitive set** (~20: alloc / offset loads and stores / scalar operations / control-flow block, loop, if, br / call / rc_inc, dec, cow). Total, exhaustive, **zero decisions**. This is the only “pain surface,” small enough to **write once and prove faithful against the wasm formal spec** (20 ops are provable; 136 routines are not → the premise for V/faithfulness in the final form).
- **Write everything else in Almide and pass it through the same Core→MIR→wasm.** list/string/map/formatting/RC bodies — 136 routines — become **Almide code**, making both targets automatically correct from one source. **Zero handwritten wasm runtime.**
- **Boundary = “MIR primitive op” vs “Call to a self-hosted runtime fn.”** Do not bake operations such as `Push`/`IndexSet`/`Print` into special MIR ops; make them `Call`s to Almide runtime functions. The MIR op set shrinks to primitives only.

> ⚠ Beware bootstrap shortcuts: to get the path running initially, the `almide-mir` wasm renderer (`render_wasm.rs`) currently handwrites list_copy/itoa/print in WAT and bakes in `Push`/`IndexSet`/`Print` as ops. **This is the very v0 trap** — temporary scaffolding to demonstrate “it runs.” Converge to the ideal through (a) replacing these with runtime-fn Calls, (b) shrinking the op set to primitives, and (c) writing the runtime in Almide. The discipline is not to grow handwritten WAT.

---

<a id="5-flight-grade-が-v1-から落ちてくる"></a>

## 5. Flight-grade “falls out” of v1

| Flight-grade goal | Realization in v1 |
|---|---|
| #529 WasmIR (structural invariants by construction) | **MIR itself** |
| #563 ALS / #530 normative spec / #564 normative interp | **MIR small-step semantics = normative; interp = the reference executing it** |
| #570 per-build translation-validation certificate (verified checker) | Validate **one path each** for Core→MIR and MIR→target. Formally specified MIR makes a checker writable |
| #572 readable Rust line-level traceability | Provenance of a single pipeline (.almd→Core→MIR→Rust) |
| #573 Ferrocene / #574 KCG qualification package | **Specified MIR → specified Rust** = the shape of a qualifiable code generator. Rust→machine is Ferrocene |
| #575 DO-333 formal credit / #576 Lean into runtime | Prove Perceus soundness against MIR semantics. Runtime is Almide code, passing through the verified pipeline |
| #567 Critical subset / #568 static memory / #569 WCET | Define as **MIR sublanguages/properties**. One specified lowering makes WCET analysis orders of magnitude easier |

The current structure makes flight-grade **a separate mountain**; v1 makes **flight-grade follow from the design**.

---

<a id="6-完全グリーンフィールド--二重オラクル2026-06-13-確定"></a>

## 6. Fully greenfield + dual oracle (decided 2026-06-13)

**Decided**: v1 is **fully greenfield** (everything from zero, lexer/parser included, freshly built as separate crates). The original §9 rejected “rewrite from zero” for loss of knowledge in 270 fixtures / second-system risk, but that was **too crude** — the core of second-system risk is **loss of tacit knowledge**, and Almide's knowledge has crystallized not in implementation code but in an **executable external specification** (270 spec fixtures / wasm_cross byte gate / 100 contracts / 137 rt-oracles / reference interp / semantic-law oracle / Lean / CHEATSHEET and STDLIB-SPEC). Knowledge resides in **fixtures and contracts**. Keep them, and it does not collapse.

**The sole safe form of “from zero” = a fresh parallel build with dual oracles:**

- Build the new pipeline **alongside the old as new crates**; **leave the old implementation entirely untouched as a differential oracle until parity is proved**.
- Judge the new implementation **three ways**: **(a) the entire external specification (fixtures/contracts/interp/semantic-law oracle)** + **(b) differences against the old implementation** + **(c) the semantic-law/formal oracle of §2.3**.
- **Cut over fixture by fixture as parity is achieved.** If the new path fails, it can be discarded without harming the old.
- **Non-negotiable constraint (the core of completeness)**: fixtures alone are incomplete (R1). The old implementation is the second implementation supplying **differential independence**; **do not remove it before parity** (that would expose the blind spot in §2.3).

> For features changing the surface (range/protocol/where, §10), the old implementation is a differential oracle **only for unchanged semantics**. Write new specs for changed surfaces. **The semantic-law oracle is surface-independent and remains fully applicable.**

**Deletion list (what disappears in v1's end state — debt from choosing two implementations):**

- emit_wasm emit-layer ownership (manually placed inc/dec) → replaced by MIR dup/drop
- `pass_perceus`/`pass_clone`/`pass_borrow_inference`/`pass_capture_clone`/`pass_box_deref` → unified into **one Core→MIR ownership + layout pass**
- byte-identity gate (wasm_cross) → unnecessary once renderer faithfulness is established (used up as a migration oracle, then retired)
- rt-oracle registry + churn (as drift detection) → unnecessary with a single runtime
- Double maintenance of `runtime/rs` ↔ emit_wasm routines → one self-hosted runtime

**⚠ Hard invariant for retirement order** (the lifeline of completeness): for the “byte-gate retirement” above, **order is everything**. Differential oracles (differences against old emit / native↔wasm agreement) may be removed **only after higher-level independent oracles (normative interp + semantic-law property tests + wasm-spec faithfulness proof) replace them first**. Otherwise, the moment of retirement exposes §2.3's blind spot: after unification, both targets are wrong **identically** due to Core→MIR decision bugs, and the detection layer misses them forever. **“Before removing differential checks, establish the higher independent oracle.”** Details: [v1-phase1-mir-core.md §5.3](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-phase1-mir-core.md).

→ **Net code decreases.** v1 is not “adding features,” but “folding away debt and turning correctness into construction.”

---

<a id="7-phase-計画"></a>

## 7. Phase plan

- **Phase 0 — spike (1–2 weeks, §8)**: demonstrate “one MIR → both renderers agree by construction” on a few ownership-hard shapes, passing the decision gate (can RC and borrowing share one canonical form?).
- **Phase 1 — MIR core + two renderers (main work)** → implementation design in [v1-phase1-mir-core.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-phase1-mir-core.md) (mapped by close reading of six components; mapping of five passes + emit_typed_rc_dec into one pass, canonical cut, five de-risking stages, grounded risk ledger R1–R12). Summary: extend to the whole language; render wasm from MIR (reuse emit_wasm layout/runtime knowledge in a dumb renderer); use existing byte gate + corpus as migration oracles, confirming old behavior while deleting old emit ownership and duplicate ownership passes; then switch Rust rendering to consume MIR. **Key measured finding: high IR is already “half MIR”** (RcInc/RcDec already are IR nodes; LayoutRegistry already is a Repr table with zero magic numbers).
- **Phase 2 — formalize MIR + make interp normative**: small-step semantics, interp as reference (#563/#564). Prove Perceus soundness against MIR in Lean.
- **Phase 3 — self-host the runtime**: write runtime in Almide, reaching all targets through MIR. Remove rt-oracle/churn-as-drift.
- **Phase 4 — flight-grade deliverables**: Critical subset (#567), static memory (#568), WCET (#569) as MIR properties; per-build translation-validation certificates (#570); KCG package (#574).

**Honest scale**: spike + Phase 1 take the first quarter (validating the bet); complete v1 is **year-scale**. Feature/issue work is almost frozen meanwhile. The largest cost is the **validation tail** (every ownership change requires making the entire corpus green again).

---

<a id="8-phase-0-スパイク仕様まずここ"></a>

## 8. Phase 0 spike specification (start here)

**Purpose**: kill or prove §2.2's central thesis — “with Perceus canonical, both targets agree by construction from a single ownership decision” — on the hardest ownership shapes.

**Five target shapes** (currently diverging or potentially divergent across targets):

1. **Alias return** — `fn first(o: Option[String]) = match o { some(s) => s, none => "" }` (return value borrows from inside the argument)
2. **`list.get(xs, i) ?? d`** (the core of #643's Some-box leak)
3. **Nested pattern in a boxed variant** — `match t { Node(Leaf(a), Leaf(b)) => a + b, ... }` (#610)
4. **Closure capture** — mutable capture + a returned closure
5. **Mutable alias + mutation (AliasCow)** — `var b = a; mutate(b)` leaves `a` unchanged

**Minimal MIR (spike scope)**: `Repr ∈ {Scalar(w), Ptr(layout), Boxed(layout)}`, nodes `Alloc/Store/Load/Dup/Drop/Borrow/Consume/Call`, ownership at function boundaries (own/borrow per parameter).

**Single pass**: one Core→MIR Perceus-based ownership resolver (start from existing `pass_perceus`, generalize so Rust-side decisions derive from the same facts).

**Two renderers**: thin MIR→Rust (§2.2 right column) and MIR→wasm (§2.2 middle column) implementations. Assert **no re-decision of ownership**.

**Faithfulness check**: build/run the five shapes on both targets; check (a) observations agree, (b) #643 disappears, (c) renderers emit MIR dup/drop counts unchanged (no re-decision).

**Decision gate (pass/fail criteria):**
- **Pass** → RC and borrowing fit cleanly in one canonical form (Perceus). Proceed to Phase 1.
- **Fail (for example, on one of the five shapes Rust rendering cannot translate to idiomatic move/borrow and must emit explicit Rc, breaking “readable Rust” (#572))** → record the fact and redesign, choosing (i) revise the canonical form, (ii) permit explicit Rc on Rust and trade readability, or (iii) dedicate Rust rendering to qualification. **Retreat is cheap.**

<a id="81-決定ゲート結果--合格2026-06-13-55"></a>

### 8.1 Decision-gate result — **PASS (2026-06-13, 5/5)**

All five shapes were checked through “one MIR decision → render (A) idiomatic Rust and (B) handwritten RC (wasm semantics) → build, run and compare with `rustc --edition 2021 -O`.” **A=B agreed by construction in every shape; only buggy variants that re-decided ownership were caught. Zero conditional / fail / escape hatch.** Evidence and reruns: `research/spike/v1-mir/` (each shape is a self-contained meta-harness; `./run-gate.sh` reruns them all), full record: `research/spike/v1-mir/GATE.md`.

| # | Shape | Single decision | A=B | RC clean | Buggy caught | Hatch | Verdict |
|---|---|---|---|---|---|---|---|
| 1 | alias_return | Last consume transfers payload ptr, frees **shell only** | ✓ | ✓ | double-free | none | **PASS** |
| 2 | list_get_643 | alias-inc + scope-dec, per-iteration drop of loop heap temp | ✓ | ✓ | leak/divergence | none | **PASS** |
| 3 | boxed_pattern_610 | **Borrow through the box** for boxed fields; Leaf payload is Scalar/Copy | ✓ | ✓ | double-free | none | **PASS** |
| 4 | closure_capture | Capture = Dup into env + Drop at closure-drop; each call borrows | ✓ | ✓ | rustc reject / double-free | none | **PASS** |
| 5 | alias_cow | Mutation of a potentially shared ref is **preceded by MakeUnique** | ✓ | ✓ | Value corruption (same in both idioms) | none | **PASS** |

The gate did more than report PASS: it revealed four **boundaries** of the canonical form (incorporated in §2.2/§3):

1. **Ownership polarity is a per-binding/per-parameter MIR fact** (alias_return). Same source, consume→move, borrow→clone/dup. Not re-deciding, but deciding once per binding/argument (§2.2).
2. **Absorb SYNTACTIC differences; no SEMANTIC decisions** (boxed_pattern_610). No box pattern is a syntax difference → tag guard + `&**deref`. Do not re-decide ownership/layout (§3).
3. **Buggy observable signatures differ by shape** (alias_cow vs 643). #643 = RC double-free; AliasCow = wrong-output (value corruption with RC balanced). → Phase 1 requires **value equivalence**, not only leak detection (§3, §6 migration oracle).
4. **Shared + mutated-across-calls + returned capture is the Rc<RefCell> boundary** (closure_capture). Read-only capture rendered with zero Rc; Phase 1 handles this edge as shared mutability in MIR (§2.2 known edge).

---

<a id="9-却下した代替案"></a>

## 9. Rejected alternatives

- **Keep strengthening detection (status quo+)**: cannot in principle reach by-construction; detection has holes (#643 heisenbug). A fight against entropy.
- **Make Rust the sole truth and wasm a rustc target**: builds on Rust semantics without a formal specification, not a foundation for DO-333/Lean; loses sovereignty too. (Raised in the first review, rejected as contrary to the formal-verification north star.)
- **Make wasm the sole truth and discard Rust entirely**: loses the Ferrocene qualification shortcut and native performance/maturity. → Revised to “MIR is truth; Rust is a stepping-stone renderer” (§1).
- ~~**Literally rewrite from zero**: loss of 270 fixtures' knowledge, second-system risk~~ → **This rejection was reversed on 2026-06-13 (§6)**. Knowledge has crystallized in the **executable external specification** (fixtures/contracts/interp/semantic-law oracle), not implementation code; keeping it avoids the core of second-system risk (loss of tacit knowledge). v1 adopts **fully greenfield + dual oracle (keep old implementation until parity)**. **But “blind rewrite deleting old first and validating only with fixtures” remains rejected** — differential independence would be lost before parity (§2.3).
- **Blind greenfield (immediately delete old, accept on fixtures alone)**: fixtures are incomplete (R1); losing the old implementation's differential independence leaves Core→MIR decision bugs in a blind spot. → Revised to “dual oracle keeping old until parity” (§6).

---

<a id="10-v1-言語表層の決定--mission-first2026-06-13-確定"></a>

## 10. v1 language-surface decisions — mission-first (decided 2026-06-13)

Evolve the surface in a greenfield build, but **judge by Almide's sole metric: LLMs write it most correctly (MSR)**. “Learn from Swift” was settled as **take Swift's clarity, not the complexity that consumes MSR**.

| Feature | v1 decision | Reason | Basis |
|---|---|---|---|
| **range** | Rust `0..5`/`0..=5` → **Swift `0..<5`/`0...5`** | `<` spells out “less than,” eliminating off-by-one ambiguity, a direct MSR benefit. Add `...`/`..<` tokens (small) | — |
| **where (generic constraints)** | **Adopt** `type Set[T] where T: Hash + Eq` / `fn max[T](..) where T: Ord` | Constraints **improve** errors (MSR+). Same keyword as test-where: “conditions under which the body is valid” | [type-where-constraints.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/type-where-constraints.md) |
| **protocol (internal model)** | **A clean Swift-style witness model is fine** (compiler internals) | Tidying internal representation adds no surface complexity | — |
| **protocol (user surface)** | **Keep current design**: declarative nominal conformance + `any P` (Go-like interface-value ergonomics) | Follow the deliberate design of `protocol-any-existentials.md` | [protocol-any-existentials.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/protocol-any-existentials.md) |
| **associated types / conditional / retroactive conformance** | **Reserve syntax only; defer from v1 core** | The same roadmap explicitly rejects them for MSR (“errors unreadable to humans and LLMs”). **Greenfield reconciliation**: reserve “AT with readable errors designed in from the start” as a future option when mission weights change | Same document, “Rejected” section |
| **HKT (where over higher-kinded types)** | **Reserve syntax only; defer from v1 core** | Same roadmap rejects it because “complexity eats MSR.” **Swift has no HKT either** (not a reference; Scala/Haskell territory). Greatest complexity and least MSR among the three features | Same document |
| **`if let`** (Optional binding) | **Adopt** `if let x = opt { … }` (bind unwrapped value in the some arm) | Makes the common “unwrap and use” clearer than match. Swift/Rust priors help LLMs write it correctly. **Frontend desugaring** = `match opt { some(x) => …, none => … }` | — |
| **`guard let … else`** (early-exit binding) | **Adopt** `guard let x = opt else { return … }` (else must diverge: return/throw/exit) | Keeps the golden path flat (less nesting = fewer LLM errors = MSR+). Take Swift's readability. **Frontend desugaring** = match with checker-enforced else-must-diverge. A path whose else does not exit gets an E-diagnostic | — |

**Decision in one sentence**: Swift-style ranges / adopt where constraints / **adopt `if let` and `guard let … else` (Optional binding, frontend desugaring, mandatory diverging else for guard)** / protocol internals only Swift-style, preserve declarative nominal + `any P` surface / **reserve syntax for associated types and HKT, defer them from v1 core**. Keep MSR foremost.

`if let`/`guard let` are **frontend desugaring** (MIR sees only the result lowered to match), so introduce no new ownership/layout decisions and do not affect §3's renderer contract. Implement when building the v1 frontend (or as a temporary extension of the existing frontend). Enforce guard's **else-must-diverge** with type/control-flow checking (same discipline as Swift, representable with `Never`).

Reader gloss for the preserved Japanese architecture diagram: .almd flows to typed, desugared, target-neutral Core IR (an organized version of the then-current high IR), then MIR, which explicitly represents Perceus ownership and layout and has small-step semantics as the sole source of truth. The wasm branch is described as the main, verified, sovereign route, descending almost trivially because MIR is aligned with wasm. Rust is the stepping stone for performance and Ferrocene qualification, rendering MIR into Rust idioms only. The runtime is written in Almide itself and follows the same Core→MIR→target path (self-host). These are the historical diagram’s architectural claims, not independent verification here.
