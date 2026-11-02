<a id="tasks-examplexlang"></a>

# tasks-example/xlang

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> 非規範の原文固定読者用スナップショット。元のパスを正本とし、機械入力として変更・置換しない。自動同期はしない。過去の測定や aviation-quality などの名称は、独立の検証や認証を意味せず、目標は達成結果ではない。
> [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/lang-bench/tasks-example/xlang/README.md)


これはベンチマークのタスクではなく、`runner_edits.rb` のフィクスチャである。`t0-smoke` は二つの言語（Almide、Rust）の初期コードで、順序付きの二つの編集、それぞれの参照解と、もっともらしい誤ったパッチを含む。したがって、

```bash
ruby runner_edits.rb --tasks tasks-example/xlang --dry-run
```

により、モデル呼び出しなしで、タスク集のゲート全体をローカルに動かせる。#1963 の実際のタスク集は Dojo の `tasks/xlang/` に同じ配置で置かれている（`runner_edits.rb` の冒頭を参照）。
