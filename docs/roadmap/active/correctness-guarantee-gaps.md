# Correctness Guarantee Gaps

> "well-typed source -> correct binary" chain: layers that lack mechanical/mathematical proof

Status: **Active** — analysis complete, milestone steps defined per-layer

## Layers WITH guarantees

| Layer | Guarantee | Basis |
|-------|-----------|-------|
| Perceus RC | heap alloc exactly-once freed | Lean 4: 23 theorems |
| StackBalance | void context never leaks stack values | structural invariant (tail=None => no Ret) |
| MonoVerify | no TypeVar survives in live code | live VarId collection + exhaustive check |
| ConcretizeTypes | IR expr.ty and VarTable agree | postcondition check |

## Layers WITHOUT guarantees

### 1. WASM emitter (`emit_wasm/*.rs`) — largest gap

Hand-written WASM instruction emission. Hundreds of `wasm!()` macro calls whose stack effects are verified by human review only.

- `rt_string.rs`, `rt_list.rs` etc: manual instruction sequences
- `expressions.rs`: BinOp, Match, Record — branch stack effects are eyeballed
- Layout offsets: `LayoutRegistry` centralizes them, but correct usage is manual

**Current state**: WasmBuilder + WasmIR + LayoutRegistry infrastructure exists in `emit_wasm/engine/`. Partial migration done (list_layout.rs uses WasmBuilder). Most of `expressions.rs` and `rt_*.rs` still use raw `wasm!()`.

### 2. ANF pass — heap alloc visibility unproven

ANF must lift every heap intermediate to a VDecl so Perceus can track it. `needs_lift()` covers known cases but there is no proof it covers ALL cases. A miss = heap leak.

**Current state**: `needs_lift()` covers Call, RuntimeCall, BinOp, If, Match, Block. **Known gaps**: Fan, List/MapLiteral/Record/Tuple literals, StringInterp, IterChain, ForIn, While, UnwrapOr, OptionalChain, Member, IndexAccess, ClosureCreate are NOT matched. No dedicated ANF test exists.

### 3. Closure conversion — env layout correctness

`ClosureConversionPass` packs capture variables into an env struct and reads them via `EnvLoad` with computed offsets. Offset correctness is verified only by tests.

**Current state**: Offset formula is `index * 8` with captures sorted by VarId. Emission uses type-aware loads (I32/I64/F64). Zero `debug_assert` on offset bounds or env size consistency. Test coverage: `closure_nested_capture_test.almd` + `monkey06_closures_test.almd`.

### 4. Type inference -> IR lowering fidelity

Types inferred in `almide-frontend` must be faithfully reflected in IR `expr.ty`. `ConcretizeTypes` postcondition checks exist but coverage of all patterns is unproven.

**Current state**: `resolve_node_ty` handles 17 variants explicitly. 24+ variants return None (MapLiteral, Record, SpreadRecord, Range, MapAccess, StringInterp, Try, UnwrapOr, ToOption, OptionalChain, Clone, Deref, Borrow, BoxNew, RcWrap, ClosureCreate, FnRef, ForIn, While, Fan, etc.). Postcondition audit (`audit_remaining_unresolved`) catches unresolved types but does not guarantee all variants were visited. Design is intentionally best-effort.

### 5. Perceus Inc/Dec insertion — Lean-to-Rust fidelity

The Lean 4 theorems prove the algorithm correct, but the Rust implementation was hand-translated, not mechanically extracted. Conformance is verified by manual comparison with the Lean spec.

**Current state**: Strongest of all gaps. `perceus_verified.rs` mirrors Lean proofs in Rust. proptest validates is_freed/has_dec. PerceusVerifyPass runs on every WASM build. `perceus_monkey_test.almd` has adversarial cases. Lean proofs: 23 theorems, 0 sorry.

## Risk ranking

1. **WASM emitter** — highest risk, broadest surface, no static checking
2. **ANF `needs_lift()`** — silent failure mode (leak, not crash)
3. **Closure env offsets** — wrong offset = memory corruption
4. **Type lowering fidelity** — mitigated by postcondition checks
5. **Perceus Lean conformance** — mitigated by extensive test suite

---

## Milestone Steps

### Gap 1: WASM emitter -> WasmIR migration

Tracked in: [wasm-engine-redesign.md](wasm-engine-redesign.md)

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

### Gap 2: ANF `needs_lift()` completeness

| Step | Description | Deliverable | Depends on |
|------|-------------|-------------|------------|
| 2a | Add missing IrExprKind variants to `needs_lift()` | Fan, List, MapLiteral, Record, Tuple, StringInterp, IterChain, ForIn, While, UnwrapOr, OptionalChain, Member, IndexAccess, ClosureCreate | — |
| 2b | Add postcondition assert: after ANF, walk all Call/RuntimeCall/BinOp args and assert each heap-typed arg is `IrExprKind::Var` | Debug-mode panic on violation | 2a |
| 2c | Add ANF-specific spec test: nested heap expressions in every position | `spec/lang/anf_lift_test.almd` | 2a |

**Gate**: after 2b, a missed case triggers a debug-mode panic instead of a silent leak.

### Gap 3: Closure env offset verification

| Step | Description | Deliverable | Depends on |
|------|-------------|-------------|------------|
| 3a | Add `debug_assert!(index < captures.len())` in EnvLoad emission | Panic on out-of-bounds index | — |
| 3b | Add ClosureVerifyPass (postcondition on ClosureConversionPass): for each lifted fn, assert all EnvLoad indices < param env_size / 8 | Registered as postcondition | — |
| 3c | Add closure capture fuzzer: random capture patterns (0-20 vars, mixed types, nested) | proptest in `tests/closure_env_test.rs` | 3b |

**Gate**: after 3b, offset mismatch is caught at compile time (debug builds).

### Gap 4: ConcretizeTypes postcondition coverage

| Step | Description | Deliverable | Depends on |
|------|-------------|-------------|------------|
| 4a | Add trivial `resolve_node_ty` cases: StringInterp->String, Clone->expr.ty, Deref->inner, Range->List[Int] | Reduce None-returning variants from 24 to ~15 | — |
| 4b | Add per-variant visit counter to postcondition audit | CI log shows which variants were never visited (coverage blind spots) | — |
| 4c | Convert audit to hard error for non-whitelisted Unknown types | Unknown in non-whitelisted variant = compile error, not silent fallthrough | 4a, 4b |

**Gate**: after 4c, new IR variants that produce Unknown without being whitelisted fail the build.

### Gap 5: Perceus Lean->Rust conformance

| Step | Description | Deliverable | Depends on |
|------|-------------|-------------|------------|
| 5a | Differential test: serialize IR to JSON, run both Lean `perceusTransform` and Rust `perceus_fnbody`, compare Inc/Dec positions | `tests/perceus_differential_test.rs` | — |
| 5b | Expand proptest coverage: closure captures, mutable reassignment, nested match | Additional proptest strategies in `perceus_verified.rs` | — |

**Gate**: 5a provides mechanical conformance evidence. Full extraction (Lean->Rust codegen) is a long-term aspiration, not a near-term step.

---

## Priority order

Quick wins first (small effort, high leverage):

1. **2a + 2b** — ANF needs_lift() fix + assert. Days, not weeks. Closes silent leak risk.
2. **3a + 3b** — Closure offset asserts. Trivial to add.
3. **4a** — ConcretizeTypes easy cases. Straightforward.
4. **5b** — Expand Perceus proptest. Low effort, incremental.
5. **1b-1g** — WASM emitter migration. Largest effort, tracked separately.

  # 11. Status snapshot (2026-05-30) — closure / where / fold inference round

  このラウンドで閉じたギャップ:
  - **WASM closure-table regression**: ANF deny-list 反転（5cae928d）が
    `Ty::Fn` のラムダを lift 対象に巻き込み、`pre_scan_closures` の
    func-table 登録から漏れて `call_indirect` が未宣言 table 0 を参照
    → 不正WASM。`needs_lift` の deny-list に Lambda/ClosureCreate を追加
    して復元（802bb944）。
  - **§1 (Inference↔Codegen contract) の実例 2 件を checker 側で根治**:
    1. `test where greet = (name) => ...` の override ラムダ param が
       Unknown 化 → checker で override 値をシャドウ元 fn シグネチャと
       unify（88993d58 / 25a6f341）。
    2. 未解決型変数を subject に `match` したとき `some(stack)` 等の内側
       パターン変数が Unknown 化（fold accumulator `some([])`）→
       `bind_pattern` で subject が `Ty::TypeVar` のとき
       `Option/Result/List[?inner]` に unify してから束縛（e8d44dcb）。
  - **anti-pattern 撤去**: lowering 側 `patch_lambda_params_from_checker` /
    `patch_lambda_from_fn_sig` を削除（930bd486）。checker が source of
    truth になり「Lowering never runs inference」原則に前進。
    残: `lower_where_call_response` 内の同種パッチはまだ残（CallResponse の
    checker 側 TypeMap 反映を確認後に撤去可能）。
  - spec POSTCONDITION: 5 → 0（wasm/native とも）。

  残る完全性ギャップ（このラウンドでは未着手、要・腰を据えた別アーク）:
  - **§5 を保証に格上げ**: postcondition の release hard-fail 化。
    現状 `hard_fail = cfg!(debug_assertions)`（pass.rs:248）。release では
    print のみで素通り。注意: そのまま hard error 化すると、下記
    under-constrained のような「今 warning で通っているプログラム」が
    全て compile error になる破壊的変更。先に spec/exercises 全量で
    影響件数を測ること。
  - **under-constrained を明確に拒否**: `ok([])` を fold init にし err 値が
    一度も現れないケースは err 型 `E` が原理的に未制約 → 現状
    POSTCONDITION を踏むが warning 止まり。「曖昧なら型エラー」にするには
    constraint solving 後の到達可能性解析（その TypeVar が本当に決まらない
    か／後で決まるか）の線引き設計が要る。§5 の格上げと同時に行うのが筋。

  # 13. Status snapshot (2026-06-03) — cross-target program termination on unhandled error

  effect / main / Option / Result は「型は付くが終了挙動が曲者」。`main` に**未処理エラー**が
  到達したときの観測挙動を 8 ケース両 target 実測（`almide run` vs `build --target wasm`＋wasmtime）:

  | # | ケース | native | wasm |
  |---|---|---|---|
  | 1 | main: effect err を auto-`?` 伝搬 | exit 1, stderr `Error: "kaboom"` | **exit 0, 無出力** ❌ |
  | 2 | main: `boom()!` で `Err` を unwrap | exit 1, stderr `Error: "kaboom"` | **exit 0, 無出力** ❌ |
  | 6 | main: `list.first([])!`（`None` を unwrap） | exit 1, stderr `Error: "none"` | **exit 0, 無出力** ❌ |
  | 7 | ネストした effect の err 伝搬 | exit 1, stderr `Error: "deep"` | **exit 0, 無出力** ❌ |
  | 3 | `boom() ?? 99`（`Err` を処理） | got 99 | got 99 ✅ |
  | 5 | `list.first([]) ?? 42`（`None` を処理） | got 42 | got 42 ✅ |
  | 4 | effect ok | got 7 | got 7 ✅ |
  | 8 | 非 effect `fn main` | plain main | plain main ✅ |

  **統一すべきセマンティクス（こうなってほしい）**: `main` に**未処理エラー**（auto-`?` 伝搬 /
  `!` で unwrap した `Err`・`None` / ネスト effect 伝搬）が到達したら、**両 target で**
  「**exit code ≠ 0** ＋ **stderr にエラーメッセージ**」。`??`・`match` で処理済みのエラーは
  値が両 target 一致（実測通り、ここは健全）。

  **根本原因**: native は `effect fn main` → `Result<(),String>` を Rust `Termination` trait が
  処理（err → stderr ＋ exit 1）。wasm は `_start = __main_runner` が `main` の `Result` を**捨てる**
  ため err が消え、**失敗が成功（exit 0・無出力）に見える** — 単一ターゲット correctness では
  決して捕まらない、最も危険な乖離。これは §12（fan の race/timeout/map-err）と同じ
  「build は通るが観測非等価」クラス。

  **修正方針**:
  1. **exit code**（明白・低リスク）: `__main_runner` を「effect main のとき `main` の `Result` tag
     （wasm Result レイアウト `[tag:i32@0][payload@4]`、tag≠0=`Err`）を検査し、`Err` なら
     `proc_exit(1)`」に拡張。`proc_exit` は import 済み（`emitter.rt.proc_exit`）。
     ⚠ **非 effect `fn main`（`Unit` 戻り）は Result tag を持たない** → 判定対象外にしないと
     ok を err と誤判定して全 wasm main が壊れる。effect 限定（`ret_ty` が `Result[_,_]`）が必須。
     回帰確認: ok/非effect main は exit 0 のまま（ケース 4・8）。
  2. **stderr フォーマット**（**要・設計判断**）: native は Rust の `{:?}` 由来で `Error: "<msg>"`
     （引用符・エスケープ付き）。完全一致には (a) wasm を同形式で fd 2 に出す か
     (b) 両 target を統一形式（例 `error: <msg>` 引用符なし、native も custom Termination で揃える）。
     後者の方がクリーンだが native の出力も変わる。どちらに寄せるか未決。

  検証は「観測出力（stdout/stderr/exit）を両 target で byte 比較する差分ゲート」が本筋
  （cross-target equivalence の rank 1 force-multiplier）。この種の意図仕様の明文化（本節）と
  ゲートが両輪。
