# Branch burn-down — worktree-stage1-charge-probe の残件台帳

> 方針: **全項目を潰し切る**。各項目は完了条件（何をもって潰れたとするか）を持ち、
> 潰したら `[x]` + コミット SHA を記す。新しく見つけた残件はこの台帳に**追記してから**
> 潰す（台帳外の作業を作らない）。develop への merge はユーザーの明示 GO まで行わない。
>
> 現在地（2026-08-02 深夜）: 23 項目中 20 完了。残り 3 = T1-1（strict cap +
> unwind — 発散カットの本丸）、T1-2（metered-clone 特殊化）、T3-5（Dyn charge）。
> 全 workspace テスト緑を維持したまま進行中。

## Tier 1 — 意味論の大物

- [ ] **T1-1 strict cap + unwind（trap 可視窓 + 発散カット）**
  charge site を check-then-charge 化し、枯渇を region 境界へ巻き戻す metered-ABI。
  完了条件: race の敗者 trap が可視窓の外で「起こらなかったことになる」fixture
  （spec の挙動表の trap 行 2 種）が両ターゲット一致。発散 callee が budget で
  切れる fixture。deviation 4/7 の閉鎖。
  **設計ノート（2026-08-02）**: T1-2 のクローンが前提（strict チェックは metered
  クローンの charge site のみに入れる — 非 region 経路のコスト 0 を維持）。
  native: charge shim が枯渇で `panic_any(BudgetExhausted)` → BUDGET_SHIM の
  enter/exit を `catch_unwind` 境界に（exit は catch 側で verdict=1 を persist）。
  wasm: unwind がないので check付き charge が枯渇時に `$__b_cut=1` を立てて
  早期 return 連鎖（metered クローンの各 call 直後に `br_if` チェック、fn は
  ダミー値 return）— これが metered-ABI。interp: det_charge で fuel<0 になったら
  Flow 伝播（新 Flow::BudgetCut）を budget_exit で吸収。trap 可視窓: strict cut は
  「枯渇後は何も起こらない」を与える — 敗者 arm の trap（div0 等）は spend が
  勝者確定前に尽きれば不可視、の 2 fixture。両レグの cut 点一致は charge 配置の
  同一性（既存の保存性）から従う。
- [ ] **T1-2 metered-clone 特殊化**
  bounded/race から到達可能な関数だけを `__fuel$` 変種に複製（mono の追加次元）。
  完了条件: bounded を含むプログラムの非 region 経路が計量ゼロ（生成物 diff で
  確認）+ 既存 fixture 全緑。deviation 1 の閉鎖。
  **設計ノート（2026-08-02）**: charge_probe.rs に `specialize_metered_clones`:
  roots = `__almd_bounded_*`; reachable = roots からの CallFn 推移閉包（user fn 内）;
  reachable を `<name>__fuel` に複製し roots/クローン内の CallFn を retarget;
  charge 挿入は roots+クローンのみ（probe mode は従来どおり全 fn — probe 意味論
  不変）。region spend は不変（region 内 callee は全て metered clone）→ 境界
  fixture の 3006/1506ns は変わらない。interp は全 user fn 課金のままで安全
  （verdict は enter/exit の差分のみ観測 — region 外課金は不可視）。lifted lambda
  は table dispatch のため clone 不可 → FuncRef 到達可能な lambda は全域 charged の
  まま（軽微な region 外課金として deviation に明記）。
- [x] **T1-3 native heap-Result ABI（裸形の開通）**（1a63629e — native-only 認識パス
  `native_result_rewrite.rs` + NTy::Res carrier。裸形 bounded/race が native v1 緑
  （`bare_result` fixture、exact boundary 同値）、**一般 Result[Int,String] plain fn
  + match も開通**（differential 行 `result_fn_match`）。wasm レグ完全不変。
  意図的 wall: String-Ok payload / Result param / mono 付き generic。詳細 REPORT）

## Tier 2 — 意味論の中物

- [x] **T2-1 body/arm の full block 化**（2e26138a — parser を parse_brace_expr に、
  outliner に自由変数パラメタ化の一般経路（単一 call は従来の args-as-params 維持）、
  fmt の Block body 対応。ADR の parse→filter→render 例が両ターゲット+interp 一致、
  bind 文は charge 0 で exact boundary 不変（3006ns の pin が block 形でも同値）。
  race の block arm も動作。旧 single-call 診断 fixture は撤去）
- [x] **T2-2 Result arm の Err スキップ（race）**（1a63629e — checker が Result arm の
  Ok 型で unify、fold に per-arm の is-ok match + candidate 合成。`race_err_skip`
  fixture: 最安 Err arm の除外 / Ok Result arm が plain arm に spend 勝ち / 全 Err
  fallback / 裸形混在 — 全て両ターゲット+interp 一致。native も Res carrier 経由で緑）
- [x] **T2-3 mapper 形 any/settle（Wave 2 宣言分）**（6aa779fb — any は self-host
  `fan_any.almd`（4 型対 + 構造的早期打ち切り）+ `any_map` 内部名で v0/wasm/checker
  全配線 + `is_self_host_result_module_fn` 登録（match/auto-unwrap 消費が wasm で
  EXECUTE）。settle mapper は list.map への脱糖（意味論同一・既存制限を継承）。
  `spec/lang/fan_mapper_test.almd` が WASM レグ含め緑）
- [x] **T2-4 settle block の tuple 戻り**（f0c3b900 — FanSettle 実ノード化（parser/
  checker/lowering/fmt）、値は arm 順 tuple リテラル（要素順評価が逐次確定契約を実現、
  v0 native の実スレッド interleave flake も消滅）。異型 arm + effect arm の Err 捕捉
  （auto-unwrap OFF）+ tuple-pattern match（health_report 形）が両ターゲット一致。
  既存 5 fixture を tuple 契約へ移行）
- [x] **T2-5 S3 演算 matrix**（24236472 — checker interceptor（Named の generic
  numeric 素通りで T*T が無警告だった穴を塞ぐ）+ 飽和 erasure（+ MAX 飽和 / −
  0 飽和 / ×Int 負 trap+MAX 飽和）。`time_ops` fixture が全セルを unit 精度で
  両ターゲット+interp pin、`negative_scale` trap fixture、checker matrix 17 セル）

## Tier 3 — 小物（半日圏）

- [x] **T3-1 飽和演算 + 負値 trap**（5d8619bf — 構築子の erasure に §13 abort
  （負値）+ MAX 飽和（overflow）の guard を IR 合成、非負リテラルは畳み込み。
  fixture `negative_trap`/`saturate` を両ターゲットゲート化。副産物: native v1 rung に
  `eprintln` shim + `ProcExit` arm（assert desugar の native v1 wall も解消）、
  **発見した既存穴** = budget prim を含む v0 fallback は rustc E0425 で死ぬ →
  明示診断で拒否に変更（`almide_rt_prim_budget_` 検出）。differential corpus +2 行）
- [x] **T3-2 UFCS 曖昧診断**（1a81dc1a — E002 経路で単位名を検出し両時計候補を
  名指し（旧: `int.abs` を提案する迷子ヒント）。matrix gate に S6-3 ケース追加）
- [x] **T3-3 S6 matrix gate 群**（e9bf6eef — `almide_types::time_units` に単位表・
  時計表・S4 時計列を単一ソース化。checker/lowering/診断 hint 全て同表読み。
  `tests/time_units_matrix_test.rs` に S6-1/4/6 + 裸 Int ゲート、ケースは表から生成。
  S6-2/3/5 は T3-1/T3-2/T4-6 側で land）
- [x] **T3-4 interp の budget prim 対応**（6102bdba — interp に決定的メーター実装:
  user-fn entry / while n+1 / for-in n+1 / closure 呼び出しの W1 鏡 + budget 四重奏
  （RuntimeCall arm 経由 — fan lowering は `almide_rt_prim_budget_*` を直接発行）。
  unit-exact 境界 sweep 含む全 metered fixture で第三票が backends に一致。
  CM-1 は almide-types/time_units へ移動し真の 1 定義に）
- [ ] **T3-5 Dyn charge**（可変コスト op の従量課金 `1 + ⌈size/16⌉` 系。CM-1 v0.3）
- [x] **T3-6 CM-1 定数の単一ソース化**（13135f61 — wasm render は補間、native shim は
  template 注入で `CM1_NS_PER_CHARGE` の 1 定義に。統合ゲートに artifact 検査を追加）
- [x] **T3-7 D5 校正ゲートの常設**（9c3d78a7 — 常設した瞬間 v0.2 の混入測定を
  ratio 0.05 で検出。CM-1 v0.3 = 3ns/unit に再校正、境界 fixture は ns 精度の
  exact flip（bounded 3006ns / race 1506ns、両ターゲット同一）に強化）
- [x] **T3-8 fan{} 並列 native × budget の裁定**（3ecf61dd — 型システムが既に排除:
  region=pure × fan{}=effect 必須（E007）で region がスレッドを跨げない。逆方向は
  arm ごとに自己完結・決定的（wasm 実証、native は honest wall/拒否）。checker pin +
  REPORT に裁定全文）
- [x] **T3-9 レポート表示 `--time-report`**（90ca1acd — `almide run --time-report`
  が `time: 0.151ms deterministic (≈38.7ms wall here)` を stderr へ。probe 行は
  swallow、決定的時間は両ターゲット一致をゲートで pin）

## Tier 4 — merge 準備（branch 上で先行、PR は GO 後）

- [x] **T4-1 CHEATSHEET 反映**（b3dd3e60 — 「Concurrency & deterministic time (fan)」
  節 + 時間構築子の閉集合 + DO NOT 5 行（async/await・thunk-list・裸 Int・偽単位・
  時間リテラル））
- [x] **T4-2 SPEC.md §13 の書き直し**（63159a24 — §13.2 block heads / §13.3 決定的
  時間 / §13.4 bounded / §13.5 race + 13.1/13.2 矛盾の後日談（T9 定理として） /
  §13.6 rules。language.md §5.17 も同期）
- [x] **T4-3 C-NNN 契約正式化**（d81cc2d8 — C-202..C-207 の 6 契約 + 新 ALS 章
  `deterministic-time.md`（ALS-D1..D4）。10 fixture を spec/wasm_cross へ移設し
  @contract 双方向、check-contracts.sh 緑（207 契約/340 fixtures）。CM-1 は C-207 で
  versioned オブジェクト化（D5 ゲートが by-construction evidence）。番号は merge 時に
  衝突があれば再採番）
- [x] **T4-4 docs/stdlib の compute / duration ページ**（6471c8ef — 2 ページ +
  CHEATSHEET 表 2 行。docs-gen counter / signature-index 生成器の両方に
  「時計ページ＝checker 表面」の carve-out（Rust 側は TIME_MODULES 直読み））
- [x] **T4-5 roadmap 文書の status 整合**（be4c13d2 — fan-v2.md /
  logical-time-implementation.md に実装状況ブロック。「食い違いは BURNDOWN/契約台帳が
  正」の規則を明文化）
- [x] **T4-6 診断 fixture の網羅**（c50134cb — 8 fixture: bare Int / Duration 混入 /
  非 call body / 未知単位 / pure 文脈(E007) / 効果 arm(E006) / T×T / UFCS 裸単位。
  併せて E006 を metered-region 文脈で専用文言化（「effect fn にせよ」の循環誘導を排除））

## Branch 外（この台帳の対象外 — 参照のみ）

B1 record/replay（claim 4）/ 効果期限の形 / fan.timeout Stage 4 / dojo async
タスクバンク（claim 5）/ Rung 1 出力 transactional / ADR-0002 批准 / AARA。
これらは merge 後 or 別レーンの台帳へ。

## 完了の定義（全体）

台帳の Tier 1–4 が全て `[x]`、REPORT.md の deviation 節が空（または「仕様に昇格」の
注記のみ）、全 workspace cargo test + spec 全部 + charge_probe 統合ゲート緑。
その状態で merge の GO を仰ぐ。
