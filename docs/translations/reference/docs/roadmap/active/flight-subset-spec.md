Historical description: Original flight-subset proposal; normative bounded-profile authority is ALS §B, not the historical @flight sketch.
<a id="almide-flight-profile--original-subset-proposal"></a>

# Almide Flight Profile — original subset proposal

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](flight-subset-spec.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](flight-subset-spec.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](flight-subset-spec.zh-CN.md)

> Non-normative fixed-source historical reader snapshot. The unchanged pinned original remains the source of this translation; current bounded-profile authority is ALS §B, not the historical @flight design. No autosync or independent verification of proofs, measurements, implementation status or standards mappings was performed. Flight/aviation-grade, certified, globally unusual, O(1), WCET and comparative wording below records the original analysis/proposal, not aviation certification, tool qualification, authority acceptance or a new guarantee. Counts, old source line numbers, proposed gates and later amendments retain their separate dates/status. Allocation counts do not alone establish byte bounds or physical WCET; technical proof assets do not establish flight applicability. The source’s MISRA/Ravenscar/DO-178C comparisons and claims about what those standards mandate were not independently validated here. Commands are reproduction references, not execution instructions. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-subset-spec.md)

The current normative bounded profile is [ALS §B](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md),
using `@bounded` and the critical-profile checks. This roadmap's `@flight`,
IN/OUT tables and old lowering line numbers are the original design, not a
second normative language specification or evidence of certified flight use.
Do not implement its policy differences as a documentation cleanup.
The per-loop certificate/WCET ideas below remain proposals where not established
by the current ALS, source and receipt.

> **Goal**: make the **language subset** of flight-grade Almide code normative — analogous to SPARK/Ravenscar
> or MISRA C. The aim is for a per-build certificate to **mechanically enforce membership by proof**,
> not by review. This is the core of ladder **G-F1** in [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md).
> Settle here the open questions left by the two keystones
> ([flight-wcet-loops](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md) / [flight-rust-ferrocene](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-rust-ferrocene.md)).
> **Related**: [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §3.5 / [flight-wcet-loops](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md).

---

<a id="1-設計核一行"></a>

## 1. Design core (one line)

> **The flight subset selects a strict subset of “what can already be lowered” with additional gates (not a separate
> compiler). Non-flight code lowers to the sound model-one-iteration / `Opaque` form and is merely
> not flight-admissible. Membership is enforced on the closed call graph of `@flight` functions in two layers
> (lowering walls + proven checker properties): the certificate carries a proof that every `@flight` function
> is in-profile. That is the differentiator: proof on every build, rather than review.**

---

<a id="2-原設計の特徴分類表in--out--restricted"></a>

## 2. Original design's feature classification (IN / OUT / RESTRICTED)

Legend — **IN**: allowed in flight code. **OUT**: prohibited (rejected). **RESTRICTED**: allowed only in explicit forms.
Enforcement records the gates/unimplemented notes at design time; current admissibility follows ALS §B.

<a id="21-制御フロー"></a>

### 2.1 Control flow

| Feature | Class | Rationale | Enforcement |
|---|---|---|---|
| counted loop `for i in lit..end` | **IN** | The only loop with a static trip count → WCET and allocation bound by construction. `n` is a well-founded ranking | Already isolated in `try_lower_scalar_for_range` (`lower/control.rs:766`). Proof backstop **not yet implemented** (`CountedLoopStart{bound}` + `NoAllocInLoop.v`/`AllocBound.v`) |
| unbounded `while` | **OUT** | No static iteration bound ⇒ no WCET. `LoopBreakUnless{cond}` is an arbitrary runtime Bool (`lib.rs:255`) | `try_lower_scalar_while` (`control.rs:603`) lowers it for execution, but it is not counted. Flight gate rejecting non-counted loops **not yet implemented** |
| `for x in <heap容器>` | **OUT** | Trip count = runtime length, not static | Flight gate rejecting non-`CountedLoop` iteration **not yet implemented** |
| recursion (self/mutual, `CallFn`) | **OUT** | Unbounded stack + unbounded work, no trip measure | Call-graph **acyclicity** check (§3 (c)) **not yet implemented** (mechanism exists in `reachable_caps`'s `visited`) |
| `if`/`match` (scalar/Unit result) | **IN** | Bounded: only one arm executes, markers have zero ownership | Executable `IfThen/Else/EndIf` (`control.rs:175,267`). Checker is a flat fold |
| `if`/`match` (heap result) | **RESTRICTED→IN** | One arm allocates and moves out (`"im"`), bounded | `try_lower_heap_result_if` (`control.rs:671`), arms limited to string literals/nested if/direct calls |
| payload-bind / guard / arm-reassign match | **OUT** | Flat fold cannot see path-dependent rebinding (potential UAF) | Already **WALLED** (`control.rs:319,330`) |
| `match` over materialized `Option[Int]` | **RESTRICTED→IN** | Tag = length, scalar payload copied by value, bounded | `try_lower_variant_match` (`control.rs:307`) |
| early return `Try` (`?`)/`Unwrap` (`!`) | **RESTRICTED** | Early exit is a control path. Allowed only when it does not skip live per-iteration heap drops | `lower/mod.rs:247`. **OUT inside a counted-loop body** is safe (freeing on the early teardown path is unproven) |
| `break`/`continue` | **OUT** | Breaks the exact trip bound + skips per-iteration drops over heap frames (wasm leak) | Counted-range already rejects it (`control.rs:777`); heap frames use `wall_break_over_heap_frame` (`:868`). §3 (b) |
| `guard … else` (diverging) | **OUT** (provisional) | Conditional early exit, failure path unmodeled | Effect-capture only (`lower/mod.rs:401`). OUT until modeled |
| `fan.*` (concurrency) | **OUT** | Nondeterministic scheduling, unbounded | Not in the MIR subset (walled during lowering) |

<a id="22-メモリ"></a>

### 2.2 Memory

| Feature | Class | Rationale | Enforcement |
|---|---|---|---|
| dynamic allocation outside loops | **IN** | Proposal assuming the original fixed-page allocator. Separately check the current size-class/free-list and heap-cap conditions; allocation count alone does not yield a byte bound or physical WCET | `Op::Alloc` (`lib.rs:162`). Bounding via `AllocBound` **not yet implemented** |
| allocation inside loops | **OUT** | Breaks the high-water mark unless multiplied by trip count | Rejecting `is_allocating` between `CountedLoopStart/End` **not yet implemented** |
| `Init::DynStr`/`DynList` (runtime length) | **OUT** (byte-bounded flight) | Length is a runtime `ValueId` (`lib.rs:137,151`) → bytes unbounded without bounding `len` | Wall until a `len` bound is proven **not yet implemented** (count criterion allows it; byte criterion does not) |
| static-`Init` heap (`IntList`/`Str` literals, `OptSome`) | **IN** | Size statically known from `Init` → byte bound countable | `Op::Alloc` with static `Init` |
| in-place mutation such as `xs[i]=v` (COW) | **IN** | Bounded, `MakeUnique` clones only when shared | `lower_place_mutation` (`lower/mod.rs:419`). `ListPush` (growth) is RESTRICTED to **outside loops only** |
| unbounded data structures (Map/Set growth, push inside loops) | **OUT** | Growth ⇒ unbounded memory. `ListPush` may reallocate | Push inside a loop = alloc-in-loop ⇒ OUT **not yet implemented** |

<a id="23-関数"></a>

### 2.3 Functions

| Feature | Class | Rationale | Enforcement |
|---|---|---|---|
| top-level `fn` | **IN** | Unit of the call graph, bounded body | `MirFunction` (`lib.rs:393`) |
| `effect fn` | **RESTRICTED→IN** | Declare capability (`Stdout`) and verify `reachable ⊆ declared`. I/O only through declared bounded capabilities | `lower/mod.rs:109` + `CapabilityBound.v` (`certificate.rs:127`) |
| closure / lambda | **OUT** | Capture ownership untracked; indirect callees break the static call graph | `is_higher_order` wall (`lower/mod.rs:621`) |
| higher-order functions (e.g. `list.map(f)`) | **OUT** | Closure arguments have unmodeled capabilities + indirect dispatch | `is_higher_order` wall (`lower/mod.rs:621`) |
| function pointer / `FnRef` | **OUT** | Indirect calls ⇒ acyclicity/caps/WCET lose the static callee | `is_higher_order` (`lower/mod.rs:629`) |
| direct user call `f(x)` (`CallFn`) | **IN** | Static callee name ⇒ explicit call graph ⇒ acyclicity + transitive caps decidable | `Op::CallFn` (`lib.rs:214`). Subject to recursion gate |
| pure stdlib call | **RESTRICTED→IN** | Only when `PURE_MODULES` and first-order | `purity.rs::PURE_MODULES` + `is_higher_order` |

<a id="24-型"></a>

### 2.4 Types

| Feature | Class | Rationale | Enforcement |
|---|---|---|---|
| scalar Int/UInt/Float/Bool | **IN** | Copy, no refcount, fixed width, WCET-flat | `Repr::Scalar{width}` (`lib.rs:75`) |
| integer arithmetic Add/Sub/Mul/Div/Mod/cmp | **IN** | Bounded constant time. Div/Mod trap on 0 (total and defined) | `IntOp` (`lib.rs:283`) |
| **floating-point arithmetic** | **OUT** (provisional) | **No `FloatOp` exists in MIR** (only `IntOp`). Float is a `Repr`, but operations are absent after lowering. Flight IEEE-754 also needs determinism rules | Absent from `lib.rs`. Fixed-point or determinism-pinned float needed, **not yet implemented** (§5) |
| record | **RESTRICTED→IN** | Bounded layout, field access borrows at container granularity | `Repr::Ptr{layout}`. Layout registry currently a placeholder, **not yet implemented** |
| enum/variant | **RESTRICTED** | `Option` (len-as-tag) allowed; general payload-bind needs a layout brick | `Init::OptSome`; general variant match is WALLED (`control.rs:343`) |
| `Option`/`Result` | **RESTRICTED→IN** | `Option[Int]` is a proven materialized form; `Result` propagation = `Try` | `Init::OptSome` + `try_lower_variant_match` |
| `Repr::Boxed` (recursive types) | **RESTRICTED** | Box read is Borrow. Recursive **types** allowed, but recursive **traversal** is recursion (OUT) | `Repr::Boxed` (`lib.rs:81`) |
| generics | **RESTRICTED→IN (after mono)** | Monomorphization erases them before MIR; each instantiation is a concrete `MirFunction` | `mono/` + `repr_of` rejects `Ty::Unknown` (`lower/mod.rs:67`) |
| `where` constraints / protocol (`any P`) | **OUT / RESTRICTED** | `any P` = existential type = dynamic dispatch (indirect callee) ⇒ OUT. `where` is compile-time, erased by mono ⇒ IN | `any P` OUT, `where` IN |

<a id="25-stdlibflight-safe-面"></a>

### 2.5 stdlib (flight-safe surface)

| Group | Class | Enforcement |
|---|---|---|
| pure modules (`int`/`float`/`string`/`list`/`map`/`set`/`math`/`option`/`result`/`value`/`json`/`bytes`/`regex`/`matrix`…), first-order | **IN** | `purity.rs::PURE_MODULES` + first-order |
| higher-order members of pure modules (`list.map`/`filter`…) | **OUT** | `is_higher_order` wall (separate from purity) |
| effectful modules (`env`/`fs`/`http`/`io`/`net`/`process`/`random`/`zlib`) | **OUT** (general case). Only `io` print is RESTRICTED | `purity.rs`. Only `Stdout` (`PrintInt/List/Str`) is a modeled capability |
| impure-plain (`datetime`/`args`/`mem`/`testing`) | **OUT** | Reaches the host without the `effect` keyword (clock/nondeterminism/abort) — must be prohibited in flight |
| pure members with unbounded allocation (`string.repeat`/concat inside a loop/runtime-length `with_capacity`) | **RESTRICTED** | Subject to no-alloc-in-loop + DynStr wall, **not yet implemented** |

---

<a id="3-キーストーン-open-question-の決着"></a>

## 3. Settling the keystone open questions

- **(a) Dup-in-loop = IN** (alloc-count profile). `Dup` (`'a'`) in a loop body increments the refcount of
  an existing object = no new heap, O(1) constant time → breaks neither WCET nor high-water mark.
  Prohibiting it eliminates all “heap reads inside loops” in a value-semantics language. The minimal proof brick
  `NoAllocInLoop.v` is already defined using `is_allocating` (only `'i'`) → no new alphabet needed.
  A zero-heap-traffic profile is reserved as a **stricter optional sub-level** when deterministic RC timing is
  needed (not the default).
- **(b) break/continue = OUT** (exact bound). Allowing early exit weakens `AllocBound` to an upper bound
  `≤ B`. Exact is stronger and the proof is already written (`n` consumed exactly once per iteration).
  Lowering already rejects it (`control.rs:777,868`); allowing it requires new teardown and contradicts the
  “cheapest wall.” Also consistent with MISRA single-exit. The `≤ B` variant is a future relaxation (Brick 2),
  outside the normative scope.
- **(c) Recursion = reject via call-graph acyclicity**. The flight module's call graph (nodes = `MirFunction`,
  edges = each `Op::CallFn`) must be a **DAG**. The detector already exists: the early return of `visited`
  in `reachable_caps` (`certificate.rs:168-188`) hits a cycle. The flight gate uses the same DFS with the
  verdict inverted — **REJECT if `visited.insert` fails on the active stack** (back-edge = recursion).
  This means a genuine back-edge, not diamond deduplication. Unknown/cross-file callees are conservatively
  rejected (do not admit hidden recursion edges). Bounded recursion remains work (§5).
- **(d) Nested counted loops = IN, bound = product of trip counts**. Coq's `lrun` already handles
  `LLoop n body` recursively (the model is nesting-ready). Total = `Σ_loops bound_L × 本体 alloc`;
  for nesting, the inner bound multiplies the outer iteration count, giving a **product** bound.
  Remaining work is on emission (`scalar_loop_depth` is flat, `try_lower_scalar_for_range` handles one
  range at a time) — Coq is ready.

---

<a id="4-強制アーキテクチャ"></a>

## 4. Enforcement architecture

**Declaration = per-function `@flight` attribute (module sugar too).** A compile flag (`--flight`) is not
acceptable (it would sweep in imported non-flight stdlib too; flight code is a **kernel** inside a larger program).
Whole-module is correct as an audit unit, but the **enforcement atom is the function** (acyclicity/caps folds are per-function).
**Decision**: `@flight` attributes on `fn`/`effect fn`. A `@flight` function can call only `@flight` functions
(and pure stdlib), because acyclicity + caps folds require the entire reachable set to be in-profile
(the same condition already required by `reachable_caps`). The flight boundary is a **closed call graph**:
the very requirement of WCET/capability proofs.

**Two-layer enforcement** (consistent with §2.3 / [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) §7.2):
1. **Lowering wall (untrusted, cheapest)**: when lowering a `@flight` function, reject these in addition to the existing
   value-semantics walls: non-counted loops / `is_allocating` between `CountedLoopStart/End` / `Init::DynStr|DynList`
   (byte flight) / closures, HOFs, `FnRef` (existing) / `CallFn` edges closing cycles / non-`PURE`,
   non-`@flight` callees. Rejection merely withholds flight certification (other code compiles normally).
2. **Proven checker properties (binding, rechecked every build)**: walls advise; binding enforcement is the
   kernel-proven checker rechecking witnesses. Add two to the existing three witnesses (ownership/name/caps)
   (`NoAllocInLoop`/`AllocBound`) + a call-graph acyclicity witness. The checker remains a flat fold of bounded size
   (the compiler calculates `B`/trip and puts them in the witness; the checker only folds and compares, no CFG walk).
   **The certificate carries a “proof of in-profile membership” per `@flight` function — the differentiator.**

---

<a id="5-misra--ravenscar--do-178c-マッピングサブセット--強み"></a>

## 5. MISRA / Ravenscar / DO-178C mapping (the subset is a strength)

Each item in the OUT list is discipline **mandated by review** in DO-178C C-level coding standards
(MISRA C / Ada-SPARK's Ravenscar). The flight profile **mechanically enforces the same discipline by proof**:

| Almide OUT/RESTRICTED | Equivalent in DO-178C C standards | Difference |
|---|---|---|
| recursion prohibited | MISRA C:2012 Rule 17.2 / Ravenscar | Those = review; Almide = **call-graph acyclicity, proof-carried** |
| unbounded while / non-counted loops OUT | DO-178C WCET obligation / MISRA Dir 4.1 / Ravenscar static iteration | Those = manual WCET + review; Almide = **trip count → AllocBound proof** |
| alloc-in-loop OUT / bounded allocation | MISRA 21.3 (no malloc) / SPARK no-dynamic-alloc | Those = prohibit all heap or review; Almide = O(1) allocator + **static high-water-mark proof** (more permissive and proven) |
| closure/function pointer/`any P` OUT | MISRA pointer-to-function restrictions / Ravenscar dynamic-dispatch prohibition | `is_higher_order` wall + closed call graph |
| break/continue prohibited | MISRA 15.x (single exit) | Exact-bound requirement, lowering wall |
| clock/random/undeclared I/O OUT | DO-178C resource determinism | **Capability-bound proof** (`used ⊆ declared`) |

To a flight engineer, this OUT list looks like **familiar coding standards**. The novelty is not the rules,
but that **each rule is discharged by rerunning a kernel-proven checker on every build** (`make verify`
as the killer demo).

---

<a id="6-正直な残務言語自体に欠けるもの"></a>

## 6. Honest remaining work (what the language itself lacks)

Things flight code requires that Almide **cannot yet express/prove**:

1. **Bounded recursion** — the profile prohibits recursion entirely (§3 c, `CallFn` cycles have no trip measure).
   Real flight code may need statically deep recursion (fixed-depth tree traversal). Missing: `@bound(n)` recursion
   + a Coq measure for call-graph cycles.
2. **Fixed-point / deterministic float** — MIR has no float arithmetic ops (only `IntOp`). DAL-A control laws need
   fixed-point or determinism-pinned IEEE-754 (rounding/NaN/Inf/no-FMA). Missing: a flight float op set or
   fixed-point type.
3. **Statically sized arrays / bounded collections** — all heap containers are dynamically sized; `DynStr`/`DynList`
   have runtime lengths. Missing: `[T; N]` (compile-time N) — **gating work for byte-level WCET** (without it,
   `DynStr`/`DynList` remain walled). The layout registry is currently `PLACEHOLDER_LAYOUT`.
4. **Byte-level memory bounds** (not object-count bounds) — RC certificates discard byte size
   (`lib.rs:130`). Currently only object-count high-water marks can be proven. Missing: size-carrying `LAlloc(size)`
   refinement (depends on 3).
5. **`≤ B` upper-bound early exit** — prohibited for exact bounds (§3 b). Data-dependent early exit (still with bounded
   WCET) is currently OUT. Missing: `≤ B` relaxation of `AllocBound` (the proof supports it; the profile does not allow it).
6. **Real layouts / readable types** — records/payload enums/uniform `Vec<i64>` lack layouts (`PLACEHOLDER_LAYOUT`).
   Rust rendering uses uniform `Vec<i64>` and is not review-grade. It fails DO-178C “source readability.” Missing:
   a layout-registry brick + `debug_name` side table.
7. **Functional-correctness traceability** (outside this profile's scope but a real flight gap) — the profile proves
   *safety* (mem/name/caps/bound), not whether the control law is *correct*. Pillar③ = zero. No subset can supply
   this: it is the application's responsibility + a trace mechanism absent from Almide.

**Summary**: the verification spine is ahead of gating — proofs and detectors mostly exist; the flight profile is
mainly **wiring existing folds to reject verdicts + adding two Subset-shaped Coq properties** (consistent with
“extend, not rebuild” in the keystones).

Reader glossary for preserved Japanese literals: “heap容器” means heap container; “本体 alloc” means allocations in the loop body.
