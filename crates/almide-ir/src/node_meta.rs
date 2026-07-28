// ── Statements ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrStmt {
    #[serde(flatten)]
    pub kind: IrStmtKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IrStmtKind {
    Bind { var: VarId, mutability: Mutability, ty: Ty, value: IrExpr },
    BindDestructure { pattern: IrPattern, value: IrExpr },
    Assign { var: VarId, value: IrExpr },
    IndexAssign { target: VarId, index: IrExpr, value: IrExpr },
    /// Map key insertion: `map[key] = value`. Distinct from IndexAssign (list).
    MapInsert { target: VarId, key: IrExpr, value: IrExpr },
    FieldAssign { target: VarId, field: Sym, value: IrExpr },
    Guard { cond: IrExpr, else_: IrExpr },
    Expr { expr: IrExpr },
    Comment { text: String },
    // ── Perceus RC operations (inserted by PerceusPass) ──
    /// Increment reference count of a heap-typed variable (shared reference created).
    RcInc { var: VarId },
    /// Decrement reference count of a heap-typed variable (reference released).
    /// When RC reaches 0, the value is freed (with recursive child drop based on type).
    RcDec { var: VarId },
    // ── Peephole-optimized list operations (inserted by PeepholePass) ──
    /// xs.swap(a, b)
    ListSwap { target: VarId, a: IrExpr, b: IrExpr },
    /// xs[..=end].reverse()
    ListReverse { target: VarId, end: IrExpr },
    /// xs[..=end].rotate_left(1)
    ListRotateLeft { target: VarId, end: IrExpr },
    /// dst[..n].copy_from_slice(&src[..n])
    ListCopySlice { dst: VarId, src: VarId, len: IrExpr },
}

// ── Type declarations ────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IrVisibility {
    Public,
    /// Same project only (pub(crate) in Rust)
    Mod,
    Private,
}

fn default_ir_visibility() -> IrVisibility { IrVisibility::Public }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrFieldDecl {
    pub name: Sym,
    pub ty: Ty,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<IrExpr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<Sym>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<almide_lang::ast::Attribute>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IrVariantKind {
    Unit,
    Tuple { fields: Vec<Ty> },
    Record { fields: Vec<IrFieldDecl> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrVariantDecl {
    pub name: Sym,
    pub kind: IrVariantKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IrTypeDeclKind {
    Record { fields: Vec<IrFieldDecl> },
    Variant {
        cases: Vec<IrVariantDecl>,
        is_generic: bool,
        /// Constructor args that need Box wrapping (recursive variants): (ctor_name, arg_index)
        boxed_args: HashSet<(String, usize)>,
        /// Record variant fields that need Box wrapping: (ctor_name, field_name)
        boxed_record_fields: HashSet<(String, String)>,
    },
    Alias { target: Ty },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrTypeDecl {
    pub name: Sym,
    pub kind: IrTypeDeclKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deriving: Option<Vec<Sym>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generics: Option<Vec<almide_lang::ast::GenericParam>>,
    pub visibility: IrVisibility,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(default)]
    pub blank_lines_before: u32,
}

impl IrTypeDecl {
    /// Base-name-normalized shape fingerprint. Two decls with the same BASE
    /// name and the same fingerprint are STRUCTURAL TWINS: the checker unifies
    /// them freely (same-shape records flow into each other across modules),
    /// so codegen must treat them as one type. Nested references compare by
    /// base name (`List[openai.ToolCall]` == `List[ToolCall]`) so a twin whose
    /// fields reference sibling twins still matches.
    pub fn structural_fingerprint(&self) -> String {
        fn norm_ty(ty: &Ty) -> String {
            match ty {
                Ty::Named(n, args) => {
                    let base = n.as_str().rsplit('.').next().unwrap_or(n.as_str());
                    let args_s: Vec<String> = args.iter().map(norm_ty).collect();
                    format!("N:{}<{}>", base, args_s.join(","))
                }
                Ty::Variant { name, .. } => {
                    let base = name.as_str().rsplit('.').next().unwrap_or(name.as_str());
                    format!("V:{}", base)
                }
                Ty::Applied(c, args) => {
                    let args_s: Vec<String> = args.iter().map(norm_ty).collect();
                    format!("A:{:?}<{}>", c, args_s.join(","))
                }
                Ty::Record { fields } | Ty::OpenRecord { fields } => {
                    let fs: Vec<String> = fields.iter().map(|(n, t)| format!("{}:{}", n, norm_ty(t))).collect();
                    format!("R{{{}}}", fs.join(","))
                }
                Ty::Tuple(ts) => format!("T({})", ts.iter().map(norm_ty).collect::<Vec<_>>().join(",")),
                Ty::Fn { params, ret } => format!(
                    "F({})->{}",
                    params.iter().map(norm_ty).collect::<Vec<_>>().join(","),
                    norm_ty(ret)
                ),
                other => format!("{:?}", other),
            }
        }
        match &self.kind {
            IrTypeDeclKind::Record { fields } => {
                let fs: Vec<String> = fields.iter()
                    .map(|f| format!("{}:{}", f.name, norm_ty(&f.ty)))
                    .collect();
                format!("record{{{}}}", fs.join(","))
            }
            IrTypeDeclKind::Variant { cases, .. } => {
                let cs: Vec<String> = cases.iter().map(|c| {
                    let payload = match &c.kind {
                        IrVariantKind::Unit => String::new(),
                        IrVariantKind::Tuple { fields } =>
                            fields.iter().map(norm_ty).collect::<Vec<_>>().join(","),
                        IrVariantKind::Record { fields } => fields.iter()
                            .map(|f| format!("{}:{}", f.name, norm_ty(&f.ty)))
                            .collect::<Vec<_>>().join(","),
                    };
                    format!("{}({})", c.name, payload)
                }).collect();
                format!("variant[{}]", cs.join("|"))
            }
            IrTypeDeclKind::Alias { target } => format!("alias:{}", norm_ty(target)),
        }
    }
}

// ── Function parameter metadata ─────────────────────────────────

/// Borrow classification for a function parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamBorrow {
    /// Parameter is owned (String, Vec<T>)
    Own,
    /// Parameter can be borrowed as &T
    Ref,
    /// Parameter can be borrowed as &str (for String params)
    RefStr,
    /// Parameter can be borrowed as &[T] (for Vec<T> params)
    RefSlice,
    /// Parameter is mutably borrowed as &mut T (for mutating intrinsics)
    RefMut,
}

/// Info about an open record field (destructured from a record param).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenFieldInfo {
    pub name: Sym,
    pub ty: Ty,
}

/// Info about an open record parameter (destructured struct fields as params).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRecordInfo {
    pub struct_name: Sym,
    pub fields: Vec<OpenFieldInfo>,
}

/// A fully-resolved function parameter in the IR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrParam {
    pub var: VarId,
    pub ty: Ty,
    pub name: Sym,
    pub borrow: ParamBorrow,
    /// The `mut` parameter modifier. A `mut` heap param is passed by mutable
    /// reference (`&mut T`): the caller hands over a `var` binding and the callee
    /// mutates it in place. Borrow inference reads this to honor the keyword
    /// directly, mirroring the `@intrinsic` path. Defaults to `false` for derived
    /// params.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_mut: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_record: Option<OpenRecordInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Box<IrExpr>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<almide_lang::ast::Attribute>,
}

// ── Top-level structures ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrFunction {
    pub name: Sym,
    pub params: Vec<IrParam>,
    pub ret_ty: Ty,
    pub body: IrExpr,
    pub is_effect: bool,
    pub is_async: bool,
    pub is_test: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generics: Option<Vec<almide_lang::ast::GenericParam>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extern_attrs: Vec<almide_lang::ast::ExternAttr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub export_attrs: Vec<almide_lang::ast::ExportAttr>,
    /// Generic `@name(args)` attributes on the source fn. Preserved
    /// verbatim from AST for downstream passes (Stdlib Unification:
    /// `@inline_rust`, `@wasm_intrinsic`, `@pure`, `@schedule`,
    /// `@rewrite`). `@extern` / `@export` still live in their typed
    /// vecs above and are NOT duplicated here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<almide_lang::ast::Attribute>,
    #[serde(default = "default_ir_visibility")]
    pub visibility: IrVisibility,
    /// Doc comment from source (`///` lines).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// Number of blank lines before this declaration in source.
    #[serde(default)]
    pub blank_lines_before: u32,
    /// Definition ID for cross-package resolution (None during migration).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub def_id: Option<DefId>,
    /// Parameter indices that this function mutates in-place.
    /// Populated from `@mutating(param_name)` attributes during lowering.
    /// Consumed by LICM to track loop-modified variables.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mutated_params: Vec<usize>,
    /// Module this function originates from (for emit-time prefixing).
    /// None = root program. Some("mc_bot_v0") = dependency module.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_origin: Option<String>,
}

/// Prefix applied to test function names in lowering to guarantee
/// uniqueness against same-named user fns (`fn foo` + `test "foo"`).
/// All downstream passes see a pre-normalized, unique `func.name`.
pub const TEST_NAME_PREFIX: &str = "__test_almd_";

impl IrFunction {
    /// Source-visible name. For test blocks this strips the
    /// `TEST_NAME_PREFIX` so reporters (test runner output, diagnostics)
    /// show the user's original `test "name"` string.
    pub fn display_name(&self) -> &str {
        let n = self.name.as_str();
        if self.is_test {
            n.strip_prefix(TEST_NAME_PREFIX).unwrap_or(n)
        } else {
            n
        }
    }
}

/// Classification of top-level let bindings for codegen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopLetKind {
    /// Simple literal value (int, float, bool) — emits as `const` in Rust.
    Const,
    /// Non-literal expression — emits as `LazyLock` in Rust.
    Lazy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrTopLet {
    pub var: VarId,
    pub ty: Ty,
    pub value: IrExpr,
    #[serde(default = "default_top_let_kind")]
    pub kind: TopLetKind,
    #[serde(default)]
    pub mutable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(default)]
    pub blank_lines_before: u32,
    /// Definition ID for cross-package resolution (None during migration).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub def_id: Option<DefId>,
}

fn default_top_let_kind() -> TopLetKind { TopLetKind::Lazy }

/// An exported symbol from a module.
#[derive(Debug, Clone)]
pub enum IrExport {
    Function { name: Sym, is_effect: bool },
    Type { name: Sym },
    Constant { name: Sym },
}

/// An imported symbol required by a module.
#[derive(Debug, Clone)]
pub struct IrImport {
    pub name: Sym,
    pub from_module: Sym,
}

/// An imported module lowered to IR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrModule {
    /// Module name (e.g., "mylib" or "mylib.parser")
    pub name: Sym,
    /// Versioned name for diamond dependency aliases (PkgId.mod_name()), if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub versioned_name: Option<Sym>,
    /// Type declarations in this module
    pub type_decls: Vec<IrTypeDecl>,
    /// Functions in this module
    pub functions: Vec<IrFunction>,
    /// Top-level let bindings in this module
    pub top_lets: Vec<IrTopLet>,
    /// Variable table for this module
    pub var_table: VarTable,
    /// Public symbols this module exports
    #[serde(skip)]
    pub exports: Vec<IrExport>,
    /// Symbols this module requires from other modules
    #[serde(skip)]
    pub imports: Vec<IrImport>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IrProgram {
    pub functions: Vec<IrFunction>,
    pub top_lets: Vec<IrTopLet>,
    pub type_decls: Vec<IrTypeDecl>,
    pub var_table: VarTable,
    /// Definition table: maps DefId → DefInfo for cross-package resolution.
    /// Populated during name resolution, consumed by codegen.
    #[serde(default)]
    pub def_table: DefTable,
    /// Imported user modules, lowered to IR
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<IrModule>,
    /// Type constructor registry with kind info and algebraic laws (HKT foundation).
    /// Populated during lowering with user-defined types.
    #[serde(skip)]
    pub type_registry: almide_lang::types::TypeConstructorRegistry,
    /// Names of all effect functions (user-defined + stdlib).
    /// Populated during lowering from TypeEnv. Used by LICM to avoid hoisting effect calls.
    #[serde(skip)]
    pub effect_fn_names: std::collections::HashSet<Sym>,
    /// Effect inference results: per-function capability analysis.
    /// Populated by EffectInferencePass during codegen pipeline.
    #[serde(skip)]
    pub effect_map: crate::effect::EffectMap,
    /// Codegen annotations populated by BoxDerefPass (recursive enums, boxed fields, defaults).
    /// Read by the walker during template rendering.
    #[serde(skip)]
    pub codegen_annotations: crate::annotations::CodegenAnnotations,
    /// Stdlib modules used across all functions and transitive deps.
    /// Populated during lowering by scanning CallTarget::Module references.
    /// Used by codegen to include only needed runtime modules.
    #[serde(skip)]
    pub used_stdlib_modules: std::collections::HashSet<String>,
}
