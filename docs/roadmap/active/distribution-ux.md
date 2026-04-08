<!-- description: Prebuilt binaries, one-line installer, package managers, and self-update -->
# Distribution UX

## Current State

Almide のインストールには Rust toolchain (1.89+) が必要。

```bash
git clone https://github.com/almide/almide.git
cd almide
cargo build --release
cp target/release/almide ~/.local/bin/
```

言語を使いたいだけのユーザーにコンパイラのコンパイルを強いている。
CI は Linux/macOS/Windows のバイナリをビルドしているが、artifact は1日で消え、GitHub Release にもバイナリが添付されていない。

### What works

- シングルバイナリ（ランタイム依存なし）— 配布に最適な形態
- クロスプラットフォーム CI（Linux x86_64, macOS arm64/x86_64, Windows x86_64）
- GitHub Release のタグ運用 (v0.12.x)
- Playground（ブラウザでの即時体験）

### What's missing

- **プリビルトバイナリ**: Release にバイナリが添付されていない
- **ワンラインインストーラ**: `curl | sh` 相当がない
- **パッケージマネージャ**: Homebrew / Scoop / AUR いずれもなし
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

### Phase 3: Package Managers

Phase 1-2 の基盤の上に、各プラットフォームの標準的な配布チャネルを追加。

#### Homebrew (macOS / Linux)

```ruby
# Formula/almide.rb
class Almide < Formula
  desc "The language LLMs can write most accurately"
  homepage "https://github.com/almide/almide"
  version "0.12.2"

  on_macos do
    on_arm do
      url "https://github.com/almide/almide/releases/download/v0.12.2/almide-macos-aarch64.tar.gz"
      sha256 "..."
    end
    on_intel do
      url "https://github.com/almide/almide/releases/download/v0.12.2/almide-macos-x86_64.tar.gz"
      sha256 "..."
    end
  end
  on_linux do
    url "https://github.com/almide/almide/releases/download/v0.12.2/almide-linux-x86_64.tar.gz"
    sha256 "..."
  end

  def install
    bin.install "almide"
  end

  test do
    system "#{bin}/almide", "--version"
  end
end
```

**リポジトリ**: `almide/homebrew-tap` を作成。`brew install almide/tap/almide` で即インストール。
リリース時に formula を自動更新する GitHub Actions を組む。

#### Scoop (Windows)

```json
{
  "version": "0.12.2",
  "architecture": {
    "64bit": {
      "url": "https://github.com/almide/almide/releases/download/v0.12.2/almide-windows-x86_64.zip",
      "hash": "..."
    }
  },
  "bin": "almide.exe"
}
```

**リポジトリ**: `almide/scoop-bucket`。`scoop bucket add almide https://github.com/almide/scoop-bucket && scoop install almide`。

#### 将来の候補 (Phase 3 後)

- **Nix flake**: 再現可能ビルド用。flake.nix をメインリポジトリに追加
- **AUR**: Arch Linux 向け PKGBUILD
- **cargo-binstall**: Rust ユーザー向けプリビルトバイナリダウンロード
- **npm**: `npx almide` — Node.js エコシステムからのアクセス（Biome/esbuild パターン）
- **Docker**: CI 向け `ghcr.io/almide/almide`

### Phase 4: Self-Update & Shell Completions

#### `almide self-update`

```bash
almide self-update          # 最新版に更新
almide self-update v0.13.0  # 指定バージョンに更新
```

実装: Phase 2 の installer と同じロジック（GitHub Release から取得）をコンパイラに内蔵。
バイナリ自身を上書き更新する（self_update crate or 自前実装）。

Homebrew 経由でインストールされた場合は `brew upgrade` を案内する（自己上書きすると Homebrew の管理が壊れるため）。

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

## Priority & Dependencies

```
Phase 1: GitHub Release Binaries  ← 全ての基盤。最優先。
    ↓
Phase 2: One-Line Installer       ← Phase 1 の Release URL に依存
    ↓
Phase 3: Package Managers         ← Phase 1 の Release assets に依存
    ↓
Phase 4: Self-Update              ← Phase 1 の Release assets に依存
```

Phase 2 と Phase 3 は Phase 1 完了後に並行可能。
Phase 4 はコンパイラへのコード追加があるため、他と独立してスケジュール可能だが Release URL が必要。

## Goal UX

```bash
# macOS
brew install almide/tap/almide

# or universal
curl -fsSL https://almide.github.io/install.sh | sh

# Windows
scoop install almide

# Update
almide self-update

# Verify
almide --version
# almide 0.13.0
```

**30秒でインストール、Rust toolchain 不要、全プラットフォーム対応。**

## References

- [Deno installer](https://github.com/denoland/deno_install) — curl | sh パターンの参考実装
- [Bun installer](https://bun.sh/docs/installation) — install.sh + Homebrew + Scoop の三本立て
- [Rye installer](https://rye.astral.sh/guide/installation/) — self-update 内蔵の参考
- [Zig download](https://ziglang.org/download/) — シングルバイナリ配布の参考
- [cargo-dist](https://opensource.axo.dev/cargo-dist/) — Rust プロジェクトのリリース自動化ツール（Phase 1 で検討）
