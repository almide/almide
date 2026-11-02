<a id="fuzz-true-green-streak-aviation-quality-stage-4"></a>

# Fuzz の真の正常継続日数（aviation-quality Stage 4）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> 非規範の原文固定読者用スナップショット。元のパスを正本とし、機械入力として変更・置換しない。自動同期はしない。過去の測定や aviation-quality などの名称は、独立の検証や認証を意味せず、目標は達成結果ではない。
> [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/fuzz-green/README.md)


ミッションクリティカルな監査担当者が読む指標は「修正の速さ」ではなく「どれだけ壊れずに続いたか」である。暦日が CLEAN となるのは、その日の完了した実行が採点器の正常条件を満たす場合だけ。シャード化した実行では、workflow の結論だけでなく `Night verdict` ジョブを使い、計画した fuzz 分数の 75% 以上を実行し、発見事項がないこと（回収されたシャードの発見事項も含む）を求める。失敗または部分実行の日は正常継続を途切れさせる。実行のない日は日数を増やさず、リセットもしない。旧方式と verdict 欠落の扱いは `scripts/fuzz-green-streak.sh` と共通採点器 `scripts/lib/fuzz-night-line.sh` を参照。
最初の目標：**90 日連続の正常日**。

計測器：`scripts/fuzz-green-streak.sh`。毎晩の ledger-counts 実行は `--update` を付けて呼び出す。継続日数が最終行と異なるたびに日付付き行を追記し、そこで作成する更新には再刻印された件数とともにその行を含める。

| 測定日（UTC） | 継続（日数） | 継続開始 | 最終実行日 |
|---|---|---|---|
| 2026-08-10 | 0 | - | 2026-08-10 |
