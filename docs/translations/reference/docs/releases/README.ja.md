[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> 非規範・原文固定の歴史的読者用写し。リリース公開の入力でも、タグ作成・push・workflow 実行・公開証拠の書き換えの許可でもない。正本リリースファイルは変更しない。v0.68.0 正式版ノートは未公開で置き換え済みの下書きであり、リリースの証明ではない。原文の主張や数字をここで独立再測定していない。自動同期しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/releases/README.md)

<a id="release-notes"></a>

# リリースノート

タグごとに `docs/releases/<tag>.md` を一つ置く（`-rc` タグも専用ファイルを持つ）。`.github/workflows/release.yml` はここから GitHub リリースのタイトルと本文を公開し、ファイルがない、または不正なタグのビルドを拒否する。タグを push する**前に**ファイルを書き、`develop` → `main` へ取り込む。タグはそのファイルを含むコミットを指す必要がある。

タグ付け前に検査する:

```bash
bash scripts/check-release-notes.sh vX.Y.Z
```

<a id="format"></a>

## 形式

```markdown
# vX.Y.Z — <lowercase headline: one or two concrete claims joined by ", and">

<One or two sentences: what this release is. For an rc: "**Release candidate N**
(#1484): published as a prerelease for the soak window, excluded from "latest"."
and what it adds after the previous tag.>

## <Theme — e.g. Language surface, Stdlib, Correctness, wasm, Performance, Diagnostics, Tooling>

- **<The observable change, stated plainly>.** What was wrong or new, and what
  a user now sees (#PR, issue #N).

## Gates and evidence

- <Internal gates, ledgers and logs — last and brief.>

## Dependencies

<Bumped crates, one line.>

**Full Changelog**: https://github.com/almide/almide/compare/vPREV...vX.Y.Z
```

テンプレート内の説明の訳：見出しは小文字で具体的な主張を一つか二つ（", and" で接続）。冒頭の一、二文でリリースの内容を示し、rc なら「リリース候補 N（#1484）、安定化の確認期間向けのプレリリースで、latest には含めない」と前タグからの追加を述べる。変更は言語、標準ライブラリ、正しさ、wasm、性能、診断、ツールなどのテーマ別にまとめる。観測可能な変更と利用者に見える違いを平易に説明し、PR・issue を付ける。内部ゲートや台帳、ログは末尾で簡潔に記し、更新した crate は一行にまとめる。

検査が強制する規則:

- 1 行目は `# <tag> — <headline>`（em dash、見出しは 10–120 文字）。これがリリースタイトルになる。
- 2 行目は空行。その後すべてが本文。
- 自動生成の `## What's Changed` リストや `* <PR title> by @user in <url>` 行は使わない。テーマごとに利用者にとって何が変わったかを述べ、関連する PR は一つの箇条書きにまとめる。
- 本文の末尾は、このタグへの `**Full Changelog**` 比較リンク。

例：[v0.68.0-rc1](../../../../releases/v0.68.0-rc1.md)、[v0.68.0-rc2](../../../../releases/v0.68.0-rc2.md)。
