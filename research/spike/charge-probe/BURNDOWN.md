# Branch burn-down — worktree-stage1-charge-probe の残件台帳

> 方針: **全項目を潰し切る**。各項目は完了条件（何をもって潰れたとするか）を持ち、
> 潰したら `[x]` + コミット SHA を記す。新しく見つけた残件はこの台帳に**追記してから**
> 潰す（台帳外の作業を作らない）。develop への merge はユーザーの明示 GO まで行わない。
>
> **現在地（2026-08-03）: 23 項目、全て完了。** REPORT の deviation 節も全件解消。
> 残る手続きは最終フルスイート緑の確認と、ユーザーの merge GO 待ちのみ
> （merge はユーザーの明示 GO まで行わない）。

## Tier 1 — 意味論の大物

- [x] **T1-1 strict cap + unwind（trap 可視窓 + 発散カット）**（unwind 不要の
  check-and-return 方式で実装 — charge site が「subtract → fuel<0 → その fn から
  ダミー値 return」、W1 により連鎖 return が outlined fn の exit（通常経路の
  verdict/spend persist）へ収束。両レグ + interp（det_region_depth gate）同型。
  fixture: `fuel_divergence_cut`（while true が切れて続行）/ `fuel_trap_cut`
  （窓の外の敗者 trap は不発）/ `fuel_trap_window`（inline で窓内に入った trap は
  両レグ同一に発火）— 全て C-204/C-205 evidence。deviation 4/7 閉鎖）
  **確定設計（unwind 不要・両レグ同型の check-and-return）**: metered fn の
  charge site を「subtract → if fuel<0 → その fn からダミー値 return」に。
  W1（全サイクルは charge site を通る）により枯渇後の残実行は有限、region の
  呼び出し連鎖は各 fn の次 charge site で連鎖 return し outlined fn 本体へ戻り、
  exit が verdict=1（fuel<0）と spend を通常経路で persist する — unwind も
  フラグも不要。カット点 = 枯渇後最初の charge site で、charge 配置保存性から
  両レグ同一。ダミー値は観測不能（verdict=Err が値を捨てる）。T1-2 により
  budget-only モードの charge 保有 fn は全て metered なので、チェックは charge
  render に無条件で置ける（probe モードは fuel=MAX から減るだけで <0 に達しない）。
  native: `//__CUT_RET__` マーカー + 末尾 ret-NTy 確定後に
  `if __almd_fuel_lt0() { return <default>; }` へ replacen（JOIN marker の前例）。
  wasm: `(if (i64.lt_s (global.get $__fuel) (i64.const 0)) (then (return <typed default>)))`。
  interp: `det_region_depth`（enter+1/exit-1）>0 かつ fuel<0 で eval_while break +
  call_function 早期 return（ダミー Value::Int(0)）。fixture: 発散カット
  （loop_forever が budget で切れる）+ race 敗者 trap 不可視 2 種。
- [x] **T1-2 metered-clone 特殊化**（de7c2d20 — `specialize_metered_clones`:
  roots=`__almd_bounded_*` からの CallFn 推移閉包を `__fuel` 複製 + retarget、
  charge は roots+クローンのみ（probe mode は全 fn のまま = probe 意味論不変）。
  native sig 表へクローンを base からコピー（Result 返し region の String 誤型を
  修正）。ゲートに「非 region 経路の charge ゼロ」を両 artifact で機械検査
  （vacuity pin 付き）。境界 3006/1506ns 不変・全 metered fixture 緑。
  既知の残: FuncRef 到達時は `__lambda_*` が全域 charged（table 複製不可のため））
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
- [x] **T3-5 Dyn charge**（`Op::ChargeDyn` — `__str_concat` の結果長ベース従量課金
  `1 + len/16`（result-keyed = 両レグ一致が構成的）。両 renderer + interp mirror +
  strict cut/trace 同則。`fuel_dyn_charge` が 252 units の spend を計算値どおり
  unit-exact で pin（750/760ns flip、3-way 一致））
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

## Tier 5 — oracle 層（インセプション完遂。2026-08-03 にスコープ拡大で編入）

- [ ] **T5-1 fan.timeout（Stage 4）** — `fan.timeout(duration.ms(n)) { body }`。
  壁時計期限を **charge site で協調チェック**（中断点統一原理 — Go の context と
  同じ協調キャンセルの型）。v1: pure body（bounded と同形、時計だけ Duration）。
  実装は T1-1/T1-2 の機構を流用: `__wall` クローン族の charge site が
  「時計読み → 期限超過 → check-and-return カット」。verdict は ω 依存
  （R_Ω 契約 — 決定的 fixture は「巨大期限=必ず Ok」「発散 body + 微小期限=必ず
  Err」の両端のみ pin）。TIME_CONSUMING_SURFACES に ("fan.timeout","Duration")
  追加（S6-6 の初の Duration 行）。完了条件: 両ターゲットで動作 + ω 両端 fixture +
  E007/純度/時計混合の診断。
- [ ] **T5-2 B1 record/replay（claim 4）** — ω = 「何回目の壁時計チェックで期限が
  切れたか」の序数列。record モードが ω を採録し、replay モードは時計を読まずに
  その序数でカット → **採録した ω での観測は byte 一致**（native で record →
  wasm で replay も定義から成立）。チャネルは env 変数（wasmtime -S inherit-env
  経由で両レグ共通）。完了条件: record→replay の三つ組一致 + クロスターゲット
  replay の gate、C-NNN（R_Ω クラス）起草。
- [ ] **T5-3 効果表面期限の型 pin** — S4 行どおり「形は将来、型だけ pin」:
  Duration を取る効果表面の予約を ADR/S4/契約側の注記として固定（実装なし、
  ドキュメント + 台帳整合のみ）。
- [ ] **T5-4 dojo async タスクバンク + 初回実測（claim 5）** — almide-dojo repo に
  fan v2 / 決定的時間のタスク群を追加し、branch ビルドの almide で MSR 初回
  ラウンドを実測・記録。完了条件: タスクが dojo ハーネスで green + 実測結果の
  記録（数値は claim 5 の解禁判定材料）。

## Branch 外（残り — 参照のみ）

Rung 1 出力 transactional / AARA は別レーンの台帳へ。ADR-0002（実行順）の批准は
ユーザー討議事項のため台帳化しない。

## 完了の定義（全体）

台帳の Tier 1–4 が全て `[x]`、REPORT.md の deviation 節が空（または「仕様に昇格」の
注記のみ）、全 workspace cargo test + spec 全部 + charge_probe 統合ゲート緑。
その状態で merge の GO を仰ぐ。
