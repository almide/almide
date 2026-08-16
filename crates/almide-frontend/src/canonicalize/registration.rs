//! Declaration registration: collecting function signatures, type declarations,
//! protocol declarations, and protocol validation into the type environment.
//!
//! These are free functions operating on `&mut TypeEnv` + `&mut Vec<Diagnostic>`,
//! extracted from the former `Checker` methods in `check/registration.rs`.

use std::collections::HashMap;
use almide_lang::ast;
use almide_base::diagnostic::Diagnostic;
use almide_base::intern::{Sym, sym};
use almide_lang::types::TypeConstructorId;
use crate::types::{Ty, TypeEnv, FnSig, ProtocolDef, ProtocolMethodSig, VariantPayload};
use super::resolve::resolve_type_expr;
fn err(msg: impl Into<String>, hint: impl Into<String>, ctx: impl Into<String>) -> Diagnostic {
    Diagnostic::error(msg, hint, ctx)
}
/// Resolve an AST type expression using the current type environment.
fn resolve(env: &TypeEnv, te: &ast::TypeExpr) -> Ty {
    resolve_type_expr(te, Some(&env.types))
}
/// Like `resolve`, but pins a user module's own-type references to the qualified canonical name `mod.Type` (#433). `cur_mod` is the module being registered.
fn resolve_in(env: &TypeEnv, te: &ast::TypeExpr, cur_mod: Option<&str>) -> Ty {
    crate::canonicalize::resolve::resolve_type_expr_in(te, Some(&env.types), cur_mod)
}
/// The module a TYPE reference should be resolved against.
///
/// `infer_module` re-registers a module's declarations with `prefix: None` on
/// purpose — the KEYS must be bare so the module's own bodies resolve their own
/// names. But the TYPES still have to pin to `mod.Type`: with `cur_mod` unset,
/// a bare `Span` declared in two modules has no unique owner, so it stayed
/// bare and the #433 name-pin gate refused the build (#1087). That pass marks
/// itself with `alias_owner_module`, which names the module the bare keys
/// really belong to.
fn type_cur_mod<'a>(env: &TypeEnv, prefix: Option<&'a str>) -> Option<&'a str>
where
    'static: 'a,
{
    prefix.or_else(|| env.alias_owner_module.map(|m| m.as_str()))
}
/// Infer type from a literal expression (for top-level `let` without annotation). Used at registration time — before the full checker runs — so module top_lets have a concrete `env.top_lets` entry the moment the main program's inference looks them up. A shallow scalar-only version regresses records / lists / maps to `Ty::Unknown`, which later surfaces as `LazyLock<_>` in generated Rust and `ConcretizeTypes` post-condition failures on WASM. Recurse structurally through record / list / tuple / map literals so the cross-module user sees the right type. Seed type for an UNANNOTATED top-let. `infer_literal_type` covers literals and anonymous records only; a NAMED constructor (`Cfg { … }`) fell to `Ty::Unknown`, and because every driver checks MAIN before the modules, main's inference read that stale Unknown for a cross-module `m.CFG` — a spread of it then carried Unknown into the AllTypesConcrete refusal (#502). Resolve the ctor name through the SAME #433 predicate an explicit `: Cfg` annotation uses, so both spellings seed identically. Generic decls (unresolved type params) deliberately stay Unknown — the ctor args are not inferable here and the later module-check writeback only corrects exact-Unknown seeds.
pub fn infer_top_let_seed(env: &TypeEnv, prefix: Option<&str>, value: &ast::Expr) -> Ty {
    match &value.kind {
        ast::ExprKind::Paren { expr } => infer_top_let_seed(env, prefix, expr),
        ast::ExprKind::Record { name: Some(n), .. } => {
            let canonical = super::resolve::canonical_user_type_sym(n.as_str(), &env.types, prefix)
                .unwrap_or_else(|| sym(n.as_str()));
            match env.types.get(&canonical) {
                Some(decl) if !decl.has_unresolved_deep() => Ty::Named(canonical, vec![]),
                _ => Ty::Unknown,
            }
        }
        _ => infer_literal_type(value),
    }
}
pub fn infer_literal_type(expr: &ast::Expr) -> Ty {
    infer_literal_scalar(expr)
        .or_else(|| infer_literal_composite(expr))
        .unwrap_or(Ty::Unknown)
}

/// Scalars and the simple leaf literals.
///
/// One group of `infer_literal_type`'s arm table, arms verbatim and in source order.
/// `None` means "not my group"; the router tries the groups in that order,
/// so which type a literal seeds is unchanged.
pub fn infer_literal_scalar(expr: &ast::Expr) -> Option<Ty> {
    Some(match &expr.kind {
        ast::ExprKind::Int { .. } => Ty::Int,
        ast::ExprKind::Float { .. } => Ty::Float,
        ast::ExprKind::String { .. } => Ty::String,
        ast::ExprKind::Bool { .. } => Ty::Bool,
        ast::ExprKind::Unit => Ty::Unit,
        ast::ExprKind::Paren { expr } => infer_literal_type(expr),
        // A signed literal is a literal: `let MARGIN_AUTO = -2.0` must seed Float, not Unknown (#784 — the Unknown seed leaked into every cross-module reader of the constant). `-` keeps the operand's numeric type; `not` is Bool.
        ast::ExprKind::Unary { op, operand } => match op.as_str() {
            "-" => infer_literal_type(operand),
            "not" | "!" => Ty::Bool,
            _ => Ty::Unknown,
        },
        ast::ExprKind::Record { name: None, fields } => {
            let mut fs: Vec<(Sym, Ty)> = fields.iter()
                .map(|fi| (fi.name, infer_literal_type(&fi.value)))
                .collect();
            fs.sort_by_key(|(n, _)| *n);
            Ty::Record { fields: fs }
        }
        _ => return None,
    })
}

/// Collections, records and the constructor forms.
///
/// One group of `infer_literal_type`'s arm table, arms verbatim and in source order.
/// `None` means "not my group"; the router tries the groups in that order,
/// so which type a literal seeds is unchanged.
pub fn infer_literal_composite(expr: &ast::Expr) -> Option<Ty> {
    Some(match &expr.kind {
        ast::ExprKind::List { elements } => {
            let elem = elements.first()
                .map(|e| infer_literal_type(e))
                .unwrap_or(Ty::Unknown);
            Ty::Applied(TypeConstructorId::List, vec![elem])
        }
        ast::ExprKind::Tuple { elements } => {
            Ty::Tuple(elements.iter().map(infer_literal_type).collect())
        }
        ast::ExprKind::MapLiteral { entries } => {
            let (k, v) = entries.first()
                .map(|(k, v)| (infer_literal_type(k), infer_literal_type(v)))
                .unwrap_or((Ty::Unknown, Ty::Unknown));
            Ty::Applied(TypeConstructorId::Map, vec![k, v])
        }
        ast::ExprKind::Some { expr } => {
            Ty::Applied(TypeConstructorId::Option, vec![infer_literal_type(expr)])
        }
        ast::ExprKind::None => {
            Ty::Applied(TypeConstructorId::Option, vec![Ty::Unknown])
        }
        ast::ExprKind::Ok { expr } => {
            Ty::Applied(TypeConstructorId::Result, vec![infer_literal_type(expr), Ty::Unknown])
        }
        ast::ExprKind::Err { expr } => {
            Ty::Applied(TypeConstructorId::Result, vec![Ty::Unknown, infer_literal_type(expr)])
        }
        _ => return None,
    })
}
/// Build a prefixed key: "module.name" or just "name".
pub fn prefixed_key(prefix: Option<&str>, name: &str) -> String {
    prefix.map(|p| format!("{}.{}", p, name)).unwrap_or_else(|| name.to_string())
}

/// The `env.functions` key for a convention method (`encode`, `repr`, …) on
/// `type_name`, or `None` when the type has no such method.
///
/// The two producers disagree on shape and both are load-bearing: an EXPLICIT
/// `fn Color.repr` inside module `lib` is registered prefixed
/// (`lib.Color.repr`), while a DERIVED method is registered bare
/// (`P.encode`) because that bare name is what lowering emits and what the
/// backend resolves via `module_origin`. A checked expression carries the
/// canonical `lib.P` either way, so the qualified spelling is tried first and
/// the bare one is the fallback. Consumers must use the key this RETURNS —
/// guessing one shape is exactly how `p.encode()`, `json.encode(p)` and
/// `[T: Codec]` came to fail across an import (#1087, #1089).
pub fn convention_fn_key(env: &TypeEnv, type_name: &str, method: &str) -> Option<Sym> {
    let qualified = sym(&format!("{}.{}", type_name, method));
    if env.functions.contains_key(&qualified) {
        return Some(qualified);
    }
    let k = sym(&format!("{}.{}", bare_type_name(type_name), method));
    env.functions.contains_key(&k).then_some(k)
}

fn bare_type_name(type_name: &str) -> &str {
    type_name.rsplit_once('.').map_or(type_name, |(_, t)| t)
}

/// The name a convention-method CALL must carry, when the method exists.
///
/// Deliberately different from [`convention_fn_key`]: the checker needs the
/// key that holds the signature (prefixed for an explicit method), while
/// lowering needs the name the DEFINITION lowers to — always the bare
/// `Type.method`, with the module re-attached later from `module_origin`.
/// Emitting the prefixed spelling produced calls to `lib_Color_repr` against a
/// definition called `almide_rt_lib_Color_repr` (#1087).
pub fn convention_emit_key(env: &TypeEnv, type_name: &str, method: &str) -> Option<Sym> {
    convention_fn_key(env, type_name, method)
        .map(|_| sym(&format!("{}.{}", bare_type_name(type_name), method)))
}
/// Substitute `Self` → concrete type in a protocol method type.
fn substitute_self(ty: &Ty, replacement: &Ty) -> Ty {
    match ty {
        Ty::TypeVar(name) if name == "Self" => replacement.clone(),
        _ => ty.map_children(&|child| substitute_self(child, replacement)),
    }
}
/// Strip module qualification from every `Named` type anywhere in the tree (`m.Pigment` → `Pigment`), so a protocol-satisfaction signature check doesn't false-positive on a cross-module type that's spelled bare on one side and qualified on the other. Mirrors `leaf_satisfies`'s bare-name leniency, generalized to nested/compound types via `map_children`.
fn strip_module_qualifier(ty: &Ty) -> Ty {
    match ty {
        Ty::Named(name, args) => {
            let bare = name.as_str().rsplit('.').next().unwrap_or(name.as_str());
            Ty::Named(sym(bare), args.iter().map(strip_module_qualifier).collect())
        }
        _ => ty.map_children(&strip_module_qualifier),
    }
}
/// Collect structural bounds from generic params: Record → OpenRecord conversion.
pub fn collect_structural_bounds(env: &TypeEnv, generics: &Option<Vec<ast::GenericParam>>) -> HashMap<Sym, Ty> {
    let mut sb = HashMap::new();
    let gs = match generics { Some(gs) => gs, None => return sb };
    for g in gs {
        let bte = match &g.structural_bound { Some(bte) => bte, None => continue };
        let bt = resolve(env, bte);
        sb.insert(sym(&g.name), match bt { Ty::Record { fields } => Ty::OpenRecord { fields }, o => o });
    }
    sb
}
/// Scalar type names that indicate a compile-time value parameter (not a protocol bound).
pub const SCALAR_TYPE_NAMES: &[&str] = &[
    "Int", "Float", "Bool", "String",
    "Int8", "Int16", "Int32", "Int64",
    "UInt8", "UInt16", "UInt32", "UInt64",
    "Float32", "Float64",
];
/// The `Ty` a scalar type NAME denotes, for the const-generic bound syntax
/// `[N: Int]`.
///
/// `Some` for exactly the names in [`SCALAR_TYPE_NAMES`] —
/// `scalar_type_names_all_resolve` asserts the two agree. Before they shared
/// this function, a name could sit in the list with no arm here and the const
/// param would be silently skipped, so `[N: Int16]` would type-check as an
/// ordinary generic and then fail in codegen.
pub fn scalar_ty_by_name(name: &str) -> Option<Ty> {
    Some(match name {
        "Int" | "Int64" => Ty::Int,
        "Float" | "Float64" => Ty::Float,
        "Bool" => Ty::Bool,
        "String" => Ty::String,
        "Int8" => Ty::Int8,
        "Int16" => Ty::Int16,
        "Int32" => Ty::Int32,
        "UInt8" => Ty::UInt8,
        "UInt16" => Ty::UInt16,
        "UInt32" => Ty::UInt32,
        "UInt64" => Ty::UInt64,
        "Float32" => Ty::Float32,
        _ => return None,
    })
}

/// Identify const (value) parameters in generic params. A param `N: Int` where `Int` is a scalar type (not a protocol) becomes a const param. Returns: param name → scalar Ty.
pub fn collect_const_params(generics: &Option<Vec<ast::GenericParam>>) -> HashMap<Sym, Ty> {
    let mut cp = HashMap::new();
    let gs = match generics { Some(gs) => gs, None => return cp };
    for g in gs {
        if let Some(bounds) = &g.bounds {
            // A single scalar-type-name bound makes this a const param.
            if let [only] = bounds.as_slice() {
                if let Some(ty) = scalar_ty_by_name(only) {
                    cp.insert(sym(&g.name), ty);
                }
            }
        }
    }
    cp
}
/// Collect protocol bounds from generic params: TypeVar name → list of protocol names. Excludes const params (scalar type bounds like `N: Int`).
pub fn collect_protocol_bounds(generics: &Option<Vec<ast::GenericParam>>) -> HashMap<Sym, Vec<Sym>> {
    let mut pb = HashMap::new();
    let gs = match generics { Some(gs) => gs, None => return pb };
    for g in gs {
        if let Some(bounds) = &g.bounds {
            if !bounds.is_empty() {
                // Skip if this is a const param (single scalar type bound)
                if bounds.len() == 1 && SCALAR_TYPE_NAMES.contains(&bounds[0].as_str()) {
                    continue;
                }
                pb.insert(sym(&g.name), bounds.iter().map(|b| sym(b)).collect());
            }
        }
    }
    pb
}
/// A borrowed view of the `fn` signature being registered.
///
/// `effect` is an `&Option<bool>` and was adjacent
/// positional parameters, so transposing them type-checked. Named fields make
/// that a compile error instead of a signature registered with the wrong
/// effect-ness.
pub struct FnSigToRegister<'a> {
    pub name: &'a str,
    pub params: &'a [ast::Param],
    pub return_type: &'a ast::TypeExpr,
    pub effect: &'a Option<bool>,
    pub generics: &'a Option<Vec<ast::GenericParam>>,
    pub prefix: Option<&'a str>,
    pub span: Option<&'a ast::Span>,
    pub visibility: ast::Visibility,
    /// The declaration's attributes, so registration can read `@deprecated`.
    pub attrs: &'a [ast::Attribute],
}

pub fn register_fn_sig(env: &mut TypeEnv, decl: &FnSigToRegister<'_>) {
    let FnSigToRegister {
        name, params, return_type, effect, generics, prefix, span, visibility, attrs,
    } = *decl;
    let gnames: Vec<Sym> = generics.as_ref().map(|gs| gs.iter().map(|g| sym(&g.name)).collect()).unwrap_or_default();
    let sb = collect_structural_bounds(env, generics);
    let pb = collect_protocol_bounds(generics);
    let const_params = collect_const_params(generics);
    for gn in &gnames {
        if let Some(scalar_ty) = const_params.get(gn) {
            env.types.insert(*gn, Ty::ConstParam { name: *gn, ty: Box::new(scalar_ty.clone()) });
        } else {
            env.types.insert(*gn, Ty::TypeVar(*gn));
        }
    }
    // A bare `self` first parameter is sugar for `self: Self` (the parser always types it `TypeExpr::Simple { name: "Self" }`). Inside a `protocol { ... }` declaration `Self` is a legitimate unresolved placeholder, but on a real convention method (`fn Type.method(self, ...)`) it must resolve to the enclosing type, the same way `Self` in a protocol's own signature gets substituted when checked against one.
    let receiver_ty = name.split_once('.').map(|(ty_name, _)| Ty::Named(sym(ty_name), Vec::new()));
    let tcm = type_cur_mod(env, prefix);
    let ptys: Vec<(Sym, Ty)> = params.iter().enumerate().map(|(i, p)| {
        if i == 0 && p.name.as_str() == "self" {
            if let (ast::TypeExpr::Simple { name: tn }, Some(rt)) = (&p.ty, &receiver_ty) {
                if tn.as_str() == "Self" {
                    return (sym(&p.name), rt.clone());
                }
            }
        }
        (sym(&p.name), resolve_in(env, &p.ty, tcm))
    }).collect();
    let mut_params: Vec<usize> = params.iter().enumerate()
        .filter(|(_, p)| p.is_mut)
        .map(|(i, _)| i)
        .collect();
    let ret = resolve_in(env, return_type, tcm);
    for gn in &gnames { env.types.remove(gn); }
    let is_effect = effect.unwrap_or(false);
    let key = prefixed_key(prefix, name);
    if prefix.is_none() && is_effect { env.effect_fns.insert(sym(name)); }
    let min_p = params.iter().take_while(|p| p.default.is_none()).count();
    env.functions.insert(sym(&key), FnSig { params: ptys, ret, is_effect, generics: gnames, structural_bounds: sb, protocol_bounds: pb, mut_params });
    if let Ok(Some(dep)) = crate::deprecation::parse(attrs) {
        env.deprecations.insert(sym(&key), dep);
    }
    // Record visibility so `resolve_module_call` can reject cross-module access to `mod fn` / `local fn`. Only non-Public entries need to be stored — the lookup in the checker treats "missing" as Public (stdlib, impl methods, derived stubs).
    if !matches!(visibility, ast::Visibility::Public) {
        env.fn_visibility.insert(sym(&key), visibility);
    }
    if let Some(s) = span {
        env.fn_decl_spans.insert(sym(&key), (s.line, s.col));
    }
    if min_p < params.len() {
        env.fn_min_params.insert(sym(&key), min_p);
        // Keyed by the prefixed name so a caller in another module can fill
        // these in — lowering's per-file map never sees an imported program.
        env.fn_defaults.insert(
            sym(&key),
            params.iter().map(|p| p.default.as_ref().map(|d| (**d).clone())).collect(),
        );
    }
}
pub fn validate_protocols(env: &TypeEnv, diagnostics: &mut Vec<Diagnostic>, derives: &[Sym], type_name: &str) {
    for d in derives {
        if !env.protocols.contains_key(&sym(d)) {
            let valid: Vec<&str> = env.protocols.keys().map(|s| s.as_str()).collect();
            diagnostics.push(err(
                format!("unknown protocol '{}' on type '{}'", d, type_name),
                format!("Known protocols: {}", {
                    let mut sorted = valid; sorted.sort(); sorted.join(", ")
                }),
                format!("type {}", type_name),
            ));
        }
    }
}
pub fn register_derive_sigs(env: &mut TypeEnv, derives: &[Sym], type_name: &str, prefix: Option<&str>) {
    // #433: the VALUE type in derived signatures must carry the canonical qualified name for a user module's type — `Pigment.decode`'s `Result[Pigment, String]` with a bare name leaked into callers' var tables (found by the NameResolutionTotal gate). The fn KEYS stay as they were (separate resolution system).
    let canonical = match prefix {
        Some(m) if !almide_lang::stdlib_info::is_bundled_module(m) => format!("{}.{}", m, type_name),
        _ => type_name.to_string(),
    };
    let type_ty = Ty::Named(sym(&canonical), vec![]);
    let value_ty = Ty::Named(sym("Value"), vec![]);
    let empty_sb: HashMap<Sym, Ty> = HashMap::new();
    let empty_pb: HashMap<Sym, Vec<Sym>> = HashMap::new();
    // The VALUE type is canonical (#433); the KEYS stay BARE. Consumers that
    // hold a qualified `Ty::Named("lib.P")` reach these through
    // `convention_fn_key`, which falls back to the bare spelling — registering
    // the qualified key here instead would change the name LOWERING emits, and
    // the backend resolves a derived method by its bare name plus
    // `module_origin` (#1087).
    let register = |env: &mut TypeEnv, method: &str, sig: FnSig| {
        let key = format!("{}.{}", type_name, method);
        if !env.functions.contains_key(&sym(&key)) {
            env.functions.insert(sym(&key), sig);
        }
    };
    for d in derives {
        match d.as_str() {
            "Eq" => register(env, "eq", FnSig { params: vec![("a".into(), type_ty.clone()), ("b".into(), type_ty.clone())], ret: Ty::Bool, is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone(), mut_params: vec![] }),
            "Repr" => register(env, "repr", FnSig { params: vec![("v".into(), type_ty.clone())], ret: Ty::String, is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone(), mut_params: vec![] }),
            "Codec" => {
                register(env, "encode", FnSig { params: vec![("v".into(), type_ty.clone())], ret: value_ty.clone(), is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone(), mut_params: vec![] });
                register(env, "decode", FnSig { params: vec![("v".into(), value_ty.clone())], ret: Ty::result(type_ty.clone(), Ty::String), is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone(), mut_params: vec![] });
            }
            _ => {}
        }
    }
}
/// Register a user-defined protocol declaration into env.protocols.
pub fn register_protocol_decl(env: &mut TypeEnv, name: &str, generics: &Option<Vec<ast::GenericParam>>, methods: &[ast::ProtocolMethod]) {
    let gnames: Vec<Sym> = generics.as_ref()
        .map(|gs| gs.iter().map(|g| sym(&g.name)).collect())
        .unwrap_or_default();

    // Temporarily register `Self` as a TypeVar so resolve_type_expr handles it
    env.types.insert(sym("Self"), Ty::TypeVar(sym("Self")));
    for gn in &gnames {
        env.types.insert(*gn, Ty::TypeVar(*gn));
    }

    let method_sigs: Vec<ProtocolMethodSig> = methods.iter().map(|m| {
        let params: Vec<(Sym, Ty)> = m.params.iter()
            .map(|p| (sym(&p.name), resolve(env, &p.ty)))
            .collect();
        let ret = resolve(env, &m.return_type);
        ProtocolMethodSig {
            name: sym(&m.name),
            params,
            ret,
            is_effect: m.effect,
        }
    }).collect();

    env.types.remove(&sym("Self"));
    for gn in &gnames {
        env.types.remove(gn);
    }

    env.protocols.insert(sym(name), ProtocolDef {
        name: sym(name),
        generics: gnames,
        methods: method_sigs,
    });
}
/// Protocols whose auto-derive RECURSES INTO EACH FIELD'S TYPE: deriving them on a struct/variant emits per-field work that requires the field type to ALSO satisfy the protocol. `Codec` calls `Field.encode` / `Field.decode`; `Ord`/`Hash` lower to a Rust `#[derive(Ord/Hash)]` that needs the field's Rust type to impl it. `Eq`/`Repr` are excluded — every generated struct gets `PartialEq` + a repr path unconditionally, so a field need not declare them (gating those would be a false positive).
/// A borrowed view of the `type` declaration being registered.
pub struct TypeDeclToRegister<'a> {
    pub name: &'a str,
    pub ty: &'a ast::TypeExpr,
    pub deriving: &'a Option<Vec<Sym>>,
    pub generics: &'a Option<Vec<ast::GenericParam>>,
    pub prefix: Option<&'a str>,
    pub visibility: ast::Visibility,
}

pub fn register_type_decl(env: &mut TypeEnv, diagnostics: &mut Vec<Diagnostic>, decl: &TypeDeclToRegister<'_>) {
    let TypeDeclToRegister { name, ty, deriving, generics, prefix, visibility } = *decl;
    if let Some(derives) = deriving {
        validate_protocols(env, diagnostics, derives, name);
    }
    let gnames: Vec<Sym> = generics.as_ref().map(|gs| gs.iter().map(|g| sym(&g.name)).collect()).unwrap_or_default();
    for gn in &gnames { env.types.insert(*gn, Ty::TypeVar(*gn)); }
    let mut resolved = resolve(env, ty);
    for gn in &gnames { env.types.remove(gn); }

    resolved = register_type_decl_opaque_alias(env, name, resolved, &gnames, prefix, visibility);
    register_type_decl_variant_ctors(env, diagnostics, name, prefix, &mut resolved);
    register_type_decl_check_duplicate(env, diagnostics, name, prefix, &resolved);
    register_type_decl_finalize(env, name, ty, prefix, resolved);

    if let Some(derives) = deriving {
        register_derive_sigs(env, derives, name, prefix);
    }
}
/// `mod`/local type alias → nominal newtype (opaque constructor), when the declared visibility isn't Public and the resolved shape isn't already a Record/Variant. Registers the opaque-alias bookkeeping and returns the (possibly rewritten) resolved type. Verbatim text move out of [`register_type_decl`].
fn register_type_decl_opaque_alias(env: &mut TypeEnv, name: &str, resolved: Ty, gnames: &[Sym], prefix: Option<&str>, visibility: ast::Visibility) -> Ty {
    let is_opaque_alias = !matches!(visibility, ast::Visibility::Public)
        && !matches!(resolved, Ty::Variant { .. })
        && !matches!(resolved, Ty::Record { .. });
    if !is_opaque_alias {
        return resolved;
    }
    // Store the inner target type for codegen
    env.opaque_alias_targets.insert(sym(name), resolved.clone());
    // Register as nominal type (not transparent alias)
    let generic_args: Vec<Ty> = gnames.iter().map(|g| Ty::TypeVar(*g)).collect();
    let resolved = Ty::Named(sym(name), generic_args);
    // Register constructor with visibility restriction
    env.opaque_alias_visibility.insert(sym(name), visibility);
    env.opaque_alias_module.insert(sym(name), prefix.map(|p| sym(p)));
    resolved
}
/// Fix up a `Variant`'s registered name to the DECLARED name, and register each of its constructors. Verbatim text move out of [`register_type_decl`].
fn register_type_decl_variant_ctors(env: &mut TypeEnv, diagnostics: &mut Vec<Diagnostic>, name: &str, prefix: Option<&str>, resolved: &mut Ty) {
    if let Ty::Variant { name: vn, cases } = resolved {
        *vn = sym(name);
        // Push (not overwrite) so a constructor name declared in multiple variant types keeps ALL candidates — needed to detect ambiguity (#413) instead of silently letting the last-registered type win. #413: record each candidate's OWNING MODULE so a shared ctor name can be disambiguated by the current module (`lookup_ctor_in`). type_name stays BARE here — other consumers expect that; `lookup_ctor_in` qualifies on demand.
        // A `None` prefix during `infer_module`'s temporary unprefixed pass still
        // belongs to the module being inferred — attribute it there so the dedupe
        // guard below collapses it onto the canonical prefixed candidate.
        let owner_mod = prefix.map(sym).or(env.alias_owner_module);
        for case in cases.iter() {
            let entry = env.constructors.entry(case.name).or_default();
            // E019 (#1426): a SECOND type in the SAME module declaring this
            // case name would make bare resolution registration-order-dependent
            // — `lookup_ctor_in`'s owned-first find() would silently keep the
            // older type winning and leave the new case unreachable. Reported
            // once, on the canonical pass (`infer_module`'s unprefixed alias
            // pass re-registers the same declarations; skip it like
            // `register_type_decl_check_duplicate` does).
            if env.alias_owner_module.is_none() {
                if let Some((prior, _, _)) = entry.iter().find(|(t, m, _)| *t != sym(name) && *m == owner_mod) {
                    diagnostics.push(err(
                        format!("ambiguous constructor '{}': declared in both '{}' and '{}' of the same module", case.name, prior, name),
                        format!("Rename the case in one of them so '{}' has exactly one meaning here.", case.name),
                        format!("constructor {}", case.name),
                    ).with_code("E019"));
                }
            }
            if !entry.iter().any(|(t, m, _)| *t == sym(name) && *m == owner_mod) {
                entry.push((sym(name), owner_mod, case.clone()));
            }
        }
    }
}
/// #433: a DIFFERENT structural type already holds this BARE name — two distinct types (a local type and a dependency's, or two sub-modules') sharing a name. Type identity is by bare name through link + codegen, so the second silently shadows the first and the function that used the shadowed type fails with a cryptic generated-Rust E0560/E0609. Until types are namespaced per package, surface the collision at the source so the user renames one. Structurally-identical re-registration (the diamond case: same package via two import paths) compares equal and is NOT flagged. #433: types are now namespaced per (user) package — `dep_a.Config` and `dep_b.Config` coexist as distinct qualified names. So a collision is only a real error when the SAME canonical key is re-declared with a different structure (a duplicate within one module/file), which we detect on the prefixed key. Structurally-identical re-registration (the diamond case) is equal and not flagged. Verbatim text move out of [`register_type_decl`].
fn register_type_decl_check_duplicate(env: &TypeEnv, diagnostics: &mut Vec<Diagnostic>, name: &str, prefix: Option<&str>, resolved: &Ty) {
    // `infer_module`'s temporary UNPREFIXED pass re-registers declarations the
    // canonical prefixed registration already validated, purely so intra-module
    // references resolve bare. Re-running the duplicate check there compares a
    // module's own type against a SIBLING package's bare alias of the same name
    // and reports a phantom E020 (`collqa.Config` vs `collqb.Config`, which #433
    // made legal). The real check already ran; skip the alias pass.
    if env.alias_owner_module.is_some() {
        return;
    }
    if matches!(resolved, Ty::Record { .. } | Ty::OpenRecord { .. } | Ty::Variant { .. }) {
        let canonical_key = prefixed_key(prefix, name);
        // A LOCAL type (main program, no prefix) is allowed to SHADOW a dependency's bare-name dual-registration rather than collide with it (#433): the existing bare `Persona` mirrors some `dep.Persona`, and a local `type Persona` should win for unqualified use (the dep stays reachable via `dep.Persona`). Only flag E020 for a genuine duplicate — another type registered under the SAME canonical key that is NOT just a dependency's bare alias being shadowed by a local.
        let shadows_dep_alias = prefix.is_none() && env.prefixed_bare_aliases.contains(&sym(&canonical_key));
        if !shadows_dep_alias {
            if let Some(existing) = env.types.get(&sym(&canonical_key)) {
                if existing != resolved
                    && matches!(existing, Ty::Record { .. } | Ty::OpenRecord { .. } | Ty::Variant { .. })
                {
                    diagnostics.push(err(
                        format!("type '{}' is declared more than once with different structures", name),
                        format!("Two distinct types share the name '{}' within the same module. Rename one so the name is unique.", name),
                        format!("type {}", name),
                    ).with_code("E020"));
                }
            }
        }
    }
}
/// Register field defaults (both plain and record-payload variant cases), insert the resolved type under its canonical key, and — for a prefixed (imported/sub-module) type — dual-register the bare name for unqualified access. Verbatim text move out of [`register_type_decl`].
fn register_type_decl_finalize(env: &mut TypeEnv, name: &str, ty: &ast::TypeExpr, prefix: Option<&str>, resolved: Ty) {
    let key = prefixed_key(prefix, name);
    // Field defaults, keyed like `types` (both keys when prefixed), so record-construction validation knows which fields may be omitted (#488).
    if let ast::TypeExpr::Record { fields } | ast::TypeExpr::OpenRecord { fields } = ty {
        let defaults: std::collections::HashSet<Sym> =
            fields.iter().filter(|f| f.default.is_some()).map(|f| f.name).collect();
        env.record_field_defaults.insert(sym(&key), defaults.clone());
        if prefix.is_some() {
            env.record_field_defaults.insert(sym(name), defaults);
        }
    }
    // Record-payload variant cases carry field defaults too (`| Rect { color: String = "" }`) — harvest them from the AST, since the resolved `VariantPayload::Record` keeps only (name, ty).
    if let ast::TypeExpr::Variant { cases } = ty {
        for c in cases {
            if let ast::VariantCase::Record { name: cname, fields } = c {
                let defs: Vec<Sym> = fields.iter().filter(|f| f.default.is_some()).map(|f| f.name).collect();
                if !defs.is_empty() {
                    env.ctor_field_defaults.entry(*cname).or_default().extend(defs);
                }
            }
        }
    }
    env.types.insert(sym(&key), resolved.clone());
    if prefix.is_some() {
        // Bare-name dual-registration of a prefixed type, for unqualified access. Record it so a local same-name type may shadow it (#433).
        env.types.insert(sym(name), resolved);
        env.prefixed_bare_aliases.insert(sym(name));
    } else {
        // A local type owns the bare name now — it is no longer a dependency alias, so a later genuine local duplicate is still caught by E020.
        env.prefixed_bare_aliases.remove(&sym(name));
    }
}
/// Walk all declarations and register them into the type environment.
pub fn register_decls(env: &mut TypeEnv, diagnostics: &mut Vec<Diagnostic>, decls: &[ast::Decl], prefix: Option<&str>) {
    // Catch duplicate `fn <name>` / `test "<name>"` at the Almide stage so that rustc's E0428 "defined multiple times" never leaks to the user with a src/main.rs span. Tracked per (kind, name), remembering the first span.
    let mut seen_fn: HashMap<String, Option<ast::Span>> = HashMap::new();
    let mut seen_test: HashMap<String, Option<ast::Span>> = HashMap::new();

    for decl in decls {
        match decl {
            ast::Decl::Fn { .. } => register_decl_fn(env, diagnostics, &mut seen_fn, decl, prefix),
            ast::Decl::Test { .. } => register_decl_test(diagnostics, &mut seen_test, decl),
            ast::Decl::Type { .. } => register_decl_type(env, diagnostics, decl, prefix),
            ast::Decl::Protocol { name, generics, methods, .. } => {
                register_protocol_decl(env, name, generics, methods);
            }
            ast::Decl::TopLet { .. } => register_decl_top_let(env, decl, prefix),
            _ => {}
        }
    }
    // `infer_module` re-registers a module's decls UNPREFIXED so its own
    // bodies resolve bare names, and marks that pass with `alias_owner_module`.
    // Validating from inside it compared one module's bare `Span` against
    // another's — over the WHOLE env, attributed to whichever file was under
    // inference — which is how a Codec on one module's type produced a field
    // mismatch pointing at an unrelated file (#1087). The canonical prefixed
    // registration validates the same declarations properly.
    if env.alias_owner_module.is_none() {
        validate_protocol_impls(env, diagnostics);
        validate_derive_field_support(env, diagnostics);
    }
}
/// `ast::Decl::Fn` arm of [`register_decls`] — E012 duplicate-function diagnostic (skipped for `@extern` re-exports), signature registration, and DefTable registration. Verbatim text move; `continue` in the original loop becomes an early `return` here (both simply skip the rest of this decl's registration and move on to the next `decl`).
fn register_decl_fn(env: &mut TypeEnv, diagnostics: &mut Vec<Diagnostic>, seen_fn: &mut HashMap<String, Option<ast::Span>>, decl: &ast::Decl, prefix: Option<&str>) {
    let ast::Decl::Fn { name, params, return_type, effect, generics, span, visibility, extern_attrs, body, attrs, .. } = decl else { unreachable!() };
    // Skip duplicates that come from @extern re-export (name may appear twice by design).
    if extern_attrs.is_empty() {
        let key = prefixed_key(prefix, name);
        if let Some(first_span) = seen_fn.get(&key) {
            let mut diag = err(
                format!("duplicate function '{}'", name),
                format!("Rename one of the definitions, or remove the earlier one. Almide requires each function name to be unique within a module."),
                format!("fn {}", name),
            ).with_code("E012");
            if let Some(s) = span {
                diag.line = Some(s.line);
                diag.col = Some(s.col);
            }
            if let Some(first) = first_span {
                diag.secondary.push(almide_base::diagnostic::SecondarySpan {
                    line: first.line,
                    col: Some(first.col),
                    label: format!("first definition of '{}' here", name),
                });
            }
            diagnostics.push(diag);
            return;
        }
        seen_fn.insert(key, span.clone());
    }
    register_fn_sig(env, &FnSigToRegister {
        name, params, return_type, effect, generics,
        prefix, span: span.as_ref(), visibility: *visibility, attrs,
    });
    // Register in DefTable
    let fn_key = prefixed_key(prefix, name);
    let pkg = prefix.and_then(|p| p.split('.').next()).unwrap_or("");
    let mod_path = prefix.unwrap_or("");
    let ret = env.functions.get(&sym(&fn_key)).map(|s| s.ret.clone()).unwrap_or(Ty::Unknown);
    let did = env.def_table.alloc(sym(pkg), sym(mod_path), sym(name), almide_ir::DefKind::Function, ret);
    env.def_map.insert(sym(&fn_key), did);
    // An EXPLICIT `fn Type.method` with a body, recorded on the shared env so
    // another module can find it. Lowering's own set only ever holds the
    // program being lowered, so a custom `repr` was silently ignored across an
    // import and `"${lib.Red}"` fell back to the variant name (#1087).
    if name.contains('.') && body.is_some() {
        env.explicit_convention_fns.insert(sym(&fn_key));
    }
}
/// `ast::Decl::Test` arm of [`register_decls`] — E012 duplicate-test diagnostic. Verbatim text move; `continue` becomes an early `return` (see [`register_decl_fn`]).
fn register_decl_test(diagnostics: &mut Vec<Diagnostic>, seen_test: &mut HashMap<String, Option<ast::Span>>, decl: &ast::Decl) {
    let ast::Decl::Test { name, span, .. } = decl else { unreachable!() };
    let test_key = name.to_string();
    if let Some(first_span) = seen_test.get(&test_key) {
        let mut diag = err(
            format!("duplicate test '{}'", name),
            format!("Rename one of the tests, or merge them. Each test name must be unique within a file."),
            format!("test \"{}\"", name),
        ).with_code("E012");
        if let Some(s) = span {
            diag.line = Some(s.line);
            diag.col = Some(s.col);
        }
        if let Some(first) = first_span {
            diag.secondary.push(almide_base::diagnostic::SecondarySpan {
                line: first.line,
                col: Some(first.col),
                label: format!("first test '{}' here", name),
            });
        }
        diagnostics.push(diag);
        return;
    }
    seen_test.insert(test_key, span.clone());
}
/// `ast::Decl::Type` arm of [`register_decls`] — type registration plus DefTable and `type_protocols` bookkeeping. Verbatim text move out of [`register_decls`].
fn register_decl_type(env: &mut TypeEnv, diagnostics: &mut Vec<Diagnostic>, decl: &ast::Decl, prefix: Option<&str>) {
    let ast::Decl::Type { name, ty, deriving, generics, visibility, .. } = decl else { unreachable!() };
    register_type_decl(env, diagnostics, &TypeDeclToRegister {
        name, ty, deriving, generics, prefix, visibility: *visibility,
    });
    // Register in DefTable
    let type_key = prefixed_key(prefix, name);
    let pkg = prefix.and_then(|p| p.split('.').next()).unwrap_or("");
    let mod_path = prefix.unwrap_or("");
    let resolved_ty = env.types.get(&sym(&type_key)).cloned().unwrap_or(Ty::Unknown);
    let did = env.def_table.alloc(sym(pkg), sym(mod_path), sym(name), almide_ir::DefKind::Type, resolved_ty);
    env.def_map.insert(sym(&type_key), did);
    if let Some(derives) = deriving {
        // Recorded under the bare name AND the canonical prefixed one: a
        // `[T: Codec]` bound resolves its argument to `Ty::Named("lib.P")` and
        // looked that up here, where only bare `P` had ever been written — so
        // a conforming type from another module was reported as not
        // implementing the protocol (#1087).
        for d in derives {
            for key in [sym(name), sym(&type_key)] {
                env.type_protocols
                    .entry(key)
                    .or_insert_with(std::collections::HashSet::new)
                    .insert(sym(d));
            }
        }
    }
}
/// `ast::Decl::TopLet` arm of [`register_decls`] — top-level `let` type seeding (or reuse of a fully-inferred prior entry) and DefTable registration. Verbatim text move out of [`register_decls`].
fn register_decl_top_let(env: &mut TypeEnv, decl: &ast::Decl, prefix: Option<&str>) {
    let ast::Decl::TopLet { name, ty, value, .. } = decl else { unreachable!() };
    let rt = ty.as_ref().map(|te| resolve(env, te))
        .unwrap_or_else(|| infer_top_let_seed(env, prefix, value));
    let key = prefixed_key(prefix, name);
    // A PREFIXED key names exactly one decl program-wide, and registration re-runs per driver leg over a persistent env — re-seeding must not downgrade a fully inferred entry (the post-solve flush's `Option[Cfg]`) back to the seed's partial `Option[Unknown]`. Unprefixed keys stay unconditional: they are scoped aliases (main program / intra-module temp) where an entry may legitimately describe a DIFFERENT decl.
    let keep_existing = prefix.is_some()
        && (rt.contains_unknown() || rt.contains_typevar())
        && env.top_lets.get(&sym(&key)).is_some_and(|t| {
            !t.contains_unknown() && !t.contains_typevar()
        });
    if !keep_existing {
        env.top_lets.insert(sym(&key), rt.clone());
    }
    // Register in DefTable
    let pkg = prefix.and_then(|p| p.split('.').next()).unwrap_or("");
    let mod_path = prefix.unwrap_or("");
    let did = env.def_table.alloc(sym(pkg), sym(mod_path), sym(name), almide_ir::DefKind::TopLet, rt);
    env.def_map.insert(sym(&key), did);
}

include!("registration_validate.rs");

#[cfg(test)]
mod scalar_name_tests {
    use super::*;

    /// Every name the const-generic bound syntax accepts must resolve to a type.
    /// A name in the list with no `scalar_ty_by_name` arm silently downgrades
    /// `[N: T]` to an ordinary generic, which then fails in codegen rather than
    /// at the annotation.
    #[test]
    fn scalar_type_names_all_resolve() {
        let unresolved: Vec<&str> = SCALAR_TYPE_NAMES.iter().copied()
            .filter(|n| scalar_ty_by_name(n).is_none())
            .collect();
        assert!(unresolved.is_empty(), "scalar type names with no Ty: {unresolved:?}");
    }
}
