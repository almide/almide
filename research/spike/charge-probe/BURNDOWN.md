# Branch burn-down — worktree-stage1-charge-probe の残件台帳

> 方針: **全項目を潰し切る**。各項目は完了条件（何をもって潰れたとするか）を持ち、
> 潰したら `[x]` + コミット SHA を記す。新しく見つけた残件はこの台帳に**追記してから**
> 潰す（台帳外の作業を作らない）。develop への merge はユーザーの明示 GO まで行わない。
>
> 現在地: 決定層（probe / bounded / race）+ 表面統一（Wave 1）+ P0 2 件まで完了、
> 全 workspace テスト緑。以下が残り。

## Tier 1 — 意味論の大物

- [ ] **T1-1 strict cap + unwind（trap 可視窓 + 発散カット）**
  charge site を check-then-charge 化し、枯渇を region 境界へ巻き戻す metered-ABI。
  完了条件: race の敗者 trap が可視窓の外で「起こらなかったことになる」fixture
  （spec の挙動表の trap 行 2 種）が両ターゲット一致。発散 callee が budget で
  切れる fixture。deviation 4/7 の閉鎖。
- [ ] **T1-2 metered-clone 特殊化**
  bounded/race から到達可能な関数だけを `__fuel$` 変種に複製（mono の追加次元）。
  完了条件: bounded を含むプログラムの非 region 経路が計量ゼロ（生成物 diff で
  確認）+ 既存 fixture 全緑。deviation 1 の閉鎖。
- [ ] **T1-3 native heap-Result ABI（裸形の開通）**
  `?? ` なしの `let r = fan.bounded(...)` / race が native v1 で描画される。
  前提となる既存 rung 制限（Result 返し plain fn）の解除を含む — 影響は fan を
  超える（rung 全体の前進）。完了条件: 裸形 fixture が native v1 で緑。

## Tier 2 — 意味論の中物

- [ ] **T2-1 body/arm の full block 化**
  単一 call 制限の解除（`let` の書ける block を bounded body / race arm に）。
  完了条件: ADR の bounded.almd 例（parse→filter→render の 3 行 body）が動く。
  実装はアウトライナの一般化（自由変数のパラメタ化）。
- [ ] **T2-2 Result arm の Err スキップ（race）**
  arm が Result を返せて、Err arm は候補から外れる（any と対称の仕様）。
  完了条件: Err/Ok 混在 arm の fixture が仕様表どおり、両ターゲット一致。
- [ ] **T2-3 mapper 形 any/settle（Wave 2 宣言分）**
  `fan.any(xs, f)` / `fan.settle(xs, f)` — 動的リスト + 1 閉包。早期打ち切り
  （any）を含む。完了条件: 「宣言済み・未実装」診断の撤去 + fixture 両ターゲット。
- [ ] **T2-4 settle block の tuple 戻り**
  v1 の List[Result] → fan-v2.md の `(Result[A], Result[B])` 契約へ。異型 arm 対応。
  完了条件: fan-v2-examples/settle.almd の health_report 例が動く。
- [ ] **T2-5 S3 演算 matrix**
  `T + T` / `T - T`（0 飽和）/ `T × Int`、`T × T` と時計混合の型エラー。
  完了条件: ADR-0001 S3 表の全セルが checker テストで pin。

## Tier 3 — 小物（半日圏）

- [ ] **T3-1 飽和演算 + 負値 trap**（構築子の overflow 飽和、負引数の決定的 trap、
  両ターゲット一致 fixture）
- [ ] **T3-2 UFCS 曖昧診断**（`n.ms()` → compute/duration 両候補の名指しエラー）
- [ ] **T3-3 S6 matrix gate 群**（構築子 12 セル / 単位集合の CLI 共有 / 時計宣言列。
  実行可能テストとして常設）
- [ ] **T3-4 interp の budget prim 対応**（3-way oracle の復帰。最低限: 明示 abstain
  ではなく prim 実装 — thread-local カウンタで semantics 一致）
- [ ] **T3-5 Dyn charge**（可変コスト op の従量課金 `1 + ⌈size/16⌉` 系。CM-1 v0.3）
- [x] **T3-6 CM-1 定数の単一ソース化**（13135f61 — wasm render は補間、native shim は
  template 注入で `CM1_NS_PER_CHARGE` の 1 定義に。統合ゲートに artifact 検査を追加）
- [x] **T3-7 D5 校正ゲートの常設**（9c3d78a7 — 常設した瞬間 v0.2 の混入測定を
  ratio 0.05 で検出。CM-1 v0.3 = 3ns/unit に再校正、境界 fixture は ns 精度の
  exact flip（bounded 3006ns / race 1506ns、両ターゲット同一）に強化）
- [ ] **T3-8 fan{} 並列 native × budget の裁定**（最低限: 併用を checker で拒否 or
  atomic 化 + 決定順序の設計判断を文書化。無定義状態の解消）
- [ ] **T3-9 レポート表示 `--time-report`**（決定的 ms + 実測壁時計の併記出力 —
  ADR D5 の tooling 面。`--fuel-probe` の出力を人間可読の ms 表示に格上げ）

## Tier 4 — merge 準備（branch 上で先行、PR は GO 後）

- [ ] **T4-1 CHEATSHEET 反映**（fan.bounded / fan.race / block any/settle /
  compute.ms — LLM が読む文書。MSR 直結、最優先）
- [ ] **T4-2 SPEC.md §13 の書き直し**（決定層の全意味論。§13.1/13.2 矛盾の後日談を
  正しい形で）
- [ ] **T4-3 C-NNN 契約下書き + fixture 正式化**（fuel fixtures を spec/wasm_cross へ、
  @contract 双方向。CM-1 を versioned オブジェクトとして台帳へ。番号は merge 時確定）
- [ ] **T4-4 docs/stdlib の compute / duration ページ**
- [ ] **T4-5 roadmap 文書の status 整合**（fan-v2.md / logical-time-*.md に
  「実装済み（branch）」マーカー。食い違い=バグ規則の適用）
- [ ] **T4-6 診断 fixture の網羅**（bounded/race の全エラー路: bare Int / Duration 混入 /
  非 call body / 未知単位 / pure 文脈 / 効果 arm — diagnostics harness に各 1）

## Branch 外（この台帳の対象外 — 参照のみ）

B1 record/replay（claim 4）/ 効果期限の形 / fan.timeout Stage 4 / dojo async
タスクバンク（claim 5）/ Rung 1 出力 transactional / ADR-0002 批准 / AARA。
これらは merge 後 or 別レーンの台帳へ。

## 完了の定義（全体）

台帳の Tier 1–4 が全て `[x]`、REPORT.md の deviation 節が空（または「仕様に昇格」の
注記のみ）、全 workspace cargo test + spec 全部 + charge_probe 統合ゲート緑。
その状態で merge の GO を仰ぐ。
