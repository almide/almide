<!-- description: almide completions subcommand for bash/zsh/fish auto-completion -->
# Shell Completions

`almide completions` サブコマンドで bash/zsh/fish 用の補完スクリプトを生成する。distribution-ux から分離した残作業。

## 想定する使い方

```bash
almide completions bash > ~/.local/share/bash-completion/completions/almide
almide completions zsh  > ~/.local/share/zsh/site-functions/_almide
almide completions fish > ~/.config/fish/completions/almide.fish
```

## 実装方針

- clap の `clap_complete` クレートで自動生成
- `Shell` 引数を取り `generate()` でスクリプトを stdout に書き出す
- `tools/install.sh` / `tools/install.ps1` にもインストール時に補完を自動配置するオプションを追加（任意）

## 変更が必要なファイル

- `Cargo.toml` — `clap_complete` を依存追加
- `src/cli/mod.rs` — `Completions { shell: Shell }` サブコマンド追加
- `src/main.rs` — ハンドラ実装

## なぜ on-hold か

distribution-ux 本体（バイナリ配布、ワンラインインストーラ、self-update）が完了したことで、補完は「あれば便利」レベルのポリッシュ作業に下がった。Almide ユーザーが増えてきたタイミングで実装する。
