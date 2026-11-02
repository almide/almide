# 標準ライブラリの再設計（3層）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](stdlib-redesign.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](stdlib-redesign.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](stdlib-redesign.zh-CN.md)



> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/stdlib-redesign.md)の非規範的な読者向け翻訳です。自動同期されない歴史的な固定スナップショットです。状態・計画・件数は原文時点の記述であり、現在の実装や完了を立証しません。この翻訳では主張を独立に実行・検証していません。

> 原文の説明：9言語と比較する標準ライブラリの3層再設計。規則、カテゴリ一覧、モジュール別の役割表、すべての公開関数の判定、採用提案から作られた issue。

設計記録は [docs/roadmap/stdlib-redesign/](../../../../../roadmap/stdlib-redesign/README.md) にあります。

- [L0](../../../../../roadmap/stdlib-redesign/L0-conventions.md)：モジュール横断の規則。命名、配置、その場での書き込み、失敗、変換、互換性を扱います。
- [L1](../../../../../roadmap/stdlib-redesign/L1-categories.md)：Rust、Go、Python、TypeScript/JavaScript、Gleam、Elixir、OCaml、Kotlin、Swift と比較したモジュール一覧。
- `L2-<module>.md`：モジュールごとのページ。役割の比較とすべての公開関数の判定（記録時点で1,001件）を含みます。
- [README](../../../../../roadmap/stdlib-redesign/README.md)：すべての提案とその判定（○ / × / 保留）、対応する issue。
- [目標となる公開面](../../../../../roadmap/stdlib-redesign/target-surface.md)：採用したすべての提案が反映された後の標準ライブラリ。モジュールごとに全公開関数の再設計後の署名（記録時点の1,001件 → 1,289件）と、削除される関数の移動先を示します。
- [プロトコルのピラミッド](../../../../../roadmap/stdlib-redesign/protocol-view.md)：Swift を参考に層を構成した標準プロトコル。各型を、準拠するプロトコルと固有の関数として示し、準拠関係から見える不足と、それにより必要になる判断を扱います。

改名と削除には epoch 15 の非推奨期間を使い、epoch 16 で削除します（R-L0-14）。
