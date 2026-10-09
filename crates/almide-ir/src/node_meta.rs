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

/// The name a type was DECLARED with: the last segment of a module-qualified
/// IR type name (`m.Cfg` -> `Cfg`, `a.b.Cfg` -> `Cfg`); a bare name is
/// returned unchanged. Before the native flatten rename every module type is
/// spelled `<module path>.<Base>` and a dot occurs nowhere else in a type
/// name, so the last segment is exactly the source spelling. This is the ONE
/// spelling a value's repr prints on every leg (#1836, C-009): the entry
/// program's `type Cfg` prints `Cfg { … }`, and so does a module's — the
/// module a value came from is no more part of its repr than the file the
/// entry type was declared in.
pub fn declared_type_name(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

impl IrTypeDecl {
    /// The source spelling of this decl's name (`declared_type_name`).
    pub fn declared_name(&self) -> &str {
        declared_type_name(self.name.as_str())
    }

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
                    let base = declared_type_name(n.as_str());
                    let args_s: Vec<String> = args.iter().map(norm_ty).collect();
                    format!("N:{}<{}>", base, args_s.join(","))
                }
                Ty::Variant { name, .. } => {
                    let base = declared_type_name(name.as_str());
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
                Ty::Fn { params, ret, is_effect } => format!(
                    "F{}({})->{}",
                    if *is_effect { "e" } else { "" },
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

/// #3488: the IR name of the `ordinal`-th test fn of a program (declaration
/// order, `where` cases expanded): `__test_almd_<NNNN>_<label>`.
///
/// The ordinal is what makes the name injective. Every backend has to spell
/// the label into an identifier, and that fold is lossy (`"a b"`, `"a_b"` and
/// `"a-b"` all become `a_b`), so two distinct labels used to meet on one Rust
/// fn (E0428). Two tests never share an ordinal, and the ordinal sits between
/// the fixed prefix and the first `_` the label can contribute, so no label
/// spelling can reach another test's name. The label stays a substring of the
/// emitted name, so `--run <part of the label>` still selects by label on both
/// legs; zero-padding keeps libtest's name-sorted run order the declaration
/// order the wasm runner uses.
pub fn test_fn_name(ordinal: usize, label: &str) -> String {
    format!("{TEST_NAME_PREFIX}{ordinal:04}_{label}")
}

/// The `test "…"` label of a test fn's IR name — the inverse of
/// [`test_fn_name`]. A name without the ordinal comes back without the prefix
/// only; a name outside the test space comes back unchanged.
pub fn test_label(ir_name: &str) -> &str {
    let Some(rest) = ir_name.strip_prefix(TEST_NAME_PREFIX) else { return ir_name };
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    match rest[digits..].strip_prefix('_') {
        Some(label) if digits > 0 => label,
        _ => rest,
    }
}

/// #3483: the prefix a user-declared fn's IR name gets when its source name
/// falls in the compiler's fn-name space (see [`is_reserved_fn_name`]).
///
/// Every fn the compiler synthesizes (`__almd_scoped_N`, `__lambda_*`,
/// `__test_almd_*`, the derive helpers, the stdlib-internal `__encode_*` /
/// `__decode_*` helpers, …) is named in the `__` space, and passes recognise
/// them there. Lowering renames a user fn spelled into that space — and, to
/// keep the mapping injective, one spelled into this escape space — so no
/// user fn shares an IR name with a synthesized one and no `__`-keyed decision
/// can fire on user code. The rename is internal: the source name stays the
/// user-visible one ([`user_fn_source_name`]).
pub const USER_FN_ESCAPE: &str = "almide_fn_";

/// Does a user fn named `name` need [`USER_FN_ESCAPE`]?
pub fn is_reserved_fn_name(name: &str) -> bool {
    name.starts_with("__") || name.starts_with(USER_FN_ESCAPE)
}

/// Did the compiler synthesize the ENTRY program fn whose IR name is
/// `ir_name`? (#3490) Every synthesized fn is named in the `__` space and
/// lowering escapes every entry fn the user spelled there
/// ([`escape_user_fn_name`]), so on an entry IR name the space is the
/// record of who made the fn — unlike the source spelling, which a user can
/// choose. The one user fn left there is a foreign binding (`@extern`,
/// `@inline_rust`, `@wasm_intrinsic`), whose name is the binding and which
/// has no body of the program's to export either. Not meaningful for a
/// linked module's fns: module fns are not escaped.
pub fn is_synthesized_entry_fn(ir_name: &str) -> bool {
    ir_name.starts_with("__")
}

/// The IR name of a user-declared fn spelled `name` (#3483).
pub fn escape_user_fn_name(name: almide_base::intern::Sym) -> almide_base::intern::Sym {
    if is_reserved_fn_name(name.as_str()) {
        almide_base::intern::sym(&format!("{USER_FN_ESCAPE}{}", name.as_str()))
    } else {
        name
    }
}

/// #3483: the separator in the name of a fn the frontend synthesizes as
/// SOURCE — an AST decl the checker types alongside the user's, like a
/// fallible user-HOF twin `__fallible:f`. No identifier contains it, so the
/// decl never meets a user fn in the checker's table; lowering respells it
/// into the `__` space ([`ast_synth_ir_name`]).
pub const AST_SYNTH_SEP: char = ':';

/// The IR name of an AST-synthesized fn (`__fallible:f` → `__fallible__f`),
/// or `None` for a name not spelled with [`AST_SYNTH_SEP`].
pub fn ast_synth_ir_name(name: &str) -> Option<almide_base::intern::Sym> {
    name.contains(AST_SYNTH_SEP).then(|| almide_base::intern::sym(&name.replace(AST_SYNTH_SEP, "__")))
}

/// The source spelling of an IR fn name — the inverse of
/// [`escape_user_fn_name`]. Only an escaped user fn's IR name starts with
/// [`USER_FN_ESCAPE`]; every other name comes back unchanged.
pub fn user_fn_source_name(name: &str) -> &str {
    name.strip_prefix(USER_FN_ESCAPE).unwrap_or(name)
}

/// #1997: the `IrFunction.attrs` marker lowering writes on a `scoped fn`.
/// A `:` is not an identifier character, so no source attribute can forge it.
pub const SCOPED_FN_ATTR: &str = "scoped:fn";
/// #1997: the marker on the fn a `scoped { … }` block was outlined into (its
/// ENTRY). Every call to an entry is a region boundary both legs must honour:
/// the structural wasm leg rewinds its allocator around it, the native leg
/// runs it in the arena window.
pub const SCOPED_BLOCK_ATTR: &str = "scoped:block";
/// #3041: the marker on a fn the compiler SYNTHESIZED out of an `effect fn`'s
/// body (a lifted heap branch `__almd_lift_*`, a metered region
/// `__almd_bounded_*`, an outlined result block `__almd_res_*`). Its own
/// `is_effect` stays false — that flag is the ABI (a Result-wrapped return)
/// — but its host reach is what its origin declared, and the capability
/// witness bounds it by that declaration. Written by the pass that creates
/// the fn; `:` keeps a source attribute from forging it.
pub const EFFECT_ORIGIN_ATTR: &str = "effect:origin";

impl IrFunction {
    /// Declared `scoped fn` (the qualifier, not the block).
    pub fn is_scoped_fn(&self) -> bool {
        self.attrs.iter().any(|a| a.name.as_str() == SCOPED_FN_ATTR)
    }

    /// The outlined body of a `scoped { … }` block.
    pub fn is_scoped_block_entry(&self) -> bool {
        self.attrs.iter().any(|a| a.name.as_str() == SCOPED_BLOCK_ATTR)
    }

    /// Synthesized from an `effect fn`'s body ([`EFFECT_ORIGIN_ATTR`]).
    pub fn is_effect_origin(&self) -> bool {
        self.attrs.iter().any(|a| a.name.as_str() == EFFECT_ORIGIN_ATTR)
    }

    /// Does the fn carry an attribute other than the #3041 effect-origin
    /// marker? The marker is a witness declaration, not an annotation, so the
    /// passes that leave an attributed fn alone (TCO, TRE, the native
    /// ownership certifier's checkable verdicts) read this, not `attrs`.
    pub fn has_attrs_besides_effect_origin(&self) -> bool {
        self.attrs.iter().any(|a| a.name.as_str() != EFFECT_ORIGIN_ATTR)
    }

    /// What the source declares about host effects: an `effect fn`, or a fn
    /// synthesized from one's body.
    pub fn declares_effect(&self) -> bool {
        self.is_effect || self.is_effect_origin()
    }

    /// Mark every fn in `synthesized` as carved out of an effect origin
    /// (idempotent) — called by a pass right after it synthesized them from
    /// a fn that [`Self::declares_effect`].
    pub fn mark_effect_origin(synthesized: &mut [IrFunction]) {
        for f in synthesized.iter_mut().filter(|f| !f.is_effect_origin()) {
            f.attrs.push(Self::scoped_marker(EFFECT_ORIGIN_ATTR));
        }
    }

    /// The marker attribute, for the lowering that writes it.
    pub fn scoped_marker(name: &str) -> almide_lang::ast::Attribute {
        almide_lang::ast::Attribute { name: almide_base::intern::sym(name), args: Vec::new(), span: None }
    }

    /// Source-visible name. For test blocks this strips the
    /// `TEST_NAME_PREFIX` and the ordinal ([`test_label`]) so reporters (test
    /// runner output, diagnostics) show the user's original `test "name"`.
    pub fn display_name(&self) -> &str {
        let n = self.name.as_str();
        if self.is_test {
            test_label(n)
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
    /// Type arguments of every explicit conformance to a GENERIC protocol
    /// (#1589): `(type name, protocol) → args`, under the bare and the
    /// qualified type name alike. Monomorphization reads it to bind a letter
    /// only an applied bound names (`[V, R: Repository[Int, V]]` with `R =
    /// UserRepo` and `type UserRepo: Repository[Int, User]` gives `V =
    /// User`) — the one conformance the type declares, never a search.
    #[serde(skip)]
    pub protocol_conformance_args: std::collections::HashMap<(Sym, Sym), Vec<Ty>>,
}
