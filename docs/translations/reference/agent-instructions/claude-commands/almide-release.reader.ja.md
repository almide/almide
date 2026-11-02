[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](almide-release.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](almide-release.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](almide-release.reader.zh-CN.md)

> 過去のリポジトリコマンドを読むための非活性・非規範スナップショット。正本は変更しない。この参照パスと .reader 名はエージェントコマンドを登録しない。引用された指示は資料であり、実行・支出・公開・push・削除の権限ではない。自動同期せず、現在の workflow や測定主張を独立検証したものではない。コードとコメントは原文を保持する。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-release.md)

<a id="release-tag--release-notes"></a>

# リリース：タグとリリースノート

タグを付けて GitHub Release を作成する。タグの push は `.github/workflows/release.yml` を起動し、全プラットフォームのバイナリを自動ビルドしてリリースへ添付する。

<a id="prerequisites"></a>

## 前提条件

- develop から main への PR がマージ済みであること。
- CI が成功していること。
- Cargo.toml のバージョンが更新済みであること。

<a id="steps"></a>

## 手順

1. **状態を確認する**
   ```bash
   git fetch origin main
   git log v{previous_version}..origin/main --oneline
   ```
   前回のタグ以降の全コミットを確認する。

2. **リリースノートを書く** — `docs/releases/v{version}.md` をタグより前にコミットし、`main` へマージする（形式と規則：`docs/releases/README.md`）。1行目の `# v{version} — <headline>` がリリースタイトル、残りがテーマ別の本文になる。自動生成の「What's Changed」PR 一覧にはしない。
   ```bash
   bash scripts/check-release-notes.sh v{version}
   ```

3. **タグを作成して push する**
   ```bash
   git tag v{version} origin/main
   git push origin v{version}
   ```
   これにより release workflow が起動し、以下を行う：
   - Linux（x86_64、aarch64）、macOS（x86_64、aarch64）、Windows（x86_64）のバイナリをビルド。
   - `almide-{os}-{arch}.tar.gz`（Windows は `.zip`）を作成。
   - `almide-checksums.sha256` を生成。
   - 全 asset を含む GitHub Release を作成し、`docs/releases/v{version}.md` からタイトルと説明を設定。
   （ファイルがない、または形式が不正なら workflow はビルド前に失敗する。）

4. **リリースを確認する** — 全5プラットフォームのアーカイブとチェックサムが添付され、各アーカイブに `almide` と `almide-verify` の両方および LICENSE が含まれることを確認する：
   ```bash
   gh release view v{version}
   ```

5. **後からノートを訂正する** — まずファイルを編集し、そこから再公開する：
   ```bash
   bash scripts/check-release-notes.sh v{version} out
   gh release edit v{version} --title "$(cat out/title)" --notes-file out/body.md
   ```

<a id="release-assets"></a>

## リリースの asset

各リリースには自動で次が含まれる：

| ファイル | 内容 |
|------|----------|
| `almide-linux-x86_64.tar.gz` | Linux x86_64 `almide` + `almide-verify` + LICENSE |
| `almide-linux-aarch64.tar.gz` | Linux ARM64 `almide` + `almide-verify` + LICENSE |
| `almide-macos-x86_64.tar.gz` | macOS Intel `almide` + `almide-verify` + LICENSE |
| `almide-macos-aarch64.tar.gz` | macOS Apple Silicon `almide` + `almide-verify` + LICENSE |
| `almide-windows-x86_64.zip` | Windows x86_64 `almide.exe` + `almide-verify.exe` + LICENSE |
| `almide-checksums.sha256` | 全アーカイブの SHA-256 チェックサム |

<a id="notes"></a>

## 注記

- バージョン形式：`v{major}.{minor}.{patch}`。
- リリースノートは英語にする。
- release workflow が全ビルドを終えるまで約10分かかる。
- ノートだけの PR は CI のドキュメント専用経路に含まれない。`scripts/check-release-notes.sh` と release workflow が `docs/releases/` を読むため、コンパイラを再ビルドし、すべての Emit & Format 検査を行う。
- ビルド失敗時は、まず何かが公開されたかを確認する。公開前の失敗と、公開済みリリースの復旧は異なる。ビルド失敗を公開版の削除やタグ付け直しの許可とみなさない。コミットされた release seal では元のタグが取得可能なままである必要がある。`CLAUDE.md` の復旧境界に従い、破壊的操作の前に明示的な所有者承認を得た証拠保全計画を用意する。
