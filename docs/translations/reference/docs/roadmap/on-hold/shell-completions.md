# Shell Completions

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](shell-completions.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](shell-completions.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](shell-completions.zh-CN.md)



> Non-normative reader translation of the [pinned source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/shell-completions.md). This is a fixed historical snapshot, not an automatically synchronized document. Status, plans, and counts are those of the source; they do not establish current implementation or completion. This translation did not independently execute or verify the claims.

> Source description: almide completions subcommand for bash/zsh/fish auto-completion.

Generate completion scripts for bash/zsh/fish with the `almide completions` subcommand. This is remaining work separated from distribution-ux.

<a id="想定する使い方"></a>

## Intended usage

```bash
almide completions bash > ~/.local/share/bash-completion/completions/almide
almide completions zsh  > ~/.local/share/zsh/site-functions/_almide
almide completions fish > ~/.config/fish/completions/almide.fish
```

<a id="実装方針"></a>

## Implementation approach

- Generate automatically with clap's `clap_complete` crate.
- Take a `Shell` argument and write the script to stdout with `generate()`.
- Also add an option to `tools/install.sh` / `tools/install.ps1` to place completions automatically during installation (optional).

<a id="変更が必要なファイル"></a>

## Files requiring changes

- `Cargo.toml` — add the `clap_complete` dependency.
- `src/cli/mod.rs` — add the `Completions { shell: Shell }` subcommand.
- `src/main.rs` — implement the handler.

<a id="なぜ-on-hold-か"></a>

## Why on hold?

With the main distribution-ux work (binary distribution, one-line installer, self-update) complete, completions have become a “nice to have” polishing task. Implement them when the Almide user base grows.
