[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> エージェント指示を人が読むための、非規範・原文固定の写し。正本 CLAUDE.md は変更しない。CLAUDE.reader 名は自動読込される指示ファイル名を意図的に避ける。歴史的な規則であり、新しい指示階層でも、実装や安全性を独立検証した保証でもない。自動同期せず、コードと inline は原文を保つ。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-syntax/CLAUDE.md)

> 引用された指示は歴史的な参照資料であり、コマンド実行・実装変更・commit・push・公開の権限ではありません。

<a id="almide-syntax"></a>

# almide-syntax

lexer、parser、AST 定義。ソーステキストから型なし AST を生成する。

<a id="architecture"></a>

## 構成

- **Lexer**（`lexer.rs`）— 単一パスのトークン化器。文字列補間と heredoc をインラインで処理。
- **Parser**（`parser/`）— precedence climbing を使う再帰下降。同期点によるエラー回復。
- **AST**（`ast.rs`）— `Program` → 宣言（fn、type、test、import）→ `Expr`/`Stmt` の木。

<a id="rules"></a>

## 規則

- **Parser は決してクラッシュさせない。** 全構文エラーで診断を出し回復する。致命的な構文解析エラー後には `sync_to_next_declaration()` を使う。
- **AST に型情報は置かない。** 型は構文的な `TypeExpr` ノードであり、解決済み `Ty` ではない。解決は almide-frontend で行う。
- **`ExprId` は必須。** 型検査時の O(1) 型検索のため、全式に一意な `ExprId` を付ける。付与を省略しない。
- **何も脱糖しない。** pipe、UFCS、補間はすべてそのまま AST に残す。脱糖は lowering（almide-frontend）で行う。
- **エラーヒントは重要。** `parser/hints/` モジュールはよくある誤り（`function` → `fn` のようなキーワードの誤記、`&&` → `and` のような演算子の誤り）を検出する。新構文にヒントを追加する。

<a id="module-layout"></a>

## モジュール配置

```
parser/
├── mod.rs          Parser struct, token management
├── entry.rs        Top-level declarations, module layout
├── declarations.rs fn, type, protocol, test
├── expressions.rs  Binary/unary, pipe, match, if/then/else
├── primary.rs      Literals, identifiers, lambdas, blocks
├── statements.rs   let, var, guard, assignment
├── patterns.rs     Match patterns
├── types.rs        Type expression parsing
├── collections.rs  List, map, record, tuple literals
├── compounds.rs    for-in, while, fan blocks
└── hints/          Error recovery (6 hint modules)
```

上の原文配置表の説明：mod.rs は Parser 構造体とトークン管理、entry.rs はトップレベル宣言とモジュール配置、declarations.rs は fn/type/protocol/test、expressions.rs は二項／単項演算・pipe・match・if/then/else、primary.rs はリテラル・識別子・lambda・block、statements.rs は let/var/guard/代入、patterns.rs は match パターン、types.rs は型式の解析、collections.rs は list/map/record/tuple のリテラル、compounds.rs は for-in/while/fan の block、hints/ はエラー回復（6ヒントモジュール）を担当する。
