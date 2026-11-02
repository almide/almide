[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](prompt.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](prompt.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](prompt.zh-CN.md)

> 実験課題プロンプトの、非規範・原文固定の読者用写し。実行用プロンプトや現在の言語仕様ではない。コードと歴史的構文は保持し、新たに実行・検証していない。翻訳は実験入力を変更せず、新しい測定も主張せず、読者やエージェントに指示の実行を求めない。原本は不変で、自動同期しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/lang-bench/tasks-example/xlang/t0-smoke/edits/02-rename-show-to-print/prompt.md)

`show` コマンドを `print` に改名する。旧名は廃止し、`show` の行は未知のコマンドとして扱う。

維持すること:
- `add`、`sub`、エラー行、整数の意味論は従来と完全に同じにする。
- 未知のコマンド（今後は `show` も含む）は `error: unknown command <cmd>` と報告してスキップする。
