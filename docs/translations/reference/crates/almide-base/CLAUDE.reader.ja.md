[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> リポジトリのエージェント指示を読むための、非規範・原文固定の写し。正本 CLAUDE.md は変更しない。この CLAUDE.reader.md 系列は、指示が自動読込されるファイル名を意図的に使わない。歴史的原文の指示であり、新たな指示階層でも、現在の実装や安全性を独立検証した保証でもない。自動同期しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-base/CLAUDE.md)

# almide-base

基盤 crate。他のすべての crate が依存する。

<a id="provides"></a>

## 提供するもの

- **`Sym`** — グローバルでスレッドセーフな文字列 interner（`lasso::ThreadedRodeo`）内を指す `Copy` ハンドル。コンパイラ全体の識別子・型名・フィールド名はすべて `Sym`。
- **`Span`** — ソース位置（line、col、end_col）。
- **`Diagnostic`** — 文脈、ヒント、Levenshtein に基づく「こちらの意味では？」という提案を含むコンパイラのエラー・警告。

<a id="rules"></a>

## 規則

- **識別子を文字列で比較しない。** `Sym` の等値比較（`==`）を使う。等値比較は O(1)（intern 済み ID の比較）。`Sym::Ord`（`<`、`>`）は決定的な順序のため意図的に文字列内容で比較する。walker は Rust 構造体の安定した配置のため、レコードフィールドをアルファベット順に並べる。
- **intern には `sym(s)`、読み取りには `.as_str()`。** `resolve()` 関数は `&'static str` を返す。interner が解放を行わないので安全である。
- **診断は対処可能にする。** 可能ならすべてのエラーにヒントや提案を含める。曖昧一致には `Diagnostic::suggest(name, candidates)` を使う。
- **実行時負荷の大きい依存を追加しない。** すべての crate の依存鎖に含まれるので、この crate は速くコンパイルできる必要がある。
