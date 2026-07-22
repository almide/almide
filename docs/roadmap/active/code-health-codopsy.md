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

## 残り: cog>100 関数（分解対象、ワースト順）

| fn | cog | cyc | file |
|---|---|---|---|
| verify_ownership | 140 | 92 | lib.rs |
| ownership_certificate | 123 | 101 | certificate.rs |
| try_lower_variant_value_match | 121 | 148 | lower/control_p2.rs |
| try_lower_defunc_tuple_acc_fold | 95 | 103 | lower/defunc_fold.rs |
| lower_heap_result_arm | 99 | 112 | lower/heap_result_arm.rs |

v0 側の最悪は `emit_call`（almide-codegen emit_wasm/calls_p2.rs、cyc=286）。

## 再計測（2026-07-20、リリース v0.33.0 後）

`codopsy analyze crates` 実測: アクティブ系（almide-codegen=v0 除く）cog>100 が
**29 本**（07-16 時点の「残り8本」から大幅増 — #806 の renderer 追加分
（render_op 129 など）と、その間の機能追加が主因。上の表は古いスナップショット
なので実測を正とする）。ワースト10: `list_call_name`(319,mod_p4.rs)
`render_fn`(288,render_native.rs) `try_render_wasm_source_impl`(288,pipeline.rs)
`lower_bind_heap`(265,binds_p2.rs) `lower_expr`(206,frontend)
`infer_expr_inner`(193) `infer_expr_inner_g2`(187)
`try_lower_option_ctor`(171,binds_p4.rs) `lower_call_arg_into`(161)
`lower_call`(157,frontend)。

**分解性トリアージ**: この規模のクラスは主に3パターンに分かれる。
1. **純粋 name-router（安全・簡単）**: `if func == "X" { …; return Some(...) }`
   の独立チェーンで局所可変状態を持たない（list_call_name の形）。
2. **均一ブロックアームの match（安全・やや手間）**: `Pattern if Guard => { … }`
   が全アームで揃っている `&mut self` メソッド（try_lower_option_ctor の形）。
   文字列リテラル内の `{}` に汚染されない brace-depth パーサ（コメント/文字列
   ストリップ）が前提 — 素朴な `line.count('{')` は format! の `"{func}"` で
   誤爆する（幸い今回は行内で対で相殺し実害なし、だが一般には危険）。
3. **状態貫通（危険・要設計）**: アーム間で共有の派生値（`map_call_name` の
   `key_heap`/`val_heap`/`val_is_string` 等）やループ内 fixpoint 状態
   （`generate_record_repr_sources_into` の `rec_emittable`）を持つ関数。
   単純な逐次 `.or_else()` 分割では意味が変わりうる — 個別設計が必要
   （verify_ownership 系と同じ「State struct 化」クラス）。

**完了（2026-07-20、`e6ca5d77`）**:
- `list_call_name`（cog 319 — 全体ワースト）→ router + 6 グループ関数
  （hof_combinators/source_keyed/ordering/transform/modifiers/accessors、
  最大 82）。**パターン1**、if チェーンを brace-depth 境界検出で機械分割。
- `try_lower_option_ctor`（cog 171）→ router + 7 グループ関数
  （opt_tuple_and_variant_payloads/opt_heap_general/opt_fallback_and_none/
  result_ok_heap/result_small_arms/result_err_heap_ok_result/
  result_err_heap_fallback、最大 41）。**パターン2**、match アームを
  「深さがいったん 1 を超えてから 1 に戻る」判定で境界検出（単純な
  `depth==1` 判定だと複数行ガードで誤爆した — `seen_gt1` フラグで修正）。
- 検証: 両方とも `WALL_NAMES=1 cargo run -p almide-mir --example
  classify_corpus -- --out DIR spec`（5113 corpus 関数）の
  caps/caps_graph/names/ownership.cert + wall report が変更前後で
  byte-identical、加えて wasm_runtime_test(72) + almide test(300) +
  cargo test --workspace(74 suites) 全緑。CI 3 workflow 緑を確認中。

**完了・続き（2026-07-20、`b370ee6a` / `578cc9c5` / `ddeef698`）**:
- `lower_expr`（cog 206、frontend/lower/expressions.rs）→ 元の match は
  49 アームの ExprKind 網羅（exhaustive、`_`なし）。ワーカーを sequential
  `.or_else()` にはできない（1つしかマッチしないので他が
  `unreachable!()` で死ぬ）— ROUTER 自体は全49アームの match を残し、
  大きい12アーム（Binary/Member/TypeName/Ident/Compose/IfLet/ForIn/
  IndexAccess/Record/InterpolatedString/Unary/Match）だけを
  `fn lower_expr_X(ctx, expr, ty, span) -> IrExpr` に抽出、helper 内で
  `let PATTERN = &expr.kind else { unreachable!() }` により再分解。小さい
  アーム（`ctx.mk(...)` 一発等）はそのまま残置。router 自体の cog は 33
  （49 分岐の「構造」分だけ）、helper 最大 33（Member）。
- `infer_expr_inner`（cog 193）→ **最初にクロージャ包み
  `Some((|| -> Ty { … })())` で試したが、これは codopsy の関数単位計測を
  すり抜けるだけで実質複雑度が消えていない（Record アームの複雑度がまるごと
  匿名 `(closure)` エントリに移動、計測不能）とユーザー指摘で気づき破棄**。
  正規の named function（`let PATTERN = &mut expr.kind else {…}`）でやり直し
  → 5 helper（type_name/record/member/tuple_index/optional_chain）、router
  cog 5、最大 66（record）。**教訓: クロージャ包みは禁じ手 — codopsy 対応は
  必ず named function で行う。**
- `infer_expr_inner_g2`（cog 187、infer_p2.rs）→ 既存の Option<Ty> グループ
  構造（`Some(match {…})`、`_ => return None`）から大きい7アーム
  （binary/match/ident/if/if_let/unary/index_access）を named method に
  抽出。router cog 24、最大 52（binary）。
- 検証: 3件とも classify_corpus 5113関数 certs+wall report byte-identical
  + wasm_runtime_test(72) + almide test(300) + cargo test --workspace(74)
  全緑 + CI 3 workflow 緑（確認済み or 確認中）。

**着手 → 撤退（2026-07-20）**: `lower_call_arg_into`（cog 161）は事前トリアージ
で見落とした3つ目の危険パターンだった — 一部アームが `out.push(...);
return Ok(());` で **関数全体を早期脱出**しており、「1アーム=1戻り値」という
前提（list_call_name/try_lower_option_ctor/lower_expr/infer_expr_inner系が
全て満たしていた不変条件）が崩れる。加えて発見した2つの実装バグ:
①`let-else` はガード付きパターン（`PATTERN if COND`）を受け付けない —
ガード付きアームは `let-else` でなく `match { PATTERN if COND => {…}, _ =>
unreachable!() }` の入れ子で再分解する必要がある ②include! される
ファイルでは `impl super::Checker` ではなく bare `impl Checker`（または
このファイルの型名 `impl LowerCtx`）で追記しないと構文エラーになる。
3つ目の複雑さが重なった時点で revert（`git checkout --`）して撤退 —
正しくやるには `out: &mut Vec<CallArg>` を helper に渡し、多くを push
してから早期 return するアームは `Result<(), LowerError>` を返す個別
シグネチャにする設計が要る。次回の着手点として記録。

**残り（次の候補、パターン別、2026-07-20 更新）**:
- パターン1候補: 未確認（要個別調査 — map_call_name は混在アームでパターン3寄り）
- パターン2候補（均一ブロックアーム、比較的安全）: 未走査の cog>100 の中で
  「`let arg = match {…}` 一発束縛」または「exhaustive match で `_`なし」の
  形をまず確認してから着手すること — **`out.push`/`return Ok(())` の
  mid-body 混入を事前 grep で必ずチェックする**（lower_call_arg_into の
  教訓）。
- パターン3（要設計・後回し）: `map_call_name`(148)、`render_fn`(288)、
  `try_render_wasm_source_impl`(288)、`generate_record_repr_sources_into`
  (138)、`try_lower_record_list_literal_as`(144)、
  `try_lower_option_unwrap_or`(123 — `ops_mark`/`lhh_mark` ロールバック
  状態が全体を貫通)、`lower_call_arg_into`(161 — 上記の通り撤退)
- 旧リスト（07-16、State struct 化必要）: `verify_ownership`/
  `ownership_certificate`/`try_lower_variant_value_match` は既に完了済み
  （本ファイル上部の記録どおり）— 上の表は STALE、削除対象。

## codopsy6/almide-mir（2026-07-23、max-depth 集中掃討ラウンド）

**新戦略**: cog 分解ではなく `max-depth`（ネスト深度、しきい値6）を主標的に
した。785→**505** issues（36%減）、**46/D → 50/D**（スコア式のクセ通り、
issue 減少ほどスコアは動かない — codopsy5 までの教訓通り）。

**max-depth: 286 → 6（98%消化）。** レバーは2種、いずれも「同じ実行順序を
保証できる場合のみ」の純粋な制御フロー等価変換：
1. **classify-and-extract**（最多）: `self.xxx.insert(dst, ...)` を条件分岐
   で振り分ける else-if チェーンを、`&mut self` の小さな helper メソッドに
   text-move し、各分岐を `if COND { ...; return; }` の guard-clause に変換
   （else-if の入れ子コストがそのままリニアに深度を積むため、helper に
   移すだけで基底深度が 1 にリセットされる）。`drop_op_for` や
   `classify_named_call_drop`/`classify_module_call_drop`（binds_p2.rs）が
   典型例。
2. **guard-clause フラット化**（ループ/関数末尾）: 「else 節が一切ない、
   条件不成立なら単に後続コードへ落ちる」形の入れ子 if は、(a) ループ内なら
   `continue`、(b) 関数末尾で条件不成立時に「何もしない」と等価なら
   `return`、(c) それ以外（ループでも関数末尾でもないが、条件不成立時に
   「このブロックの外の後続コードへ進む」形）は **ラベル付きブロック
   `'name: { if !cond { break 'name; } ... }`** で実現。(c) は
   `discover_generic_variant_list_instantiations`（drop_sources.rs）や
   `try_lower_option_unwrap_or` の ctor-fallback 節、`typed_slot_eq` 呼び出し
   元の `try_lower_custom_variant_match` の single-ctor strip 節などで使用。

**危険域は明示的にスキップ**（正しさ優先、num を追わない）:
- `lower_defunc_list_hof_inner`（defunc_hof.rs、cog 79/112）— ループ内
  drop-old + SetLocal アキュムレータの所有権不変条件。round brief で
  「extreme care or skip」指定。max-depth 2 件を未着手のまま残す。
- `lower_stmt_assign`（mod_p3.rs、`scalar_loop_depth > 0` 内の
  ConcatList append アキュムレータ）— 同族の loop-carried heap-slot
  パターン（`d id m` 証明済み cert）。max-depth 4 件を未着手のまま残す。
  隣接する `lower_stmt_index_assign` の3件（ResultOk/Err の
  materialize_result_str/opt_str_some/result_ok ルーティング）も同じ
  理由で見送り済み（このラウンド内で既に0件化 — 上の「残り4件」に統合済み）。
- `try_lower_defunc_tuple_acc_fold` 系（defunc_tuple_fold.rs）— max-params
  4件を検出したが同じアキュムレータ族のため着手せず。

**"ほぼ同じだが微妙に違う" 分類チェーンの罠**（3回踏んだ）: binds_p2.rs の
`classify_named_call_drop` vs `classify_module_call_drop`（後者は
`is_scalar_elem_list_ty` アームが無い）、control_p2.rs の
`seed_nested_option_result_bind_payload` vs control.rs の
`seed_nested_option_bind_payload`（後者だけ record/tuple aggregate アームを
持つ）、defunc_str_acc.rs/control_p2.rs の heap-Ok Result 分類（片方だけ
`result_ok_record_drop_fn` の resrec チェックを持つ）。**教訓: 見た目が
同じでも helper 共有は絶対にしない — diff で1行でも違えば別関数として
text-move する。** 実際、3箇所とも共有を試みず個別 helper 化して安全側に倒した。

**ラベル付きブロックの罠**: `continue;`（unlabeled）がラベル付きブロックを
「突き抜けて」外側のループに届く形は Rust がコンパイルエラーにする
（E0695 "unlabeled continue inside of a labeled block"）— render_wasm.rs の
`'try_defer: { ... continue; }` で発生、for ループに `'op_loop:` ラベルを
付けて `continue 'op_loop;` に修正。同じラベル付きブロック内でも、その
ブロックが直接くるむ内側の別ループ（`for a in args.iter_mut()`）向けの
`continue`（ラベルを突き抜けない）はそのままで問題なし。

**関数抽出時のドキュメントコメント罠**: 新しいメソッドを `impl LowerCtx {`
直後に挿入する際、既存メソッドの doc comment ブロックの「後」ではなく
「前」に挿入しないと、rustdoc が doc comment を隣の（新しい）関数に
誤帰属させる（コンパイルは通るが cosmetic bug）。control.rs で1回発生、
Python スクリプトで関数ブロックごと `impl LowerCtx {` 直後に re-locate して修正。

**検証**: 全20コミット、それぞれ `cargo build --release`（新規warning 0、
baseline 25件のまま）+ `cargo test -p almide-mir --release`（593 tests
green）+ corpus-wall（5 cert ファイル byte-identical、baseline との diff
常に空）で個別検証。ラウンド末尾に `cargo test --workspace --release`
（exit 0, all green）、`./target/release/almide test`（300/300 green）、
`cargo test --release --test wasm_runtime_test`（render_wasm.rs/
render_native.rs 変更後に72/72 green、追加で実施）を実施。corpus-wall は
ラウンド開始前の baseline と最終状態も diff — 完全一致（診断: 主要な
runtime/codegen 経路の観測可能な出力に一切の変化なし）。

**最終**: 46/D (785 issues) → **50/D (505 issues)**。max-depth 286→6。
ブランチ `codopsy6/almide-mir`、develop から分岐、20 English commits。

**次ラウンドへの punch-list**:
- max-depth の残り6件は上記の理由で意図的に未着手（触るなら専用ウィンドウ
  + 不変条件の再確認から）。
- max-complexity 187 / max-cognitive-complexity 108 が次の最大カテゴリ
  （旧戦略の cog 分解に相当）— render_fn(237, render_native.rs) が単独最悪
  （このラウンドで261→237まで軽く削れたが本体は未着手）。5ラウンド分の
  cog 分解の教訓（本ファイル上部）を踏まえて着手すること。
- max-params 13件は概ね同じ「loop-carried accumulator」族
  （defunc_tuple_fold.rs に4件集中）— 着手するなら struct 化 + 全呼び出し元
  更新の設計作業、後回し推奨。
- no-println(77)/no-unwrap(70) は ABSOLUTE RULES により touch 禁止。
- スコア60到達には issue 数のみでなく質的カテゴリ（cog系）の削減が必要
  という、5ラウンド通しての結論が今回も再確認された。
