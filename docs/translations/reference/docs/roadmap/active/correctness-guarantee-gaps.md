Description: Which layers of the well-typed-source-to-correct-binary chain still lack a mechanical or mathematical proof
<a id="correctness-guarantee-gaps"></a>

# Correctness Guarantee Gaps

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](correctness-guarantee-gaps.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](correctness-guarantee-gaps.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](correctness-guarantee-gaps.zh-CN.md)

> Non-normative reader snapshot of the pinned source; original remains authoritative. No automatic synchronization, independent implementation validation or execution of quoted instructions. Historical status, plans and comparative claims retain their source scope.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/correctness-guarantee-gaps.md)

> Source caveat: retired-backend inventory and May–June logs are historical, not current vulnerabilities or outstanding implementation tasks. The current-boundary preface takes precedence over old release-warning and missing-gate statements; June 4 also supersedes June 3 stdout-only coverage. Algorithm RC proofs, runtime behavior, translation conformance and byte equality remain distinct. The reported 46 bugs and listed cluster counts (41 for A–H, unnumbered A-case) are not silently reconciled. Historical fan winner proposals and remaining Option propagation failures are not claims about today. No commands, gate modifications or tests were executed.


> "well-typed source -> correct binary" chain: layers that lack mechanical/mathematical proof

Status: **historical gap analysis with later correction logs**. The original
Perceus/`emit_wasm` architecture below is retired; do not read its task list as
the current structural backend inventory. Current route and proof boundaries
are in `src/wasm_route.rs`, `proofs/receipt.sh` and `TRUSTED_BASE.md`.

<a id="evidence-boundary"></a>

## Evidence boundary

The original headline “heap alloc exactly-once freed” overstated what the
algorithm theorem establishes. A proof about modeled RC events is not, by
itself, a proof of the Rust translation, generated drop routines or arbitrary
executing programs. The historical analysis below itself records that
model-to-implementation gap.

Current IR postconditions (`crates/almide-codegen/src/pass.rs`) fail in release
as well as debug builds; the later historical milestone proposing that change
is not a current missing gate. The structural runtime proofs and remaining
trusted seams should be read from the current receipt, not the old fixed
23-theorem/Perceus/StackBalance inventory.

<a id="original-gap-inventory-historical-architecture"></a>

## Original gap inventory (historical architecture)

<a id="1-wasm-emitter-emit_wasmrs--largest-gap"></a>

### 1. WASM emitter (`emit_wasm/*.rs`) — largest gap

Hand-written WASM instruction emission. Hundreds of `wasm!()` macro calls whose stack effects are verified by human review only.

- `rt_string.rs`, `rt_list.rs` etc: manual instruction sequences
- `expressions.rs`: BinOp, Match, Record — branch stack effects are eyeballed
- Layout offsets: `LayoutRegistry` centralizes them, but correct usage is manual

**Current state**: WasmBuilder + WasmIR + LayoutRegistry infrastructure exists in `emit_wasm/engine/`. Partial migration done (list_layout.rs uses WasmBuilder). Most of `expressions.rs` and `rt_*.rs` still use raw `wasm!()`.

<a id="2-anf-pass--heap-alloc-visibility-unproven"></a>

### 2. ANF pass — heap alloc visibility unproven

ANF must lift every heap intermediate to a VDecl so Perceus can track it. `needs_lift()` covers known cases but there is no proof it covers ALL cases. A miss = heap leak.

**Current state**: `needs_lift()` covers Call, RuntimeCall, BinOp, If, Match, Block. **Known gaps**: Fan, List/MapLiteral/Record/Tuple literals, StringInterp, IterChain, ForIn, While, UnwrapOr, OptionalChain, Member, IndexAccess, ClosureCreate are NOT matched. No dedicated ANF test exists.

<a id="3-closure-conversion--env-layout-correctness"></a>

### 3. Closure conversion — env layout correctness

`ClosureConversionPass` packs capture variables into an env struct and reads them via `EnvLoad` with computed offsets. Offset correctness is verified only by tests.

**Current state**: Offset formula is `index * 8` with captures sorted by VarId. Emission uses type-aware loads (I32/I64/F64). Zero `debug_assert` on offset bounds or env size consistency. Test coverage: `closure_nested_capture_test.almd` + `monkey06_closures_test.almd`.

<a id="4-type-inference---ir-lowering-fidelity"></a>

### 4. Type inference -> IR lowering fidelity

Types inferred in `almide-frontend` must be faithfully reflected in IR `expr.ty`. `ConcretizeTypes` postcondition checks exist but coverage of all patterns is unproven.

**Current state**: `resolve_node_ty` handles 17 variants explicitly. 24+ variants return None (MapLiteral, Record, SpreadRecord, Range, MapAccess, StringInterp, Try, UnwrapOr, ToOption, OptionalChain, Clone, Deref, Borrow, BoxNew, RcWrap, ClosureCreate, FnRef, ForIn, While, Fan, etc.). Postcondition audit (`audit_remaining_unresolved`) catches unresolved types but does not guarantee all variants were visited. Design is intentionally best-effort.

<a id="5-perceus-incdec-insertion--lean-to-rust-fidelity"></a>

### 5. Perceus Inc/Dec insertion — Lean-to-Rust fidelity

The Lean 4 theorems prove the algorithm correct, but the Rust implementation was hand-translated, not mechanically extracted. Conformance is verified by manual comparison with the Lean spec.

**Current state**: Strongest of all gaps. `perceus_verified.rs` mirrors Lean proofs in Rust. proptest validates is_freed/has_dec. PerceusVerifyPass runs on every WASM build. `perceus_monkey_test.almd` has adversarial cases. Lean proofs: 23 theorems, 0 sorry.

<a id="risk-ranking"></a>

## Risk ranking

1. **WASM emitter** — highest risk, broadest surface, no static checking
2. **ANF `needs_lift()`** — silent failure mode (leak, not crash)
3. **Closure env offsets** — wrong offset = memory corruption
4. **Type lowering fidelity** — mitigated by postcondition checks
5. **Perceus Lean conformance** — mitigated by extensive test suite

---

<a id="original-milestone-steps-retain-with-the-dated-follow-up-logs"></a>

## Original milestone steps (retain with the dated follow-up logs)

<a id="gap-1-wasm-emitter---wasmir-migration"></a>

### Gap 1: WASM emitter -> WasmIR migration

Tracked in: [wasm-engine-redesign.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/wasm-engine-redesign.md)

| Step | Description | Deliverable | Depends on |
|------|-------------|-------------|------------|
| 1a | Audit remaining raw `wasm!()` call sites | List of files/functions not yet using WasmBuilder | — |
| 1b | Migrate `rt_string.rs` to WasmBuilder | Zero raw `wasm!()` in rt_string | — |
| 1c | Migrate `rt_list.rs` to WasmBuilder | Zero raw `wasm!()` in rt_list | — |
| 1d | Migrate `expressions.rs` (BinOp, Match, Record) | Zero raw `wasm!()` in expressions | — |
| 1e | Add stack-effect type annotations to WasmIR Op enum | Each Op declares `(pops, pushes)` | 1b-1d |
| 1f | Implement stack-effect verifier on WasmIR | Reject instruction sequences where net effect != expected | 1e |
| 1g | Delete old raw emission paths | `wasm!()` macro removed or dead | 1f |

**Gate**: after 1f, every WASM function's instruction stream is statically verified for stack balance before encoding.

<a id="gap-2-anf-needs_lift-completeness"></a>

### Gap 2: ANF `needs_lift()` completeness

| Step | Description | Deliverable | Depends on |
|------|-------------|-------------|------------|
| 2a | Add missing IrExprKind variants to `needs_lift()` | Fan, List, MapLiteral, Record, Tuple, StringInterp, IterChain, ForIn, While, UnwrapOr, OptionalChain, Member, IndexAccess, ClosureCreate | — |
| 2b | Add postcondition assert: after ANF, walk all Call/RuntimeCall/BinOp args and assert each heap-typed arg is `IrExprKind::Var` | Debug-mode panic on violation | 2a |
| 2c | Add ANF-specific spec test: nested heap expressions in every position | `spec/lang/anf_lift_test.almd` | 2a |

**Gate**: after 2b, a missed case triggers a debug-mode panic instead of a silent leak.

<a id="gap-3-closure-env-offset-verification"></a>

### Gap 3: Closure env offset verification

| Step | Description | Deliverable | Depends on |
|------|-------------|-------------|------------|
| 3a | Add `debug_assert!(index < captures.len())` in EnvLoad emission | Panic on out-of-bounds index | — |
| 3b | Add ClosureVerifyPass (postcondition on ClosureConversionPass): for each lifted fn, assert all EnvLoad indices < param env_size / 8 | Registered as postcondition | — |
| 3c | Add closure capture fuzzer: random capture patterns (0-20 vars, mixed types, nested) | proptest in `tests/closure_env_test.rs` | 3b |

**Gate**: after 3b, offset mismatch is caught at compile time (debug builds).

<a id="gap-4-concretizetypes-postcondition-coverage"></a>

### Gap 4: ConcretizeTypes postcondition coverage

| Step | Description | Deliverable | Depends on |
|------|-------------|-------------|------------|
| 4a | Add trivial `resolve_node_ty` cases: StringInterp->String, Clone->expr.ty, Deref->inner, Range->List[Int] | Reduce None-returning variants from 24 to ~15 | — |
| 4b | Add per-variant visit counter to postcondition audit | CI log shows which variants were never visited (coverage blind spots) | — |
| 4c | Convert audit to hard error for non-whitelisted Unknown types | Unknown in non-whitelisted variant = compile error, not silent fallthrough | 4a, 4b |

**Gate**: after 4c, new IR variants that produce Unknown without being whitelisted fail the build.

<a id="gap-5-perceus-lean-rust-conformance"></a>

### Gap 5: Perceus Lean->Rust conformance

| Step | Description | Deliverable | Depends on |
|------|-------------|-------------|------------|
| 5a | Differential test: serialize IR to JSON, run both Lean `perceusTransform` and Rust `perceus_fnbody`, compare Inc/Dec positions | `tests/perceus_differential_test.rs` | — |
| 5b | Expand proptest coverage: closure captures, mutable reassignment, nested match | Additional proptest strategies in `perceus_verified.rs` | — |

**Gate**: 5a provides mechanical conformance evidence. Full extraction (Lean->Rust codegen) is a long-term aspiration, not a near-term step.

---

<a id="priority-order"></a>

## Priority order

Quick wins first (small effort, high leverage):

1. **2a + 2b** — ANF needs_lift() fix + assert. Days, not weeks. Closes silent leak risk.
2. **3a + 3b** — Closure offset asserts. Trivial to add.
3. **4a** — ConcretizeTypes easy cases. Straightforward.
4. **5b** — Expand Perceus proptest. Low effort, incremental.
5. **1b-1g** — WASM emitter migration. Largest effort, tracked separately.

<a id="11-status-snapshot-2026-05-30--closure--where--fold-inference-round"></a>

  # 11. Status snapshot (2026-05-30) — closure / where / fold inference round

  Gaps closed in this round:
  - **WASM closure-table regression**: the ANF deny-list inversion (5cae928d) included `Ty::Fn` lambdas in lifting, omitting them from `pre_scan_closures` func-table registration. `call_indirect` then referenced undeclared table 0 → invalid WASM. Restored by adding Lambda/ClosureCreate to the `needs_lift` deny-list (802bb944).
  - **Two concrete §1 (Inference↔Codegen contract) cases root-fixed in the checker**:
    1. Override lambda params in `test where greet = (name) => ...` became Unknown → unify override values with the shadowed function signature in the checker (88993d58 / 25a6f341).
    2. When `match` subject was an unresolved type variable, inner pattern variables such as `some(stack)` became Unknown (fold accumulator `some([])`) → in `bind_pattern`, when subject is `Ty::TypeVar`, unify with `Option/Result/List[?inner]` before binding (e8d44dcb).
  - **Anti-pattern removed**: deleted lowering-side `patch_lambda_params_from_checker` / `patch_lambda_from_fn_sig` (930bd486). Checker becomes source of truth, advancing “Lowering never runs inference.” Remaining: the similar patch in `lower_where_call_response` persists (removable after checking CallResponse's checker-side TypeMap reflection).
  - spec POSTCONDITION: 5 → 0 (both wasm/native).

  Remaining completeness gaps (not started this round; require a separate focused arc):
  - **Promote §5 to a guarantee**: make postconditions hard-fail in release. Currently `hard_fail = cfg!(debug_assertions)` (pass.rs:248); release only prints and passes through. Caution: simply making it a hard error is breaking—all programs currently passing with warnings, such as the under-constrained case below, become compile errors. Measure impact over all spec/ first.
  - **Explicitly reject under-constrained cases**: with `ok([])` as fold init and no err value ever appearing, err type `E` is fundamentally unconstrained; currently it triggers POSTCONDITION but only warns. “Ambiguous means type error” requires designing the line drawn by post-constraint-solving reachability analysis: whether that TypeVar truly cannot be resolved or will resolve later. Do this together with the §5 promotion.

<a id="12-status-snapshot-2026-06-03--fan-cross-target-codegen--semantics"></a>

  # 12. Status snapshot (2026-06-03) — fan.* cross-target codegen + semantics

  Gaps closed this round (build-correctness: well-typed fan source → correct native binary):
  - **E0308 / E0277 in native fan thunk codegen**: passing multiple differently capturing thunks to race/any/settle packed distinct closure types into generated Rust `Vec<impl Fn>`, yielding E0308 (“no two closures have the same type”). Passing a closure VALUE (uniform-repr `Rc<dyn Fn>`, !Send/!Sync) to fan.map yielded E0277. WASM table dispatch handles both correctly, so value-blind WASM-only specs hid the native build break. Fix (20739311):
    - Instead of unboxing fan thunks on the box path, box race/any/settle with `Box<dyn Fn() -> _ + Send + Sync>`. `Box<dyn Fn + Send + Sync>` itself is `Fn + Send + Sync`, unifying heterogeneous-capture thunks into one element type while leaving runtime `Vec<impl Fn + Send + Sync>` signatures unchanged. Add `wrap: FnBox` (`Rc` | `BoxSendSync`) to IR `RcWrap`.
    - fan.map accepts `Rc<dyn Fn>` and executes sequentially (no threads). Closure VALUEs now pass too (parallelism lost, same results).
  - Added value-checking capturing-thunk specs (baac55c3). CI `almide test spec/ --target rust` now permanently exercises native compilation, detecting recurrence of this class of build failures.

  Remaining completeness gaps (not started this round—cross-target **SEMANTIC** divergence, not build failure):
  - **fan.race / fan.any winners differ by target**: native spawns threads and picks wall-clock fastest (nondeterministic); WASM uses list order (race executes only fns[0], any takes first OK in list order). The same well-typed source can return different winners. Currently regression tests only allow nondeterminism with `assert(a or b)`. The proper route is common deterministic semantics on both targets (e.g. preserve parallel execution but choose the first OK in list order).
  - ~~**fan.timeout is a no-op on WASM**~~: **Resolved (0.29.0)**—removed fan.timeout from the language (C-006 flip, E027 tombstone). Apply deadlines at host boundaries.
  - **fan.map err behavior diverges**: native panics via `.unwrap()`; WASM wraps err and propagates outside the enclosing function (return). An err-returning thunk creates an observation difference.
  All build (type-check) but create a cross-target equivalence gap: same source, different behavior by target. The [determinism-belt.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/determinism-belt.md) framework is appropriate.

<a id="13-status-snapshot-2026-06-03--cross-target-program-termination-on-unhandled-error"></a>

  # 13. Status snapshot (2026-06-03) — cross-target program termination on unhandled error

  effect / main / Option / Result “type-check, but termination is tricky.” Measured 8 cases on both targets when an **unhandled error** reaches `main` (`almide run` versus `build --target wasm` + wasmtime):

  | # | Case | native | wasm |
  |---|---|---|---|
  | 1 | main: auto-`?` propagation of effect err | exit 1, stderr `Error: "kaboom"` | **exit 0, no output** ❌ |
  | 2 | main: unwrap `Err` with `boom()!` | exit 1, stderr `Error: "kaboom"` | **exit 0, no output** ❌ |
  | 6 | main: `list.first([])!` (unwrap `None`) | exit 1, stderr `Error: "none"` | **exit 0, no output** ❌ |
  | 7 | Nested effect err propagation | exit 1, stderr `Error: "deep"` | **exit 0, no output** ❌ |
  | 3 | `boom() ?? 99` (handle `Err`) | got 99 | got 99 ✅ |
  | 5 | `list.first([]) ?? 42` (handle `None`) | got 42 | got 42 ✅ |
  | 4 | effect ok | got 7 | got 7 ✅ |
  | 8 | Non-effect `fn main` | plain main | plain main ✅ |

  **Semantics to unify (desired)**: when an **unhandled error** (auto-`?` propagation / `Err` or `None` unwrapped by `!` / nested effect propagation) reaches `main`, **both targets** produce “**nonzero exit code** + **error message on stderr**.” Errors handled with `??` / `match` yield equal values on both targets (as measured, sound here).

  **Root cause**: native `effect fn main` → `Result<(),String>` is handled by Rust's `Termination` trait (err → stderr + exit 1). wasm `_start = __main_runner` **discarded** `main`'s `Result`, swallowing err so **failure looked like success (exit 0, no output)**—the most dangerous divergence.

  **✅ Fixed (Result-error path)**—decision: follow WASI/POSIX practice (success=exit0 / failure=nonzero+stderr), unify formatting with Display, dropping Rust Debug's `"引用符"` wart (quotation marks):
  1. **native**: rename `effect fn main` to `__almide_main`, generate a `fn main` wrapper outputting `Err` with `eprintln!("Error: {}", e)` + `exit(1)` (`walker/mod.rs`). Output changes `Error: "<msg>"` → `Error: <msg>` (quotes disappear—the right direction).
  2. **wasm**: `__main_runner` checks effect main's `Result` tag (`[tag:i32@0][payload@4]`, tag≠0=`Err`); on `Err`, write `Error: <msg>\n` to fd 2 + `proc_exit(1)` (`emit_wasm/mod.rs`). Non-effect `fn main` (`Unit`) has no tag, so **effect-only** (`is_effect`); otherwise ok is mistaken for err and every wasm main breaks.
  - Verification: `tests/wasm_runtime_test_parts/p3.rs::{unhandled_main_error_terminates_consistently,
    successful_main_exits_zero_both_targets}`. Of 8 measured cases, 7 are native==wasm (exit/stderr equal).

  **Residual divergence (separate bug)**: unwrapping **Option `None`** with `!` (`list.first([])!`, case 6) still fails to propagate wasm `Err` into main's `Result` (turns it into `Ok`), silently exiting 0. This is not `__main_runner`, but an **upstream wasm propagation-lowering bug for `!`/Option-`None`**. Result `Err` propagation is unified above.

  Same class as §12 (fan race/timeout/map-err): “build succeeds, observations differ.” Broad detection should use a differential gate comparing both targets' observable output (stdout/stderr/exit) bytewise—the rank-1 force multiplier for cross-target equivalence. The spec/wasm_cross stdout comparison harness in `tests/wasm_runtime_cross_target.rs` is a partial implementation (stdout only; these 2 tests lead on stderr/exit comparison).

<a id="14-cross-target-divergence-burndown-map-2026-06-04--面的ハントの結果"></a>

  # 14. Cross-target divergence burndown map (2026-06-04) — broad hunt results

  **The gate already exists** (#352). `tests/wasm_runtime_cross_target.rs::wasm_cross_target_spec` runs `spec/wasm_cross/*.almd` on native+wasm and **byte-compares (exit, stdout, stderr)**; `// @xt-allow: <理由>` tracks divergences (currently 1 = shortest float round-trip; the Japanese placeholder means reason). Native is oracle. Thus the main task is not “build a gate,” but **burn down `@xt-allow` debt** (same ratchet as traversal-totality LEGACY_DEBT).

  **Broad hunt (61 parallel agents, 169 probes, each divergence independently reproduced) found 46 actual native↔WASM bugs—not just fan.** Deduplicated into **8 root-cause clusters**:

  | # | Cluster (root cause) | Count | Priority | locus |
  |---|---|---|---|---|
  | A | WASM strings use **byte indices**, native **codepoints** (chars/slice/reverse/take/codepoint/pad…; reverse makes invalid UTF-8) | 9 | High | `emit_wasm/rt_string*` |
  | B | WASM `int/float.parse` is lax + **unchecked overflow** (returns `ok(ゴミ)`, meaning garbage) | 8 | High (silent wrong values) | wasm parse runtime |
  | C | WASM `float.to_string` **traps** at `\|x\|≥2^63`, loses `-0.0` sign, differs in to_fixed rounding | 4 | Medium-high | wasm float fmt |
  | D | Compound element equality is **pointer identity** on WASM, structural on native (map/set compound keys, list.contains/dedup on nested) | 4 | High | wasm element-eq |
  | E | WASM list **unchecked bounds → OOB heap read/corruption** (slice/insert/remove_at/swap) | 5 | **High (security)** | wasm list runtime |
  | F | Type-changing closures in map.map/set.map **trap** (indirect call type mismatch) | 2 | Medium | wasm closure dispatch |
  | G | **fan.\*** = §12. WASM sequential stub runs only `fns[0]`; native thread::scope. Side-effect set/winner/all-failed (panic101/trap134/propagate1) diverge | 6 | **Contract decision first** | `emit_wasm/calls.rs:1478-1561` + `runtime/rs/src/fan.rs` |
  | H | div/mod by zero (native panic101 vs wasm trap134), constant-folded div0 (native compile error), Map Display | 3 | Medium | wasm arith/display |
  | A-case | to_upper/to_lower/capitalize: WASM **ASCII only** (±32 bytewise), native all Unicode (é→É, ß→SS expansion, Greek/Cyrillic). Found in A's adversarial verification | — | Medium (Unicode case tables needed; heavy) | `emit_wasm/calls_string.rs` emit_str_case_convert |

  §13 (termination) and §12 (fan) are parts of these 8 (+ A-case) clusters.

  **Progress (burndown, 2026-06-04)**: ✅ **E** (#363 merged, OOB bounds checks), ✅ **B-int** (#364 merged, parse overflow/sign/trim + 4 error strings), ✅ **A** (strings become codepoint-based: 4 UTF-8 helpers + all op conversions, negative n clamped to i32::MAX, 2-arg slice i64::MAX default fixed by i64 clamping). Remaining: D / C+B-float / H / F / G(fan) / A-case.

  **fan(G) requires a contract decision** (deeper §12 analysis). Both native/wasm currently violate the spec (native race picks only Ok, effectively any; wasm only fns[0]). Recommended contract: **deterministic list-order-first**—execute all thunks on both targets → first OK in list order wins → all failures become unified Err on the main-error termination path (§13). This removes concurrent nondeterminism, makes native==wasm immediate, and fits the differential gate. The alternative “true Promise.race (first completion, whether Ok/Err)” depends on wall-clock, is nondeterministic, and cannot fit the gate.

  **Burndown procedure** (same grandfather→drain as LEGACY_DEBT): add deterministic repro for each cluster to `spec/wasm_cross/` → root fix → remove `@xt-allow`. Raw race is unsuitable for the byte gate because its winner is nondeterministic; frame with all-failure / side-effect sets / order-guaranteed fan.settle.
  **Recommended order (value × ease of starting)**: **E (security) → B (parse) → A (strings) → D (equality) → C (float) → H (div/mod) → F (closure) → G (fan, after contract decision)**. Each cluster = 1 PR. Connect to [determinism-belt.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/determinism-belt.md).
