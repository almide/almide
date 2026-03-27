<!-- description: Self-contained typed IR so codegen never references AST -->
<!-- done: 2026-03-14 -->
# Codegen IR Redesign

Self-contained typed IR — codegen never references AST at all, performing complete code generation from IR alone. Phases 1-5 all complete.

## Why

Current codegen requires reading both IR and AST, with 20+ HashMap/HashSet side-channels on the Emitter. This causes:

| Problem | Impact |
|---------|--------|
| Phase ordering | Crashes if collect passes don't run before emit |
| AST dependency | Type declarations and function signatures read directly from AST. Codegen impossible from IR alone |
| Side-channel bloat | open record info, borrow info, single-use vars, boxed args etc. are all Emitter fields |
| Fragile module processing | Cross-module type imports reconstructed from AST `Decl` |
| No monomorphization | IR lacks generics information, cannot build instantiation-based codegen |

## Current state (Phase 5 complete — all done)

```
IrProgram {
    functions: Vec<IrFunction>,     // IrParam (borrow, open_record), generics, extern_attrs
    type_decls: Vec<IrTypeDecl>,    // Record, Variant (boxed_args), Alias, visibility
    top_lets: Vec<IrTopLet>,        // TopLetKind::Const | Lazy
    var_table: VarTable,            // name, ty, mutability, use_count
    modules: Vec<IrModule>,         // imported modules (self-contained IR)
}
```

**Phases 1-5 all complete. Both Rust emitter and TS emitter have eliminated `use crate::ast::*`.**
Codegen takes only IR (`&IrProgram`) as input and never references AST.

## Target state

```
IrProgram {
    type_decls: Vec<IrTypeDecl>,      // type declarations (codegen no longer needs AST)
    functions: Vec<IrFunction>,        // enriched: generics, extern, borrow
    top_lets: Vec<IrTopLet>,           // classified as const/lazy
    var_table: VarTable,               // with use_count
    modules: Vec<IrModule>,            // imported modules are also IR
}
```

**Emitter shrinks from ~25 fields to ~8 fields.**

## Design

### IrTypeDecl — Type Declarations in IR

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

### IrFunction — Enriched Function Metadata

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

### IrTopLet — const/lazy Classification

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

### VarInfo — Embedded Analysis Results

```rust
pub struct VarInfo {
    pub name: String,
    pub ty: Ty,
    pub mutability: Mutability,
    pub span: Option<Span>,
    pub use_count: u32,   // 1 = move, 2+ = clone
}
```

### IrModule — Modules in IR

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

**What remains on Emitter:**
- `out`, `indent` — output buffer
- `in_effect`, `in_do_block`, `in_test` — codegen state machine
- `anon_record_structs`, `anon_record_counter` — generated during codegen
- `module_aliases` — runtime resolution
- `skip_auto_q` — codegen control flow
- `user_modules` — module name list

## Implementation phases

### Phase 1: IrTypeDecl ✅

Place type declarations in IR, eliminating direct AST reads from codegen.

- [x] `ir.rs`: add `IrTypeDecl`, `IrTypeDeclKind`, `IrFieldDecl`, `IrVariantDecl`, `IrVariantKind`
- [x] `ir.rs`: add `type_decls: Vec<IrTypeDecl>` to `IrProgram`
- [x] `lower.rs`: `Decl::Type` → `IrTypeDecl` conversion logic
- [x] `lower.rs`: compute boxed_args / boxed_record_fields during lowering
- [x] `emit_rust/program.rs`: rewrite `emit_type_decl` to `IrTypeDecl`-based (`emit_ir_type_decl`)
- [x] `emit_rust/program.rs`: change `collect_named_records` to IR-based
- [x] Tests: `cargo test` + `almide test` all pass

### Phase 2: IrFunction enrichment ✅

- [x] `ir.rs`: add `IrParam` struct (`var`, `ty`, `name`, `borrow`, `open_record`)
- [x] `ir.rs`: change `IrFunction::params` to `Vec<IrParam>`
- [x] `ir.rs`: add `generics`, `extern_attrs`, `visibility` to `IrFunction`
- [x] `ir.rs`: add `ParamBorrow`, `OpenRecordInfo`, `OpenFieldInfo`
- [x] `lower.rs`: propagate generics, extern_attrs during function lowering
- [x] `borrow.rs`: post-pass writing analysis results to `IrParam::borrow`
- [x] `emit_rust/program.rs`: IrFunction-based codegen in `emit_ir_fn_decl`
- [x] Tests: all pass

### Phase 3: Analysis embedding ✅

- [x] `ir.rs`: add `use_count: u32` to `VarInfo`
- [x] `ir.rs`: `compute_use_counts` post-pass (traverses entire IR tree)
- [x] `ir.rs`: add `kind: TopLetKind` to `IrTopLet` (Const vs Lazy)
- [x] `ir.rs`: `classify_top_let_kind`: literal → Const, expression → Lazy
- [x] Tests: all pass

### Phase 4: IrModule ✅

- [x] `ir.rs`: add `IrModule` struct (name, versioned_name, type_decls, functions, top_lets, var_table)
- [x] `ir.rs`: add `modules: Vec<IrModule>` to `IrProgram`
- [x] `lower.rs`: imported module Program → IrModule conversion (`lower_module`)
- [x] `emit_rust/program.rs`: `emit_program` builds cross-module type info from IR modules
- [x] `emit_rust/program.rs`: `find_module_ir_function` searches IR modules first
- [x] `emit_rust/program.rs`: `emit_user_module` uses IR module's VarTable
- [x] `emit_rust/borrow.rs`: `analyze_program` prioritizes IR modules
- [x] `main.rs`, `cli.rs`: `compile_with_options`/`cmd_emit` populates IrProgram.modules
- [x] Tests: all pass

### Phase 5: AST removal from codegen ✅

Eliminated `use crate::ast::*` from all codegen. Emitter performs complete code generation from IR alone.

**Rust emitter (5a):**
- [x] Changed `open_record_aliases` from `Vec<FieldType>` → `Vec<(String, Ty)>`, built from IR (`IrTypeDeclKind::Alias`)
- [x] Deleted all AST fallback branches from `emit_program()` (including `ast_decl_map`, `has_unknown_ret`)
- [x] Deleted AST functions: `collect_fn_info`, `collect_named_records`, `collect_open_record_aliases`, `emit_decl`, `emit_type_decl`, `emit_type_decl_vis`, `emit_fn_decl`, `emit_user_module`, `gen_type`, `gen_type_boxed`, `type_references_name`, `build_open_field_infos`, `ty_to_type_expr`, `ty_contains_unknown`, `count_var_uses`
- [x] Changed `emit_with_options` signature to: `(ir: &IrProgram, options, import_aliases, module_irs)`
- [x] `emit_rust/mod.rs`, `emit_rust/program.rs`: `use crate::ast::*` 削除

**TS emitter (5b):**
- [x] Added `ir_ty_to_ts(&Ty)` (IR version of `gen_type_expr`)
- [x] IR-based declarations: `collect_generic_variant_info_from_ir`, `gen_ir_type_decl`, `gen_ir_fn_decl`, `gen_ir_test`, `emit_ir_user_module`
- [x] Rewrote `emit_program()`, `emit_npm_program()`, `generate_dts()` to IR-based
- [x] Changed entry point signatures to: `emit_with_modules(ir: &IrProgram)`, `emit_npm_package(ir: &IrProgram, config)`
- [x] Deleted AST functions: `collect_generic_variant_info`, `gen_decl`, `gen_type_decl`, `gen_type_expr`, `gen_fn_decl`, `emit_user_module`
- [x] `emit_ts/mod.rs`, `emit_ts/declarations.rs`: `use crate::ast::*` 削除

- [x] Updated caller side in `cli.rs`, `main.rs`
- [x] Tests: `cargo test` (567 tests) + `almide test` (66 files) all pass

## Monomorphization readiness

This IR design naturally supports monomorphization:

- `IrFunction::generics` holds type parameters
- `IrParam::ty` can be substituted with concrete types
- `IrTypeDecl` holds complete variant/record information (needed for instantiation)
- `VarInfo::use_count` can be recomputed per instantiation
- Phase 3 (Named rows) expands `IrParam` row variables to concrete types

## Affected files

| File | Change |
|------|--------|
| `src/ir.rs` | Major extension: IrTypeDecl, IrParam, IrModule, VarInfo expansion |
| `src/lower.rs` | Type declaration lowering, analysis result embedding |
| `src/emit_rust/mod.rs` | Major Emitter field reduction |
| `src/emit_rust/program.rs` | Complete rewrite to IR-based codegen |
| `src/emit_rust/ir_expressions.rs` | AST dependency removal |
| `src/emit_rust/borrow.rs` | Write results back to IR |
| `src/emit_ts/declarations.rs` | IR-based type declaration emit |
| `build.rs` | IR type support for generated code |

## Risk

- **Phase 1 scope**: type declarations are diverse (record, variant, generic variant, recursive variant, alias), making lowering logic large. Implement incrementally: record → variant → alias
- **Borrow inference integration**: currently runs in bulk at codegen start. The post-pass pattern of writing results back to IR requires careful lowering pipeline design
- **TS emitter synchronization**: simultaneous IR migration of Rust emitter and TS emitter causes conflicts. Proceed Rust first, TS second
- **Regression risk**: all `cargo test` + `almide test` must pass at each Phase completion as a mandatory condition

## References

- Cranelift IR: declarations + functions + data all in IR. codegen only references IR
- LLVM IR: Module > Function > BasicBlock. All metadata attached to IR nodes
- GHC Core: type class dictionaries resolved at IR level. codegen needs no type inference
