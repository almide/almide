//! Declaration registration: collecting function signatures, type declarations,
//! protocol declarations, and protocol validation into the type environment.
//!
//! These are free functions operating on `&mut TypeEnv` + `&mut Vec<Diagnostic>`,
//! extracted from the former `Checker` methods in `check/registration.rs`.

use std::collections::HashMap;
use almide_lang::ast;
use almide_base::diagnostic::Diagnostic;
use almide_base::intern::{Sym, sym};
use crate::types::{Ty, TypeEnv, FnSig, ProtocolDef, ProtocolMethodSig};
use super::resolve::resolve_type_expr;

fn err(msg: impl Into<String>, hint: impl Into<String>, ctx: impl Into<String>) -> Diagnostic {
    Diagnostic::error(msg, hint, ctx)
}

/// Resolve an AST type expression using the current type environment.
fn resolve(env: &TypeEnv, te: &ast::TypeExpr) -> Ty {
    resolve_type_expr(te, Some(&env.types))
}

/// Infer type from a literal expression (for top-level `let` without annotation).
pub fn infer_literal_type(expr: &ast::Expr) -> Ty {
    match &expr.kind {
        ast::ExprKind::Int { .. } => Ty::Int,
        ast::ExprKind::Float { .. } => Ty::Float,
        ast::ExprKind::String { .. } => Ty::String,
        ast::ExprKind::Bool { .. } => Ty::Bool,
        ast::ExprKind::Unit => Ty::Unit,
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

/// Collect protocol bounds from generic params: TypeVar name → list of protocol names.
pub fn collect_protocol_bounds(generics: &Option<Vec<ast::GenericParam>>) -> HashMap<Sym, Vec<Sym>> {
    let mut pb = HashMap::new();
    let gs = match generics { Some(gs) => gs, None => return pb };
    for g in gs {
        if let Some(bounds) = &g.bounds {
            if !bounds.is_empty() {
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
    for gn in &gnames { env.types.insert(*gn, Ty::TypeVar(*gn)); }
    let ptys: Vec<(Sym, Ty)> = params.iter().map(|p| (sym(&p.name), resolve(env, &p.ty))).collect();
    let ret = resolve(env, return_type);
    for gn in &gnames { env.types.remove(gn); }
    let is_effect = effect.unwrap_or(false) || r#async.unwrap_or(false);
    let key = prefixed_key(prefix, name);
    if prefix.is_none() && is_effect { env.effect_fns.insert(sym(name)); }
    let min_p = params.iter().take_while(|p| p.default.is_none()).count();
    env.functions.insert(sym(&key), FnSig { params: ptys, ret, is_effect, generics: gnames, structural_bounds: sb, protocol_bounds: pb });
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

pub fn register_derive_sigs(env: &mut TypeEnv, derives: &[Sym], type_name: &str) {
    let type_ty = Ty::Named(sym(type_name), vec![]);
    let value_ty = Ty::Named(sym("Value"), vec![]);
    let empty_sb: HashMap<Sym, Ty> = HashMap::new();
    let empty_pb: HashMap<Sym, Vec<Sym>> = HashMap::new();
    for d in derives {
        match d.as_str() {
            "Eq" => {
                let fn_key = format!("{}.eq", type_name);
                if !env.functions.contains_key(&sym(&fn_key)) {
                    env.functions.insert(sym(&fn_key), FnSig { params: vec![("a".into(), type_ty.clone()), ("b".into(), type_ty.clone())], ret: Ty::Bool, is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone() });
                }
            }
            "Repr" => {
                let fn_key = format!("{}.repr", type_name);
                if !env.functions.contains_key(&sym(&fn_key)) {
                    env.functions.insert(sym(&fn_key), FnSig { params: vec![("v".into(), type_ty.clone())], ret: Ty::String, is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone() });
                }
            }
            "Codec" => {
                let encode_key = format!("{}.encode", type_name);
                if !env.functions.contains_key(&sym(&encode_key)) {
                    env.functions.insert(sym(&encode_key), FnSig { params: vec![("v".into(), type_ty.clone())], ret: value_ty.clone(), is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone() });
                }
                let decode_key = format!("{}.decode", type_name);
                if !env.functions.contains_key(&sym(&decode_key)) {
                    env.functions.insert(sym(&decode_key), FnSig { params: vec![("v".into(), value_ty.clone())], ret: Ty::result(type_ty.clone(), Ty::String), is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone() });
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
                        "Eq" | "Repr" | "Ord" | "Hash" | "Codec" | "Encode" | "Decode");
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
                       generics: &Option<Vec<ast::GenericParam>>, prefix: Option<&str>) {
    if let Some(derives) = deriving {
        validate_protocols(env, diagnostics, derives, name);
    }
    let gnames: Vec<Sym> = generics.as_ref().map(|gs| gs.iter().map(|g| sym(&g.name)).collect()).unwrap_or_default();
    for gn in &gnames { env.types.insert(*gn, Ty::TypeVar(*gn)); }
    let mut resolved = resolve(env, ty);
    for gn in &gnames { env.types.remove(gn); }
    if let Ty::Variant { name: ref mut vn, ref cases } = resolved {
        *vn = sym(name);
        for case in cases { env.constructors.insert(case.name, (sym(name), case.clone())); }
    }
    let key = prefixed_key(prefix, name);
    env.types.insert(sym(&key), resolved.clone());
    if prefix.is_some() {
        env.types.insert(sym(name), resolved);
    }
    if let Some(derives) = deriving {
        register_derive_sigs(env, derives, name);
    }
}

/// Walk all declarations and register them into the type environment.
pub fn register_decls(env: &mut TypeEnv, diagnostics: &mut Vec<Diagnostic>, decls: &[ast::Decl], prefix: Option<&str>) {
    for decl in decls {
        match decl {
            ast::Decl::Fn { name, params, return_type, effect, r#async, generics, span, visibility, .. } => {
                register_fn_sig(env, name, params, return_type, effect, r#async, generics, prefix, span.as_ref(), *visibility);
            }
            ast::Decl::Type { name, ty, deriving, generics, .. } => {
                register_type_decl(env, diagnostics, name, ty, deriving, generics, prefix);
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
                let rt = ty.as_ref().map(|te| resolve(env, te)).unwrap_or_else(|| infer_literal_type(value));
                let key = prefixed_key(prefix, name);
                env.top_lets.insert(sym(&key), rt);
            }
            _ => {}
        }
    }
    validate_protocol_impls(env, diagnostics);
}
