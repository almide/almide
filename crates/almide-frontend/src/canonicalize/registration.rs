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

/// Like `resolve`, but pins a user module's own-type references to the qualified
/// canonical name `mod.Type` (#433). `cur_mod` is the module being registered.
fn resolve_in(env: &TypeEnv, te: &ast::TypeExpr, cur_mod: Option<&str>) -> Ty {
    crate::canonicalize::resolve::resolve_type_expr_in(te, Some(&env.types), cur_mod)
}

/// Infer type from a literal expression (for top-level `let` without annotation).
///
/// Used at registration time — before the full checker runs — so module
/// top_lets have a concrete `env.top_lets` entry the moment the main
/// program's inference looks them up. A shallow scalar-only version
/// regresses records / lists / maps to `Ty::Unknown`, which later surfaces
/// as `LazyLock<_>` in generated Rust and `ConcretizeTypes` post-condition
/// failures on WASM. Recurse structurally through record / list / tuple /
/// map literals so the cross-module user sees the right type.
/// Seed type for an UNANNOTATED top-let. `infer_literal_type` covers
/// literals and anonymous records only; a NAMED constructor (`Cfg { … }`)
/// fell to `Ty::Unknown`, and because every driver checks MAIN before the
/// modules, main's inference read that stale Unknown for a cross-module
/// `m.CFG` — a spread of it then carried Unknown into the AllTypesConcrete
/// refusal (#502). Resolve the ctor name through the SAME #433 predicate an
/// explicit `: Cfg` annotation uses, so both spellings seed identically.
/// Generic decls (unresolved type params) deliberately stay Unknown — the
/// ctor args are not inferable here and the later module-check writeback
/// only corrects exact-Unknown seeds.
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
    match &expr.kind {
        ast::ExprKind::Int { .. } => Ty::Int,
        ast::ExprKind::Float { .. } => Ty::Float,
        ast::ExprKind::String { .. } => Ty::String,
        ast::ExprKind::Bool { .. } => Ty::Bool,
        ast::ExprKind::Unit => Ty::Unit,
        ast::ExprKind::Paren { expr } => infer_literal_type(expr),
        // A signed literal is a literal: `let MARGIN_AUTO = -2.0` must seed
        // Float, not Unknown (#784 — the Unknown seed leaked into every
        // cross-module reader of the constant). `-` keeps the operand's
        // numeric type; `not` is Bool.
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
        _ => Ty::Unknown,
    }
}

/// Build a prefixed key: "module.name" or just "name".
pub fn prefixed_key(prefix: Option<&str>, name: &str) -> String {
    prefix.map(|p| format!("{}.{}", p, name)).unwrap_or_else(|| name.to_string())
}

/// Substitute `Self` → concrete type in a protocol method type.
fn substitute_self(ty: &Ty, replacement: &Ty) -> Ty {
    match ty {
        Ty::TypeVar(name) if name == "Self" => replacement.clone(),
        _ => ty.map_children(&|child| substitute_self(child, replacement)),
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

/// Identify const (value) parameters in generic params.
/// A param `N: Int` where `Int` is a scalar type (not a protocol) becomes a const param.
/// Returns: param name → scalar Ty.
pub fn collect_const_params(generics: &Option<Vec<ast::GenericParam>>) -> HashMap<Sym, Ty> {
    let mut cp = HashMap::new();
    let gs = match generics { Some(gs) => gs, None => return cp };
    for g in gs {
        if let Some(bounds) = &g.bounds {
            // Single bound that is a scalar type name → const param
            if bounds.len() == 1 && SCALAR_TYPE_NAMES.contains(&bounds[0].as_str()) {
                let ty = match bounds[0].as_str() {
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
                    _ => continue,
                };
                cp.insert(sym(&g.name), ty);
            }
        }
    }
    cp
}

/// Collect protocol bounds from generic params: TypeVar name → list of protocol names.
/// Excludes const params (scalar type bounds like `N: Int`).
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

pub fn register_fn_sig(
    env: &mut TypeEnv,
    name: &str, params: &[ast::Param], return_type: &ast::TypeExpr,
    effect: &Option<bool>, r#async: &Option<bool>, generics: &Option<Vec<ast::GenericParam>>,
    prefix: Option<&str>, span: Option<&ast::Span>,
    visibility: ast::Visibility,
) {
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
    let ptys: Vec<(Sym, Ty)> = params.iter().map(|p| (sym(&p.name), resolve_in(env, &p.ty, prefix))).collect();
    let mut_params: Vec<usize> = params.iter().enumerate()
        .filter(|(_, p)| p.is_mut)
        .map(|(i, _)| i)
        .collect();
    let ret = resolve_in(env, return_type, prefix);
    for gn in &gnames { env.types.remove(gn); }
    let is_effect = effect.unwrap_or(false) || r#async.unwrap_or(false);
    let key = prefixed_key(prefix, name);
    if prefix.is_none() && is_effect { env.effect_fns.insert(sym(name)); }
    let min_p = params.iter().take_while(|p| p.default.is_none()).count();
    env.functions.insert(sym(&key), FnSig { params: ptys, ret, is_effect, generics: gnames, structural_bounds: sb, protocol_bounds: pb, mut_params });
    // Record visibility so `resolve_module_call` can reject cross-module access
    // to `mod fn` / `local fn`. Only non-Public entries need to be stored — the
    // lookup in the checker treats "missing" as Public (stdlib, impl methods,
    // derived stubs).
    if !matches!(visibility, ast::Visibility::Public) {
        env.fn_visibility.insert(sym(&key), visibility);
    }
    if let Some(s) = span {
        env.fn_decl_spans.insert(sym(&key), (s.line, s.col));
    }
    if min_p < params.len() {
        env.fn_min_params.insert(sym(&key), min_p);
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
    // #433: the VALUE type in derived signatures must carry the canonical
    // qualified name for a user module's type — `Pigment.decode`'s
    // `Result[Pigment, String]` with a bare name leaked into callers' var
    // tables (found by the NameResolutionTotal gate). The fn KEYS stay as
    // they were (separate resolution system).
    let canonical = match prefix {
        Some(m) if !almide_lang::stdlib_info::is_bundled_module(m) => format!("{}.{}", m, type_name),
        _ => type_name.to_string(),
    };
    let type_ty = Ty::Named(sym(&canonical), vec![]);
    let value_ty = Ty::Named(sym("Value"), vec![]);
    let empty_sb: HashMap<Sym, Ty> = HashMap::new();
    let empty_pb: HashMap<Sym, Vec<Sym>> = HashMap::new();
    for d in derives {
        match d.as_str() {
            "Eq" => {
                let fn_key = format!("{}.eq", type_name);
                if !env.functions.contains_key(&sym(&fn_key)) {
                    env.functions.insert(sym(&fn_key), FnSig { params: vec![("a".into(), type_ty.clone()), ("b".into(), type_ty.clone())], ret: Ty::Bool, is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone(), mut_params: vec![] });
                }
            }
            "Repr" => {
                let fn_key = format!("{}.repr", type_name);
                if !env.functions.contains_key(&sym(&fn_key)) {
                    env.functions.insert(sym(&fn_key), FnSig { params: vec![("v".into(), type_ty.clone())], ret: Ty::String, is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone(), mut_params: vec![] });
                }
            }
            "Codec" => {
                let encode_key = format!("{}.encode", type_name);
                if !env.functions.contains_key(&sym(&encode_key)) {
                    env.functions.insert(sym(&encode_key), FnSig { params: vec![("v".into(), type_ty.clone())], ret: value_ty.clone(), is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone(), mut_params: vec![] });
                }
                let decode_key = format!("{}.decode", type_name);
                if !env.functions.contains_key(&sym(&decode_key)) {
                    env.functions.insert(sym(&decode_key), FnSig { params: vec![("v".into(), value_ty.clone())], ret: Ty::result(type_ty.clone(), Ty::String), is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone(), mut_params: vec![] });
                }
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

/// Protocols whose auto-derive RECURSES INTO EACH FIELD'S TYPE: deriving them
/// on a struct/variant emits per-field work that requires the field type to
/// ALSO satisfy the protocol. `Codec` calls `Field.encode` / `Field.decode`;
/// `Ord`/`Hash` lower to a Rust `#[derive(Ord/Hash)]` that needs the field's
/// Rust type to impl it. `Eq`/`Repr` are excluded — every generated struct
/// gets `PartialEq` + a repr path unconditionally, so a field need not declare
/// them (gating those would be a false positive).
const FIELD_RECURSIVE_PROTOCOLS: &[&str] = &["Codec", "Ord", "Hash"];

/// The field-type slots a structural type exposes to its derive: record fields,
/// and every variant case's payload (tuple positions / record fields).
fn type_field_slots(ty: &Ty) -> Vec<(String, Ty)> {
    match ty {
        Ty::Record { fields } | Ty::OpenRecord { fields } =>
            fields.iter().map(|(n, t)| (n.to_string(), t.clone())).collect(),
        Ty::Variant { cases, .. } => {
            let mut out = Vec::new();
            for c in cases {
                match &c.payload {
                    VariantPayload::Unit => {}
                    VariantPayload::Tuple(ts) => for (i, t) in ts.iter().enumerate() {
                        out.push((format!("{}.{}", c.name, i), t.clone()));
                    },
                    VariantPayload::Record(fs) => for (n, t) in fs {
                        out.push((n.to_string(), t.clone()));
                    },
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

/// The nominal leaf types a derive must recurse into for one field type,
/// descending through the standard containers (List/Option/Set/Map/Result via
/// `Applied`, tuples, nested anon records). A `List[Pigment]` field under a
/// `: Codec` type requires `Pigment` to be Codec, so the leaf is `Pigment`.
fn collect_leaf_nominals(ty: &Ty, out: &mut Vec<Sym>) {
    match ty {
        Ty::Named(n, args) => {
            out.push(*n);
            for a in args { collect_leaf_nominals(a, out); }
        }
        Ty::Variant { name, .. } => out.push(*name),
        Ty::Applied(_, args) => for a in args { collect_leaf_nominals(a, out); },
        Ty::Tuple(elems) => for e in elems { collect_leaf_nominals(e, out); },
        Ty::Record { fields } | Ty::OpenRecord { fields } =>
            for (_, t) in fields { collect_leaf_nominals(t, out); },
        _ => {}
    }
}

/// Does user type `leaf` satisfy protocol `proto`? Keyed leniently: `type_protocols`
/// is interned bare, but a cross-module field type may carry a qualified
/// `mod.Type` name — accept either spelling. For `Codec`, a hand-written
/// `Type.encode`/`Type.decode` pair (without a `: Codec` declaration) also
/// satisfies the requirement, since the derive only needs those functions to exist.
fn leaf_satisfies(env: &TypeEnv, leaf: Sym, proto: &str) -> bool {
    let bare = leaf.as_str().rsplit('.').next().unwrap_or(leaf.as_str());
    let declares = |name: &str| env.type_protocols.get(&sym(name))
        .map_or(false, |s| s.contains(&sym(proto)));
    if declares(leaf.as_str()) || declares(bare) {
        return true;
    }
    if proto == "Codec" {
        let has = |m: &str| env.functions.contains_key(&sym(&format!("{}.{}", leaf, m)))
            || env.functions.contains_key(&sym(&format!("{}.{}", bare, m)));
        return has("encode") && has("decode");
    }
    false
}

/// The Codec derive serializes a field by structural recursion over
/// String/Int/Float/Bool/Option/List/Named — it has NO Map or Set arm, so a
/// `Map[K,V]` / `Set[T]` field silently falls through to the `Value`-as-String
/// fallback: invalid Rust natively (E0614/E0308) and wrong/silent on wasm
/// (#655). Detect such a container anywhere in the field type (under
/// List/Option/Result/Tuple/anon-record). A `Map`/`Set` reached only through a
/// NAMED type is that type's own concern (its `: Codec` is checked by the leaf
/// rule), so we stop at `Ty::Named`.
fn codec_unsupported_container(ty: &Ty) -> Option<&'static str> {
    use almide_lang::types::TypeConstructorId as TC;
    match ty {
        Ty::Applied(TC::Map, _) => Some("Map"),
        Ty::Applied(TC::Set, _) => Some("Set"),
        Ty::Applied(_, args) => args.iter().find_map(|a| codec_unsupported_container(a)),
        Ty::Tuple(elems) => elems.iter().find_map(|e| codec_unsupported_container(e)),
        Ty::Record { fields } | Ty::OpenRecord { fields } =>
            fields.iter().find_map(|(_, t)| codec_unsupported_container(t)),
        _ => None,
    }
}

/// A type that derives a field-recursive protocol (Codec/Ord/Hash) requires
/// every field type to ALSO satisfy it — otherwise the derive emits a call to a
/// non-existent `Field.encode` (Codec) or a Rust `#[derive(Ord/Hash)]` over a
/// field whose Rust type lacks the impl, both of which the checker previously
/// accepted and codegen then rejected as "invalid Rust" (#611). This validates
/// the requirement structurally, at the checker, independent of target.
fn validate_derive_field_support(env: &TypeEnv, diagnostics: &mut Vec<Diagnostic>) {
    let pairs: Vec<(Sym, Vec<Sym>)> = env.type_protocols.iter()
        .map(|(ty, protos)| (*ty, protos.iter().copied().collect()))
        .collect();
    let mut reported: std::collections::HashSet<(Sym, Sym, Sym)> = std::collections::HashSet::new();
    for (type_name, protocols) in &pairs {
        let Some(ty) = env.types.get(type_name) else { continue };
        let slots = type_field_slots(ty);
        if slots.is_empty() { continue; }
        for proto in protocols {
            let p = proto.as_str();
            if !FIELD_RECURSIVE_PROTOCOLS.contains(&p) { continue; }
            for (field_name, field_ty) in &slots {
                // #655: the Codec derive has no Map/Set arm — reject such a
                // field here rather than emitting invalid Rust / silent-wrong
                // wasm. Same E023 family (a field that cannot satisfy Codec).
                if p == "Codec" {
                    if let Some(container) = codec_unsupported_container(field_ty) {
                        if reported.insert((*type_name, *proto, sym(container))) {
                            diagnostics.push(err(
                                format!("type '{}' derives 'Codec' but field '{}' has a '{}' type, which the Codec derive cannot encode",
                                    type_name, field_name, container),
                                format!("The Codec derive serializes a {} as a String, which is wrong. Use a List[(K, V)] field (or List[T] for a Set), or implement encode/decode manually.",
                                    container),
                                format!("type {} : Codec", type_name),
                            ).with_code("E023"));
                        }
                    }
                }
                let mut leaves = Vec::new();
                collect_leaf_nominals(field_ty, &mut leaves);
                for leaf in leaves {
                    if leaf == *type_name { continue; }          // self-reference is fine
                    if !env.types.contains_key(&leaf) { continue; } // not a user nominal → native support
                    if leaf_satisfies(env, leaf, p) { continue; }
                    if !reported.insert((*type_name, *proto, leaf)) { continue; }
                    diagnostics.push(err(
                        format!("type '{}' derives '{}' but field '{}' has type '{}', which does not derive '{}'",
                            type_name, p, field_name, leaf, p),
                        format!("Add `: {}` to the declaration of type '{}' (every field of a `: {}` type must itself be `{}`)",
                            p, leaf, p, p),
                        format!("type {} : {}", type_name, p),
                    ).with_code("E023"));
                }
            }
        }
    }
}

/// Validate that types declaring `: ProtocolName` have all required convention methods.
/// Called after all declarations are registered so all `Type.method` functions are available.
pub fn validate_protocol_impls(env: &TypeEnv, diagnostics: &mut Vec<Diagnostic>) {
    let type_protocols: Vec<(Sym, Vec<Sym>)> = env.type_protocols.iter()
        .map(|(ty, protos)| (*ty, protos.iter().copied().collect()))
        .collect();

    for (type_name, protocol_names) in &type_protocols {
        for proto_name in protocol_names {
            if env.impl_validated.contains(&(*type_name, *proto_name)) {
                continue;
            }
            let proto_def = match env.protocols.get(proto_name) {
                Some(p) => p.clone(),
                None => continue,
            };

            for method_sig in &proto_def.methods {
                let fn_key = format!("{}.{}", type_name, method_sig.name);
                if !env.functions.contains_key(&sym(&fn_key)) {
                    let is_builtin = matches!(proto_name.as_str(),
                        "Eq" | "Repr" | "Ord" | "Hash" | "Codec" | "Encode" | "Decode"
                        | "Numeric");
                    if !is_builtin {
                        diagnostics.push(err(
                            format!("type '{}' declares protocol '{}' but missing method '{}'",
                                type_name, proto_name, method_sig.name),
                            format!("Add: fn {}.{}({}) -> {}",
                                type_name, method_sig.name,
                                method_sig.params.iter()
                                    .map(|(n, t)| {
                                        let display_ty = if *t == Ty::TypeVar(sym("Self")) {
                                            type_name.to_string()
                                        } else {
                                            t.display()
                                        };
                                        format!("{}: {}", n, display_ty)
                                    })
                                    .collect::<Vec<_>>()
                                    .join(", "),
                                {
                                    let ret = &method_sig.ret;
                                    if *ret == Ty::TypeVar(sym("Self")) {
                                        type_name.to_string()
                                    } else {
                                        ret.display()
                                    }
                                }),
                            format!("type {} : {}", type_name, proto_name),
                        ));
                    }
                }
            }
        }
    }
}

/// Register an `impl Protocol for Type { ... }` block.
pub fn register_impl_decl(env: &mut TypeEnv, diagnostics: &mut Vec<Diagnostic>, trait_name: &str, for_type: &str, methods: &[ast::Decl]) {
    // 1. Validate protocol exists
    let proto_def = match env.protocols.get(&sym(trait_name)) {
        Some(p) => p.clone(),
        None => {
            let valid: Vec<&str> = env.protocols.keys().map(|s| s.as_str()).collect();
            diagnostics.push(err(
                format!("unknown protocol '{}' in impl block", trait_name),
                format!("Known protocols: {}", {
                    let mut sorted = valid; sorted.sort(); sorted.join(", ")
                }),
                format!("impl {} for {}", trait_name, for_type),
            ));
            // Still register methods as convention functions so downstream doesn't break.
            // `impl` methods follow the trait's visibility, not a custom modifier, so they
            // are always publicly callable through the trait interface.
            for m in methods {
                if let ast::Decl::Fn { name, params, return_type, effect, r#async, generics, span, .. } = m {
                    register_fn_sig(env, name, params, return_type, effect, r#async, generics, Some(for_type), span.as_ref(), ast::Visibility::Public);
                }
            }
            return;
        }
    };

    // 2. Register each method as Type.method and validate signature
    let type_ty = Ty::Named(sym(for_type), vec![]);
    let mut impl_methods: std::collections::HashSet<String> = std::collections::HashSet::new();

    for m in methods {
        if let ast::Decl::Fn { name, params, return_type, effect, r#async, generics, span, .. } = m {
            register_fn_sig(env, name, params, return_type, effect, r#async, generics, Some(for_type), span.as_ref(), ast::Visibility::Public);
            impl_methods.insert(name.to_string());

            // 3. Validate signature matches protocol definition
            if let Some(proto_method) = proto_def.methods.iter().find(|pm| pm.name == *name) {
                let gnames: Vec<Sym> = generics.as_ref().map(|gs| gs.iter().map(|g| sym(&g.name)).collect()).unwrap_or_default();
                for gn in &gnames { env.types.insert(*gn, Ty::TypeVar(*gn)); }
                // Resolve impl signatures structurally so nominal types
                // (e.g. `d: Dog`) unify with the protocol's structural
                // expectation (Self → `{ name: String }`).
                let impl_params: Vec<Ty> = params.iter()
                    .map(|p| env.resolve_named(&resolve(env, &p.ty)))
                    .collect();
                let impl_ret = env.resolve_named(&resolve(env, return_type));
                for gn in &gnames { env.types.remove(gn); }

                let expected_params: Vec<Ty> = proto_method.params.iter()
                    .map(|(_, ty)| {
                        let substituted = substitute_self(ty, &type_ty);
                        env.resolve_named(&substituted)
                    })
                    .collect();
                let expected_ret = env.resolve_named(&substitute_self(&proto_method.ret, &type_ty));

                if impl_params.len() != expected_params.len() {
                    diagnostics.push(err(
                        format!("method '{}' in impl {} for {} has {} parameter(s), expected {}",
                            name, trait_name, for_type, impl_params.len(), expected_params.len()),
                        format!("Protocol '{}' defines: fn {}({})", trait_name, name,
                            proto_method.params.iter().map(|(n, t)| {
                                let display_ty = substitute_self(t, &type_ty).display();
                                format!("{}: {}", n, display_ty)
                            }).collect::<Vec<_>>().join(", ")),
                        format!("impl {} for {}", trait_name, for_type),
                    ));
                } else {
                    for (i, (impl_ty, expected_ty)) in impl_params.iter().zip(expected_params.iter()).enumerate() {
                        if impl_ty != expected_ty && *expected_ty != Ty::Unknown && *impl_ty != Ty::Unknown {
                            let param_name = &params[i].name;
                            diagnostics.push(err(
                                format!("method '{}.{}' parameter '{}' has type '{}', expected '{}'",
                                    for_type, name, param_name, impl_ty.display(), expected_ty.display()),
                                format!("Change type to '{}'", expected_ty.display()),
                                format!("impl {} for {}", trait_name, for_type),
                            ));
                        }
                    }
                    if impl_ret != expected_ret && expected_ret != Ty::Unknown && impl_ret != Ty::Unknown {
                        diagnostics.push(err(
                            format!("method '{}.{}' returns '{}', expected '{}'",
                                for_type, name, impl_ret.display(), expected_ret.display()),
                            format!("Change return type to '{}'", expected_ret.display()),
                            format!("impl {} for {}", trait_name, for_type),
                        ));
                    }
                }
            }
        }
    }

    // 4. Check all required methods are present
    for proto_method in &proto_def.methods {
        if !impl_methods.contains(proto_method.name.as_str()) {
            diagnostics.push(err(
                format!("impl {} for {} is missing method '{}'", trait_name, for_type, proto_method.name),
                format!("Add: fn {}({}) -> {}", proto_method.name,
                    proto_method.params.iter().map(|(n, t)| {
                        let display_ty = substitute_self(t, &type_ty).display();
                        format!("{}: {}", n, display_ty)
                    }).collect::<Vec<_>>().join(", "),
                    substitute_self(&proto_method.ret, &type_ty).display()),
                format!("impl {} for {}", trait_name, for_type),
            ));
        }
    }

    // 5. Track protocol conformance + mark as impl-validated
    env.type_protocols
        .entry(sym(for_type))
        .or_insert_with(std::collections::HashSet::new)
        .insert(sym(trait_name));
    env.impl_validated.insert((sym(for_type), sym(trait_name)));
}

pub fn register_type_decl(env: &mut TypeEnv, diagnostics: &mut Vec<Diagnostic>, name: &str, ty: &ast::TypeExpr, deriving: &Option<Vec<Sym>>,
                       generics: &Option<Vec<ast::GenericParam>>, prefix: Option<&str>, visibility: ast::Visibility) {
    if let Some(derives) = deriving {
        validate_protocols(env, diagnostics, derives, name);
    }
    let gnames: Vec<Sym> = generics.as_ref().map(|gs| gs.iter().map(|g| sym(&g.name)).collect()).unwrap_or_default();
    for gn in &gnames { env.types.insert(*gn, Ty::TypeVar(*gn)); }
    let mut resolved = resolve(env, ty);
    for gn in &gnames { env.types.remove(gn); }
    // mod/local type alias → nominal newtype (opaque constructor)
    let is_opaque_alias = !matches!(visibility, ast::Visibility::Public)
        && !matches!(resolved, Ty::Variant { .. })
        && !matches!(resolved, Ty::Record { .. });
    if is_opaque_alias {
        // Store the inner target type for codegen
        env.opaque_alias_targets.insert(sym(name), resolved.clone());
        // Register as nominal type (not transparent alias)
        let generic_args: Vec<Ty> = gnames.iter().map(|g| Ty::TypeVar(*g)).collect();
        resolved = Ty::Named(sym(name), generic_args);
        // Register constructor with visibility restriction
        env.opaque_alias_visibility.insert(sym(name), visibility);
        env.opaque_alias_module.insert(sym(name), prefix.map(|p| sym(p)));
    }
    if let Ty::Variant { name: ref mut vn, ref cases } = resolved {
        *vn = sym(name);
        // Push (not overwrite) so a constructor name declared in multiple variant
        // types keeps ALL candidates — needed to detect ambiguity (#413) instead of
        // silently letting the last-registered type win.
        // #413: record each candidate's OWNING MODULE so a shared ctor name can be
        // disambiguated by the current module (`lookup_ctor_in`). type_name stays
        // BARE here — other consumers expect that; `lookup_ctor_in` qualifies on demand.
        let owner_mod = prefix.map(sym);
        for case in cases {
            let entry = env.constructors.entry(case.name).or_default();
            if !entry.iter().any(|(t, m, _)| *t == sym(name) && *m == owner_mod) {
                entry.push((sym(name), owner_mod, case.clone()));
            }
        }
    }
    // #433: a DIFFERENT structural type already holds this BARE name — two
    // distinct types (a local type and a dependency's, or two sub-modules')
    // sharing a name. Type identity is by bare name through link + codegen, so
    // the second silently shadows the first and the function that used the
    // shadowed type fails with a cryptic generated-Rust E0560/E0609. Until types
    // are namespaced per package, surface the collision at the source so the user
    // renames one. Structurally-identical re-registration (the diamond case: same
    // package via two import paths) compares equal and is NOT flagged.
    // #433: types are now namespaced per (user) package — `dep_a.Config` and
    // `dep_b.Config` coexist as distinct qualified names. So a collision is only a
    // real error when the SAME canonical key is re-declared with a different
    // structure (a duplicate within one module/file), which we detect on the
    // prefixed key. Structurally-identical re-registration (the diamond case) is
    // equal and not flagged.
    if matches!(resolved, Ty::Record { .. } | Ty::OpenRecord { .. } | Ty::Variant { .. }) {
        let canonical_key = prefixed_key(prefix, name);
        // A LOCAL type (main program, no prefix) is allowed to SHADOW a
        // dependency's bare-name dual-registration rather than collide with it
        // (#433): the existing bare `Persona` mirrors some `dep.Persona`, and a
        // local `type Persona` should win for unqualified use (the dep stays
        // reachable via `dep.Persona`). Only flag E020 for a genuine duplicate —
        // another type registered under the SAME canonical key that is NOT just a
        // dependency's bare alias being shadowed by a local.
        let shadows_dep_alias = prefix.is_none() && env.prefixed_bare_aliases.contains(&sym(&canonical_key));
        if !shadows_dep_alias {
            if let Some(existing) = env.types.get(&sym(&canonical_key)) {
                if existing != &resolved
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
    let key = prefixed_key(prefix, name);
    // Field defaults, keyed like `types` (both keys when prefixed), so
    // record-construction validation knows which fields may be omitted (#488).
    if let ast::TypeExpr::Record { fields } | ast::TypeExpr::OpenRecord { fields } = ty {
        let defaults: std::collections::HashSet<Sym> =
            fields.iter().filter(|f| f.default.is_some()).map(|f| f.name).collect();
        env.record_field_defaults.insert(sym(&key), defaults.clone());
        if prefix.is_some() {
            env.record_field_defaults.insert(sym(name), defaults);
        }
    }
    // Record-payload variant cases carry field defaults too
    // (`| Rect { color: String = "" }`) — harvest them from the AST, since
    // the resolved `VariantPayload::Record` keeps only (name, ty).
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
        // Bare-name dual-registration of a prefixed type, for unqualified access.
        // Record it so a local same-name type may shadow it (#433).
        env.types.insert(sym(name), resolved);
        env.prefixed_bare_aliases.insert(sym(name));
    } else {
        // A local type owns the bare name now — it is no longer a dependency
        // alias, so a later genuine local duplicate is still caught by E020.
        env.prefixed_bare_aliases.remove(&sym(name));
    }
    if let Some(derives) = deriving {
        register_derive_sigs(env, derives, name, prefix);
    }
}

/// Walk all declarations and register them into the type environment.
pub fn register_decls(env: &mut TypeEnv, diagnostics: &mut Vec<Diagnostic>, decls: &[ast::Decl], prefix: Option<&str>) {
    // Catch duplicate `fn <name>` / `test "<name>"` at the Almide stage so that
    // rustc's E0428 "defined multiple times" never leaks to the user with a
    // src/main.rs span. Tracked per (kind, name), remembering the first span.
    let mut seen_fn: HashMap<String, Option<ast::Span>> = HashMap::new();
    let mut seen_test: HashMap<String, Option<ast::Span>> = HashMap::new();

    for decl in decls {
        match decl {
            ast::Decl::Fn { name, params, return_type, effect, r#async, generics, span, visibility, extern_attrs, .. } => {
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
                        continue;
                    }
                    seen_fn.insert(key, span.clone());
                }
                register_fn_sig(env, name, params, return_type, effect, r#async, generics, prefix, span.as_ref(), *visibility);
                // Register in DefTable
                let fn_key = prefixed_key(prefix, name);
                let pkg = prefix.and_then(|p| p.split('.').next()).unwrap_or("");
                let mod_path = prefix.unwrap_or("");
                let ret = env.functions.get(&sym(&fn_key)).map(|s| s.ret.clone()).unwrap_or(Ty::Unknown);
                let did = env.def_table.alloc(sym(pkg), sym(mod_path), sym(name), almide_ir::DefKind::Function, ret);
                env.def_map.insert(sym(&fn_key), did);
            }
            ast::Decl::Test { name, span, .. } => {
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
                    continue;
                }
                seen_test.insert(test_key, span.clone());
            }
            ast::Decl::Type { name, ty, deriving, generics, visibility, .. } => {
                register_type_decl(env, diagnostics, name, ty, deriving, generics, prefix, *visibility);
                // Register in DefTable
                let type_key = prefixed_key(prefix, name);
                let pkg = prefix.and_then(|p| p.split('.').next()).unwrap_or("");
                let mod_path = prefix.unwrap_or("");
                let resolved_ty = env.types.get(&sym(&type_key)).cloned().unwrap_or(Ty::Unknown);
                let did = env.def_table.alloc(sym(pkg), sym(mod_path), sym(name), almide_ir::DefKind::Type, resolved_ty);
                env.def_map.insert(sym(&type_key), did);
                if let Some(derives) = deriving {
                    for d in derives {
                        env.type_protocols
                            .entry(sym(name))
                            .or_insert_with(std::collections::HashSet::new)
                            .insert(sym(d));
                    }
                }
            }
            ast::Decl::Protocol { name, generics, methods, .. } => {
                register_protocol_decl(env, name, generics, methods);
            }
            ast::Decl::Impl { trait_, for_, methods, .. } => {
                register_impl_decl(env, diagnostics, trait_, for_, methods);
            }
            ast::Decl::TopLet { name, ty, value, .. } => {
                let rt = ty.as_ref().map(|te| resolve(env, te))
                    .unwrap_or_else(|| infer_top_let_seed(env, prefix, value));
                let key = prefixed_key(prefix, name);
                // A PREFIXED key names exactly one decl program-wide, and
                // registration re-runs per driver leg over a persistent env —
                // re-seeding must not downgrade a fully inferred entry (the
                // post-solve flush's `Option[Cfg]`) back to the seed's partial
                // `Option[Unknown]`. Unprefixed keys stay unconditional: they
                // are scoped aliases (main program / intra-module temp) where
                // an entry may legitimately describe a DIFFERENT decl.
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
            _ => {}
        }
    }
    validate_protocol_impls(env, diagnostics);
    validate_derive_field_support(env, diagnostics);
}
