説明：型が正しい source から正しい binary までの鎖で、機械的または数学的な証明がまだ欠けている層
<a id="correctness-guarantee-gaps"></a>

# 正しさ保証の欠落

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](correctness-guarantee-gaps.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](correctness-guarantee-gaps.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](correctness-guarantee-gaps.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/correctness-guarantee-gaps.md)

> 原文注記：廃止 backend 一覧と5〜6月ログは歴史で、現在の脆弱性や未実装一覧ではない。冒頭の現行境界は旧release警告・gate未実装の記述に優先し、6月4日記録は6月3日のstdoutのみ被覆を更新する。algorithm RC証明、runtime挙動、翻訳適合、byte一致を区別する。報告46bugとA–H計41／A-case未計数の不整合は推測で合わせない。旧fan勝者提案やOption伝搬の残件は今日の状態と断定しない。command・gate変更・testは実行していない。


> 「well-typed source -> correct binary」の鎖：機械的／数学的証明が欠けている層

状態：**歴史的な欠落分析と、その後の訂正ログ**。以下の元 Perceus／`emit_wasm` architecture は廃止済みであり、この作業一覧を現行 structural backend の棚卸しと解釈しないこと。現行経路と証明境界は `src/wasm_route.rs`、`proofs/receipt.sh`、`TRUSTED_BASE.md` にある。

<a id="evidence-boundary"></a>

## 証拠の境界

元の見出し「heap alloc は厳密に 1 回解放される」は、algorithm の定理が確立する範囲を超えていた。モデル化した RC event の証明だけでは、Rust 翻訳、生成 drop routine、任意の実行 program の証明にはならない。以下の歴史的分析自身が、この model と実装の欠落を記録している。

現行 IR postcondition（`crates/almide-codegen/src/pass.rs`）は debug だけでなく release build でも失敗させる。後の歴史的 milestone にあるその変更提案は、現在欠けている gate ではない。structural runtime の証明と残る trusted seam は、旧固定 23 定理／Perceus／StackBalance 一覧ではなく現行 receipt から読むこと。

<a id="original-gap-inventory-historical-architecture"></a>

## 元の欠落一覧（歴史的 architecture）

<a id="1-wasm-emitter-emit_wasmrs--largest-gap"></a>

### 1. WASM emitter（`emit_wasm/*.rs`）— 最大の欠落

手書き WASM instruction emission。数百の `wasm!()` macro call の stack effect は人の review のみで検証する。

- `rt_string.rs`、`rt_list.rs` など：手作業の instruction sequence。
- `expressions.rs`：BinOp、Match、Record の branch stack effect は目視検査。
- layout offset：`LayoutRegistry` が一元化するが、正しい使用は手作業。

**当時の状態**：`emit_wasm/engine/` に WasmBuilder + WasmIR + LayoutRegistry 基盤がある。部分移行済み（list_layout.rs は WasmBuilder を使用）。`expressions.rs` と `rt_*.rs` の大半はまだ raw `wasm!()` を使う。

<a id="2-anf-pass--heap-alloc-visibility-unproven"></a>

### 2. ANF pass — heap alloc の可視性が未証明

Perceus が追跡できるよう、ANF はすべての heap intermediate を VDecl へ lift しなければならない。`needs_lift()` は既知の case を扱うが、全 case を扱う証明はない。漏れ＝heap leak。

**当時の状態**：`needs_lift()` は Call、RuntimeCall、BinOp、If、Match、Block を扱う。**既知の欠落**：Fan、List/MapLiteral/Record/Tuple literal、StringInterp、IterChain、ForIn、While、UnwrapOr、OptionalChain、Member、IndexAccess、ClosureCreate は match しない。専用 ANF test はない。

<a id="3-closure-conversion--env-layout-correctness"></a>

### 3. Closure conversion — env layout の正しさ

`ClosureConversionPass` は capture 変数を env struct に詰め、計算した offset の `EnvLoad` で読む。offset の正しさは test だけで検証する。

**当時の状態**：offset は `index * 8`、capture は VarId 順。出力は型に応じた load（I32/I64/F64）を使用。offset 境界や env size 整合性に対する `debug_assert` はゼロ。test 被覆は `closure_nested_capture_test.almd` + `monkey06_closures_test.almd`。

<a id="4-type-inference---ir-lowering-fidelity"></a>

### 4. 型推論 → IR lowering の忠実性

`almide-frontend` が推論した型は IR `expr.ty` に忠実に反映されなければならない。`ConcretizeTypes` postcondition 検査はあるが、全 pattern の被覆は未証明。

**当時の状態**：`resolve_node_ty` は 17 variant を明示的に扱う。24 以上が None を返す（MapLiteral、Record、SpreadRecord、Range、MapAccess、StringInterp、Try、UnwrapOr、ToOption、OptionalChain、Clone、Deref、Borrow、BoxNew、RcWrap、ClosureCreate、FnRef、ForIn、While、Fan など）。postcondition audit（`audit_remaining_unresolved`）は未解決型を検出するが、全 variant を訪問した保証はない。意図的な best-effort 設計。

<a id="5-perceus-incdec-insertion--lean-to-rust-fidelity"></a>

### 5. Perceus Inc/Dec 挿入 — Lean から Rust への忠実性

Lean 4 の定理は algorithm の正しさを証明するが、Rust 実装は機械的抽出ではなく手翻訳である。Lean spec と手作業で比較して適合を検証する。

**当時の状態**：欠落のうち最も強い裏づけがある。`perceus_verified.rs` が Lean 証明を Rust へ写す。proptest が is_freed/has_dec を検証。PerceusVerifyPass は全 WASM build で動く。`perceus_monkey_test.almd` に敵対 case がある。Lean 証明は 23 定理、0 sorry。

<a id="risk-ranking"></a>

## リスク順位

1. **WASM emitter** — 最大リスク、最広範囲、静的検査なし。
2. **ANF `needs_lift()`** — 無言の失敗（crash ではなく leak）。
3. **closure env offset** — 誤 offset＝メモリ破損。
4. **型 lowering の忠実性** — postcondition 検査で緩和。
5. **Perceus Lean 適合** — 広範な test suite で緩和。

---

<a id="original-milestone-steps-retain-with-the-dated-follow-up-logs"></a>

## 元の milestone 手順（日付付き後続ログとともに保持）

<a id="gap-1-wasm-emitter---wasmir-migration"></a>

### Gap 1：WASM emitter → WasmIR 移行

追跡先：[wasm-engine-redesign.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/wasm-engine-redesign.md)

| 手順 | 説明 | 成果物 | 依存 |
|------|-------------|-------------|------------|
| 1a | 残る raw `wasm!()` call site を監査 | WasmBuilder 未使用 file/function 一覧 | — |
| 1b | `rt_string.rs` を WasmBuilder に移行 | rt_string の raw `wasm!()` がゼロ | — |
| 1c | `rt_list.rs` を WasmBuilder に移行 | rt_list の raw `wasm!()` がゼロ | — |
| 1d | `expressions.rs`（BinOp、Match、Record）を移行 | expressions の raw `wasm!()` がゼロ | — |
| 1e | WasmIR Op enum に stack-effect 型注釈を追加 | 各 Op が `(pops, pushes)` を宣言 | 1b-1d |
| 1f | WasmIR 上の stack-effect verifier を実装 | 正味 effect が期待値と異なる instruction sequence を拒否 | 1e |
| 1g | 旧 raw emission path を削除 | `wasm!()` macro を削除または dead 化 | 1f |

**Gate**：1f 後は、encoding 前に全 WASM function の instruction stream の stack balance を静的検証する。

<a id="gap-2-anf-needs_lift-completeness"></a>

### Gap 2：ANF `needs_lift()` の完全性

| 手順 | 説明 | 成果物 | 依存 |
|------|-------------|-------------|------------|
| 2a | `needs_lift()` に欠けた IrExprKind variant を追加 | Fan、List、MapLiteral、Record、Tuple、StringInterp、IterChain、ForIn、While、UnwrapOr、OptionalChain、Member、IndexAccess、ClosureCreate | — |
| 2b | ANF 後、全 Call/RuntimeCall/BinOp 引数を走査し、heap 型引数が各々 `IrExprKind::Var` である postcondition assertion を追加 | 違反時 debug-mode panic | 2a |
| 2c | ANF 専用 spec test：各位置の nested heap expression | `spec/lang/anf_lift_test.almd` | 2a |

**Gate**：2b 後は、case 漏れが無言の leak ではなく debug-mode panic を起こす。

<a id="gap-3-closure-env-offset-verification"></a>

### Gap 3：Closure env offset 検証

| 手順 | 説明 | 成果物 | 依存 |
|------|-------------|-------------|------------|
| 3a | EnvLoad emission に `debug_assert!(index < captures.len())` を追加 | 境界外 index で panic | — |
| 3b | ClosureVerifyPass（ClosureConversionPass の postcondition）を追加：各 lifted fn の全 EnvLoad index が param env_size / 8 未満と assert | postcondition として登録 | — |
| 3c | closure capture fuzzer：ランダム capture pattern（0–20 変数、混合型、nested） | `tests/closure_env_test.rs` の proptest | 3b |

**Gate**：3b 後は offset mismatch を compile 時に検出する（debug build）。

<a id="gap-4-concretizetypes-postcondition-coverage"></a>

### Gap 4：ConcretizeTypes postcondition 被覆

| 手順 | 説明 | 成果物 | 依存 |
|------|-------------|-------------|------------|
| 4a | 単純な `resolve_node_ty` case を追加：StringInterp→String、Clone→expr.ty、Deref→inner、Range→List[Int] | None を返す variant を 24 から約 15 へ減らす | — |
| 4b | postcondition audit に variant ごとの訪問 counter を追加 | CI log に未訪問 variant（被覆の blind spot）を表示 | — |
| 4c | whitelist 外 Unknown 型を audit の hard error にする | whitelist 外 variant の Unknown＝無言の fallthrough ではなく compile error | 4a, 4b |

**Gate**：4c 後は、whitelist にない Unknown を生成する新 IR variant が build を失敗させる。

<a id="gap-5-perceus-lean-rust-conformance"></a>

### Gap 5：Perceus Lean→Rust 適合

| 手順 | 説明 | 成果物 | 依存 |
|------|-------------|-------------|------------|
| 5a | 差分 test：IR を JSON 化し Lean `perceusTransform` と Rust `perceus_fnbody` を両方動かし Inc/Dec 位置を比較 | `tests/perceus_differential_test.rs` | — |
| 5b | proptest 被覆を拡大：closure capture、mutable 再代入、nested match | `perceus_verified.rs` の追加 proptest strategy | — |

**Gate**：5a は機械的適合の証拠になる。完全抽出（Lean→Rust codegen）は長期的な目標で、近い段階の手順ではない。

---

<a id="priority-order"></a>

## 優先順位

低工数・高効果を先に：

1. **2a + 2b** — ANF needs_lift() 修正と assertion。週単位でなく日単位。無言の leak リスクを塞ぐ。
2. **3a + 3b** — Closure offset assertion。追加は容易。
3. **4a** — ConcretizeTypes の容易な case。単純。
4. **5b** — Perceus proptest 拡大。低工数、漸進的。
5. **1b-1g** — WASM emitter 移行。最大工数、別途追跡。

<a id="11-status-snapshot-2026-05-30--closure--where--fold-inference-round"></a>

  # 11. 状態記録（2026-05-30）— closure / where / fold 推論ラウンド

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
    全て compile error になる破壊的変更。先に spec/ 全量で
    影響件数を測ること。
  - **under-constrained を明確に拒否**: `ok([])` を fold init にし err 値が
    一度も現れないケースは err 型 `E` が原理的に未制約 → 現状
    POSTCONDITION を踏むが warning 止まり。「曖昧なら型エラー」にするには
    constraint solving 後の到達可能性解析（その TypeVar が本当に決まらない
    か／後で決まるか）の線引き設計が要る。§5 の格上げと同時に行うのが筋。

<a id="12-status-snapshot-2026-06-03--fan-cross-target-codegen--semantics"></a>

  # 12. 状態記録（2026-06-03）— fan.* 複数 target codegen と意味論

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
  cross-target 等価性ギャップ。[determinism-belt.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/determinism-belt.md) の枠で
  扱うのが妥当。

<a id="13-status-snapshot-2026-06-03--cross-target-program-termination-on-unhandled-error"></a>

  # 13. 状態記録（2026-06-03）— 未処理エラー時の複数 target program 終了

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
  - 検証: `tests/wasm_runtime_test_parts/p3.rs::{unhandled_main_error_terminates_consistently,
    successful_main_exits_zero_both_targets}`。実測 8 ケース中 7 が native==wasm（exit/stderr 一致）。

  **残存乖離（別バグ）**: `!` で **Option の `None`** を unwrap（`list.first([])!`、ケース 6）は
  wasm がまだ `Err` を main の `Result` に伝搬できず（`Ok` にしてしまう）exit 0 で黙殺。これは
  `__main_runner` ではなく **`!`/Option-`None` の wasm 伝搬 lowering の上流バグ**。`Err` Result の
  伝搬は上記で統一済み。

  これは §12（fan の race/timeout/map-err）と同じ「build は通るが観測非等価」クラス。残りの面的な
  検出には「観測出力（stdout/stderr/exit）を両 target で byte 比較する差分ゲート」（cross-target
  equivalence の rank 1 force-multiplier）が本筋。`tests/wasm_runtime_cross_target.rs` の spec/wasm_cross
  stdout 比較ハーネスがその部分実装（stdout のみ。stderr/exit 比較は本節の 2 テストで先行）。

<a id="14-cross-target-divergence-burndown-map-2026-06-04--面的ハントの結果"></a>

  # 14. 複数 target 乖離の解消地図（2026-06-04）— 面的ハントの結果

  **ゲートは既に存在する**（#352）。`tests/wasm_runtime_cross_target.rs::wasm_cross_target_spec` が
  `spec/wasm_cross/*.almd` を native+wasm 両方で走らせ **(exit, stdout, stderr) を byte 比較**、
  `// @xt-allow: <理由>` で tracked 乖離を管理（現状 1 件 = float 最短往復）。native がオラクル。
  よって本丸は「ゲートを建てる」ではなく **`@xt-allow` 債務の burn down**（traversal-totality の
  LEGACY_DEBT と同型の ratchet）。

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

  **進捗（burndown, 2026-06-04）**: ✅ **E**（#363 マージ, OOB 境界チェック）、✅ **B-int**（#364 マージ, parse オーバーフロー/符号/trim + 4 エラー文字列）、✅ **A**（文字列コードポイント化: 4 UTF-8 ヘルパー + 全 op 変換、負 n は i32::MAX クランプ、2-arg slice の i64::MAX デフォルトは i64 クランプで修正）。残: D / C+B-float / H / F / G(fan) / A-case。

  **fan(G) は契約の決定が前提**（§12 の深掘り）。native/wasm のどちらも今は仕様逸脱（native race は
  Ok のみ拾う＝実質 any、wasm は fns[0] のみ）。推奨契約は **「決定論的 list-order-first」**: 両 target
  で全 thunk 実行 → リスト順で最初の OK を勝者 → 全失敗は統一 Err を main-error 終了パス（§13）へ。
  これで並行非決定性が消え native==wasm が自明に成立し差分ゲートに乗る。代替「true Promise.race
  （最初に完了、Ok/Err 問わず）」は wall-clock 依存で決定論にならずゲートに乗らない。

  **burndown 手順**（LEGACY_DEBT と同じ grandfather→drain）: 各クラスタの決定論 repro を
  `spec/wasm_cross/` に追加 → 根本修正 → `@xt-allow` を外す。fan の勝者は非決定的なので raw race は
  byte ゲートに不適 — 全失敗 / 副作用集合 / 順序保証のある fan.settle で枠組む。
  **推奨順（価値×着手容易さ）**: **E（セキュリティ）→ B（parse）→ A（文字列）→ D（等価）→
  C（float）→ H（div/mod）→ F（closure）→ G（fan、契約決定後）**。各クラスタ = 1 PR。
  [determinism-belt.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/determinism-belt.md) の枠と接続。
