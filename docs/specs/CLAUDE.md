# Spec Rules

## コードが truth、spec は証跡

- spec は実装と同時に更新する。コードを変更したら対応する spec も同じコミットで更新
- spec にはテストファイルのパスを書く。テストが通る = 仕様が正しい
- テストなき仕様は存在しない。`spec/` にテストがなければ書くな
- 古い spec は消す。実装と乖離した spec は誤導する。`_deprecated/` に溜めない
- 「〜のはず」「〜予定」は書かない。動くコードがあるものだけ書く
- 未実装の設計は `docs/roadmap/` に書く。ここは実装済みの仕様のみ

## フォーマット

- 冒頭に `Last updated: YYYY-MM-DD` を入れる
- 各セクションに検証テストのパスを明記: 「テスト: `spec/integration/modules/diamond_test.almd`」
- 仕様の記述にはコード例を添える。コード例はそのまま `.almd` ファイルに貼ってコンパイルできるものにする

## 現在の spec

| ファイル | 内容 |
|---|---|
| `module-system.md` | import, サブモジュール, ダイヤモンド依存, 可視性, @extern |
| `package-system.md` | 依存管理, MVS, バージョン共存, モジュール境界, レジストリ構想 |
