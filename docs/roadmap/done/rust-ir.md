<!-- description: Two-stage Rust codegen pipeline via RustIR intermediate repr -->
<!-- done: 2026-03-15 -->
# RustIR: Rust Codegen Intermediate Representation

## 動機

現在の Rust codegen は `IR → 文字列` を1パスで行い、25+ フィールドの Emitter 構造体が状態フラグ（`in_effect`, `in_do_block`, `skip_auto_q` 等）で挙動を切り替える。この設計が以下のバグの根本原因：

| バグ | 原因 |
|------|------|
| do ブロック内 `let` で auto-`?` が効かない | checker と emitter で `?` 挿入判定が分散 |
| do + guard で unreachable loop | guard → loop 変換と Ok ラップの相互作用 |
| effect fn 内 for ループで Result 型不一致 | for 式を Ok() で包む判定が文脈依存 |
| auto-`?` がユーザー fn と stdlib fn で異なる | 2 箇所の独立したロジック |
| clone 挿入の散在 | ir_expressions, ir_blocks, program に分散 |

共通パターン: **「文字列を組み立てる時点で、今の状態に応じて条件分岐する」** → 状態の組み合わせが爆発してテストで踏まないパスにバグが潜む。

## 設計: 2段パイプライン

```
現在:  IrProgram → Emitter(状態フラグ山盛り) → String

提案:  IrProgram → [Pass 1: Lower to RustIR] → RustIR → [Pass 2: Render] → String
                     ↑ ここで全ての判定        ↑ 状態なし、純粋な文字列化
```

### Pass 1: IR → RustIR（判定パス）

全ての codegen 判定をここで行う：
- auto-`?` 挿入: Result を返す呼び出しに `TryOp` を付ける
- clone 挿入: borrow 分析 + use-count で `Clone` ノードを付ける
- Ok ラップ: effect fn の戻り値に `ResultOk` を付ける
- mut 判定: 代入がある変数に `mutable: true` を付ける
- 型注釈: 必要な箇所にのみ型を付ける

全て **RustIR のデータ構造への変換** として表現。文字列操作なし。状態フラグなし。

### Pass 2: RustIR → String（描画パス）

RustIR を Rust ソースコードに変換する純粋な関数。判定ロジックゼロ。インデントと構文規則だけ。

## RustIR の定義

```rust
/// Rust コードの構造を表すデータ型。
/// 文字列ではなく構造として持つことで、変換・検査・テストが容易になる。

// ── 式 ──

enum RustExpr {
    // リテラル
    IntLit(i64),
    FloatLit(f64),
    StringLit(String),
    BoolLit(bool),
    Unit,

    // 変数
    Var(String),

    // 演算
    BinOp { op: RustBinOp, left: Box<RustExpr>, right: Box<RustExpr> },
    UnOp { op: RustUnOp, operand: Box<RustExpr> },

    // 呼び出し
    Call { func: String, args: Vec<RustExpr> },
    MethodCall { receiver: Box<RustExpr>, method: String, args: Vec<RustExpr> },
    MacroCall { name: String, args: Vec<RustExpr> },  // println!, format!, vec!, etc.

    // 制御フロー
    If { cond: Box<RustExpr>, then: Box<RustExpr>, else_: Option<Box<RustExpr>> },
    Match { subject: Box<RustExpr>, arms: Vec<RustMatchArm> },
    Block { stmts: Vec<RustStmt>, expr: Option<Box<RustExpr>> },
    For { var: String, iter: Box<RustExpr>, body: Vec<RustStmt> },
    While { cond: Box<RustExpr>, body: Vec<RustStmt> },
    Loop { body: Vec<RustStmt> },  // guard 用
    Break,
    Continue,
    Return(Option<Box<RustExpr>>),

    // 所有権・エラー
    Clone(Box<RustExpr>),              // expr.clone()
    ToOwned(Box<RustExpr>),            // expr.to_owned() / .to_string() / .to_vec()
    Borrow(Box<RustExpr>),             // &expr
    TryOp(Box<RustExpr>),              // expr?
    ResultOk(Box<RustExpr>),           // Ok(expr)
    ResultErr(Box<RustExpr>),          // Err(expr)
    OptionSome(Box<RustExpr>),         // Some(expr)
    OptionNone,                        // None

    // コレクション
    Vec(Vec<RustExpr>),                // vec![a, b, c]
    HashMap(Vec<(RustExpr, RustExpr)>), // HashMap::from([(k, v), ...])
    Tuple(Vec<RustExpr>),              // (a, b, c)

    // アクセス
    Field(Box<RustExpr>, String),      // expr.field
    Index(Box<RustExpr>, Box<RustExpr>), // expr[idx]
    TupleIndex(Box<RustExpr>, usize),  // expr.0

    // 構造体
    StructInit { name: String, fields: Vec<(String, RustExpr)> },
    StructUpdate { base: Box<RustExpr>, fields: Vec<(String, RustExpr)> }, // { ..base, field: val }

    // ラムダ
    Closure { params: Vec<RustParam>, body: Box<RustExpr> },

    // 文字列
    Format { template: String, args: Vec<RustExpr> },  // format!("...", args)

    // 型キャスト
    Cast { expr: Box<RustExpr>, ty: RustType },  // expr as Type

    // unsafe
    Unsafe(Box<RustExpr>),
}

// ── 文 ──

enum RustStmt {
    Let { name: String, ty: Option<RustType>, mutable: bool, value: RustExpr },
    Assign { target: String, value: RustExpr },
    FieldAssign { target: String, field: String, value: RustExpr },
    IndexAssign { target: String, index: RustExpr, value: RustExpr },
    Expr(RustExpr),  // 式文（副作用のみ）
    Comment(String),
}

// ── 型 ──

enum RustType {
    I64, F64, Bool, String, Unit,
    Vec(Box<RustType>),
    HashMap(Box<RustType>, Box<RustType>),
    Option(Box<RustType>),
    Result(Box<RustType>, Box<RustType>),
    Tuple(Vec<RustType>),
    Named(String),                    // ユーザー定義型
    Generic(String, Vec<RustType>),   // Type<A, B>
    Ref(Box<RustType>),               // &Type
    RefStr,                           // &str
    Slice(Box<RustType>),             // &[T]
    Fn(Vec<RustType>, Box<RustType>), // impl Fn(A) -> B
    Infer,                            // _ (型推論に任せる)
}

// ── トップレベル ──

struct RustFunction {
    name: String,
    params: Vec<RustParam>,
    ret_ty: RustType,
    body: Vec<RustStmt>,
    tail_expr: Option<RustExpr>,
    attrs: Vec<String>,       // #[test], #[inline], etc.
    is_pub: bool,
}

struct RustParam {
    name: String,
    ty: RustType,
    mutable: bool,
}

struct RustStruct {
    name: String,
    fields: Vec<(String, RustType)>,
    derives: Vec<String>,
    is_pub: bool,
}

struct RustEnum {
    name: String,
    variants: Vec<RustVariant>,
    derives: Vec<String>,
    is_pub: bool,
}

struct RustVariant {
    name: String,
    kind: RustVariantKind,
}

enum RustVariantKind {
    Unit,
    Tuple(Vec<RustType>),
    Struct(Vec<(String, RustType)>),
}

struct RustProgram {
    uses: Vec<String>,            // use statements
    consts: Vec<RustConst>,
    statics: Vec<RustStatic>,
    structs: Vec<RustStruct>,
    enums: Vec<RustEnum>,
    functions: Vec<RustFunction>,
    impls: Vec<RustImpl>,
    runtime: String,              // 埋め込みランタイムコード
}
```

## 移行戦略

### Phase 1: RustIR 定義 + Render パス

1. `src/emit_rust/rust_ir.rs` に RustIR のデータ型を定義
2. `src/emit_rust/render.rs` に RustIR → String の純粋な描画関数を実装
3. 既存テストで render の正しさを検証（IR → 旧 codegen と IR → RustIR → render の出力を比較）

### Phase 2: Lower パス（段階的移行）

1つずつ既存の `gen_ir_expr` を RustIR 生成に置き換える：

```
Week 1: リテラル、変数、二項演算、単項演算
Week 2: 関数呼び出し（auto-? をここで統一）
Week 3: if/match/block
Week 4: for/while/do-block/guard（バグの巣窟を一掃）
Week 5: clone/borrow 挿入（散在ロジックを集約）
Week 6: トップレベル（関数宣言、型宣言、main ラッパー）
```

各ステップで `almide test` 全通過を確認。

### Phase 3: 旧 codegen 削除

全ての IR → RustIR 変換が完了したら：
- 旧 `Emitter` の `gen_ir_expr` / `gen_ir_stmt` / `gen_ir_block` 等を削除
- Emitter の 25+ フィールドのうち状態フラグ系を全て除去
- RefCell/Cell を除去

## 利点

| 問題 | 現在 | RustIR 後 |
|------|------|-----------|
| auto-`?` 挿入 | checker + emitter に散在、状態フラグ依存 | Lower パスの 1 箇所で決定 |
| clone 挿入 | ir_expressions, ir_blocks, program に散在 | Lower パスの 1 箇所で決定 |
| Ok ラップ | do-block codegen 内でアドホックに判定 | Lower パスで effect fn の return に ResultOk を付ける |
| guard 変換 | loop + break + return の文字列結合 | RustIR の Loop + Break + Return ノードとして表現 |
| テスト | 生成文字列の比較（脆い） | RustIR の構造比較（堅牢） |
| Emitter 状態 | 25+ フィールド、Cell/RefCell | Lower コンテキスト（少数のフィールド）+ 状態なし Render |
| IrProgram clone | 丸ごとディープコピー | `&IrProgram` 参照で十分 |
| 新ターゲット追加 | Emitter を丸ごと複製 | RustIR の代わりに GoIR/CIR を作るだけ |

## 残すもの（変更不要）

- `src/emit_rust/borrow.rs` — borrow 分析。IR → RustIR 変換で結果を参照するだけ
- `src/emit_rust/*_runtime.txt` — 埋め込みランタイム。RustProgram.runtime にそのまま入る
- `build.rs` + `stdlib/defs/*.toml` — stdlib codegen dispatch。変更不要
- `src/emit_rust/mod.rs` の `EmitOptions` — オプションは Lower コンテキストに渡す

## TS codegen との関係

TS codegen にも同じパターンを適用可能：

```
IR → TsIR → String
```

ただし TS codegen は Rust ほど複雑ではない（clone/borrow/`?` がない）ので、優先度は低い。Rust 側で RustIR が成功したら同じ設計を適用する。

## 関連ロードマップ

- [Architecture Hardening](architecture-hardening.md) — Emitter リファクタ、IrProgram clone 除去（RustIR で解決）
- [Codegen Correctness](codegen-correctness.md) — auto-? バグ群（RustIR で根本解決）
- [Clone Reduction Phase 4](clone-reduction.md) — clone 挿入の集約（RustIR の Lower パスで実現）
- [New Codegen Targets](new-codegen-targets.md) — Go/C/Python ターゲット（同じ 2 段パイプラインで追加）
