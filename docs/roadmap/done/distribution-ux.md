<!-- description: GitHub Release binaries, one-line installer, and almide self-update -->
<!-- done: 2026-04-09 -->
# Distribution UX

## Status

完了 (v0.13.0–v0.13.1)。Almide は Rust toolchain なしでインストールでき、自身でアップデートできる。

- ✅ Phase 1: GitHub Release Binaries — 5プラットフォーム自動ビルド
- ✅ Phase 2: One-Line Installer — `curl | sh` / `irm | iex`
- ✅ `almide self-update` — GitHub Release から自己更新（SHA-256 検証付き）

シェル補完 (`almide completions`) は polish 作業として `on-hold/shell-completions.md` に分離。

## Implementation

Phase 1 と Phase 2 は完了済み。

```bash
# macOS / Linux — ワンラインインストール
curl -fsSL https://raw.githubusercontent.com/almide/almide/main/tools/install.sh | sh

# Windows
irm https://raw.githubusercontent.com/almide/almide/main/tools/install.ps1 | iex
```

### What works

- シングルバイナリ（ランタイム依存なし）
- `.github/workflows/release.yml` — タグ push で5プラットフォームのバイナリを自動ビルド・Release 添付
- `tools/install.sh` / `tools/install.ps1` — OS/arch 自動検出、SHA-256 検証、バージョン指定対応
- Playground（ブラウザでの即時体験）

### What's missing

- **自動更新**: `almide self-update` がない
- **シェル補完**: bash/zsh/fish の補完スクリプトがない

## Design

### 原則

1. **Rust toolchain を前提にしない** — Almide ユーザーは Rust ユーザーではない
2. **30秒以内にインストール完了** — 1コマンドで使い始められる
3. **段階的に充実させる** — GitHub Release → installer → package managers → self-update
4. **プラットフォーム平等** — macOS (ARM/Intel), Linux (x86_64), Windows を全て一級対応

### ターゲットマトリクス

| Platform | Target Triple | Binary Name |
|----------|--------------|-------------|
| macOS ARM | aarch64-apple-darwin | almide |
| macOS Intel | x86_64-apple-darwin | almide |
| Linux x86_64 | x86_64-unknown-linux-gnu | almide |
| Linux ARM | aarch64-unknown-linux-gnu | almide |
| Windows x86_64 | x86_64-pc-windows-msvc | almide.exe |

## Implementation Plan

### Phase 1: GitHub Release Binaries

タグ push で全プラットフォームのバイナリを自動ビルドし、GitHub Release に添付する。

**新規 workflow**: `.github/workflows/release.yml`

```yaml
on:
  push:
    tags: ['v*']

jobs:
  build:
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
            archive: almide-linux-x86_64.tar.gz
          - target: aarch64-unknown-linux-gnu
            os: ubuntu-latest
            archive: almide-linux-aarch64.tar.gz
          - target: x86_64-apple-darwin
            os: macos-latest
            archive: almide-macos-x86_64.tar.gz
          - target: aarch64-apple-darwin
            os: macos-latest
            archive: almide-macos-aarch64.tar.gz
          - target: x86_64-pc-windows-msvc
            os: windows-latest
            archive: almide-windows-x86_64.zip
    # cross-compile, tar/zip, upload as release asset
  release:
    needs: build
    # gh release create with all assets
```

成果物の命名規則: `almide-{os}-{arch}.tar.gz` (Windows は `.zip`)

各 archive に含めるもの:
- `almide` バイナリ
- `LICENSE`
- `README.md` (インストール手順のみの短縮版)

**Checksum**: `almide-checksums.sha256` を Release に添付。信頼性の基盤。

**変更が必要なファイル**:
- `.github/workflows/release.yml` — NEW
- `.claude/commands/almide-release.md` — リリース手順を更新

### Phase 2: One-Line Installer

Deno, Bun, Rye と同じパターン。GitHub Release からバイナリをダウンロードしてインストールする。

```bash
curl -fsSL https://almide.github.io/install.sh | sh
```

installer スクリプトの仕事:
1. OS / arch を検出
2. GitHub API で最新リリースの URL を取得
3. バイナリをダウンロード
4. sha256 を検証
5. `~/.local/bin/almide` にインストール
6. PATH に `~/.local/bin` がなければヒントを表示

```bash
# バージョン指定も可能
curl -fsSL https://almide.github.io/install.sh | sh -s -- v0.13.0
```

**Windows 用**: PowerShell スクリプト (`install.ps1`)

```powershell
irm https://almide.github.io/install.ps1 | iex
```

**ホスティング**: `almide.github.io` (既存の GitHub Pages) にスクリプトを配置。
playground リポジトリに `install.sh` / `install.ps1` を追加するか、専用の `almide.github.io` リポジトリを作る。

**変更が必要なファイル**:
- `tools/install.sh` — NEW (ソース管理、GitHub Pages にデプロイ)
- `tools/install.ps1` — NEW
- `README.md` — Quick Start セクションを更新

### Self-Update & Shell Completions

#### `almide self-update`

```bash
almide self-update          # 最新版に更新
almide self-update v0.13.0  # 指定バージョンに更新
```

実装: install.sh と同じロジック（GitHub Release から取得）をコンパイラに内蔵。
バイナリ自身を上書き更新する（self_update crate or 自前実装）。

#### シェル補完

```bash
almide completions bash > ~/.local/share/bash-completion/completions/almide
almide completions zsh  > ~/.local/share/zsh/site-functions/_almide
almide completions fish > ~/.config/fish/completions/almide.fish
```

clap の `generate` 機能で自動生成。installer で自動設定するオプションも提供。

**変更が必要なファイル**:
- `src/cli/mod.rs` — `self-update` / `completions` サブコマンド追加
- `Cargo.toml` — 依存追加（reqwest or ureq, flate2, sha2）

## Remaining Work

- [x] `almide self-update` サブコマンド (v0.13.0)
- [ ] `almide completions` サブコマンド → `on-hold/shell-completions.md` に分離

## References

- [Deno installer](https://github.com/denoland/deno_install) — curl | sh パターンの参考実装
- [Bun installer](https://bun.sh/docs/installation) — install.sh + Homebrew + Scoop の三本立て
- [Rye installer](https://rye.astral.sh/guide/installation/) — self-update 内蔵の参考
- [Zig download](https://ziglang.org/download/) — シングルバイナリ配布の参考
- [cargo-dist](https://opensource.axo.dev/cargo-dist/) — Rust プロジェクトのリリース自動化ツール（Phase 1 で検討）
