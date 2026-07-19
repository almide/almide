<!-- description: Codopsy-driven code health: split 1000+ line files, decompose cog>100 fns -->
# Code Health: Codopsy-Driven File Splits and Function Decomposition

計測: `codopsy analyze crates -o /tmp/codopsy.json`（AST 品質解析、Rust 対応）。
2026-07-11 時点の全体: 390 ファイル、総合 46 点 (D)、warning 3764。1000 行超は
22 ファイル、うち 17 が almide-mir（ほぼ `lower/`）。

## 方針

1. **ファイル分割（機械的・安全）**: `include!` パート方式のまま、意味的な
   ファイル名に再分割する。純テキスト移動（行総和保存をスクリプトで検証）＋
   corpus classify の壁リスト byte 一致 ＋ mir 583 ＋ spec 283 で検証。
2. **関数分解（挙動リスクあり）**: cog>100 の巨大関数を 1 本ずつ、フルラダー
   付きで分解する。壁キャンペーンのステージ境界に 1 本ずつ挟む。

## 完了（2026-07-11、いずれもフルラダー green）

- `bc8341bb` mod_p6.rs (4820, score 19/F) → desugar.rs / desugar_unwrap.rs /
  desugar_loop.rs / desugar_branch.rs / desugar_fan.rs / desugar_match.rs /
  desugar_match_subject.rs
- `a6d25e76` control_p5.rs (3550, score 11/F — リポジトリ最低) → defunc_hof.rs /
  defunc_fold.rs / defunc_str_acc.rs / defunc_find.rs / defunc_tuple_fold.rs /
  control_while.rs
- `d1c08978` lower/mod.rs (2320) → drop_sources.rs (910) / repr_sources.rs (609)
  を抽出、mod.rs は 810 に
- `c4cbcda4` control_p4.rs (1992) → heap_result_arm.rs / result_materialize.rs /
  result_ctors.rs / scalar_for.rs

## 残り: ファイル分割候補（1000 行超、大きい順）

tests_part5 (1968・テスト), certificate.rs (1653), control_p2 (1509+),
mod_p4 (1498), classify_corpus.rs (1464), calls_p4 (1357), lib.rs (1330),
mod_p2 (1329), control.rs (~1190), calls_p2 (1141), mod_p5 (1128),
tail.rs (~1120), binds_p4 (1110), binds_p2 (1095), tests_part2 (1076・A),
frontend check/calls.rs (1056), control_p3 (1024), codegen calls_option.rs (1016)

この帯の主因はファイル長よりも巨大関数 — 分割の限界効用は低下しており、
関数分解が本丸。

## 完了: cog>100 関数分解

- 2026-07-15 `list_heap_call_name`（cog 324 — リポジトリ最悪）→ 23 行の
  per-module router + 7 helpers（random/fan/heap-fold/unwrap_or/list/set/map、
  最大 350 行）。純テキスト移動 + Option 化のみ。検証: classify wall-list
  byte 一致、フルゲート。routing ORDER が load-bearing（heap-acc fold guard は
  per-module table より先）— router のコメントに明記。

## 完了: cog>100 関数分解（続き）

- 2026-07-16 `lower_bind`（cog 272 — 残ワースト）→ router（unwrap wall +
  Block 再帰 + is_heap 分岐、~25 行）+ `lower_bind_scalar`（scalar 半分、
  verbatim 移動）+ `lower_bind_heap`（heap 半分、verbatim 移動）。検証:
  classify wall-list byte 一致 + certs 3 本 byte 一致 + フルゲート。
- 2026-07-16 `lower_tail`（cog 232）→ router（Block 再帰 + voiding gate +
  型分岐）+ `lower_tail_unit` / `lower_tail_heap` / `lower_tail_scalar`
  （いずれも verbatim 移動）。検証: certs 3 本 byte 一致 + フルゲート。
  レシピはインデックス機械導出の python テキスト移動で再現可能。
- 2026-07-16 `lower_scalar_value_inner`（cog 198）→ BinOp dispatch（340 行の
  主因 arm）を `lower_scalar_binop` に抽出。verbatim 移動 + `(**left)` →
  `left.clone()` の参照調整（Box パターン束縛→&IrExpr param）のみ。検証:
  certs 3 本 byte 一致 + フルゲート。
- 2026-07-16 `lower_call_args`（cog 184）→ per-arg dispatch を
  `lower_call_arg_into(&mut Vec)` に抽出（multi-push arm の early-`continue`
  → early-`return Ok(())`、8 箇所）。検証: certs 3 本 byte 一致 + フルゲート。
- 2026-07-16 `generate_variant_repr_sources`（cog 137）→ NAMED-RECORD 節
  （292 行）を `generate_record_repr_sources_into(&mut String)` に抽出。
  検証: certs 3 本 byte 一致（生成テキスト byte 保存の証明）+ フルゲート。
- 注記: `verify_ownership`（cog 140）は 6+ の &mut 状態（object_of/rc/dead/
  borrowed/branches/violations）が match を貫通しており text-move の域を
  超える — OwnershipState struct 化が必要（別種の設計作業として保留）。
- 2026-07-16 `interp_to_string_call`（cog 121）→ List/Option/Result の 3 大
  routing arm を `interp_{list,option,result}_to_string` に抽出（pure table
  fn、状態なし）。検証: certs 3 本 byte 一致 + フルゲート。
- 2026-07-16 `main`（classify_corpus、cog 199）→ per-file loop 本体（~380 行）
  を `classify_file(&Path, &mut Tally, &mut CertStreams, …)` に抽出。5 本の
  stream String は `CertStreams` struct に束ね、continue→return ×3。検証:
  certs 3 本 + wall report が byte 一致 + フルゲート。
- 2026-07-16 `check_named_call_with_type_args`（cog 137、frontend）→ no-sig
  fallback（~110 行、name+arg_tys+self のみ参照のクリーン片）を
  `check_unresolved_named_call` に抽出。検証: full suite（frontend の挙動
  証明は suite）+ corpus-wall。
- 2026-07-16 `check_call_with_type_args`（cog 129、frontend）→ TypeName
  ctor-call arm（~88 行、name+arg_tys のみ）を `check_type_name_call` に
  抽出。検証: full suite + corpus-wall。**#781 は 10/14 — 残 4 は全て
  State-struct 設計組（verify_ownership / ownership_certificate /
  try_lower_variant_value_match + 台帳外 1 本の再計測が次アクション）。**
- 2026-07-16 `verify_ownership`（cog 140）→ scan 状態 6 本（object_of/rc/
  dead/borrowed/branches/violations）を `OwnershipScan` struct に束ね、
  loop body を `step(i, op)` メソッド化（機械 rename + verbatim 移動）。
  初回は closure-wall 起因の 77 fails と誤って共倒れ revert したが、単独
  再適用で 583 pins + certs byte 一致（fresh baseline 比較）+ フルゲート
  全緑を確認。**#781 は 11/14 — 残 3 = ownership_certificate(123) /
  try_lower_variant_value_match(121) + 台帳外 1 本の再計測。**
- 2026-07-16 `ownership_certificate`（cog 123）→ 発行状態 7 本（depth/Streams/
  released_merge_dsts/consumed_values/feeder_to_slot/slots/line_slots）を
  `CertScan` struct + `step(op)` に束ね（OwnershipScan と同パターン）。検証:
  583 pins + **ownership.cert 含む 4 certs byte 一致**（cert 生成コード自身の
  分解なので決定的）+ フルゲート。**#781 は 12/14 — 残 2 =
  try_lower_variant_value_match(121) + 台帳外 1 本の再計測。**

## 再計測（2026-07-16、codopsy 全 workspace — 「台帳外 1 本」の解）

cog>100 は workspace 全体で **47 本**。内訳とスコープ判定:

1. **v0 emitter（退役予定 — #782 完了で消滅、分解しない）**: emit_call(424)、
   emit_match_arms(395)、render_expr(306)、check_needs_ownership(285)、
   transform_expr(245)、emit(230)、emit_stmt(221)、render_stmt(218) ほか
   codegen/ の大半。
2. **分解 halves の第二ラウンド候補**（親の分解で 100 切り、half が残存）:
   lower_bind_heap(202)、lower_call_arg_into(161)、lower_tail_heap(109)、
   lower_scalar_binop(114)、classify_file(118)、
   generate_record_repr_sources_into(101)。
3. **アクティブ系の未着手**: lower_stmt(253)、try_render_wasm_source_impl(250)、
   frontend lower_expr(206)、list_call_name(197)、infer_expr_inner(188)+_g2(171)、
   lower_call_target/lower_call(150×2)、try_lower_variant_value_match(129)、
   lower_branch(128) ほか。

- 2026-07-16 `try_lower_variant_value_match`（cog 129）→ SUBJECT フェーズ
  （~185 行: effect-result/self-host/user-call/member/var の materialize +
  Option/Result 分類）を `variant_match_subject(subject, ops_mark, lhh_mark)
  -> Option<(subj, is_option, is_result_str, is_result)>` に抽出。preflight
  の懸念（rollback closure）は marks 上の非捕捉 closure と判明し、method 内
  再構築で解決。`heap_or_scalar_bind` closure と lower_arm（&mut self 捕捉）
  は後半フェーズに残置 — 後半の追加分割は closure のメソッド化が前提。
  検証: 583 pins + 4 certs byte 一致 + フルゲート。**元の 14 本テーブルは
  全消化**（残るのはアクティブ系 cog>100=0 への後続リスト）。
- `try_lower_variant_value_match`（129）の着手前解析（2026-07-16）: 34 top-level
  locals / 537 行 / 5 フェーズ（subject 解決 297-464 → 分類フラグ 465-527 →
  arm slot 収集 528-608 → tag 読み+dispatch 609-657 → arm lowering+merge
  658-733）。**`lower_arm` が &mut self 捕捉の local closure、then/else_slot が
  lifetime 付き &IrExpr** のため OwnershipScan/CertScan の struct パターンは
  直接適用不可 — フェーズを LowerCtx メソッド化し出力を明示 return する
  dataflow 写像が必要。mt2 miscompile class の震源関数なので、専用ウィンドウ
  + certs/pins フルラダーで着手すること。

判定: 元の「14 本」テーブルは mir/frontend の初回スナップショットで、完遂の
定義は「アクティブ系の cog>100 = 0」に更新する（v0 退役分は #782 に従属)。
確立済みレシピ: 純テキスト移動（python 機械導出）→ certs byte 比較、
State-struct 束ね（OwnershipScan / CertScan 前例）→ ownership.cert 一致が
決定的証明。

## 再計測（2026-07-18、#782 file-level WALL 焼却アーク完結後）

アクティブ系（almide-codegen 除外）cog>100 は **26 本**（wall 焼却の新機構分を
含む）。ワースト: lower_stmt(315)、render_fn(285)、try_render_wasm_source_impl
(269)、list_call_name(239)、lower_bind_heap(225)、frontend lower_expr(206) …。

- 2026-07-18 `lower_stmt`（cog 315 — アクティブ系ワースト）→ ~95 行の
  per-stmt-kind router + 5 メソッド（lower_stmt_assign / _index_assign /
  _field_assign / _map_insert / _expr、いずれも verbatim 移動 + 参照調整
  （match 束縛 `*var`→param `var: VarId`/`Sym` の機械 rename）のみ）。検証:
  **4 certs + wall report byte 一致** + corpus-wall（kernel oracle 込み）+
  almide test 288 + sweep PASS 279/TRAP 0 + cargo test。halves の
  `lower_stmt_assign`(120) が第二ラウンド候補に残存、`lower_stmt_expr` は 75。

## 残り 8 本の分解性分類（2026-07-16 精査）

- **text-move 可（レシピ適用可能）**: `main`（classify_corpus 199 — example、
  cert 出力生成側なので出力 byte 比較で証明）、`check_named_call_with_type_args`
  (137) / `check_call_with_type_args` (129)（frontend — resolved_name /
  qualified_via_direct が節を跨ぐため粗い境界選定が必要、証明は full suite）
- **State struct 化が必要（text-move 不可）**: `verify_ownership` (140、
  object_of/rc/dead/borrowed/branches/violations が match を貫通)、
  `ownership_certificate` (123、同族)、`try_lower_variant_value_match` (121、
  then/else slot + consumed 集合 + heap_res フラグ群が 5 フェーズを貫通)
  — 各々 `OwnershipState` / `VariantMatchPlan` 的な struct に束ねてから
  フェーズをメソッド化する設計作業。cert byte 比較が安全網になるのは同じ。

## 残り: cog>100 関数（再計測 2026-07-19、旧テーブルは自己矛盾のため置換）

旧「残り」テーブルは `verify_ownership` / `ownership_certificate` /
`try_lower_variant_value_match` を残存扱いしていたが、直前の各節（2026-07-16
〜2026-07-18）で**同じ 3 本を分解済みと明記**しており矛盾していた（stale）。
issue #781 も同様に古いまま — OPEN だが最新コメントが挙げる次ターゲット
（`lower_bind` / `lower_tail` / `lower_scalar_value_inner`）はすでに本ドキュメント
の他節で分解済みと記録されている。この矛盾は issue 側では直さず、ここで解消する。

`codopsy analyze crates -o codopsy-report.json` を再実行（2026-07-19、workspace
414 ファイル、score D 45/100）して cog>100 を再抽出: workspace 全体で **47 本**、
うち `crates/almide-codegen`（v0 emitter、#782 完了で消滅予定・分解しない方針は
変わらず）を除いた**アクティブ系は 27 本**（前回 2026-07-18 の 26 本から純増 1 —
`lower_stmt` 分解の half `lower_stmt_assign`(120) が新たにワースト圏内に残存）。
`verify_ownership` / `ownership_certificate` / `try_lower_variant_value_match` は
このリストに**出現しない** — 分解済みという既存記述と整合。

| fn | cog | cyc | file |
|---|---|---|---|
| render_fn | 285 | 259 | almide-mir/src/render_native.rs |
| try_render_wasm_source_impl | 269 | 192 | almide-mir/src/pipeline.rs |
| list_call_name | 263 | 243 | almide-mir/src/lower/mod_p4.rs |
| lower_bind_heap | 238 | 242 | almide-mir/src/lower/binds_p2.rs |
| lower_expr | 206 | 228 | almide-frontend/src/lower/expressions.rs |
| infer_expr_inner | 193 | 127 | almide-frontend/src/check/infer.rs |
| infer_expr_inner_g2 | 187 | 147 | almide-frontend/src/check/infer_p2.rs |
| try_lower_option_ctor | 163 | 145 | almide-mir/src/lower/binds_p4.rs |
| lower_call_arg_into | 161 | 156 | almide-mir/src/lower/calls_p2.rs |
| lower_call | 157 | 101 | almide-frontend/src/lower/calls.rs |
| lower_call_target | 150 | 118 | almide-frontend/src/lower/calls.rs |
| lower_branch | 150 | 102 | almide-mir/src/lower/control.rs |
| try_lower_record_list_literal_as | 143 | 148 | almide-mir/src/lower/binds_p3.rs |
| lower_heap_result_arm | 141 | 160 | almide-mir/src/lower/heap_result_arm.rs |
| lower_scalar_binop | 127 | 146 | almide-mir/src/lower/calls_p4.rs |
| lower_stmt_assign | 120 | 79 | almide-mir/src/lower/mod_p3.rs |
| classify_file | 118 | 63 | almide-mir/examples/classify_corpus.rs |
| check_named_call_with_type_args | 116 | 94 | almide-frontend/src/check/calls.rs |
| resolve_static_member | 111 | 69 | almide-frontend/src/check/static_dispatch.rs |
| lower_tail_heap | 111 | 116 | almide-mir/src/lower/tail.rs |
| generate_record_repr_sources_into | 109 | 74 | almide-mir/src/lower/repr_sources.rs |
| map_call_name | 108 | 103 | almide-mir/src/lower/mod_p4.rs |
| try_lower_option_unwrap_or | 103 | 116 | almide-mir/src/lower/control_p3.rs |
| check_call_with_type_args | 102 | 70 | almide-frontend/src/check/calls.rs |
| parse_postfix | 102 | 53 | almide-syntax/src/parser/expressions.rs |
| fmt_expr | 102 | 119 | almide-tools/src/fmt_p2.rs |
| eval_expr | 101 | 141 | almide-interp/src/eval.rs |

v0 側の最悪は変わらず `emit_call`（almide-codegen emit_wasm/calls_p2.rs、
cog=427 / cyc=293 — 前回計測から純増、分解対象外の方針通り未着手）。

このテーブルが次の「着手先」の一次ソース。`classify_file`（example、cert
出力生成側で text-move + byte 比較が安全網）と `lower_stmt_assign`（`lower_stmt`
分解の half、verbatim 移動候補）が最も低リスク；`render_fn` / `list_call_name` /
`lower_bind_heap` はワースト級で未着手 — 次の分解キャンペーンの筆頭候補。
