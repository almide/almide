<a id="1437-the-lifted-carriers-abi--declaration-driven-one-readable-layout"></a>

# #1437: lift された carrier の ABI — 宣言駆動の、読み取れる単一レイアウト

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](lifted-carrier-abi.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](lifted-carrier-abi.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](lifted-carrier-abi.zh-CN.md)

> 非規範の固定読者用スナップショット。変更しない原文を正本とし、機械入力として置き換えない。自動同期や独立実行はしない。歴史的測定・提案・原文時点の実装更新を区別して保持し、現在の正しさや性能の独立証明にはしない。コードとコメントは原文どおり保持する。 L1 は明示的に実装済みだが L2・L3 は未完。8月の probe 手順・拒否数・gate 案を、宣言だけの ABI や既定切替の完了と読まない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/lifted-carrier-abi.md)

> 元 metadata の非活性要約：#1437 — 生成側の drift の代わりに宣言駆動の単一 carrier レイアウト。

<a id="l1-implemented-later-stages-remain-separate"></a>

## L1 は実装済み、後続段階は別のまま

#1437 の **L1 の probe 専用 wrapping と heap-family 受け入れは実装済み**であり、[mod_c_tail.rs](../../../../../../crates/almide-mir/src/lower/mod_c_tail.rs) と [binds_p2_bang.rs](../../../../../../crates/almide-mir/src/lower/binds_p2_bang.rs) にある。以下の L1 の手順と拒否件数は 8 月の調査スナップショットである。これは L2 の宣言だけによる ABI や L3 の既定切替の成立を示さない。`bang_return_probe` と probe 条件付きの位置脱糖はまだ存在する。後続のマイルストーンは未完のまま扱い、この取り組み全体の完了とみなさない。

状態: DESIGN（2026-08-24 調査。方向は issue に記録された候補に従う — 構成による生成側の一致）。return-op R2 行列の fn-family クラス（fn レベルの拒否 40 件、残る fallback ファイルの約 30 件）の障害を解く。親の取り組み: return-op-eradication R2/R3、result-family-from-type Phase 3、rot-eradication R5。

<a id="the-disease-measured-precisely"></a>

## 正確に測定した問題

#1437 は絡み合う 2 つの欠陥であり、調査は「lift が len-as-tag を実体化する」の正体を明らかにした。

1. **タグオフセットの選択ではなく、hybrid ABI（#841）。** エラーを返し得る lift 済み heap-ret fn で、`auto_wrap_abi_body` はルート本体を `Result[T, String]` に型変更するが、`wrap_return_positions_in_ok` は `opt_ret || !has_propagation_site(body)`（mod_c.rs:599）で制限される。`!` を含む本体は ok の末尾を RAW のまま残し、err 経路は実際の wrapped block を実体化する。どの読み手も raw String block と carrier を区別できない。記録された crash（fs_fold_lines_range、`0x2f2f`）は raw payload のバイトをタグとして読んだものだった。probe が OFF なら、その後の位置脱糖が本体を明示的な ok()/err() arm に書き換え、hybrid を隠す。R2（脱糖 OFF）では何も wrap しないため、このクラスは `decl_ret_family = None` で拒否される（mod_c.rs:552 — 封じ込め）。
2. **ABI の判断が試行 lowering に対する不動点である。** `AUTO_WRAP_ABI_FNS` の membership = 本体述語 + 相互再帰の試行 inline（`lowers = |f| lower_function_all_with_types(f, ..).is_ok()`、mod_p2_b.rs:264）であり、never-err strip とともにスナップショット不動点まで反復する（pipeline_b.rs:406-447、#485）。受け入れが本体を変え、本体が membership を変える。どの fn が Result ABI を持つかは、その時点の lowering が何を受け入れるかに依存する。frontend 自身の lift は既にシグネチャ駆動（`should_lift_effect_fn_ret`、pass_result_propagation.rs:78）。MIR registry は名前をキーとする第 2 の、より狭い判断である（rot-eradication R5 の残存クラス）。

<a id="the-measured-fact-that-shrinks-the-fix"></a>

## 修正を小さくする測定事実

すべての出口が実際に wrap されれば、@16 の tag slot は lifted-heap クラスが生成できるすべての block で既に有効である。

- err 経路: `materialize_result_err_str` は SUPERSET block を書く（len@4 = 1 かつ tag@16 = 1、result_materialize.rs:59-93）。
- heap ok 経路: heap の `x` に対する `ok(x)` は `materialize_result_str` / `_aggregate` に進む。もともと cap-as-tag（Ok で @16 = 0）。
- scalar materializer の block は構成上 @16 をゼロにする（`Init::ResOkScalar` の 8 バイト payload store、lib.rs:196）。

したがって `result_family` が HeapOk と型付けする `Result[<heap T>, String]` では、**hybrid がなくなった時点で**、生成側と型は @16 で構成上一致する。新レイアウトも stdlib reader 変更も生成 drop 変更も不要。（Scalar ファミリー全体の @16 への収束 — RESEARCH Stage A、約 70 ファイル、stdlib reader 14 個、生成 drop 5 個 — は明示的にこの取り組みには不要で、独立した roadmap のままとする。）

<a id="stages"></a>

## 段階

<a id="l1--kill-the-hybrid-under-the-probe-lift-the-containment-the-r2-unblocker"></a>

### L1 — probe 下で hybrid をなくし、封じ込めを解除する（R2 の障害解消）

- `auto_wrap_abi_body`: `bang_return_probe()` 下では return 位置を無条件に wrap する（省略を正当化していた脱糖はそこで実行されない）。probe OFF の挙動はバイト同一。
- `decl_ret_family` の arm 2 を heap 戻り値へ広げる。`Some(result_family(Ty::result(T, String)))` — 2026-08-16 実験の 1 行変更が、今は正当化される。probe 下ではすべての出口が wrapped block となり、その @16 読み取りは上記生成側一覧から正しい。（その実験は、封じ込めがこのクラスを拒否していたため、差を識別する再現例がなく失敗した。R2 では差を識別するプログラムこそ拒否される行列ファイルであり、現状は拒否、修正後は実行、3-way byte-equal となる。）
- 値位置の `e!` reader（calls_p4.rs:463-500）は tag@4 / payload@12 をハードコードする。probe 下で heap callee から到達できるなら、family によってオフセットを選ぶ（generic hoist が先に bind 位置へ移すはずなので、測定してから修正するか、到達不能の理由を記録する）。
- **Gate**: 赤→緑の probe ファミリー（エラーを返し得る lifted heap-ret producer + `!` consumer、String / List[String] / record payload、ok と err の両経路）、census fn-family の減少（40 → 残件）、probe ON で `spec/wasm_cross/effect_tco` が緑を維持（TCO の例外範囲 — wrapping は if spine を型変更し、それでもループ変換が発火しなければならない。回帰するなら wrap に末尾自己再帰例外を残して文書化する）、coverage ratchet と probe OFF の battery 不変、heap-cap churn、因果 A/B。

<a id="l2--the-abi-decision-becomes-a-declaration-fact-kills-the-fixpoint"></a>

### L2 — ABI 判断を宣言の事実にする（不動点をなくす）

- 書き換え前に元の宣言と構文的 can-err（compute_can_err は受け入れに非依存）から membership を 1 度だけ計算する。populate→rewrite 不動点（pipeline_b.rs:406-447）と、試行 lowering が membership に与える影響を削除する。#485 の drift クラスは逆方向に閉じる。書き換えは fn の ABI を変えてはならない。最後の伝播 `!` を除去する strip でも wrapped ABI を保ち、wrap は自明に実体化する。
- never-err の raw-T ABI（yaml/parser TCO クラスタを支える）は残す。NEVER_ERR ∧ ¬can-err 自体が宣言レベルの事実である。
- 整合対象: frontend の `should_lift_effect_fn_ret`。判断は 1 つ、場所は 1 つ、境界でシグネチャへ織り込む（rot-eradication R5 の治療をそのまま適用）。
- **Gate**: `ALMIDE_ABI_PROBE` snapshot 差分（前後の membership を fn ごとに説明できること）、コーパス全体の 3-way、census、#485/#786 回帰 fixture。

<a id="l3--r3-folds-the-probe-default-the-hybrid-dies-everywhere"></a>

### L3 — R3 が probe を既定に統合し、hybrid が全面的になくなる

R2 行列が空になり、probe が既定に切り替わると（return-op R3）、L1 wrap が唯一の経路になり、`has_propagation_site` と hybrid を位置脱糖とともに削除する。別作業ではない。L1 を最終状態と誤解しないようここに記録する。

<a id="non-goals"></a>

## 対象外

- Scalar-family のレイアウト変更はしない（Result[scalar, String] は len-as-tag のままで、現状の生成側と読み手は一致している）。
- stdlib .almd reader 変更も生成 drop 変更もしない（理由は上記 @16 一覧）。
- L2+L3 より前に probe OFF の封じ込めは解除しない。probe OFF で hybrid を扱う読み手は位置脱糖であり、現在それが処理している。

<a id="exit-criteria"></a>

## 完了条件

1. R2 census: fn-family の拒否を void-fn の残件まで減らす。約 30 ファイルのクラスが probe ON の wasm レッグで実行し、3-way byte-equal になる。
2. `AUTO_WRAP_ABI_FNS` の構築が lowering verdict を読まない（L2）。
3. #1437 issue は、差を識別する fixture ファミリーを契約の下にコミットして閉じる。L1 前のコンパイラでは拒否（因果 A/B）、修正後には実行する。
