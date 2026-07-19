<!-- description: The completion plan to bring the v1 MIR trust-spine to full v0 parity -->
# V1 → V0 Parity — the completion plan

> 2026-07-04 起票。v1（MIR trust-spine）を **v0 相当** まで仕上げるための完成計画。
> /goal で phase 単位に回せるよう、各 phase に **測定可能な exit gate** と **正確な
> 再開点** を付す。v1-backlog.md（個別バグ台帳）の上位に立つ「完成」ロードマップ。

## v1.0.0 SHIP GATE（出荷条件 — 100% 北極星のサブセット）

**「全 wall を 0 にする」のは北極星であって出荷条件ではない。** v1.0.0 として何を名乗るかで
ゲートが変わる：

### 位置づけ A —「検証済みサブセットのコンパイラ」v1.0.0 → **今すぐ出荷可能**
v1 の核（honest wall）は既に成立：受理プログラムは必ず正しく、拒否は clean。既知 miscompile
ゼロ、受理全プログラムで native⇄wasm byte-identical + PCC（ownership∧names∧caps）証明済み、
3-way オラクル green。壁は「バグ」ではなく「未対応」として明示される。この約束の下なら出荷できる。

### 位置づけ B —「v0 の完全な置き換え」v1.0.0 → 下の #1〜4 が致命的ブロッカー
| # | ブロッカー | 状態 |
|---|---|---|
| **B-1** | **動的ディスパッチ（Phase C）** — first-class クロージャ + `.method()`（method/computed ~79 + heap-result match tail 74 の大半）。「普通に書いたコードがコンパイルできるか」を決める実用の本丸。**最大ブロッカー**。 | 未着手 ⚠️ **要再検証（2026-07-19）**: `Op::CallIndirect` は commit 436222c2（2026-06-15、"closures foundation"）で既に SHIP 済み — この doc 自身の起票日 2026-07-04 より**前**。つまりこの行はドキュメント作成時点で既に古かった（stale-since-day-1）。書いた後の劣化ではなく、書いた時点の誤り。current closure-env commits（`git log --oneline --grep="closure" --since=2026-06-15`、[v1-selfhost-machinery.md](v1-selfhost-machinery.md)の Machinery 3 参照）に照らして「未着手」を再確認/再記述してから使うこと。 |
| **B-2** | **derived Codec `.decode()`**（Camp-4 heap-Ok `?`-bind）。**進行中 — 精密診断まで完了、次セッション用の handoff あり**。詳細は下の「B-2 handoff」参照。 | value.field 済 / desugar 残 |
| **B-3** | **nn end-to-end（fast-exp 族）**。wasm oracle は SIMD ではなく scalar libm exp（= self-host 済み math.exp）と判明。**7/7 全て開通・byte一致**（softmax_rows / gelu / swiglu_gate / rope_rotate / multi_head_attention / masked_multi_head_attention / from_q1_0_bytes）。nn の matrix スタックは完全 self-host・byte 検証済み。**✅ 実質クローズ**（残る nn unlinked は fft の enumerate_h/zip_h の 4 補助サイトのみ、非推論経路）。 | ✅ 7/7 |
| **B-4** | **native ターゲット** — v0-native matrix codegen 破損（引き継ぎ）。native を出荷対象にする場合のみ。 | 外部ブロック |

**Phase E（caps 証明の完全形）と G（契約網羅）は v1.0.0 に不要** — 北極星（下記 DONE 基準）の
一部だが出荷ゲートではない。推奨進行: **B-3 完走 → B-2 → B-1**（近い順・実用インパクト順）。

## 完成の定義（DONE の判定基準 = 100% 北極星）

v1 が v0 相当とは、次を**同時に**満たす状態を指す（すべて既存ツールで機械判定可能）：

1. **walls = 0**: `classify_corpus` の `walled real (lowering)` が 0
   （structural native-FFI の 5 は除外対象のまま）。spec 全 4556 関数 + org 全 corpus。
2. **output-parity 完全**: `proofs/output-parity.sh` が MISMATCH=0 / RUNERR=0、
   baseline が全 runnable fixture を網羅（v0 が走る全プログラムで byte 一致）。
3. **PCC 不変**: `proofs/corpus-wall.sh` が 3プロパティ（ownership ∧ names ∧ caps）で
   ACCEPT を維持（各 phase で回帰させない）。
4. **3-way オラクル green**: `interp_cross_target_test` が全 fixture で
   native==wasm==interp consensus（abstain ledger は 0 へ収束）。
5. **org byte-verify 両ターゲット**: `scripts/org-trust-status.sh` の BYTE_VERIFIED が
   全 org リポジトリで native + wasm 両方 pass。
6. **native ターゲット復旧**: `almide run`（v0-native）が matrix 系を含め全 spec で通る。

> ⚠️ **STALE — needs refresh (flagged 2026-07-19)**: this dashboard is dated
> "2026-07-04 セッション後" and the figures below (spec in-profile 4374/4556, real walls 469,
> stdlib 211/171 in Phase D) are almost certainly out of date — `crates/almide-mir` alone had
> **283 commits** in the two weeks since (`git log --oneline --since=2026-07-05 -- crates/almide-mir`,
> counted 2026-07-19). Verified fresh: `ls stdlib/*.almd | wc -l` = **273** files today (vs the
> "211 `.almd`" cited in Phase D below) — the stdlib surface has grown by ~62 files since this
> doc was written; the "171 登録" self-host-registration count is very likely stale too (see
> [v1-selfhost-machinery.md](v1-selfhost-machinery.md)'s 2026-07-19 refresh: the registry now
> wires 232 stdlib files / 993 unique self-host entries — a different counting method than
> Phase D's "211/171" but confirms the same magnitude of drift). None of the other dashboard
> numbers (walls/parity/PCC counts) were independently re-verified here — treat them as stale
> until re-run through `classify_corpus`/`output-parity.sh`/`corpus-wall.sh`.
>
> 現在地（2026-07-04 セッション後）: spec in-profile **4374/4556（96%）**、実 walls
> **469**、nn walls **0**、parity baseline 180、PCC ACCEPT（3プロパティ）、3-way green。
> miscompile は既知ゼロ（honest wall のみ）。
> **セッション成果**: Phase A 完了（+38 in-profile、guard miscompile を wall→正しく lower に）
> ＋ Phase B 主要バケット前進（Option[heap] フィールド match / let束縛 custom-variant match
> / List[record-ctor variant] リテラル — 計 +14 in-profile）。実 walls **520→469**。
> **残り最大レバーは Phase C（method/computed dispatch）**: `.decode()`/`.encode()` の
> derived Codec メソッドと first-class クロージャで、heap-result match tail 74・
> method/computed 79 sites の大半を占める。roadmap が「大」とする architectural 領域。

## ✅ Phase A 完了（2026-07-04）

`desugar_guard`（mod_p6）: `guard cond else E; rest` を bottom-up 再帰で
`if cond then { rest } else E` に構造化。**関数早期 return**（`else err(…)` は tail の
heap/scalar-result-if へ）と**ループ continue**（`else continue` → `else ()` で continue
ノード除去、スカラーループが受理）を開通。break は当面 wall（真のループ早期脱出が必要）。
call-count-invariant なので `mir == ir` 維持、count 側（classify_corpus）にも desugar 適用。
guard-else バケット **39→0**、in-profile **4322→4360**。旧「wall」pin は「desugar 発火」
検証に更新、end-to-end pin 追加。

## Phase B 進行中 — heap-result の位置網羅

- ✅ **Option[heap] / Result[heap] フィールド subject の match**
  （`match u.email { some(e) => "…${e}…", none => u.name }`）: borrowed フィールドの
  variant handle を tracking（materialized_options/results + heap_elem_lists）し、heap-payload
  borrow bind を実行。
- ✅ **let束縛 custom-variant heap-result match**（`let nm = match s.shape { Circle(_) =>
  "circle", … }; "${nm}…"`）: `desugar_match_to_if` が literal 専用で declines していたのを、
  `wrap_match_arms`（各 arm に継続を押し込み Match を保持）で開通。bucket **17→2**。
- ✅ **List[record-ctor variant] リテラル**（`[Click { x, y }, KeyPress { key }, Close]`）:
  list-literal builder の要素 gate に `IrExprKind::Record { name: Some(ctor) }` を追加、
  `try_lower_variant_ctor` で材料化。bucket **29→24**。
- 残 sub-case: heap-result match tail（74、大半は `.decode()` メソッド = Phase C）、
  default-field variant ctor の束縛（Group の `items = []` 省略）、compound interp
  （Map/heterogeneous tuple の `${…}` — Phase D self-host）。

## Phase A（設計メモ・原文保持）— 制御フローの完成（言語機能の唯一の欠落）

**A1. early-return / guard-else の modeling**（39 sites）。
`guard cond else E; rest` を `if cond then { rest } else { E }` に構造化（block 末尾の
heap-result-if machinery を再利用、残 statement を then 枝へ畳む）。`return` 式も同経路。
- **exit gate**: `guard-else early return` バケット → 0。guarded な
  `Result[String,String]` ok/err tail（今 backlog T4-7 で保留中）も自動開通。
- **再開点**: `lower_stmt` の `IrStmtKind::Guard`（現在は honest wall）+ `lower_body_into`
  の statement ループを、Guard 検出で then/else split する形へ。
- **重み**: 中〜大（core lowering の構造変更）。**parity 回帰に最注意**。

## Phase B — heap-result の位置網羅（最大バケット ~130 sites）

v0 は heap-result の match/if/lambda を「あらゆる位置」で返せる。v1 は位置ごとに
ビルダを足す方式で穴が残る。

- **B1** `heap-result match` の tail / let束縛（77 + 17）。
- **B2** `heap-result if/lambda` の残位置（9）。
- **B3** call-arg 位置の `List[heap] literal`（29）+ `string interpolation`（30）。
- **exit gate**: 上記バケット群 → 0。
- **再開点**: `tail.rs` / `control_p4.rs` の heap-result-arm builders、`calls_p2.rs` の
  引数材料化。今 session で record-ctor / ArrV consumer を埋めた延長線。
- **重み**: 中（既存機構の拡張、パターンは確立済み）。

## Phase C — 動的ディスパッチ（first-class クロージャ）

v0 はクロージャ変換で全対応。v1 は inline-defunctionalize できる範囲のみ。

- **C1** `method/computed heap call`（61 + 11）— 持ち回るクロージャの opaque 呼び出し。
- **C2** `heap-result Lambda` 返し（9）。
- **exit gate**: closure-passing プログラムが lower（`unresolvable method/computed`
  バケット → 0）。
- **再開点**: `funcref_value_of` / `CallIndirect` 経路の拡張、closure env の
  value-model 材料化。docs/roadmap/active/closure-architecture-v2.md と接続。
- **重み**: 大（クロージャ env の所有権を PCC に載せる設計）。

## Phase D — stdlib self-host の完成

未 self-host = wall。現状 211 `.almd` / 171 登録。残る穴：

- **D1 fast-exp 族 7本**（`softmax_rows` / `gelu` / `swiglu_gate` /
  `multi_head_attention` ×2 / `rope_rotate` / `from_q1_0_bytes`）。**nn end-to-end の
  直接ブロッカー**。v0 は almide-kernel の SIMD fast-exp を **lane 順加算**するため、
  bit-exact 転写に exp_pd_{wasm,neon} 多項式（Horner 6項）+ 2-lane 部分和 + 奇数 tail
  の libm exp を要す。**医療グレード: ULP 差分テストを先に組んでから着手**、片手間禁止。
  参照実装 `crates/almide-kernel/src/silu.rs::exp_pd_neon`、前例 `stdlib/math_exp.almd`。
- **D2 effectful brick 群**（`fan.*` 並行 / `http.*` / `net.*` / `datetime.parse_iso` /
  `env.os` / `env.temp_dir` / `fs.stat` / `process.env`）。各々 WASI prim + capability
  宣言 + admitted-effectful 登録。fs.read_bytes（今 session）と同流儀で開くものから。
  fan の non-determinism は capability 証明に載せる設計判断が別途必要（→ Phase E）。
- **D3 残 pure combinator**（`list.zip`/`enumerate` の rich 要素版、Map/Set 未対応 repr、
  等）。ROI 順に。
- **exit gate**: nn unlinked → 0（D1 で 7、D3 で 5）。effectful-27 set functional。
- **重み**: D1 大（精度）、D2 中×n（brick 毎）、D3 小〜中。

## Phase E — capability / effect の完成

- `fan.*`（並行・非決定）を caps 証明モデルに載せる設計。`http`/`net`/`process` の
  capability 種別確定。effectful-27-blueprint.md を吸収。
- **exit gate**: effectful stdlib の wall（`needs a declared capability`）→ 0。
- **重み**: 大（証明機構の拡張）。

## Phase F — native ターゲット parity

- **F1（引き継ぎ）v0-native matrix codegen 破損の修復**: `bridge::AlmideMatrix` の
  enum 化と vendored glue（旧 Vec API）の不整合。flat-ABI/burn リワークの過渡状態。
  **所有セッションへ引き継ぎ**、または本ロードマップで巻き取り判断。
- **F2 v1 MIR→Rust native emit** の production 化（flight-profile の keystone）。
- **exit gate**: `almide run` が matrix 系含め全 spec 通過、native==wasm byte 一致。
- **重み**: F1 中（既存破損の修復）、F2 大（flight 品質の床）。

## Phase G — 契約・計測の ratchet

- matrix.mul（k昇順）/ from_bytes OOB（全ゼロ行列）の契約台帳起票（**F1 完了が前提** —
  native oracle が無いと byte-verify 不能）。
- coverage を目標値へ（control_p5 defunc エンジン 43% が最大の未踏面）。
- **exit gate**: 新 C-NNN + fixture、`check-contracts.sh` 通過。

## 推奨着手順（依存 + レバレッジ）

**A → B → D1 → C → D2/D3 → E → F → G**。

- **A（early-return）** は言語機能の唯一の穴で、B/D の tail 材料化の前提にもなるので最初。
- **B** はパターン確立済みで件数が最大 → 早期に walls を大きく削れる。
- **D1（fast-exp）** は nn end-to-end の唯一のブロッカーなので、精度テスト整備と並行で
  独立レーンとして進める（他 phase を待たない）。
- **C（クロージャ）** は最大の architectural 山の一つ、B の後。
- **F1** は v0 破損の修復で G の前提。並行セッション状況次第で順序前後可。

## /goal の切り方（提案）

phase 単位で goal 設定するのが回しやすい。例：
- 「Phase A（guard-else early return）を完成させ、`guard-else` バケットを 0 にする。
  全ゲート green を維持」
- 「Phase B の heap-result match 網羅（tail/let/arg）で 3 バケットを 0 に」
- 「Phase D1 fast-exp 7本を ULP 差分テスト付きで self-host、nn unlinked を 7 減らす」

各 goal の完了判定は上記 exit gate（`classify_corpus` の該当バケット count + 横断
バッテリー green）で機械的に確認できる。

## B-2 handoff（次セッション再開点 — 2026-07-05）

### 状況
- ✅ `value.field` を self-host（Object タグチェック、byte一致、commit 済み）。
- ✅ **下流 lowering は正しい**: `effect fn dec(v) = { let fv=value.field(v,k)!; let x=value.as_T(fv)!; …; ok(R{…}) }`
  （別 bind 形）は **multi-field でも valid wasm・byte一致**（scratch: man5/man6 の `dec`）。
- ❌ **derive の `Basic.decode`（plain fn, `?`）は invalid wasm**（`type mismatch: expected i32, found i64`）。

### 精密診断（ここが肝）
デバッグ用の **desugared-IR ダンプを実装済み**（commit 済み、env ゲート）:
```
DBG_DESUGAR_FN=<fn名> [DBG_DESUGAR_RAW=1] almide/render_program <file>   # eprintln に desugared IR
```
`crates/almide-mir/src/lower/mod_p6.rs::dump_ir` / `dump_desugared_ir`、呼び出しは
`lower_function_all_impl`（mod.rs）。

**発見**: man6（`type Basic: Codec` + 手書き effect-fn `dec` の両方を含む単一ファイル）で
`dec`（valid）と `Basic.decode`（invalid）の **desugared IR は VarId 番号を除いて完全一致**
（`DBG_DESUGAR_RAW` の diff で確認）。にもかかわらず **MIR は異なる**（`Basic.decode` は local が
2 個多く、v91 付近で i32/i64 が入れ替わる — wat の local 宣言で確認可能）。

→ **同一 desugared IR が別の MIR に lower される**。差は fn レベル:
  - `dec` は `is_effect: true`、`Basic.decode` は `is_effect: false`（plain fn）。
  - derive の param `_v: Value` は `ParamBorrow::Own`（手書きは borrow）。ただし現状の
    `bind_params`（mod_p3）は `ParamBorrow` を見ず全 heap param を borrow 扱い → これ単体は
    差にならないはず。
  - **実際の lowering 経路は `desugar_heap_branches(func.body)` → TCO → `lower_body_into`** で、
    ダンプの `desugar_all` とは入口が違う。**次はここを疑う**: pre_tco の `desugar_heap_branches`
    が二つの関数で違う中間形を作る／`lower_body_into` の実際の desugar 済み body をダンプして
    `desugar_all` 版と突き合わせる。

### 次の一手（順に）
1. **実 lowering 経路の body をダンプ**: `lower_body_into` の入口（desugar 完了後）で同じ
   `dump_ir` を出し、`dec` と `Basic.decode` で diff。ダンプが `desugar_all` 経由なので、
   pre_tco 経路との差がここで出るはず。
2. desugar-lift の正解形は **man5/man6 の `dec` の別 bind nested-match**（`subj: vN`（lift 済み）
   であって `subj: Call(value.as_int, [Try(...)])` ではない）。これを derive でも生成する。
   前回の試み（take_or_recurse に Try 認識 + 外側 unwrap 再帰 + callarg-unwrap を effect-unwrap
   の前に）で単一/effect は開通、**multi-field の plain-fn `?` だけ invalid wasm**（revert 済み、
   git 履歴の diff 参照）。上記 #1 で「同一 desugared でも MIR が違う」理由を潰してから再適用する。
3. **honest-wall 厳守**: invalid wasm は絶対に commit しない。各試行で `render + wasmtime` の
   valid 検証を必須にする。
