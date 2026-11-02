Historical description: Design note on borrowing Perceus / borrow-checker-style mechanical completeness guarantees for Almide's own passes.
<a id="completeness-by-construction"></a>

# Completeness by Construction

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](completeness-by-construction.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](completeness-by-construction.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](completeness-by-construction.zh-CN.md)

> Non-normative fixed-source historical design reader. The pinned original remains unchanged; no autosync, independent testing or implementation re-verification. Principles such as accepted programs being correct and making bad states unrepresentable are design goals/invariants, not a new universal guarantee. Finite matrices and byte tests retain their tested scope; proof coverage excludes the runtime/emitter operations explicitly identified below. June drafting/status rows coexist with July retirement and October matrix amendments: §4 completed stages and later “remaining/current scattered” prose, §6 open design and later checked completion, and §9 unwired third judge versus the later matrix’s third vote are preserved rather than silently reconciled as today’s state. Memory-leak exceptions remain exceptions to the O(1) claim. Historical CI commands and work order are records, not execution authority. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/completeness-by-construction.md)

> Bring the “mechanically ensure completeness” property of Perceus and Rust's borrow checker
> into compiler details. Verifiers rather than conventions, construction rather than verifiers:
> make incorrect states **unrepresentable**.

Status: **Active** — drafted 2026-06-10. Extend the existing defensive belts
([correctness-guarantee-gaps](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/correctness-guarantee-gaps.md) = codegen chain,
[determinism-belt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/determinism-belt.md) = determinism,
[certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md) = semantic authority,
[almide-perceus-belt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/almide-perceus-belt.md) = RC) to **frontend rule consistency and name identity**.

<a id="原則"></a>

## Principles

1. **One rule, one place.** If the same semantic rule (coercion, name qualification, unwrap) is copied
   into N locations, N−1 will eventually drift. Put the rule in one function; every syntactic position calls it.
2. **Producers over consumers.** Rather than patching incorrect values (bare names, target-blind wraps)
   downstream, constrain production to one correct producer.
3. **Accept ⟹ correct, reject ⟹ actionable.** A checker-accepted program either runs correctly on both
   targets or stops with `[COMPILER BUG]`. An “accepted program” reaching a rustc error or runtime trap
   is a design hole.
4. **Verifier first, then tighten the types.** First make holes observable with verification gates;
   once zero is achieved, make recurrence unrepresentable with types (newtype / type-state),
   following the Verified→Canonical precedent.

<a id="実証-2026-06-10-の-1-セッションで踏んだ穴-全て修正済み"></a>

## Evidence: holes hit in one session on 2026-06-10 (all fixed)

All seven found that day correspond exactly to an “absence of mechanical guarantees.”
Retain them as field samples of which guarantee mechanism was missing.

| Bug | Class | Missing mechanism |
|------|--------|----------------|
| #484 variant payload emitted with a bare name (E0425) | Missing qualification producer | Qualification solely in resolver (§1) + cross-module shape matrix (§2) |
| #485 only `x = effectCall()` not unwrapped (E001) | Rule drift between syntactic positions | Single binding-coercion chokepoint (§3) |
| #486 cross-module top-let moves from LazyLock (E0507) | Old predicate debris after representation change (`starts_with("ALMIDE_RT_")` became dead) | Unified storage-class decision (§4) + matrix (§2) |
| Record literal type stays bare into IrTopLet.ty → native E0425 / **wasm runtime trap** | Last missing qualification producer | §1 + ban silent traps in emit (§5) |
| Parenthesized record construction `Cfg(name: x)` silently discards args (passes checker) | Unverified fallthrough on acceptance path | Exhaustive TypeName-call verification (§6) |
| `almide fmt` removes imports used only in type positions | Formatter semantic preservation unguaranteed | fmt semantic-preservation gate (§7) |
| Parenthesized record-variant pattern gives native E0164 / wasm acceptance | Same as above (other half of §6) | §6 |

Lesson: #484/#486 escaped notice for **12 releases**, because the spec corpus had almost no cross-module
shapes. Shapes a corpus never exercises might as well not exist: thus a gate that **generates** shapes (§2).

<a id="フロンティア-優先順"></a>

## Frontiers (priority order)

<a id="1-qualifiedref-名前修飾を構築時に強制--433484-クラスの根絶"></a>

### §1 QualifiedRef: enforce qualification by construction — eradicate #433/#484 class
- **Invariant**: IR after name resolution contains no bare cross-module type/function names.
- **(a) ✅ Done (2026-06-10)**: `verify_names.rs::assert_names_resolvable` scans every Ty position in
  IrProgram at codegen entry (before pipeline, both targets/profiles); a bare `Ty::Named` with no bare
  declaration but only a qualified declaration stops with `[COMPILER BUG]`.
  **Caught a live defect the moment installed**: value types in derive signatures (`register_derive_sigs`)
  reached callers' var_table still bare (latent, symptomless). Fixed producer; verifier remains with zero
  offenders. Pure-function detector + three unit tests.
- **(b) Remaining**: after sustained evidence of zero, replace with a `QualifiedRef` newtype constructible
  only in resolver, removing per-pass rederivation such as `MODULE_METHOD_FNS`.
- **Prior art**: rustc DefId / GHC Name vs OccName. Effort: (b) weeks.

<a id="2-cross-module-形状マトリクスゲート--rustc-エラー--コンパイラバグの機械化"></a>

### §2 Cross-module shape-matrix gate — mechanize “rustc error = compiler bug”
- **Invariant**: every cell in a generated matrix (definition site × reference shape × type class ×
  binding position) compiles with `--target rust` and does not trap on wasm.
- **Basis**: #484/#486/N2/N3 are all cells of this matrix. Four empty cells were hit in one day.
- **✅ Done (2026-06-10)**: `tests/crossmod_matrix_test.rs` generates 20 cells (definition site ×
  reference shape) into temporary projects; native build (rustc error = red gate) + execution + wasmtime
  byte equality are hard gates. A `KnownBroken` ratchet forces flag removal when fixed, only decreases.
  **Caught a real hole on first run**: cross-module parenthesized named construction `m.Cfg(name:)`
  produced E002 — #488 normalization handled only `TypeName` callees, not Member callees (`m.Cfg(...)`).
  Fixed that day, 20/20 green.
- **Expansion room**: two-package setup (#433 same-named type class), generic records, effect-shape cells.
- **Generated matrix (2026-10-04, #3309)**: outside the handwritten 20 cells, real-app porting hit 11 issues
  (#3283–#3307) in two days. `tests/shape_matrix_test.rs` + `tests/shape_matrix/gen.rs` enumerate family-wise
  Cartesian products of layout (single / sibling / path-dependent package) × declaration site × use site ×
  mutation shape × binding shape × naming (case differences, generated binder names, mangled static spellings)
  × same-field records, with expected output for each cell known by construction (both legs judged independently
  + interp as third vote). Known reds live in `proofs/shape-matrix-baseline.txt` (cell → open issue, shrink-only).
  PRs use a fixed quarter slice; full runs use `shape-matrix-nightly.yml`. xtarget-fuzz's `--family shape`
  randomizes the same vocabulary beyond those boundaries.

<a id="3-binding-強制の単一チョークポイント--485-クラスの根絶"></a>

### §3 Single binding-coercion chokepoint — eradicate #485 class
- **Invariant**: `let x = e` accepted ⟺ `x = e` accepted (except mutability).
  Effect-Result unwrap/coercion is identical at all binding positions.
- **Status**: halfway (2026-06-10) — checker consolidated into `effect_unwrap_rhs`, lowering into
  `coerce_to_target`. But agreement between the checker↔lowering phases is pinned only by fixture (C-064),
  not verified metamorphically.
- **Remaining**: reuse xtarget-fuzz's term generator for metamorphic CI that rewrites binding forms in
  accepted programs into one another and asserts acceptance equivalence. It would also detect missing
  auto-? in lambda bodies (checker retains auto_unwrap, auto_try passes Lambda through: known shared bug).
- **Prior art**: EMI / metamorphic compiler testing. Effort: week.

<a id="4-top-let-storage-class-決定表--486-クラスの根絶"></a>

### §4 Top-let storage-class decision table — eradicate #486 class
- **Stage 0 ✅ (2026-06-11)**: adding (mutability × module-origin) cells to the shape matrix immediately
  caught four harmful bugs: #500 (cross-module `var` dies on both targets, wasm silently uses typed-zero),
  #501 (self-append in a module fn pushes onto a discarded clone), #502 (spread base Unknown),
  #505 (direct assignment `m.x = v` yields a false lvalue via VarId(0) fallback).
  All fixed (v0.27.1), matrix 29/29. Global resolution for reads/assignment/capture unified in shared helpers
  (`lookup_global` / `module_top_let_var`).
- **Stage 1 ✅ (2026-06-11)**: `almide_ir::top_let_storage` = THE decision table (single CopyClass predicate /
  exhaustive classify_storage match / sole static_name formatting site / sole init_can_abort abort predicate /
  total alias resolution). `TopLetStoragePass` computes once at the end of both pipelines;
  **walker-side agreement verifier** asserts agreement with legacy predicates (lazy_top_let_names /
  const_top_let_vars / eager_force order / var_storage) every build: the next drift rejects the build with
  `[COMPILER BUG]` rather than silently miscompiling. The pass rejects unresolvable module-origin references
  with structured errors (totality for #500 class).
- **Stage 2 ✅ (2026-06-11, v0.27.2 + endgame)**: all consumers switched over, legacy fully removed.
  Removed walker pre-index/register/agreement-verifier and lazy_vars / lazy_top_let_names /
  const_top_let_vars / eager_force_top_lets / var_storage_by_name fields. `get_var_storage` is VarId-only
  (name fallback gone), **VarStorage locals-only** (ModuleCell/ModuleRc variants removed; TopLetStorage
  alone handles module-global storage). pass_clone's `__cap_`/`__licm` name-prefix predicates replaced
  by producer-side VarId snapshots (`always_clone_vars`, marked by CaptureClone/LICM).
  All switches: zero byte diff ×342.
- **Stage 2c (remaining, with behavior review)**: canonicalize CopyClass (unify four predicates, e.g.
  Cell treatment of Float32/Unit/numeric tuples).
- **Invariant**: top-let native storage (const / LazyLock / ModuleCell / ModuleRc) and access style
  (move / clone / borrow) are decided exactly once by an **exhaustive match** over (type Copy-ness ×
  same/different module × mutability × use style); emission only consumes the attribute.
- **Status**: decisions are scattered across pass_clone (always/eligible), walker (lazy_top_let_names,
  const_top_let_vars, var_storage_by_name), lowering (module_origin). #486 was an omission caused by this scattering.
- **Shape**: consolidate into a `TopLetStorage` attribute pass following VarStorageClassification (v0.22).
  §2 matrix turns cells into fixtures. Effort: days.

<a id="5-silent-trap-禁止-emit-の-miss-arm-を-compiler-bug-に"></a>

### §5 Ban silent traps: turn emit miss-arms into `[COMPILER BUG]`
- **Invariant**: emit_wasm lookup failures (record_fields miss, func_map miss, etc.) are compile-time errors,
  not latent `unreachable` instructions until runtime.
- **Basis**: N3's wasm trap came from emit_member's field_offset miss silently emitting `unreachable`.
  A miss on a resolved type is 100% a compiler bug.
- **✅ Done (2026-06-10)**: six sites converted: emit_member field miss (resolved types only; Unknown
  retains trap as dead-code residue), user-fn func_map miss, **unsupported IrExprKind catch-all
  `_ => unreachable`** (largest hole), FnRef wrapper miss, ClosureCreate table miss, value-dispatch catch-all.
  All become `[ICE]` build failures. Zero false positives across corpus 264/264.
  **Remaining**: equality.rs:797-798 (deferred due to high Unknown-in-dead-code risk),
  calls.rs 994/1139 (non-Fn-type else in closure-call).

<a id="6-コンストラクタ呼び出しの全数検証--accept-then-explode-の根絶"></a>

### §6 Exhaustive constructor-call verification — eradicate accept-then-explode
- **Invariant**: `TypeName(...)` / `TypeName { ... }` either (a) normalize into verified construction,
  or (b) are rejected with actionable E-codes. No path bypasses the checker and reaches rustc / wasm traps.
- **Current hole** (measured 2026-06-10): unconditional `else` fallthrough at check/calls.rs:131.
  Parenthesized record construction **silently discards** named args; brace forms also lack
  unknown/duplicate/missing-field validation (rustc E0560/E0063 leakage).
  Parenthesized record-payload variant patterns behave differently on native/wasm.
- **Shape**: (1) checker AST-normalizes parenthesized NAMED construction into the brace pipeline
  (LLM priors frequently produce `Cfg(name: x)`; acceptance is correct for MSR).
  (2) Reject positional-on-record with a new E-code (field reordering changes meaning, an anti-MSR shape).
  (3) Add field-set checks to Record arm (unknown/did-you-mean/duplicate/missing-without-default),
  shared by braces and normalized parentheses. (4) Replace calls.rs:131 fallthrough with a diagnostic.
  (5) Reject parenthesized record-payload case patterns with a `SetEmotion { .. }` hint.
- Effort: week. **Next candidate #3** (root-cause investigation and fix design complete).

<a id="7-fmt-意味保存ゲート"></a>

### §7 fmt semantic-preservation gate
- **Invariant**: fmt does not change meaning: if input typechecks, output also does.
- **Basis**: fmt wrongly classified imports used only in type positions as “unused” and removed them
  (fixed 2026-06-10). Roundtrip gates check only “reparseable + fixpoint,” allowing meaning-changing
  formatting through: a designed-in blind spot.
- **✅ Done (2026-06-10)**: `fmt_output_typechecks_single_file_specs` gates fmt output of 341 single-file
  specs through `almide check` + four direct unit pins for today's bug class.
  **Detected five classes of semantic damage immediately upon installation** (in addition to yesterday's two):
  (1) Imports also removed for `json.parse` in match subjects (another AST-walker hole) → unused detection
  replaced by a **superset of token scanning** (cannot omit by construction; asymmetric design: removal for recall,
  addition for precision). (2) Missing `mut` parameters → E007. (3) Module-level `var` rewritten to `let`.
  (4) **Entire test `where` clauses removed**. (5) Spurious imports injected for UFCS on local variables with stdlib
  module names → ADD side receives bundled-source function-existence gate. fmt suite 48/48.
  **Remaining**: re-typechecking multi-module files (requires sibling resolution), `Decl::TestWhereDef` printer
  (module-scope where, unused syntax).

<a id="8-stdlib-意味論マニフェスト--419-クラスの根絶"></a>

### §8 Stdlib semantics manifest — eradicate #419 class
- **Invariant**: documented contract dimensions of stdlib functions (index units codepoint/byte,
  boundary clamping, error variants, Unicode ranges) are structured fields in stdlib definitions;
  docs generated from them; dimension-specific property fixtures verify claims on both targets.
- **Basis**: #419 — docs explicitly call `string.len` “character count,” implementation counts bytes.
  Mixing `index_of` (bytes) and `take/drop` (characters) silently breaks on multibyte input.
  Until today, no machine checked doc↔implementation agreement.
- **Shape**: first fix #419 itself (unify len/index_of/last_index_of to codepoints, native+wasm together,
  multibyte spec/wasm_cross fixtures + contract). Second, stdlib/*.almd @semantics annotations → generated
  docs + derived property fixtures (same regen-and-diff shape as runtime-registry regen gate).
  Effort: first step days, second weeks.

<a id="9-almide-interp-第三審を配線--nativeoracle-の循環を切る"></a>

### §9 Wire almide-interp as third judge — break native=oracle circularity
- Same as certification-grade CG-1 (details there). Fuzzer `Oracle` trait has zero implementations,
  interp zero consumers: two-way voting is structurally blind to “both wrong the same way.” Effort: week.

<a id="10-release-parity-debug-only-検証器の常時化"></a>

### §10 Release parity: make debug-only verifiers permanent
- **First step ✅ (2026-06-10)**: make wasmparser::validate **always-on and fatal**, immediately after
  §2 matrix CI proved real harm: phantom stack values from Unit tail vars made **emitter ship invalid modules**;
  only environments with wasm-opt accidentally repaired them; wasmtime rejected them in CI jobs without wasm-opt
  (print-only validation could not stop shipment). Emitter fixed too (Unit Var emits no value, following ty_to_valtype
  contract). Batch raw-module verification: wasm_cross 106/106 valid.
- **Remaining**: promote per-pass postconditions / MonoVerify to release, post-emit RC counts
  (currently debug-only). Effort: days.

<a id="残存地図-post-v0270-2026-06-11"></a>

## Remaining map (post-v0.27.0, 2026-06-11)

Ledger of things for which one **cannot say** “completeness secured” at v0.27.0
(true Perceus: wasm frees default ON). Pillars erected, but a direction rather than a declaration; remaining below.

<a id="メモリ回収の既知有界例外-1-つ-契約-c-066-に明記済み"></a>

### One known bounded exception to memory reclamation (explicit in contract C-066)
1. **Per-iteration leak in TCO self-recursive loops** — awaits Stage C redesign
   (see [wasm-frees-ownership-discipline](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/wasm-frees-ownership-discipline.md).
   Cherry-picking old M2 polluted the free list and was rejected; acceptance =
   `spec/churn/tco_loop_churn.almd`).
   Safe direction (leak, not corruption): O(1) memory claims carry this footnote.

   ~~2. construct-from-temp reference +1 leak~~ — **retired (#911, 2026-07-28)**.
   `emit_stored_field` deleted with the entire v0 emitter (#782); on v1 MIR, ownership is decided once
   under `verify_ownership`, and the class was not rederived. Measured RSS flat
   (200k/2M/10M iterations, including nested-record shapes).
   Pin = `spec/churn/construct_from_temp_churn.almd`.

<a id="フロントエンド規則一貫性の残り-本文--優先順"></a>

### Remaining frontend rule consistency (body §; priority order)
- **§4** top-let storage-class decision table (structurally eradicate #486 class) — days
- **§3** metamorphic binding gate (`let x = e` ⟺ `x = e` acceptance equivalence) — week
- **§1b** QualifiedRef typing (bare names unrepresentable by construction) — weeks
- **§9** wire interp as third judge (two-way voting blind to both-guilty bugs) — week
- **Core #433**: per-package namespace for type identity: end-to-end structural debt from bare-name type identity.
  §1b mechanizes this — weeks

<a id="証明の被覆ギャップ"></a>

### Proof coverage gaps
- **Lean Perceus belt reaches only IR-level Inc/Dec balance**: runtime frees bodies materialized in 0.27.0
  (free-list push/reuse, sentinels, allocator-state invariants for region reset) are **outside proof scope**.
  Churn/byte gates are currently the only guards → next-phase candidate in [almide-perceus-belt](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/almide-perceus-belt.md).
- Emitter-level RC operations (stored-field dup, runtime inc/dec) are **invisible** to IR verifier;
  it counts only IR nodes.

<a id="着手順序-更新-2026-06-10-第-2-ラウンド"></a>

## Start order (updated 2026-06-10, round 2)

1. ~~§2 matrix gate + §5 silent-trap ban~~ ✅
2. ~~§6 exhaustive constructor verification~~ ✅ (v0.26.20)
3. ~~§7 fmt semantic preservation + §8 first step (#419)~~ ✅
4. ~~§1(a) NameResolutionTotal verifier~~ ✅ → next: §4 decision table → §3 metamorphic
5. §1(b) QualifiedRef typing, §9 interp third judge, §10 release parity — permanent belts

<a id="進捗ログ"></a>

## Progress log

- 2026-06-10: drafted. Fixed #484/#485/#486 + record-literal qualification + fmt import removal
  (PR #487). Unified `canonical_user_type_sym`; made `effect_unwrap_rhs` /
  `coerce_to_target` chokepoints; C-064 contract. §6 root-cause investigation complete
  (check/calls.rs:131 fallthrough, lower/calls.rs:83-98 silently discarding named args).

- 2026-06-10 (round 2): installed four gates §5/§1a/§2/§7. Installation itself found eight latent bugs
  (§1a: bare-name producer in derive signatures / §2: E002 for Member-callee parenthesized construction /
  §7: five fmt semantic-damage classes + permanent detector).
  Evidence for the hypothesis “bugs keep living in shapes the corpus does not exercise.”
