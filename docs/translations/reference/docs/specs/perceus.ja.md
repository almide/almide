<a id="almide-perceus-type-guided-automatic-memory-management"></a>

# Almide Perceus：型に基づく自動メモリ管理

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](perceus.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](perceus.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](perceus.zh-CN.md)



> 正本：[docs/specs/perceus.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/perceus.md)、固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`。非規範的な読者向けスナップショット訳で、自動同期しない。原仕様とその明示する規範上の位置付けは変更しない。モデル証明・実装の主張・歴史的状態・証拠の範囲は原文の限定を保ち、この翻訳による独立の実行・測定・適合性／安全性証明はない。コードブロック内のコードとコメントは原文のまま。

> 最終更新：2026-10-31
>
> ユーザーコードに所有権注釈や明示的な `free` はない。所有権推論と参照カウントの仕組みは実装機構であり、受理されたすべてのプログラムや出力成果物がメモリ安全であることの証明ではない。

<a id="overview"></a>

## 概要

Almide は Perceus スタイルの所有権解析を使い、ヒープ値をどこで導入・共有・消費するか決める。実行はターゲットごとに異なり、ネイティブ出力は Rust の所有権／ランタイム操作を使い、構造 Wasm エミッタは自身のヒープと参照カウントを管理する。証明済みの性質、信頼するコンポーネント、未解決の不足についての現在の説明は [メモリ安全性](../../../../design/MEMORY-SAFETY.md) にある。

<a id="reference-counting-model"></a>

## 参照カウントのモデル

以下は概念上の規則であり、各ソース式について約束する文字通りの命令列ではない。lowering、借用、静的な値、最適化により、必要な操作は変わる。

1. **共有**：新しい所有参照を計上する必要がある。所有権を単に移動・借用できない場合、通常は増分操作で行う。
2. **解放**：所有参照の消費・破棄を計上する必要がある。減分によってカウントがゼロになったとき、割当てを解放できる。
3. **複合的な drop**：所有するコンテナを解放するときは、それが所有するヒープ値も計上する必要がある。子の訪問方法はレイアウトと表現で決まり、クロージャ環境もこの義務の一部である。

静的文字列、所有されたヒープバッファ、借用参照は相互に置き換え可能なケースではない。値を 1 回だけ使うことは、最後の解放を省略してよいという意味ではない。

<a id="optimizations-and-limits"></a>

## 最適化と限界

所有権と一意性の情報により、対応する操作では再利用やその場での更新が可能になる。融合により中間コンテナを避けられる場合もある。これらは条件付きの最適化であり、任意の `map` / `filter` / `fold` パイプラインが割当てゼロになる保証ではない。コールバックの動作、表現、エイリアシング、選んだ lowering のすべてが関わる。

単一使用の値や所有された一時値は、冗長な共有操作を避けられる。それでも正しい消費・後始末は必要。単一使用であることも、型が解決済みであることも、それだけですべての RC 操作を除去できるとは証明しない。

<a id="verification-boundary"></a>

## 検証の境界

実装には、範囲の異なる複数種類の根拠がある。

- [`perceus_verified.rs`](../../../../../crates/almide-codegen/src/perceus_verified.rs) は Lean の定義を写し、モデル内に限定された挿入／計数の性質を名指す。それらは、完全な出力プログラム内のすべての lowering とランタイム操作の正しさを立証しない。
- Rocq/Coq の証明書とメモリ安全性モデルは、[メモリ安全性](../../../../design/MEMORY-SAFETY.md) と [`proofs/Certified.v`](../../../../../proofs/Certified.v) に記載される。明示された仮定と表現対象のトレースは主張の一部であり、省略してよい詳細ではない。
- [`almide verify`](../../../../../src/cli/verify.rs) は構造 Wasm の witness を生成し、独立した `almide-verify` バイナリを呼ぶ。成果物のバイトへの結び付けはバンドルを特定ファイルに接続するもので、現在の検査を、無条件で成果物全体のメモリ安全性を保証する定理に変えるものではない。

特に、**現時点では型チェッカーの受理から、すべての出力成果物について no-leak / no-use-after-free / no-double-free の証明済み定理は導けない**。lowering、トレース、ネイティブランタイムに残る義務は、リンク先のメモリ安全性の不足一覧に記載される。型主導 RC モデルからそれらを推測しないこと。

<a id="references"></a>

## 参考文献

- Reinking et al., "Perceus: Garbage Free Reference Counting with Reuse" (ICFP 2021)
- Ullrich & de Moura, "Counting Immutable Beans" (IFL 2019)
- Tofte & Talpin, "Region-Based Memory Management" (POPL 1997)
