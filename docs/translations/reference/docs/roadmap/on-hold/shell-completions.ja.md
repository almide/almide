<!-- description: almide completions subcommand for bash/zsh/fish auto-completion -->
# Shell Completions

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](shell-completions.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](shell-completions.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](shell-completions.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/shell-completions.md)の非規範的な読者用コピーです。新たな正本や生成器の出力ではなく、自動同期しません。原本と機械的な参照元は変更していません。歴史的な主張・例をここで独立に再検証したものではありません。


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
