# ツールチェイン拡張

## REPL (`almide repl`)
式を入力して即座に評価。学習・プロトタイピング用。
- AST→Rust emit→rustc→実行のパイプラインを対話的に回す
- 履歴・補完

## LSP (`almide lsp`)
エディタ統合。Go の gopls 相当。
- 補完（関数名・モジュール関数・型名）
- 定義ジャンプ
- ホバーで型表示
- エラー表示（チェッカー統合）
- フォーマット（almide fmt統合）

## ドキュメント生成 (`almide doc`)
- `///` docコメントをlexer/ASTに追加
- モジュール・関数・型のドキュメントをHTML/Markdown生成

## ベンチマーク (`almide bench`)
```almide
bench "list sort 1000 elements" {
  let xs = list.reverse(range(0, 1000))
  list.sort(xs)
}
```

## パッケージレジストリ
- `almide add fizzbuzz` で中央レジストリから取得
- 現状はGit URL直指定のみ
- バージョン解決（semver）

## almide fmt のコメント保存
- lexerでコメントをトークンとして保持
- ASTノードにコメント情報を付与
- フォーマット時にコメントを復元

## Priority
LSP > REPL > docコメント > ベンチマーク > レジストリ > fmtコメント保存
