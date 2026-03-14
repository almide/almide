# Codegen IR Redesign [ACTIVE]

Self-contained typed IR — codegen が AST を一切参照せず、IR のみで完全なコード生成を行えるようにする。

## Why

現在の codegen は IR と AST の二重読みが必要で、Emitter に 20+ の HashMap/HashSet が side-channel として存在する。これにより:

| 問題 | 影響 |
|------|------|
| Phase ordering | collect パスが emit の前に走らないとクラッシュ |
| AST 依存 | 型宣言・関数シグネチャを AST から直接読む。IR だけでは codegen できない |
| Side-channel 膨張 | open record info, borrow info, single-use vars, boxed args 等が全て Emitter のフィールド |
| Module 処理の脆弱性 | cross-module type import を AST の `Decl` から再構築 |
| Monomorphization 不可 | IR に generics 情報がなく、instantiation-based codegen を組めない |

## Current state

```
IrProgram {
    functions: Vec<IrFunction>,    // 関数本体のみ
    top_lets: Vec<IrTopLet>,       // 値のみ、const/lazy 区別なし
    var_table: VarTable,           // name, ty, mutability のみ
}
// 型宣言なし、モジュール情報なし、分析結果なし
```

Emitter の side-channel フィールド (25+):

- `effect_fns`, `result_fns` — 関数分類
- `named_record_types` — field set → struct name
- `generic_variant_constructors`, `generic_variant_unit_ctors` — variant info
- `boxed_variant_args`, `boxed_variant_record_fields` — recursive variant boxing
- `single_use_vars` — move vs clone 判定
- `borrow_info`, `borrowed_params` — borrow inference 結果
- `top_let_names` — const/lazy 分類
- `open_record_params`, `open_record_aliases` — open record projection
- `anon_record_structs`, `anon_record_counter` — 生成中の状態（これは残る）

## Target state

```
IrProgram {
    type_decls: Vec<IrTypeDecl>,      // 型宣言（codegen が AST 不要に）
    functions: Vec<IrFunction>,        // enriched: generics, extern, borrow
    top_lets: Vec<IrTopLet>,           // const/lazy 分類済み
    var_table: VarTable,               // use_count 付き
    modules: Vec<IrModule>,            // imported modules も IR
}
```

**Emitter は ~25 フィールド → ~8 フィールドに縮小。**

## Design

### IrTypeDecl — 型宣言の IR 化

```rust
pub struct IrTypeDecl {
    pub name: String,
    pub kind: IrTypeDeclKind,
    pub deriving: Vec<String>,
    pub generics: Vec<GenericParam>,
    pub visibility: Visibility,
}

pub enum IrTypeDeclKind {
    Record { fields: Vec<IrFieldDecl> },
    OpenRecord { fields: Vec<IrFieldDecl> },           // shape alias
    Variant {
        variants: Vec<IrVariantDecl>,
        is_generic: bool,
        boxed_args: HashSet<(String, usize)>,          // recursive variant boxing
        boxed_record_fields: HashSet<(String, String)>,
    },
    Alias { target: Ty },
}

pub struct IrFieldDecl {
    pub name: String,
    pub ty: Ty,
    pub default: Option<IrExpr>,
}

pub struct IrVariantDecl {
    pub name: String,
    pub kind: IrVariantKind,
}

pub enum IrVariantKind {
    Unit,
    Tuple(Vec<Ty>),
    Record(Vec<IrFieldDecl>),
}
```

### IrFunction — 関数メタデータの充実

```rust
pub struct IrFunction {
    pub name: String,
    pub params: Vec<IrParam>,
    pub ret_ty: Ty,
    pub body: IrExpr,
    pub generics: Vec<GenericParam>,
    pub is_effect: bool,
    pub is_async: bool,
    pub is_test: bool,
    pub extern_attrs: Vec<ExternAttr>,
}

pub struct IrParam {
    pub var: VarId,
    pub ty: Ty,
    pub borrow: ParamBorrow,
    pub open_record: Option<OpenRecordInfo>,
}

pub enum ParamBorrow {
    Own,
    Ref,       // &T
    RefStr,    // &str
    RefSlice,  // &[T]
}

pub struct OpenRecordInfo {
    pub struct_name: String,
    pub fields: Vec<OpenFieldInfo>,
}
```

### IrTopLet — const/lazy 分類

```rust
pub struct IrTopLet {
    pub var: VarId,
    pub ty: Ty,
    pub value: IrExpr,
    pub kind: TopLetKind,
}

pub enum TopLetKind {
    Const,  // simple literal → Rust const
    Lazy,   // complex → LazyLock
}
```

### VarInfo — 分析結果の埋め込み

```rust
pub struct VarInfo {
    pub name: String,
    pub ty: Ty,
    pub mutability: Mutability,
    pub span: Option<Span>,
    pub use_count: u32,   // 1 = move, 2+ = clone
}
```

### IrModule — モジュールの IR 化

```rust
pub struct IrModule {
    pub name: String,
    pub versioned_name: Option<String>,
    pub type_decls: Vec<IrTypeDecl>,
    pub functions: Vec<IrFunction>,
    pub top_lets: Vec<IrTopLet>,
}
```

## Side-channel elimination map

| Emitter field | → IR location |
|---|---|
| `effect_fns` | `IrFunction::is_effect`（既存、クエリで取得） |
| `result_fns` | `IrFunction::ret_ty` が `Ty::Result` |
| `named_record_types` | `IrTypeDecl::Record` から構築 |
| `generic_variant_constructors` | `IrTypeDeclKind::Variant { is_generic }` |
| `generic_variant_unit_ctors` | `IrVariantKind::Unit` + `is_generic` |
| `boxed_variant_args` | `IrTypeDeclKind::Variant::boxed_args` |
| `boxed_variant_record_fields` | `IrTypeDeclKind::Variant::boxed_record_fields` |
| `single_use_vars` | `VarInfo::use_count == 1` |
| `borrow_info` / `borrowed_params` | `IrParam::borrow` |
| `top_let_names` | `IrTopLet::kind` |
| `open_record_params` | `IrParam::open_record` |
| `open_record_aliases` | `IrTypeDeclKind::OpenRecord` |

**Emitter に残るもの:**
- `out`, `indent` — 出力バッファ
- `in_effect`, `in_do_block`, `in_test` — codegen 状態マシン
- `anon_record_structs`, `anon_record_counter` — codegen 中に生成
- `module_aliases` — runtime 解決
- `skip_auto_q` — codegen 制御フロー
- `user_modules` — モジュール名リスト

## Implementation phases

### Phase 1: IrTypeDecl

最もインパクトが大きい変更。型宣言を IR に載せ、codegen の AST 直接読みを排除。

- [ ] `ir.rs`: `IrTypeDecl`, `IrTypeDeclKind`, `IrFieldDecl`, `IrVariantDecl`, `IrVariantKind` 追加
- [ ] `ir.rs`: `IrProgram` に `type_decls: Vec<IrTypeDecl>` 追加
- [ ] `lower.rs`: `Decl::Type` → `IrTypeDecl` 変換ロジック
- [ ] `lower.rs`: boxed_args / boxed_record_fields 計算を lowering 時に実行
- [ ] `emit_rust/program.rs`: `emit_type_decl` を `IrTypeDecl` ベースに書き換え
- [ ] `emit_rust/program.rs`: `collect_named_records` を IR ベースに変更
- [ ] Emitter から `boxed_variant_args`, `boxed_variant_record_fields`, `generic_variant_constructors`, `generic_variant_unit_ctors`, `open_record_aliases` 削除
- [ ] テスト: `cargo test` + `almide test` 全パス

### Phase 2: IrFunction enrichment

- [ ] `ir.rs`: `IrParam` struct 追加（`var`, `ty`, `borrow`, `open_record`）
- [ ] `ir.rs`: `IrFunction::params` を `Vec<IrParam>` に変更
- [ ] `ir.rs`: `IrFunction` に `generics`, `extern_attrs` 追加
- [ ] `ir.rs`: `ParamBorrow`, `OpenRecordInfo` 追加
- [ ] `lower.rs`: 関数 lowering 時に generics, extern_attrs を伝播
- [ ] `borrow.rs`: 分析結果を `IrParam::borrow` に書き込む post-pass
- [ ] `emit_rust/program.rs`: `emit_fn_decl` を `IrFunction` ベースに書き換え
- [ ] Emitter から `borrow_info`, `borrowed_params`, `open_record_params` 削除
- [ ] テスト: 全パス

### Phase 3: Analysis embedding

- [ ] `ir.rs`: `VarInfo` に `use_count: u32` 追加
- [ ] `lower.rs` or post-pass: use count 計算
- [ ] `ir.rs`: `IrTopLet` に `kind: TopLetKind` 追加
- [ ] `lower.rs`: top-level let の const/lazy 分類
- [ ] Emitter から `single_use_vars`, `top_let_names` 削除
- [ ] テスト: 全パス

### Phase 4: IrModule

- [x] `ir.rs`: `IrModule` struct 追加
- [x] `ir.rs`: `IrProgram` に `modules: Vec<IrModule>` 追加
- [x] `lower.rs`: imported module の Program → IrModule 変換 (`lower_module`)
- [x] `emit_rust/program.rs`: `emit_program` が IR modules から cross-module type info を構築
- [x] `emit_rust/program.rs`: `find_module_ir_function` が IR modules を優先検索
- [x] `emit_rust/program.rs`: `emit_user_module` が IR module の VarTable を使用
- [x] `emit_rust/borrow.rs`: `analyze_program` が IR modules を優先使用
- [x] `main.rs`, `cli.rs`: `compile_with_options`/`cmd_emit` が IrProgram.modules を populated
- [ ] `emit_with_options` のシグネチャ変更: `&IrProgram` のみ受け取る (Phase 5 で完了予定)
- [ ] Emitter から `module_irs`, `user_modules` 削除 (Phase 5 で完了予定)
- [x] テスト: 全パス

### Phase 5: AST removal from codegen

- [ ] `emit_rust/program.rs`: `use crate::ast::*` 削除
- [ ] `emit_rust/program.rs`: `emit_program` が `&IrProgram` のみ受け取る
- [ ] `emit_rust/ir_expressions.rs`: AST 依存箇所の排除
- [ ] `emit_ts/`: 同様の IR 化（TypeScript emitter）
- [ ] 全 codegen ファイルから AST import 除去を確認
- [ ] テスト: 全パス

## Monomorphization readiness

この IR 設計はモノモーフィゼーションを自然にサポートする:

- `IrFunction::generics` が型パラメータを保持
- `IrParam::ty` を具体型で置換可能
- `IrTypeDecl` が variant/record の完全情報を保持（instantiation に必要）
- `VarInfo::use_count` を instantiation ごとに再計算可能
- Phase 3 (Named rows) で `IrParam` の row variable を具体型に展開

## Affected files

| File | Change |
|------|--------|
| `src/ir.rs` | 大幅拡張: IrTypeDecl, IrParam, IrModule, VarInfo 拡張 |
| `src/lower.rs` | 型宣言 lowering, 分析結果埋め込み |
| `src/emit_rust/mod.rs` | Emitter フィールド大幅削減 |
| `src/emit_rust/program.rs` | IR ベース codegen に全面書き換え |
| `src/emit_rust/ir_expressions.rs` | AST 依存排除 |
| `src/emit_rust/borrow.rs` | 結果を IR に書き戻す |
| `src/emit_ts/declarations.rs` | IR ベース型宣言 emit |
| `build.rs` | 生成コードの IR 型対応 |

## Risk

- **Phase 1 の規模**: 型宣言は多様（record, variant, generic variant, recursive variant, alias）で、lowering ロジックが大きくなる。段階的に record → variant → alias の順で実装
- **Borrow inference の組み込み**: 現在は codegen 開始時に一括実行。IR に結果を書き戻す post-pass パターンは lowering パイプラインの設計が重要
- **TS emitter の同期**: Rust emitter の IR 化と TS emitter の IR 化を同時に進めると衝突。Rust first, TS second で進める
- **回帰リスク**: 各 Phase 完了時に `cargo test` + `almide test` 全パスを必須条件とする

## References

- Cranelift IR: 宣言 + 関数 + データが全て IR 内。codegen は IR のみ参照
- LLVM IR: Module > Function > BasicBlock。全メタデータが IR ノードに付与
- GHC Core: 型クラス辞書が IR レベルで解決済み。codegen は型推論不要
