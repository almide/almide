[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> 正本のエージェント指示を読むための、リンク先原文に固定した非規範の写し。CLAUDE.reader は自動読込される指示ファイル名を意図的に避ける。原本は変更しない。新しい指示階層・実行権限・自動同期・独立検証した実装保証ではない。原文の規則と留保を保ち、コードは原文どおり。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-frontend/CLAUDE.md)

<a id="almide-frontend"></a>

# almide-frontend

型検査と AST → IR lowering。意味解析の中核。

<a id="architecture"></a>

## 構成

<a id="type-checker-check"></a>

### 型検査器（`check/`）

3パスの制約ベース推論：

1. **推論**（`infer.rs`）— AST を走査し、新しい `TypeVar` を割り当て、`Constraint` を収集。
2. **解決**（`solving.rs`）— 構造的単一化で制約を単一化し、置換を生成。
3. **確定** — `TypeMap` に置換を適用し、全 `TypeVar` を具体型へ置き換える。

出力：`TypeMap = HashMap<ExprId, Ty>` — 全式の型の正本。

<a id="lowering-lower"></a>

### Lowering（`lower/`）

AST + TypeMap → `IrProgram`：

- 各変数束縛に `VarId` を割り当てる。
- pipe、UFCS、文字列補間、演算子を脱糖する。
- 呼び出し先（Module/Method/Named/Computed）を解決する。
- Eq、Repr、Ord、Hash、Codec を自動導出する。

<a id="rules"></a>

## 規則

- **型検査器が正本。** 型が誤っているなら型検査器で直し、lowering や codegen では直さない。
- **TypeMap は型検査器が設定し、lowering が利用する。** lowering は `type_map[expr_id]` を読んで IR ノードに型を付ける。推論を実行しない。
- **脱糖は型検査器ではなく lowering で行う。** 型検査器は元の AST を見て、lowering が IR 形式へ変換する。
- **`Unknown` によるエラー回復。** 推論失敗時、型検査器は診断を出して `Ty::Unknown` を割り当てる。連鎖エラーを防ぐが IR に漏れうるため、codegen は `Unknown` を適切に扱う必要がある（fallback の既定値）。
- **網羅性は必須。** match パターンを `check/exhaustiveness.rs` で網羅性検査する。不足ケースは E010 を出す。
- **自動導出は実際の IR を生成する。** `lower/derive.rs` は規約メソッドの `IrFunction` 本体を生成する。通常の関数であり、codegen は特別扱いしない。

<a id="module-layout"></a>

## モジュール配置

```
check/
├── mod.rs            Checker orchestration
├── infer.rs          Pass 1: constraint collection
├── calls.rs          Call resolution (UFCS, builtins, constructors)
├── solving.rs        Pass 2: constraint solving
├── types.rs          TyVarId, Constraint, UnionFind
├── builtin_calls.rs  Special type rules for builtins
├── static_dispatch.rs  Impl block dispatch
├── diagnostics.rs    Error formatting
└── exhaustiveness.rs Match exhaustiveness

lower/
├── mod.rs            Lowering entry, LowerCtx
├── expressions.rs    Expr → IrExpr
├── calls.rs          Call target resolution
├── statements.rs     Stmt + pattern → IrStmt
├── types.rs          Type decl lowering
├── derive.rs         Eq/Repr/Ord/Hash derive
└── derive_codec.rs   Codec derive
```

上の原文配置表の説明：check/ の mod.rs は型検査の統括、infer.rs は第1パスの制約収集、calls.rs は呼び出し解決（UFCS、組込み、コンストラクタ）、solving.rs は第2パスの制約解決、types.rs は TyVarId/Constraint/UnionFind、builtin_calls.rs は組込みの特殊な型規則、static_dispatch.rs は impl block の dispatch、diagnostics.rs はエラー整形、exhaustiveness.rs は match 網羅性を担当する。lower/ の mod.rs は入口と LowerCtx、expressions.rs は Expr → IrExpr、calls.rs は呼び出し先解決、statements.rs は Stmt＋pattern → IrStmt、types.rs は型宣言の lowering、derive.rs は Eq/Repr/Ord/Hash 導出、derive_codec.rs は Codec 導出を担当する。
