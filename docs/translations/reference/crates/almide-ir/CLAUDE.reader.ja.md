[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> エージェント指示を人が読むための、非規範・原文固定の写し。正本 CLAUDE.md は変更しない。CLAUDE.reader 名は自動読込される指示ファイル名を意図的に避ける。歴史的な規則であり、新しい指示階層でも、実装や安全性を独立検証した保証でもない。自動同期せず、コードと inline は原文を保つ。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-ir/CLAUDE.md)

> 引用された指示は歴史的な参照資料であり、コマンド実行・実装変更・commit・push・公開の権限ではありません。

<a id="almide-ir"></a>

# almide-ir

すべての codegen ターゲットが利用する型付き中間表現。

<a id="key-types"></a>

## 主要な型

- **`IrProgram`** — トップレベル：関数、型宣言、top_lets、modules、var_table。
- **`IrExpr`** / **`IrExprKind`** — すべてのノードに `ty: Ty` を持つ式ノード。
- **`IrFunction`** — params、ret_ty、body、flags（is_effect、is_test）を持つ関数定義。
- **`VarId(u32)`** — 一意の変数識別子。**`VarTable`** が VarId → VarInfo（名前、型、可変性）を対応付ける。
- **`BinOp`** / **`UnOp`** — 型別に選択する演算子（AddInt と AddFloat など）。
- **`CallTarget`** — 解決済み呼び出し先（Module、Method、Named、Computed）。

<a id="rules"></a>

## 規則

- **すべての IrExpr が型を持つ。** lowering 時に `expr.ty` を設定する必要がある。codegen は直接読み取り、再推論しない。
- **変数参照は VarId のみ。** IR で文字列による検索をしない。変数名が必要なら `var_table.get(var_id).name` を使う。
- **BinOp は型別に選択する。** Int の `a + b` は `BinOp::AddInt`、Float は `BinOp::AddFloat`、String は `BinOp::ConcatStr` になる。結果型には `op.result_ty()` を使う。
- **走査には visitor パターン。** 網羅的走査には `visit::IrVisitor` と `walk_expr`/`walk_stmt` を使う。必要なノードだけを override する。
- **`use_count` は後から計算する。** 変数の使用回数は lowering 後に設定される。lowering 中に正確だと仮定しない。

<a id="adding-new-ir-nodes"></a>

## 新しい IR ノードの追加

1. `IrExprKind`（または `IrStmtKind`）に variant を追加。
2. `visit.rs`（`walk_expr`/`walk_stmt`）を更新。visitor はすべての子を走査しなければならない。
3. 変換可能な部分式があれば `fold.rs` を更新。
4. 変数を束縛または参照するなら `substitute.rs` を更新。
5. Codegen：Rust walker（`almide-codegen/src/walker/expressions.rs`）と structural Wasm emitter（`almide-wasm/src/`）で扱う。そのノードが native verified-renderer 経路に関わる場合は MIR lowering も更新する。
