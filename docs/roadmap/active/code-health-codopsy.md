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

## 残り: cog>100 関数（分解対象、ワースト順）

| fn | cog | cyc | file |
|---|---|---|---|
| main (classify) | 199 | 96 | examples/classify_corpus.rs |
| lower_call_args | 184 | 137 | lower/calls_p2.rs |
| verify_ownership | 140 | 92 | lib.rs |
| generate_variant_repr_sources | 137 | 92 | lower/repr_sources.rs |
| check_named_call_with_type_args | 137 | 114 | frontend check/calls.rs |
| check_call_with_type_args | 129 | 83 | frontend check/calls.rs |
| ownership_certificate | 123 | 101 | certificate.rs |
| try_lower_variant_value_match | 121 | 148 | lower/control_p2.rs |
| interp_to_string_call | 121 | 127 | lower/mod_p4.rs |
| try_lower_defunc_tuple_acc_fold | 95 | 103 | lower/defunc_fold.rs |
| lower_heap_result_arm | 99 | 112 | lower/heap_result_arm.rs |

v0 側の最悪は `emit_call`（almide-codegen emit_wasm/calls_p2.rs、cyc=286）。
