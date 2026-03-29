# Spec Rules

## コードが truth、spec は証跡

- spec は実装と同時に更新する。コードを変更したら対応する spec も同じコミットで更新
- spec にはテストファイルのパスを書く。テストが通る = 仕様が正しい
- テストなき仕様は存在しない。`spec/` にテストがなければ書くな
- 古い spec は消す。実装と乖離した spec は誤導する。`_deprecated/` に溜めない
- 「〜のはず」「〜予定」は書かない。動くコードがあるものだけ書く
- 未実装の設計は `docs/roadmap/` に書く。ここは実装済みの仕様のみ

## フォーマット

- 冒頭に `> Last updated: YYYY-MM-DD` を入れる
- 各セクションに検証テストのパスを明記: 「テスト: `spec/integration/modules/diamond_test.almd`」
- 仕様の記述にはコード例を添える。コード例はそのまま `.almd` ファイルに貼ってコンパイルできるものにする

## 現在の spec

| ファイル | 内容 |
|---|---|
| `language.md` | 言語仕様: 型, 宣言, 式, 文, パターン, 演算子, 可視性, コメント |
| `type-system.md` | 型システム: 推論, ジェネリクス, レコード, バリアント, プロトコル, Union |
| `effect-system.md` | エフェクト: fn vs effect fn, 自動?伝搬, fan, 権限, E006/E007/E008 |
| `codegen.md` | コード生成: Nanopass pipeline, テンプレート, Rust/WASM 出力, モジュール命名 |
| `cli.md` | CLI: 全コマンド, オプション, エラーコード, Legacy Mode |
| `module-system.md` | モジュール: import, サブモジュール, ダイヤモンド依存, 可視性, @extern |
| `package-system.md` | パッケージ: 依存管理, MVS, バージョン共存, モジュール境界 |
