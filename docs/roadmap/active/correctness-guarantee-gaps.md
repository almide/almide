<!-- description: Audit of the well-typed-source-to-correct-binary chain — which layers have mechanical/mathematical proof and which don't, with per-layer milestones -->
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

### 2. ANF pass — heap alloc visibility ✅ COMPLETE (2026-07-19 re-audit)

ANF must lift every heap intermediate to a VDecl so Perceus can track it. This section originally described `needs_lift()` as an ALLOW-list (Call, RuntimeCall, BinOp, If, Match, Block matched, everything else silently un-lifted = leak risk). That is no longer the design.

**Current state**: `needs_lift()` (`crates/almide-codegen/src/pass_anf.rs` lines ~26-67) is now an inverted DENY-list: any heap-typed expression is lifted UNLESS it is one of the deliberately-excluded kinds — simple values/references (`Var`, `EnvLoad`, `FnRef`, literals, `Unit`, `OptionNone`, `EmptyMap`, `Hole`, `Todo`, `Break`, `Continue`), read/borrow operations that don't allocate (`Member`, `TupleIndex`, `IndexAccess`, `MapAccess`, `UnOp`, `Deref`, `Borrow`), and `Lambda`/`ClosureCreate` (excluded on purpose — they must stay in argument position for the WASM closure-table pre-scan, per the 5cae928d regression guard). New `IrExprKind` variants are therefore lifted BY DEFAULT (safe) unless explicitly deny-listed, closing exactly the "miss = leak" failure mode this gap described. No dedicated `spec/lang/anf_lift_test.almd` exists yet, but the design itself is no longer the risk.

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
| 2a | ~~Add missing IrExprKind variants to `needs_lift()`~~ | ✅ **COMPLETE** — `needs_lift()` is a deny-list (lift by default); `Member`/`IndexAccess`/`MapAccess`/`UnOp`/`Deref`/`Borrow` are deliberately excluded (read-only, no new alloc), only `ClosureCreate`/`Lambda` remain excluded by design (must stay in argument position for closure-table pre-scan). Not a gap. | — |
| 2b | Add postcondition assert: after ANF, walk all Call/RuntimeCall/BinOp args and assert each heap-typed arg is `IrExprKind::Var` | Debug-mode panic on violation | 2a |
| 2c | Add ANF-specific spec test: nested heap expressions in every position | `spec/lang/anf_lift_test.almd` | 2a |

**Gate**: after 2b, a missed case triggers a debug-mode panic instead of a silent leak. (2a itself no longer needs a gate — the deny-list design makes a missed variant impossible by construction.)

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

  # 12. Status snapshot (2026-06-03) — fan.* cross-target codegen + semantics

  このラウンドで閉じたギャップ（build-correctness: well-typed fan source → 正しい native binary）:
  - **native fan thunk codegen の E0308 / E0277**: race/any/settle に相異なる
    キャプチャを持つ複数 thunk を渡すと、生成 Rust が `Vec<impl Fn>` に
    別個のクロージャ型を詰めて E0308（"no two closures have the same type"）。
    fan.map にクロージャ VALUE（uniform repr の `Rc<dyn Fn>`、!Send/!Sync）を
    渡すと E0277。WASM は table-dispatch で両方とも正しく動くため、value-blind
    な WASM-only spec がこの native build 破綻を隠していた。修正（20739311）:
    - box パスが fan thunk を un-box する代わりに、race/any/settle は
      `Box<dyn Fn() -> _ + Send + Sync>` で box する。`Box<dyn Fn + Send + Sync>`
      自身が `Fn + Send + Sync` なので runtime の `Vec<impl Fn + Send + Sync>`
      sig は無変更のまま、異種キャプチャ thunk が単一の要素型に統一される。
      IR `RcWrap` に `wrap: FnBox`（`Rc` | `BoxSendSync`）を追加。
    - fan.map は `Rc<dyn Fn>` を受けて sequential 実行（thread を張らない）。
      これでクロージャ VALUE も通る（並列性は失うが結果は同一）。
  - capturing-thunk の値検証 spec を追加（baac55c3）。CI の
    `almide test spec/ --target rust` が native コンパイル経路を恒久的に踏む
    ようになり、この種の native build 破綻が再発しても検出される。

  残る完全性ギャップ（このラウンド未着手 — build ではなく cross-target **SEMANTIC** 乖離）:
  - **fan.race / fan.any の勝者が target で非等価**: native は thread を張って
    wall-clock 最速（非決定的）、WASM は list 順（race は fns[0] のみ実行、any は
    list 順で最初の OK）。同一 well-typed source が target で異なる勝者を返しうる。
    現状は回帰テストを `assert(a or b)` で非決定性許容にして整合させただけ。筋は
    両 target を同一の決定的セマンティクス（例: 並列実行は保ったまま list 順で
    最初の OK を勝者にする）へ揃えること。
  - ~~**fan.timeout が WASM では no-op**~~: **解消 (0.29.0)** — fan.timeout を言語
    から削除 (C-006 flip、E027 tombstone)。デッドラインはホスト境界で課す。
  - **fan.map の err 時挙動が乖離**: native は `.unwrap()` で panic、WASM は err を
    包んで enclosing fn の外へ伝播（return）。err を返す thunk で観測差。
  いずれも build は通る（型は付く）が「同一ソースが target で別挙動」を起こす
  cross-target 等価性ギャップ。[determinism-belt.md](determinism-belt.md) の枠で
  扱うのが妥当。

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
  処理（err → stderr ＋ exit 1）。wasm は `_start = __main_runner` が `main` の `Result` を**捨てて**
  いた ため err が消え、**失敗が成功（exit 0・無出力）に見えた** — 最も危険な乖離。

  **✅ 修正済み（Result エラー経路）** — 設計判断は「WASI/POSIX の作法（成功=exit0 / 失敗=非ゼロ
  +stderr）に従い、フォーマットは Rust Debug の `"引用符"` ワートを捨てて Display で統一」に決定:
  1. **native**: `effect fn main` を `__almide_main` にリネームし、`Err` を `eprintln!("Error: {}", e)`
     ＋ `exit(1)` で出す `fn main` ラッパを生成（`walker/mod.rs`）。出力が `Error: "<msg>"` →
     `Error: <msg>` に（引用符が消える、これが正しい方向）。
  2. **wasm**: `__main_runner` が effect main の `Result` tag（`[tag:i32@0][payload@4]`、tag≠0=`Err`）
     を検査し、`Err` なら `Error: <msg>\n` を fd 2 + `proc_exit(1)`（`emit_wasm/mod.rs`）。
     非 effect `fn main`（`Unit`）は tag を持たないので **effect 限定**（`is_effect`）— でないと ok を
     err と誤判定して全 wasm main が壊れる。
  - 検証: `tests/wasm_runtime_test.rs::{unhandled_main_error_terminates_consistently,
    successful_main_exits_zero_both_targets}`。実測 8 ケース中 7 が native==wasm（exit/stderr 一致）。

  **残存乖離（別バグ）**: `!` で **Option の `None`** を unwrap（`list.first([])!`、ケース 6）は
  wasm がまだ `Err` を main の `Result` に伝搬できず（`Ok` にしてしまう）exit 0 で黙殺。これは
  `__main_runner` ではなく **`!`/Option-`None` の wasm 伝搬 lowering の上流バグ**。`Err` Result の
  伝搬は上記で統一済み。

  これは §12（fan の race/timeout/map-err）と同じ「build は通るが観測非等価」クラス。残りの面的な
  検出には「観測出力（stdout/stderr/exit）を両 target で byte 比較する差分ゲート」（cross-target
  equivalence の rank 1 force-multiplier）が本筋。`tests/wasm_runtime_test.rs` の spec/wasm_cross
  stdout 比較ハーネスがその部分実装（stdout のみ。stderr/exit 比較は本節の 2 テストで先行）。

  # 14. Cross-target divergence burndown map — SUPERSEDED (2026-07-19)

  **この節がかつて所有していた burndown queue はもう存在しない。** 現状は
  [cross-target-completeness.md](cross-target-completeness.md) を見よ — 元の8クラスタ
  カタログ（46件、A〜H + A-case）は **完全に drain 済み（#363–#385）** と明記されている。
  `tests/wasm_runtime_test.rs::wasm_cross_target_spec` の `@xt-allow` 件数も現状 **0**
  （`grep -r '// @xt-allow:' spec/wasm_cross/*.almd | wc -l` = 0、fan.timeout は 0.29.0 で
  言語から削除済み）。本節はもうこのバーンダウンを所有しない — 以下は歴史的記録として残す。

  <details>
  <summary>歴史的記録（2026-06-04 時点、以後は cross-target-completeness.md が正）</summary>

  **ゲートは既に存在する**（#352）。`tests/wasm_runtime_test.rs::wasm_cross_target_spec` が
  `spec/wasm_cross/*.almd` を native+wasm 両方で走らせ **(exit, stdout, stderr) を byte 比較**、
  `// @xt-allow: <理由>` で tracked 乖離を管理（当時 1 件 = float 最短往復）。native がオラクル。

  **面的ハント（並列 61 エージェント・169 プローブ・各乖離を独立再現）で native↔WASM の実バグ
  46 件を検出。fan だけではない。** dedup すると **8 つの根本原因クラスタ**:

  | # | クラスタ（根本原因） | 件数 | 優先 | locus |
  |---|---|---|---|---|
  | A | WASM 文字列が**バイト index**、native は**コードポイント**（chars/slice/reverse/take/codepoint/pad…、reverse は不正UTF-8生成） | 9 | 高 | `emit_wasm/rt_string*` |
  | B | WASM `int/float.parse` が緩い＋**オーバーフロー無検査**（`ok(ゴミ)` を返す） | 8 | 高（無言の誤値） | wasm parse runtime |
  | C | WASM `float.to_string`：`\|x\|≥2^63` で **trap**、`-0.0` 符号消失、to_fixed 丸め差 | 4 | 中高 | wasm float fmt |
  | D | 複合要素の等価が WASM では**ポインタ同一性**、native は構造的（map/set 複合キー, list.contains/dedup on nested） | 4 | 高 | wasm element-eq |
  | E | WASM list が**境界無検査 → OOB ヒープ読み/破壊**（slice/insert/remove_at/swap） | 5 | **高（セキュリティ）** | wasm list runtime |
  | F | 型を変えるクロージャの map.map/set.map が **trap**（indirect call 型不一致） | 2 | 中 | wasm closure dispatch |
  | G | **fan.\*** = §12。WASM は `fns[0]` のみ走る逐次スタブ、native は thread::scope。副作用集合・勝者・全失敗（panic101/trap134/propagate1）が乖離 | 6 | **契約決定が先** | `emit_wasm/calls.rs:1478-1561` + `runtime/rs/src/fan.rs` |
  | H | div/mod by zero（native panic101 vs wasm trap134）、const畳み込み div0（native コンパイルエラー）、Map の Display | 3 | 中 | wasm arith/display |
  | A-case | to_upper/to_lower/capitalize が WASM は**ASCII のみ**（±32 byte-wise）、native は全 Unicode（é→É, ß→SS 伸長, Greek/Cyrillic）。A の敵対的検証で発見 | — | 中（要 Unicode case 表、重い）| `emit_wasm/calls_string.rs` emit_str_case_convert |

  §13（termination）も §12（fan）も、この 8（+A-case）クラスタの一部分。

  **全クラスタ drain 済み**（#363–#385、cross-target-completeness.md で確認）。fan(G) の契約は
  「決定論的 list-order-first」で決着。

  </details>
