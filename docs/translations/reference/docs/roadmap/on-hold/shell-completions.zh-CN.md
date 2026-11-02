<a id="shell-completions"></a>

# Shell 补全

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](shell-completions.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](shell-completions.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](shell-completions.zh-CN.md)



> 这是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/shell-completions.md)的非规范读者译文，是不自动同步的历史固定快照。状态、计划和数量均为原文时点的记录，不证明当前实现或完成情况。本次翻译未独立执行或验证这些主张。

> 原文说明：用于 bash/zsh/fish 自动补全的 almide completions 子命令。

通过 `almide completions` 子命令生成 bash/zsh/fish 的补全脚本。这是从 distribution-ux 分离出来的剩余工作。

<a id="想定する使い方"></a>

## 预期用法

```bash
almide completions bash > ~/.local/share/bash-completion/completions/almide
almide completions zsh  > ~/.local/share/zsh/site-functions/_almide
almide completions fish > ~/.config/fish/completions/almide.fish
```

<a id="実装方針"></a>

## 实现方针

- 使用 clap 的 `clap_complete` crate 自动生成。
- 接收 `Shell` 参数，用 `generate()` 将脚本写入 stdout。
- 也为 `tools/install.sh` / `tools/install.ps1` 添加安装时自动放置补全文件的选项（可选）。

<a id="変更が必要なファイル"></a>

## 需要修改的文件

- `Cargo.toml` — 添加 `clap_complete` 依赖。
- `src/cli/mod.rs` — 添加 `Completions { shell: Shell }` 子命令。
- `src/main.rs` — 实现处理器。

<a id="なぜ-on-hold-か"></a>

## 为何搁置？

distribution-ux 主体（二进制分发、一行安装程序、self-update）完成后，补全降为“有则更方便”的完善工作。等 Almide 用户增加时再实现。
