<!-- description: The completion plan to bring the v1 MIR trust-spine to full v0 parity -->
# V1 → V0 Parity — the completion plan

> 2026-07-04 起票。v1（MIR trust-spine）を **v0 相当** まで仕上げるための完成計画。
> /goal で phase 単位に回せるよう、各 phase に **測定可能な exit gate** と **正確な
> 再開点** を付す。v1-backlog.md（個別バグ台帳）の上位に立つ「完成」ロードマップ。

## 完成の定義（DONE の判定基準）

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

> 現在地（2026-07-04）: spec in-profile **4322/4556（95%）**、実 walls **520**、
> nn walls **0**（unlinked render-reject 12）、parity baseline 180、PCC ACCEPT、
> 3-way green（ledger 134）。miscompile は既知ゼロ（honest wall のみ）。

## Phase A — 制御フローの完成（言語機能の唯一の欠落）

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
