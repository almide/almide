<!-- description: Two-stage Rust codegen pipeline via RustIR intermediate repr -->
<!-- done: 2026-03-15 -->
# RustIR: Rust Codegen Intermediate Representation

## Motivation

The current Rust codegen performs `IR -> string` in a single pass, with a 25+ field Emitter struct switching behavior via state flags (`in_effect`, `in_do_block`, `skip_auto_q`, etc.). This design is the root cause of the following bugs:

| Bug | Cause |
|-----|-------|
| auto-`?` not working for `let` inside do block | `?` insertion logic split between checker and emitter |
| Unreachable loop with do + guard | Interaction between guard-to-loop conversion and Ok wrapping |
| Result type mismatch in for loop inside effect fn | Context-dependent decision to wrap for expression in Ok() |
| auto-`?` behaves differently for user fn vs stdlib fn | Two independent logic paths |
| Scattered clone insertion | Spread across ir_expressions, ir_blocks, program |

Common pattern: **"branching on current state when assembling strings"** -> state combinations explode and bugs hide in paths not covered by tests.

## Design: Two-Stage Pipeline

```
Current:  IrProgram → Emitter(overloaded state flags) → String

Proposed: IrProgram → [Pass 1: Lower to RustIR] → RustIR → [Pass 2: Render] → String
                       ↑ All decisions here         ↑ Stateless, pure stringification
```

### Pass 1: IR -> RustIR (Decision Pass)

All codegen decisions are made here:
- auto-`?` insertion: attach `TryOp` to calls that return Result
- clone insertion: attach `Clone` nodes based on borrow analysis + use-count
- Ok wrapping: attach `ResultOk` to effect fn return values
- mut determination: set `mutable: true` on variables with assignments
- type annotations: attach types only where needed

All expressed as **transformations to RustIR data structures**. No string manipulation. No state flags.

### Pass 2: RustIR -> String (Rendering Pass)

A pure function that converts RustIR to Rust source code. Zero decision logic. Only indentation and syntax rules.

## RustIR Definition

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

## Migration Strategy

### Phase 1: RustIR Definition + Render Pass

1. Define RustIR data types in `src/emit_rust/rust_ir.rs`
2. Implement pure rendering functions for RustIR -> String in `src/emit_rust/render.rs`
3. Verify render correctness with existing tests (compare output of IR -> old codegen vs IR -> RustIR -> render)

### Phase 2: Lower Pass (Gradual Migration)

Replace existing `gen_ir_expr` one by one with RustIR generation:

```
Week 1: リテラル、変数、二項演算、単項演算
Week 2: 関数呼び出し（auto-? をここで統一）
Week 3: if/match/block
Week 4: for/while/do-block/guard（バグの巣窟を一掃）
Week 5: clone/borrow 挿入（散在ロジックを集約）
Week 6: トップレベル（関数宣言、型宣言、main ラッパー）
```

Verify all `almide test` passes at each step.

### Phase 3: Delete Old Codegen

Once all IR -> RustIR conversions are complete:
- Delete old `Emitter`'s `gen_ir_expr` / `gen_ir_stmt` / `gen_ir_block` etc.
- Remove all state flag fields from Emitter's 25+ fields
- Remove RefCell/Cell

## Benefits

| Problem | Current | After RustIR |
|---------|---------|--------------|
| auto-`?` insertion | Scattered across checker + emitter, state flag dependent | Decided in 1 place in the Lower pass |
| clone insertion | Scattered across ir_expressions, ir_blocks, program | Decided in 1 place in the Lower pass |
| Ok wrapping | Ad-hoc decision in do-block codegen | Attach ResultOk to effect fn return in Lower pass |
| guard conversion | String concatenation of loop + break + return | Expressed as RustIR Loop + Break + Return nodes |
| Testing | Comparing generated strings (brittle) | Structural comparison of RustIR (robust) |
| Emitter state | 25+ fields, Cell/RefCell | Lower context (few fields) + stateless Render |
| IrProgram clone | Full deep copy | `&IrProgram` reference is sufficient |
| Adding new targets | Clone entire Emitter | Just create GoIR/CIR instead of RustIR |

## What Stays (No Changes Needed)

- `src/emit_rust/borrow.rs` — borrow analysis. Just referenced during IR -> RustIR conversion
- `src/emit_rust/*_runtime.txt` — embedded runtime. Goes directly into RustProgram.runtime
- `build.rs` + `stdlib/defs/*.toml` — stdlib codegen dispatch. No changes needed
- `EmitOptions` in `src/emit_rust/mod.rs` — options are passed to the Lower context

## Relationship to TS Codegen

The same pattern can be applied to TS codegen:

```
IR → TsIR → String
```

However, TS codegen is less complex than Rust (no clone/borrow/`?`), so priority is low. Apply the same design after RustIR succeeds on the Rust side.

## Related Roadmap

- [Architecture Hardening](architecture-hardening.md) — Emitter refactor, IrProgram clone removal (solved by RustIR)
- [Codegen Correctness](codegen-correctness.md) — auto-? bug cluster (root-cause fixed by RustIR)
- [Clone Reduction Phase 4](clone-reduction.md) — Consolidation of clone insertion (realized in RustIR Lower pass)
- [New Codegen Targets](new-codegen-targets.md) — Go/C/Python targets (added via the same 2-stage pipeline)
