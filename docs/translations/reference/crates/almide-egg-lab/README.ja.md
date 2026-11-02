# almide-egg-lab

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)



> 原文: [crates/almide-egg-lab/README.md @ 35297a](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-egg-lab/README.md)。非規範的な読者向けスナップショット翻訳です。自動同期されません。計測値・主張の独立した再検証は行っていません。原文を正本とします。
>
> 原文上の注意: PoC のテスト数・計測・将来段階の記述は元の実験記録です。冒頭の統合状態と区別し、現在の計測や今後の確定計画と解釈しないでください。

**コンパイラが使用する等式飽和ルールと IR ブリッジ。**

> 現在の状態（2026-10-31）:
> [`EggSaturationPass`](../../../../../crates/almide-codegen/src/pass_egg_saturation.rs) は、このクレートのルール、ブリッジ、コストモデルを使います。独立した、自由に削除できる実験ではありません。以下の PoC の判定、時間、段階別の課題は、元の実験記録であり、現在のテスト数や統合チェックリストではありません。

元の実験では、10 か月の [MLIR Backend + Egg Rewrite Engine](../../../../roadmap/on-hold/mlir-backend-adoption.md) arc に着手する前に一つの問いを立てました。*egg は Almide の既存 stream-fusion ルールを実際に表現し、すばやく停止して、融合した形を選べるのか？*

<a id="verdict"></a>

## 判定

**はい。** 小さな PoC（5 テスト）と実 IR ブリッジ（11 テスト。実際の `VarTable` に基づくベータ簡約で lift → saturate → **lower** の往復を検証する 5 テストを含む）は、release ビルドで約 2.5 s で合格します。三つの代表形で融合が働き、小さな反復予算内で飽和が収束し、手動のフェーズ順序なしでルール間の合成が動きます。最も重要なのは、`compose` と `and-pred` マーカーが lower 時に新しい `VarId` を持つ実際の `IrExprKind::Lambda` ノードへベータ簡約され、抽出出力が既存パイプラインへ戻せる型の正しい `IrExpr` 断片になることです。

| テスト | 示すこと |
|---|---|
| `identity_map_collapses` | `map(xs, identity) ≡ xs` — 単一ルール書換え |
| `map_map_fuses` | `map(map(xs, f), g) ≡ map(xs, g ∘ f)` — 関手則 |
| `filter_filter_fuses` | `filter(filter(xs, p), q) ≡ filter(xs, p ∧ q)` — 述語の論理積 |
| `triple_map_fuses_transitively` | 三段の連鎖が一回の走査になる — 反復数/ノード数予算内で飽和が収束 |
| `identity_inside_map_chain_eliminates` | 恒等除去 **+** map 融合が**明示的なフェーズ順序なしで**合成 — 優先順位付き書換え器（GHC RULES、MLIR PDL）でなく egg を選ぶ中心的な動機 |

最後のテストが重要です。命令的な `pass_stream_fusion` では順序を決める必要があります。先に恒等を除去してから融合するか、先に融合してから縮約に気づくか。どちらの順でも取りこぼすプログラムがあります。egg では選びません。飽和が両候補を e-graph に保持し、抽出器が最も安いものを選びます。

<a id="scope-intentionally-small"></a>

## スコープ（意図的に小さい）

- 三つのルールに必要な IR 形状のみをモデル化します。`map`、`filter`、`fold`、`lam`、`compose`、`and-pred` と数値/記号アトムです。
- **ラムダ代入は飽和内部ではなく lower 時に行います。** E-graph は追加の alpha-renaming 機構なしで束縛子を表現できません（egg の `examples/lambda.rs` 参照）。飽和は `compose` / `and-pred` をゼロコストのマーカーとして保持します。その後、抽出された最良の一形状を、新しい `VarId` を持つ実際の `IrExprKind::Lambda` ノードへベータ簡約します。一度限りの割当量は出力サイズで制限され、メイン IR が求める fresh-id の規律を失わずに e-graph の alpha-equivalence 問題を避けます。
- 元の実験は独立していました。現在の codegen 統合は上記リンクのとおりです。このクレートの変更・削除はコンパイラ出力に影響し得ます。

<a id="whats-included"></a>

## 含まれるもの

- **Toy Language PoC**（`src/lib.rs`、`tests::`）: 文字列から解析する `AlmideExpr` 式、三つの融合ルール、五つのテスト。
- **IrExpr ブリッジ**（`src/bridge.rs`、`tests/bridge_test.rs`）: `almide_ir::IrExpr` 部分木（list コンビネータ＋恒等ラムダ検出）を `RecExpr<AlmideExpr>` に lift し、抽出形を型の正しい `IrExpr` へ lower します。十一のテストは恒等除去、map/filter 融合、ルール間合成、不透明な pass-through、および lower 出力で新しい `VarId` 割当、本体代入、構造形状を検証する五つの往復例をカバーします。

<a id="what-this-does-not-yet-answer"></a>

## まだ答えていないこと

arc 文書で未解決の問いとして追跡しています。

- **型を考慮するルール。** 実際の Almide ルールは要素型（`List[Int]` 対 `List[String]`）に依存します。egg は `Analysis` で対応しますが、まだ試していません。
- **大規模 IR。** 三段連鎖までテスト済みです。Stage 1 では 100–1000 IR ノードのプログラムを測定し、飽和時間が許容範囲に収まるかを確認します。
- **ターゲット向けコスト関数調整。** 現在の `FusionCost` は単にループにペナルティを与えます。本当のターゲット別コスト（Rust iter-collect、WASM 手動ループ、GPU オフロード）は Stage 3 の課題です。
- **e-graph 内のラムダ代入。** `lower` 時のベータ簡約で往復の動作は示せますが、グラフ内代入（egg の `egg/examples/lambda.rs` のようなカスタム `Applier` + alpha-renaming）なら飽和が compose マーカーの内部を見て、さらに融合を発見できる可能性があります。どのルールが必要とするか判明した後、Stage 1+ の対象です。

<a id="running"></a>

## 実行

```bash
cargo test -p almide-egg-lab --release
```

<a id="original-next-step-proposal-if-kept-historical"></a>

## 維持する場合の元の次段階提案（歴史的記録）

この PoC がレビューを通れば、MLIR 採用 arc の Stage 1（`docs/roadmap/on-hold/mlir-backend-adoption.md` 参照）で次を行います。

1. 言語を拡張して `IrExpr` enum 全体をカバーする
2. lower 時のベータ簡約を維持するか、カスタム `Applier` で代入を e-graph 内へ移すか決める
3. 型パラメータ付きルールを追加する
4. 残る 4 つの `pass_stream_fusion` ルールを移植する
5. 既存 Nanopass パイプラインの前に opt-in パスとして等式飽和を実行する

<a id="original-rejection-alternative-historical"></a>

## 元の不採用時の代替案（歴史的記録）

実際の Almide プログラムで飽和が扱いきれないと判明した場合、既存 Nanopass フレームワーク内の GHC RULES 型優先順位書換えに戻ります。MLIR 採用 arc は egg なしで続けます。PoC 段階ではクレート削除も選択肢でした。現在は独立して削除できません。現行コンパイラが依存しています。
