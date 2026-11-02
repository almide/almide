> 原文の説明：コンパイラの劣化の分布と、参照資料に裏付けられた対策
<a id="rot-eradication--the-decay-map-and-its-reference-backed-cures"></a>

# rot-eradication — 劣化の分布と参照資料に裏付けられた対策

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](rot-eradication.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](rot-eradication.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](rot-eradication.zh-CN.md)

> 非規範の固定読者用スナップショット。原文を正本とし、自動同期せず、実装・実測・安全性を独立に再検証したものではない。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/rot-eradication.md)
> 歴史的な active の地図・作業順と後段の導入済み節が併存し、後者では R1 完了、R3 前半のみ完了、R2 の生成時形状という最終設計は未完了。R1 段落は fs_metadata_family を R6 の link-set 分類と呼ぶが、地図の link-set 乖離は R4 であり、この原文の不整合を黙って直さず保持する。調査の普遍性・件数・安全性・性能判断は固定原文の主張で、新たな監査や現在の課題一覧ではない。


**状態**：active（result-family の一連の作業と #1418 活動の後、2026-08-15 に基本原則を記した。以下の各主張は、その作業中の測定または事象に裏付けられている）。
**調査の証拠**：`../almide-references/RESEARCH.md`（調査 1–3：呼出し結果の配置、新規変数の規律、coalescing 糖衣の除去、payload 束縛の知識、値位置の早期 return、codegen 包含集合。すべて rustc/Swift/Zig/Roc/Lean/Koka/Grain の clone への file:line 引用付き）。

<a id="the-laws-what-every-surveyed-compiler-agrees-on"></a>

## 法則（調査したすべてのコンパイラが一致する点）

1. **型が配置を選び、名前がコードを選ぶ。**（調査 1、8/8。）result-family の一連の作業として導入済み。
2. **新規 ID は単調増加するコンテキスト所有の allocator から得る。部分木走査からは得ない。**（調査 2、4/4。）band allocator として導入済み。
3. **糖衣は一度だけ汎用機構へ落とし込んで除去する。payload の分類が変えるのは出力する検査であり、通る経路ではない。**（調査 2 — Roc の `??` desugar は私たちのものと文字単位で同じ。）
4. **値ごとの知識こそ、その値の型／配置であり、生成時に付与する。射影は全域関数により子の知識を導く。「未追跡」は存在しない。**（調査 3a — Roc `Local{layout_idx}`、rustc `Place→Ty`。）
5. **形状と所有権は別の pass で扱う。**（調査 3a — Roc の match lowering は全域的で配置に基づく。ARC が後で borrow/incref を決める。）
6. **発散する arm は合流継続を落とす。生き残る分岐が ≤1 なら継続をそこへ複製し、それ以外ではローカル join point として具体化する。**（調査 3b — Koka UnReturn。私たちの末尾複製 desugar はすでに複製側の半分を担う。）
7. **codegen 包含集合は、出力された呼出しの成果物から推移的走査で計算する。並行する予測からは計算しない。**（調査 3c — rustc の collector は codegen と同じ MIR を走査する。Roc は出力 LIR から到達しないものをすべて削除し、backend は store 全体をコンパイルする。）

<a id="the-rot-map--cures"></a>

## 劣化の分布 → 対策

<a id="r1--the--route-zoo-coupling-web-active-rot-1418"></a>

### R1 — `??` 経路群の結合網（進行中の劣化；#1418）
一晩に三つの失敗形態を測定した。名前テーブルへの微調整が通常経路を不安定化した（実験を revert）。caps カウンターが route helper 呼出しを静的予測していた（#1079 の結合）。投機的試行が追跡集合を汚染した（`speculate` で修正）。**対策（法則 3）：route の本体を value-match 機構の payload-class arm に移し、chain、admission gate、counter credit、`ALMIDE_QQ_DESUGAR` 実験 pass を一度に削除する。** 測定器は導入済み：match 優先の反転（scalar：214 match / 1 fallback）、`ALMIDE_QQ_NO_ROUTES` 削除準備 probe（25 → 2：fs_metadata_family = R6 の link-set 分類；unwrap_or_heap_payload = route 本体の移行に包含される subject 分類の残留）。

<a id="r2--the-per-value-tracking-side-sets-1414"></a>

### R2 — 値ごとの追跡用補助集合（#1414）
ValueId をキーとする集合が約 20 個。「未追跡」が暗黙の状態であり、追加は巻き戻せない（`speculate` の snapshot は応急処置であって治療ではない）。bind/subject の非対称がすでに一件漏れた（修正済み）。**対策（法則 4+5）：値生成時に付ける値ごとの形状（Roc の `Local{layout_idx}` 方式。私たちの場合は `fresh_value` 近傍の構築子が割り当てる `ValueId → ValueShape`）。射影は子の形状を全域的に導き、DROP／所有権の振り分けは同じ形状を扱う後段 pass へ移す。`result_family` が最初の全域的部分であり、一度だけ seed する入口が移行手段になる。** 状態を表現不能にすることで、`speculate` の clone コストと「未追跡 subject への match」という wall 分類をなくす。

<a id="r3--value-position-propagation-arms-1421-wrong-code"></a>

### R3 — 値位置の伝播 arm（#1421、誤コード）
値位置の `match r { ok(v) => v, err(_) => f()! }` は無効な merge を出力する（受理するが誤り。`??` 反転の guard が隔離している）。**対策（法則 6）：Koka の格子 — 伝播する arm は merge を消費しない。生き残る arm が ≤1 なら継続をそこへ押し込む（既存の末尾複製機構）。それ以外では継続を合成 join fn として具体化する。** `fallback_propagates` guard を取り除き、最後の scalar route fallback を解消する。この対策の基盤部分、すなわち伝播 arm が発散できるようにするフレーム指定の `Op::Return` は、独立した一連の作業となった：[return-op-eradication.md](../../../../../roadmap/active/return-op-eradication.md)。これは、その op がないためだけに存在する `!` の位置別 desugar 群（`BRANCH_PASSES` の約 9 行）も削除する。

<a id="r4--the-link-set-divergence-envcwd-class-unfiled-until-now"></a>

### R4 — link-set の乖離（env.cwd 分類；これまで未起票）
lowering の経路変更時に、コンパイルする self-host 本体の選択が lowering の出力した呼出しと食い違った。**対策（法則 7）：lower 済み関数の出力呼出し名（codegen が使う成果物）を不動点まで走査して self-host 包含集合を導き、並行する予測をすべて削除する。さらに render の最後の防御として、未解決の内部シンボル数ゼロを構造的ゲートにする。wall メッセージではない。**

<a id="r5--residual-name-registries-purity-never_errauto_wrap-router"></a>

### R5 — 残る名前 registry（purity、NEVER_ERR/AUTO_WRAP、router の特例）
撤去した family テーブルと同類で、現在はテストに支えられている。**対策（調査 1 の Swift/Lean パターン）：名前由来の事実を境界で一度だけ signature/registry へ畳み込み、下流では名前から再導出しない。materialized set を registry から導出すること（result-family 作業の未完了部分）がひな型となる。** 機会を捉えて、各 registry に触れる際に移行する。

<a id="r6--the-harness-reports-walls-as-panicked"></a>

### R6 — ハーネスが wall を「panicked」と報告する
診断の完全な回り道を生んだ（12 件の「panic」は実際には正当な wall）。**対策：クロスターゲットハーネスの報告行で wall/build-fail/run-fail/diverge を区別する。** 小さく、すぐ行う作業。

<a id="r7--include-splicing-blinds-rust-analyzer"></a>

### R7 — include! による継ぎ込みで rust-analyzer が見えなくなる
安全上最重要の crate で IDE 診断が機能しない（一晩中の偽 E0308／unlinked-file の雑音）。**対策：実際の `mod` ファイルにする（行数上限ポリシーはファイル単位でなくモジュール単位で数えられる）、または継ぎ込みルートを認識する workspace の rust-analyzer 設定にする。** ツールの判断で、低リスクかつレビュー処理能力への効果が大きい。

<a id="landed-2026-08-15-the-night-of-the-constitution"></a>

## 導入済み（2026-08-15、基本原則を記した夜）

- **R1 完了** — #1418 をクローズ。経路群を削除（+148/−1054）。`??` は match 機構の経路だけになり、admission gate、operand resolver、counter credit、実験 pass はなくなった。三つの byte test が削除を固定する。`speculate`、型付き seeding、両 payload 幅での subject-ANF、merge 所有権規約により、準備 probe は 25 → 0 となった。closure と fallible-carrier の route 本体は、payload-class の場合分けとして経路内に移った。
- **R3 の前半** — 誤コードの受理を解消した（値位置の scalar match は伝播 arm を拒否する。既知の生成元はすべて、両 payload 分類に対応した `(match …)!` 文の書換えを通る）。値位置でこれらの arm を受理するための join-point lowering は、引き続き #1421 の作業である。
- **R2 一度だけ seed する節目** — `seed_variant_value_shape` が、すべての位置で variant の読取り形状と drop-route を seed する単一の型付き入口となった。位置ごとのずれの分類を解消した。生成時に形状を持たせる最終目標は残る。
- **R6 の症状を記録** — 準備 probe の盲点（unit test と spec/lang は wasm_cross corpus の外）が wall と panic の報告混同を悪化させた。R6 に取り組む際は両方を扱う。

<a id="order-and-gates"></a>

## 順序とゲート

R3 → R1（R3 が最後の scalar fallback を解放し、R1 の移行で準備 probe が空になる）→ R4（そのゲートで R1 の一括削除を安全に検証できる）→ R2（R1 が露わにした接点を使う根本対策）→ R5/R6/R7 は機会を捉えて行う。
各段階は既存ゲート（parity corpus、verify-trust、ratchet）と専用の測定器を使う。R1 = `ALMIDE_QQ_NO_ROUTES` の予期しない件数（目標 0）と `ALMIDE_DBG_QQ` の fallback 件数（目標 0）；R2 = 補助集合数（目標：集合の削除）；R4 = 未解決シンボルの render ゲート（目標：存在し、0 である）。
