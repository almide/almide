<a id="compiler-structural-coverage--2026-07-04-proofscoveragesh"></a>

# コンパイラの構造カバレッジ — 2026-07-04（proofs/coverage.sh）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](COVERAGE.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](COVERAGE.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](COVERAGE.zh-CN.md)

> 非規範の原文固定・歴史記録用読者スナップショット。原本は変更せず利用側の正本とする。自動同期、再計測、独立した証明検証、ゲート変更、記載コマンドの実行は行っていない。日付・件数・表・制約・後日の追記は原文の範囲を保ち、現時点のカバレッジ報告やメモリ安全・認証の保証ではない。7月のスナップショットは廃止済みレンダラーをまだ含む。9月29日の「削除はTOTALを上げる方向のみ」という予測に続き、9月30日にはワークロードのカバーが失われたとの記録がある点を、そのまま保持する。10月2日のクレート別増加数と表の差分も、黙って整合させず原文どおり残す。下限更新やmutation実行の記述は過去の原文資料であり、実行承認ではない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/COVERAGE.md)

ワークロード：almide-mir と almide-codegen のテストスイート、実行可能な全 spec に対する render_program（v1 経路）、`almide test spec/`（v0 本番経路）。

> **#2753（2026-09-29）以降**、render_program の全体検査は廃止された。ゲートは全実行可能 spec プログラムを、出荷版 `almide build --target wasm`（構造経路を強制）と `almide verify --emit` の証拠生成器に通す。既存 WAT レンダラーのファイル（#2761で削除）は報告から除外する。下のスナップショットはそれ以前のため、まだそれらを含む。

```
crates/almide-frontend/src/lower/mod_p2.rs                                                      735               111    84.90%          21                 4    80.95%         314                41    86.94%           0                 0         -
crates/almide-frontend/src/lower/mod_p3.rs                                                      398               127    68.09%          17                 4    76.47%         157                43    72.61%           0                 0         -
crates/almide-frontend/src/lower/statements.rs                                                  665                90    86.47%          23                 3    86.96%         273                44    83.88%           0                 0         -
crates/almide-frontend/src/lower/types.rs                                                       127                 6    95.28%          13                 1    92.31%          58                 4    93.10%           0                 0         -
crates/almide-frontend/src/stdlib.rs                                                            310               270    12.90%          12                 6    50.00%         148               128    13.51%           0                 0         -
crates/almide-frontend/src/type_env.rs                                                          376                79    78.99%          38                 7    81.58%         219                41    81.28%           0                 0         -
crates/almide-mir/src/certificate.rs                                                           1237               321    74.05%          48                 3    93.75%         688               171    75.15%           0                 0         -
crates/almide-mir/src/certificate_p2.rs                                                         824                 5    99.39%          32                 1    96.88%         347                 3    99.14%           0                 0         -
crates/almide-mir/src/coown_names.rs                                                             39                 0   100.00%           2                 0   100.00%          17                 0   100.00%           0                 0         -
crates/almide-mir/src/lib.rs                                                                    373                61    83.65%          13                 2    84.62%         192                42    78.12%           0                 0         -
crates/almide-mir/src/lib_p2.rs                                                                 186                 0   100.00%          20                 0   100.00%         114                 0   100.00%           0                 0         -
crates/almide-mir/src/lower/binds.rs                                                            515                57    88.93%           5                 0   100.00%         208                17    91.83%           0                 0         -
crates/almide-mir/src/lower/binds_p2.rs                                                        1143               376    67.10%           8                 3    62.50%         569               207    63.62%           0                 0         -
crates/almide-mir/src/lower/binds_p3.rs                                                         998                55    94.49%          26                 1    96.15%         419                21    94.99%           0                 0         -
crates/almide-mir/src/lower/binds_p4.rs                                                        1223               484    60.43%          17                 3    82.35%         502               186    62.95%           0                 0         -
crates/almide-mir/src/lower/calls.rs                                                            575               143    75.13%          18                 9    50.00%         307                64    79.15%           0                 0         -
crates/almide-mir/src/lower/calls_p2.rs                                                         817               212    74.05%          11                 2    81.82%         431               149    65.43%           0                 0         -
crates/almide-mir/src/lower/calls_p3.rs                                                         576               207    64.06%          16                 2    87.50%         273                96    64.84%           0                 0         -
crates/almide-mir/src/lower/calls_p4.rs                                                        1953               583    70.15%          28                16    42.86%         883               285    67.72%           0                 0         -
crates/almide-mir/src/lower/control.rs                                                         1129               352    68.82%          31                 3    90.32%         673               218    67.61%           0                 0         -
crates/almide-mir/src/lower/control_p2.rs                                                      1258               191    84.82%          33                 3    90.91%         635               108    82.99%           0                 0         -
crates/almide-mir/src/lower/control_p3.rs                                                      1108               349    68.50%          25                 5    80.00%         535               162    69.72%           0                 0         -
crates/almide-mir/src/lower/control_p4.rs                                                      2321               551    76.26%          39                 6    84.62%         918               174    81.05%           0                 0         -
crates/almide-mir/src/lower/control_p5.rs                                                      3463              2062    40.46%          50                28    44.00%        1483               838    43.49%           0                 0         -
crates/almide-mir/src/lower/layout.rs                                                            16                 0   100.00%           4                 0   100.00%          12                 0   100.00%           0                 0         -
crates/almide-mir/src/lower/mod.rs                                                             1281               223    82.59%          80                11    86.25%         832               122    85.34%           0                 0         -
crates/almide-mir/src/lower/mod_p2.rs                                                          1101               172    84.38%          77                 3    96.10%         569                69    87.87%           0                 0         -
crates/almide-mir/src/lower/mod_p3.rs                                                           904               201    77.77%          23                 4    82.61%         472               119    74.79%           0                 0         -
crates/almide-mir/src/lower/mod_p4.rs                                                          1189               351    70.48%          39                12    69.23%         629               202    67.89%           0                 0         -
crates/almide-mir/src/lower/mod_p5.rs                                                          1193               138    88.43%          55                 3    94.55%         695                81    88.35%           0                 0         -
crates/almide-mir/src/lower/mod_p6.rs                                                          3111               676    78.27%         104                17    83.65%        1941               464    76.09%           0                 0         -
crates/almide-mir/src/lower/tail.rs                                                            1037               408    60.66%          12                 4    66.67%         530               207    60.94%           0                 0         -
crates/almide-mir/src/purity.rs                                                                  81                 0   100.00%           5                 0   100.00%          46                 0   100.00%           0                 0         -
crates/almide-mir/src/render_rust.rs                                                            457               106    76.81%          20                 2    90.00%         268                57    78.73%           0                 0         -
crates/almide-mir/src/render_wasm.rs                                                            769                54    92.98%          43                 0   100.00%         433                28    93.53%           0                 0         -
crates/almide-mir/src/render_wasm/registry.rs                                                     3                 0   100.00%           1                 0   100.00%         648                 0   100.00%           0                 0         -
crates/almide-mir/src/render_wasm_p2.rs                                                         585               109    81.37%          10                 0   100.00%         304                57    81.25%           0                 0         -
crates/almide-mir/src/render_wasm_p3.rs                                                           3                 0   100.00%           1                 0   100.00%           3                 0   100.00%           0                 0         -
crates/almide-mir/src/translation_validation.rs                                                 256                84    67.19%          15                 0   100.00%         145                41    71.72%           0                 0         -
TOTAL                                                                                        199403             70872    64.46%        6394              2316    63.78%       98363             33553    65.89%           0                 0         -
```

低カバレッジ台帳（次にテストを書く対象。出力への影響があるものを優先）：
- lower/control_p5.rs 43%：defunc HOF エンジン（最も新しく、未テスト範囲も最大）。
- lower/tail.rs 61%、binds_p4 63%、binds_p2 64%：bind/tail lowering の分岐。
- lower/calls_p3 65%、mod_p4 68%：ルーティングと呼び出し引数の実体化。

<a id="re-measurement--2026-07-04-after-the-crush-pass"></a>

## 再計測 — 2026-07-04（crush パス後）

TOTAL 行カバレッジは、分母が増えた状態で **65.89%**（98,363行から98,573行へ。パスが zip-fusion / scalar-tuple-fold / beta-reduce / eager-init の仕組みと固定した単体テストを追加したため、対象が増えても率を維持した）。昇格した5つの probe（tests_part5）は最新の defunc 分岐を直接カバーする。次の対象は変わらず、control_p5 の defunc エンジン、tail.rs、binds_p2/p4。

<a id="re-measurement--2026-07-04-crush-pass-close-nn-walls-0"></a>

## 再計測 — 2026-07-04（crush パス完了：nn の拒否箇所0）

TOTAL 行カバレッジは **66.30%**（99,602行、未到達33,565行）。分母が約1,240行増えた（matrix_core のルーティング、一般的な非忠実クロージャの拒否、defunc `list.find`、モジュール間レコード名の正規化）一方、未到達は12行増のみで、65.89%から上昇した。昇格した7つの新規固定テスト（tests_part5：adt タプルコンストラクター、コンストラクターのリストフィールド再帰、matrix の基盤/拒否/正規化＋bytes、defunc find、load_weights のレコード返却）が新範囲をほぼ完全にカバーする。render_wasm/registry.rs のカバー行は648 → 679（matrix_core の自ホスト化）に増えた。次の対象は引き続き control_p5 の defunc エンジン（未テスト範囲最大）、tail.rs、binds_p2/p4。

<a id="rung-2-instruments--2026-08-26-566"></a>

## 第2段階の計測手段 — 2026-08-26（#566）

まず制約を測定した。rustc の MC/DC 計装はもはや存在しない（2024年追加、2025年8月削除、rust-lang/rust#144999。1.96.1 は `block | branch | condition` を受理すると実測）。そのため MC/DC 級の証拠はコンパイラフラグではなく、判断ごとの論拠である。

- **proofs/mcdc-ledger.sh / .toml**：10の安全性ファイル（perceus_verified、certificate 系、translation_validation、effect/capability 検査）の全ブール演算子箇所に行を持つ。独立性ペアの単体テスト（各テストで1条件のみを反転し、判定も反転）を名指す `vectors`、または縮小のみの上限下での `pending` のいずれか。導入時は51箇所、16テストで6箇所を解決（perceus_verified 87/207/213、calls.rs 402/449）、45保留。
- **安全性集合のファイル別行カバレッジ下限**：proofs/coverage-safety-baseline.txt。TOTAL ラチェット後に coverage.sh が強制し、TOTAL が維持されても安全性ファイルが劣化しないようにする。ローカル全実行（1.96.1/macOS）からプラットフォーム差の余裕5ポイントを引いて初期化した（CI 1.89.0/linux は数ポイント異なる。同じ夜に translation_validation はローカル70.09対CI68.18、TOTAL はローカル65.50対CI67.59）。
- **条件レベルの補完**：`ALMIDE_COVERAGE_CONDITION=1` と、日付固定の nightly（nightly-2026-08-01）による夜間 `coverage-condition` ジョブ。最初にフラグを probe し、遅延評価演算子の右辺を含む条件別分岐を記録する。下限とラチェットは同じ。
- **push 時のラチェット**：ci.yml の `coverage-ratchet` ジョブが、関連性チェックの後に push/PR 時点で同じゲートを動かす（CI で11〜14分と測定。夜間のみでは回帰発見が最大24時間遅れた）。

スキャナー自身の正直さも初回実行で調整した。クロージャ先頭（`.or_else(|| …)`）は先行トークン規則で除外し、ファイル最初の `#[cfg(test)]` より後はテストそのもののため台帳に含めない。

<a id="vector-truth-check--2026-08-27-566-mutation"></a>

## vector の真実性チェック — 2026-08-27（#566、mutation）

`vectors` 台帳行は従来、指定テストの存在しか検証せず、中身が空虚なテストでもゲートが緑になり得た。`proofs/mcdc-mutation.sh` がこれを塞ぐ。解決済み箇所ごとに正確な文字オフセットで演算子交換 mutant（&& <-> ||）を適用し（全終了経路でソースを復元しハッシュ検証）、指定 vectors が変更前に緑、mutant 下で赤になることを要求する。v1 の mutant クラスは演算子交換。条件レベルの固定値 mutant にはオペランド範囲（AST）が必要で、次のラチェットとなる。夜間にジョブ「mcdc-mutation」で実行する。cargo をこれと並行実行してはならない。

<a id="condition-floor-re-anchored--2026-09-29-2941"></a>

## 条件カバレッジ下限の基準を再設定 — 2026-09-29（#2941）

夜間の条件別ジョブが v0.65.1-rc4 で赤になった。TOTAL は55.14%、下限56.55%（fuzz-nightly run 36537776113）。最後に緑だった夜は9d80eb004上の36457122588で、56.94%だった。#2922がゲートのワークロードを既存の `render_program` から構造 wasm ビルドと証拠生成器へ移した。この変更が、構造に由来する2部分で低下全体を説明する。

2夜の表からファイル別に測定（分岐数 / 未到達）：

| 段階 | 分岐数 | 未到達 | TOTAL |
|---|---|---|---|
| 最後に緑だった夜（9d80eb004） | 37886 | 16313 | 56.94% |
| 同じ夜から INCUMBENT_ONLY ファイルを除外 | 36480 | 15888 | 56.45% |
| rc4（371de8ff5） | 36820 | 16518 | 55.14% |
| #2930後のdevelop（54349cd6c、probe run 36550517708） | 36745 | 16429 | **55.29%** |

1. **分母。** #2922が既存側専用ファイル（`render_wasm*`、`region_*`、`pipeline_{b,c,link,global_slots,test_runner}` など）を対象から外した。#2935が削除する1406分岐で、そのうち425未到達（70%カバー）。除外すると同じ緑の夜が56.94%から56.45%となり、すでに下限を割る。その下限は2026-09-04にこれらを対象として実行した状態で初期化されていた。
2. **削除済みパイプラインからのみ到達する lowering。** `almide-mir/src/lower/` は総数約11270が変わらず、未到達分岐が3720から4315へ増えた（+595）。既存パイプラインは各プログラムが生成した `__drop_*` / `__repr_*` ソースを lowering し、`pipeline_b` のプログラムパス（`populate_abi_registries`、`rewrap_never_err_into_result_targets` など）を動かしていた。製品のどちらの経路もこのコードを呼ばなくなった。
   - 証拠生成器はプログラム自身の関数だけを lowering する。
   - ネイティブ経路もプログラム自身の関数だけを lowering する。

   #2935 の head でデッドコード probe（`lower` を非公開化）を行うと、残る呼び出し元が自身の単体テストと維持された `emit_cert*` / `classify_corpus*` の例だけである172項目、約7,400行を発見する。#2950が削除する。それまではどんなワークロードからも到達しないため、ワークロードを追加しても製品が実行するものを測れない。

残りは両方向のノイズである。他の報告対象クレートでは未到達が40減、新しい構造ファイルは140分岐のうち46未到達。

そのため下限を #2930 後の develop の CI 測定値 **55.29%** に再設定する（`proofs/coverage-baseline-condition.txt` 5655 → 5529）。#2935 と #2950 はどのワークロードも到達しないコードだけを削除するので、それぞれ TOTAL を上げる方向にしか働かない。各変更が入った後、最初に緑の夜で `--update` を実行し、下限を再び引き上げる。

<a id="condition-floor-raised--2026-09-30-batch-073-2950"></a>

## 条件カバレッジ下限の引き上げ — 2026-09-30（batch-073、#2950）

develop は **53.46%**（e5b570e19 の一時 probe run 36583088728、36691分岐・17076未到達）へ落ち、55.29%の下限を下回った。下限自身の測定（probe run 36550517708、36745 / 16429）と比較すると、ほぼ同じ総数で未到達が647分岐増えた。既存側の削除で、そのレンダリング経路を通じて MIR lowering を動かしていたテストとワークロードが失われ、lowering が未実行になった。大部分は #2950 が削除する不要な lowering（`repr_sources*`、`drop_sources*`、`usage_scan`、`variant_drop_field_frees`、`mod_p2_rewrap` など）だった。#2950 単独では55.06%（run 36578464452）。

batch-073（#2950を含む）は **55.35%**（6da40736b の run 36595422938、35957 / 16054）で、下限をそこへ上げる（`coverage-baseline-condition.txt` 5529 → 5535）。

TOTAL は戻ったが、存続するファイルでは分岐総数が不変なのにカバー分岐が失われた。主なもの：`lower/defunc_tuple_fold_b.rs` は78中34 → 0、`lower/calls_p4_b_tail.rs` は61中44 → 21、`lower/defunc_tuple_fold.rs` は70中35 → 16、`lower/result_ctors.rs` は116中74 → 62、`lower/mod_p5_b.rs` は146中119 → 108、`lower/calls_p4_c.rs` は96中55 → 45。#3007は基準再設定だけでなく、製品経路が動かすワークロードで回復することを追跡する。

<a id="condition-floor-raised--2026-09-30-3007"></a>

## 条件カバレッジ下限の引き上げ — 2026-09-30（#3007）

`crates/almide-mir/tests/lower_witness_live_shapes.rs` は、#3007に挙げた存続する lowering を、証拠生成器（`almide verify` の `program_witnesses`。今もそこへ到達する唯一の製品経路）を通して動かす。batch-074（7b0e8a93f）から分けた2つの一時ブランチで、夜間の条件ジョブと全く同じ方法で測定した。

| 実行 | 分岐数 | 未到達 | TOTAL |
|---|---|---|---|
| batch-074（probe run 36642157804） | 36255 | 16217 | 55.27% |
| batch-074 + テスト（probe run 36642433391） | 36255 | 16123 | **55.53%** |

下限をこの値へ上げる（`coverage-baseline-condition.txt` → 5553）。ファイル別カバー分岐、前 → 後：`defunc_tuple_fold_b.rs` は78中0 → 34、`defunc_tuple_fold.rs` は70中16 → 35、`result_ctors.rs` は116中62 → 73、`mod_p5_b.rs` は146中108 → 113、`defunc_hof_inner.rs` は128中89 → 93。`calls_p4_b_tail.rs`（61中21）、`calls_p4_c.rs`（96中45）、`calls.rs`（120中89）は変わらなかった。未実行の分岐は `prim.*` 基盤の分岐で、既存側が自ホスト標準ライブラリ本体を lowering したときだけ到達していた。テストするか削除するかは、`prim` がユーザー向け公開機能かによる（#3025）。

この下限は batch-074 で develop に入る #2950 の削除を含む。含まないツリーで測ると TOTAL は約2ポイント低い。

<a id="condition-floor-restored-without-moving-it--2026-10-02-3195"></a>

## 条件カバレッジ下限を変えずに回復 — 2026-10-02（#3195）

夜間の条件別ジョブは v0.66.0 の期間中毎晩赤だった（下限55.53%に対して52.08〜52.30%）。テストが消えたのではなく、ワークロードがネイティブ経路を失っていた。`almide test spec/` は各ファイルをまず wasm 経路で実行し、それで通らないものだけネイティブにコンパイルする。wasm 経路がカバレッジランナーにはない wasmtime CLI を必要としていた間は、すべてのファイルがネイティブへフォールバックし、偶然 Rust コード生成を測っていた。7fb199704 が wasm 経路を組み込みホストへ移すと、フォールバックがなくなり、almide-codegen は一晩で約1,500カバー分岐を失った（develop 595d60459、51.86%）。`pass_peephole.rs` は248中148 → 32、`pass_region_window.rs` は188中154 → 46、`pass_list_pattern.rs` は86中59 → 1、`pass_shared_cell_borrow.rs` は44中41 → 1、と全ネイティブパス・walker にわたった。

`coverage.sh` は `almide test spec/ --target rust` も実行するようになった（CONDITION モードでは有効。夜間の行モードジョブは `ALMIDE_COVERAGE_NATIVE_LEG=1` で有効化。push 時ジョブはこれなしで45分予算を維持）。修正ブランチで夜間条件ジョブとして測定した（run 36972952531）。

| 実行 | 分岐数 | 未到達 | TOTAL |
|---|---|---|---|
| v0.66.0-rc5 の夜（36966772832） | 38303 | 18270 | 52.30% |
| 修正ブランチ（36972952531） | 38303 | 16618 | **56.61%** |

almide-codegen のカバー分岐は1,579増、almide-frontend は8増、他クレートは不変。下限は55.53%のまま。赤の #566 ジョブは `coverage-ratchet` ラベルの issue を作成またはコメントする（`scripts/coverage-night-route.sh`、`coverage-route` ジョブ）ため、実行ログを開かなくても赤い夜が人に届く。
