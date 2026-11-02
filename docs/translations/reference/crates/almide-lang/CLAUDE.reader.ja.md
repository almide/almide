[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> リポジトリのエージェント指示を読むための、非規範・原文固定の写し。正本 CLAUDE.md は変更しない。この CLAUDE.reader.md 系列は、指示が自動読込されるファイル名を意図的に使わない。歴史的原文の指示であり、新たな指示階層でも、現在の実装や安全性を独立検証した保証でもない。自動同期しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-lang/CLAUDE.md)

# almide-lang

ファサード crate。`almide-syntax`（AST、lexer、parser）と `almide-types`（Ty、stdlib_info）を単一の依存として再公開する。

<a id="rules"></a>

## 規則

- **ここにはロジックを置かない。** この crate は再公開のみ行う。共有ロジックが必要なら適切な末端 crate に置く。
- **下流 crate はこれに依存する。** almide-syntax と almide-types の両方へ別々に依存する代わりに使う。
