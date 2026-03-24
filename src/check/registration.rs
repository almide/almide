/// Declaration registration: collecting function signatures, type declarations,
/// protocol declarations, and protocol validation into the type environment.

use std::collections::HashMap;
use crate::ast;
use crate::intern::{Sym, sym};
use crate::types::{Ty, FnSig, ProtocolDef, ProtocolMethodSig};
use super::Checker;
use super::err;

impl Checker {
    /// Collect structural bounds from generic params: Record → OpenRecord conversion.
    pub(super) fn collect_structural_bounds(&self, generics: &Option<Vec<ast::GenericParam>>) -> HashMap<Sym, Ty> {
        let mut sb = HashMap::new();
        let gs = match generics { Some(gs) => gs, None => return sb };
        for g in gs {
            let bte = match &g.structural_bound { Some(bte) => bte, None => continue };
            let bt = self.resolve_type_expr(bte);
            sb.insert(sym(&g.name), match bt { Ty::Record { fields } => Ty::OpenRecord { fields }, o => o });
        }
        sb
    }

    /// Build a prefixed key: "module.name" or just "name".
    pub(super) fn prefixed_key(prefix: Option<&str>, name: &str) -> String {
        prefix.map(|p| format!("{}.{}", p, name)).unwrap_or_else(|| name.to_string())
    }

    /// Collect protocol bounds from generic params: TypeVar name → list of protocol names.
    pub(super) fn collect_protocol_bounds(&self, generics: &Option<Vec<ast::GenericParam>>) -> HashMap<Sym, Vec<Sym>> {
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

    pub(super) fn register_fn_sig(&mut self, name: &str, params: &[ast::Param], return_type: &ast::TypeExpr,
                        effect: &Option<bool>, r#async: &Option<bool>, generics: &Option<Vec<ast::GenericParam>>, prefix: Option<&str>) {
        let gnames: Vec<Sym> = generics.as_ref().map(|gs| gs.iter().map(|g| sym(&g.name)).collect()).unwrap_or_default();
        let sb = self.collect_structural_bounds(generics);
        let pb = self.collect_protocol_bounds(generics);
        for gn in &gnames { self.env.types.insert(*gn, Ty::TypeVar(*gn)); }
        let ptys: Vec<(Sym, Ty)> = params.iter().map(|p| (sym(&p.name), self.resolve_type_expr(&p.ty))).collect();
        let ret = self.resolve_type_expr(return_type);
        for gn in &gnames { self.env.types.remove(gn); }
        let is_effect = effect.unwrap_or(false) || r#async.unwrap_or(false);
        let key = Self::prefixed_key(prefix, name);
        if prefix.is_none() && is_effect { self.env.effect_fns.insert(sym(name)); }
        let min_p = params.iter().take_while(|p| p.default.is_none()).count();
        self.env.functions.insert(sym(&key), FnSig { params: ptys, ret, is_effect, generics: gnames, structural_bounds: sb, protocol_bounds: pb });
        if min_p < params.len() {
            self.env.fn_min_params.insert(sym(&key), min_p);
        }
    }

    pub(super) fn validate_protocols(&mut self, derives: &[String], type_name: &str) {
        for d in derives {
            if !self.env.protocols.contains_key(&sym(d)) {
                let valid: Vec<&str> = self.env.protocols.keys().map(|s| s.as_str()).collect();
                self.emit(err(
                    format!("unknown protocol '{}' on type '{}'", d, type_name),
                    format!("Known protocols: {}", {
                        let mut sorted = valid; sorted.sort(); sorted.join(", ")
                    }),
                    format!("type {}", type_name),
                ));
            }
        }
    }

    pub(super) fn register_derive_sigs(&mut self, derives: &[String], type_name: &str) {
        let type_ty = Ty::Named(sym(type_name), vec![]);
        let value_ty = Ty::Named(sym("Value"), vec![]);
        let empty_sb: HashMap<Sym, Ty> = HashMap::new();
        let empty_pb: HashMap<Sym, Vec<Sym>> = HashMap::new();
        for d in derives {
            match d.as_str() {
                "Eq" => {
                    let fn_key = format!("{}.eq", type_name);
                    if !self.env.functions.contains_key(&sym(&fn_key)) {
                        self.env.functions.insert(sym(&fn_key), FnSig { params: vec![("a".into(), type_ty.clone()), ("b".into(), type_ty.clone())], ret: Ty::Bool, is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone() });
                    }
                }
                "Repr" => {
                    let fn_key = format!("{}.repr", type_name);
                    if !self.env.functions.contains_key(&sym(&fn_key)) {
                        self.env.functions.insert(sym(&fn_key), FnSig { params: vec![("v".into(), type_ty.clone())], ret: Ty::String, is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone() });
                    }
                }
                "Codec" => {
                    let encode_key = format!("{}.encode", type_name);
                    if !self.env.functions.contains_key(&sym(&encode_key)) {
                        self.env.functions.insert(sym(&encode_key), FnSig { params: vec![("v".into(), type_ty.clone())], ret: value_ty.clone(), is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone() });
                    }
                    let decode_key = format!("{}.decode", type_name);
                    if !self.env.functions.contains_key(&sym(&decode_key)) {
                        self.env.functions.insert(sym(&decode_key), FnSig { params: vec![("v".into(), value_ty.clone())], ret: Ty::result(type_ty.clone(), Ty::String), is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone() });
                    }
                }
                _ => {}
            }
        }
    }

    /// Register a user-defined protocol declaration into env.protocols.
    pub(super) fn register_protocol_decl(&mut self, name: &str, generics: &Option<Vec<ast::GenericParam>>, methods: &[ast::ProtocolMethod]) {
        let gnames: Vec<Sym> = generics.as_ref()
            .map(|gs| gs.iter().map(|g| sym(&g.name)).collect())
            .unwrap_or_default();

        // Temporarily register `Self` as a TypeVar so resolve_type_expr handles it
        self.env.types.insert(sym("Self"), Ty::TypeVar(sym("Self")));
        for gn in &gnames {
            self.env.types.insert(*gn, Ty::TypeVar(*gn));
        }

        let method_sigs: Vec<ProtocolMethodSig> = methods.iter().map(|m| {
            let params: Vec<(Sym, Ty)> = m.params.iter()
                .map(|p| (sym(&p.name), self.resolve_type_expr(&p.ty)))
                .collect();
            let ret = self.resolve_type_expr(&m.return_type);
            ProtocolMethodSig {
                name: sym(&m.name),
                params,
                ret,
                is_effect: m.effect,
            }
        }).collect();

        self.env.types.remove(&sym("Self"));
        for gn in &gnames {
            self.env.types.remove(gn);
        }

        self.env.protocols.insert(sym(name), ProtocolDef {
            name: sym(name),
            generics: gnames,
            methods: method_sigs,
        });
    }

    /// Validate that types declaring `: ProtocolName` have all required convention methods.
    /// Called after all declarations are registered so all `Type.method` functions are available.
    pub(super) fn validate_protocol_impls(&mut self) {
        // Snapshot type_protocols to avoid borrow conflict
        let type_protocols: Vec<(Sym, Vec<Sym>)> = self.env.type_protocols.iter()
            .map(|(ty, protos)| (*ty, protos.iter().copied().collect()))
            .collect();

        for (type_name, protocol_names) in &type_protocols {
            for proto_name in protocol_names {
                // Skip if already validated via impl block
                if self.env.impl_validated.contains(&(*type_name, *proto_name)) {
                    continue;
                }
                let proto_def = match self.env.protocols.get(proto_name) {
                    Some(p) => p.clone(),
                    None => continue, // Unknown protocol — already reported by validate_protocols
                };

                for method_sig in &proto_def.methods {
                    let fn_key = format!("{}.{}", type_name, method_sig.name);
                    if !self.env.functions.contains_key(&sym(&fn_key)) {
                        // Only emit error for user-defined protocols (not built-in ones
                        // which may be auto-derived)
                        let is_builtin = matches!(proto_name.as_str(),
                            "Eq" | "Repr" | "Ord" | "Hash" | "Codec" | "Encode" | "Decode");
                        if !is_builtin {
                            self.emit(err(
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

    /// Register an `impl Protocol for Type { ... }` block:
    /// 1. Validate that the protocol exists
    /// 2. Register each method as `Type.method` in env.functions
    /// 3. Validate method signatures match the protocol definition
    /// 4. Track the conformance in type_protocols
    pub(super) fn register_impl_decl(&mut self, trait_name: &str, for_type: &str, methods: &[ast::Decl]) {
        // 1. Validate protocol exists
        let proto_def = match self.env.protocols.get(&sym(trait_name)) {
            Some(p) => p.clone(),
            None => {
                let valid: Vec<&str> = self.env.protocols.keys().map(|s| s.as_str()).collect();
                self.emit(err(
                    format!("unknown protocol '{}' in impl block", trait_name),
                    format!("Known protocols: {}", {
                        let mut sorted = valid; sorted.sort(); sorted.join(", ")
                    }),
                    format!("impl {} for {}", trait_name, for_type),
                ));
                // Still register methods as convention functions so downstream doesn't break
                for m in methods {
                    if let ast::Decl::Fn { name, params, return_type, effect, r#async, generics, .. } = m {
                        self.register_fn_sig(name, params, return_type, effect, r#async, generics, Some(for_type));
                    }
                }
                return;
            }
        };

        // 2. Register each method as Type.method and validate signature
        let type_ty = Ty::Named(sym(for_type), vec![]);
        let mut impl_methods: std::collections::HashSet<String> = std::collections::HashSet::new();

        for m in methods {
            if let ast::Decl::Fn { name, params, return_type, effect, r#async, generics, .. } = m {
                // Register as convention function: Type.method
                self.register_fn_sig(name, params, return_type, effect, r#async, generics, Some(for_type));
                impl_methods.insert(name.clone());

                // 3. Validate signature matches protocol definition
                if let Some(proto_method) = proto_def.methods.iter().find(|pm| pm.name == *name) {
                    // Resolve the impl method's types for comparison
                    let gnames: Vec<Sym> = generics.as_ref().map(|gs| gs.iter().map(|g| sym(&g.name)).collect()).unwrap_or_default();
                    for gn in &gnames { self.env.types.insert(*gn, Ty::TypeVar(*gn)); }
                    let impl_params: Vec<Ty> = params.iter().map(|p| self.resolve_type_expr(&p.ty)).collect();
                    let impl_ret = self.resolve_type_expr(return_type);
                    for gn in &gnames { self.env.types.remove(gn); }

                    // Substitute Self → Type in protocol method signature,
                    // then resolve Named types to structural types for comparison
                    let expected_params: Vec<Ty> = proto_method.params.iter()
                        .map(|(_, ty)| {
                            let substituted = self.substitute_self(ty, &type_ty);
                            self.env.resolve_named(&substituted)
                        })
                        .collect();
                    let expected_ret = self.env.resolve_named(&self.substitute_self(&proto_method.ret, &type_ty));

                    // Compare param count
                    if impl_params.len() != expected_params.len() {
                        self.emit(err(
                            format!("method '{}' in impl {} for {} has {} parameter(s), expected {}",
                                name, trait_name, for_type, impl_params.len(), expected_params.len()),
                            format!("Protocol '{}' defines: fn {}({})", trait_name, name,
                                proto_method.params.iter().map(|(n, t)| {
                                    let display_ty = self.substitute_self(t, &type_ty).display();
                                    format!("{}: {}", n, display_ty)
                                }).collect::<Vec<_>>().join(", ")),
                            format!("impl {} for {}", trait_name, for_type),
                        ));
                    } else {
                        // Compare param types (both sides already resolved to structural types)
                        for (i, (impl_ty, expected_ty)) in impl_params.iter().zip(expected_params.iter()).enumerate() {
                            if impl_ty != expected_ty && *expected_ty != Ty::Unknown && *impl_ty != Ty::Unknown {
                                let param_name = &params[i].name;
                                self.emit(err(
                                    format!("method '{}.{}' parameter '{}' has type '{}', expected '{}'",
                                        for_type, name, param_name, impl_ty.display(), expected_ty.display()),
                                    format!("Change type to '{}'", expected_ty.display()),
                                    format!("impl {} for {}", trait_name, for_type),
                                ));
                            }
                        }
                        // Compare return type
                        if impl_ret != expected_ret && expected_ret != Ty::Unknown && impl_ret != Ty::Unknown {
                            self.emit(err(
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
                self.emit(err(
                    format!("impl {} for {} is missing method '{}'", trait_name, for_type, proto_method.name),
                    format!("Add: fn {}({}) -> {}", proto_method.name,
                        proto_method.params.iter().map(|(n, t)| {
                            let display_ty = self.substitute_self(t, &type_ty).display();
                            format!("{}: {}", n, display_ty)
                        }).collect::<Vec<_>>().join(", "),
                        self.substitute_self(&proto_method.ret, &type_ty).display()),
                    format!("impl {} for {}", trait_name, for_type),
                ));
            }
        }

        // 5. Track protocol conformance + mark as impl-validated
        self.env.type_protocols
            .entry(sym(for_type))
            .or_insert_with(std::collections::HashSet::new)
            .insert(sym(trait_name));
        self.env.impl_validated.insert((sym(for_type), sym(trait_name)));
    }

    /// Substitute Self → concrete type in a protocol method type.
    fn substitute_self(&self, ty: &Ty, replacement: &Ty) -> Ty {
        match ty {
            Ty::TypeVar(name) if name == "Self" => replacement.clone(),
            _ => ty.map_children(&|child| self.substitute_self(child, replacement)),
        }
    }

    pub(super) fn register_type_decl(&mut self, name: &str, ty: &ast::TypeExpr, deriving: &Option<Vec<String>>,
                           generics: &Option<Vec<ast::GenericParam>>, prefix: Option<&str>) {
        if let Some(derives) = deriving {
            self.validate_protocols(derives, name);
        }
        let gnames: Vec<Sym> = generics.as_ref().map(|gs| gs.iter().map(|g| sym(&g.name)).collect()).unwrap_or_default();
        for gn in &gnames { self.env.types.insert(*gn, Ty::TypeVar(*gn)); }
        let mut resolved = self.resolve_type_expr(ty);
        for gn in &gnames { self.env.types.remove(gn); }
        if prefix.is_none() {
            if let Ty::Variant { name: ref mut vn, ref cases } = resolved {
                *vn = sym(name);
                for case in cases { self.env.constructors.insert(case.name, (sym(name), case.clone())); }
            }
        }
        let key = Self::prefixed_key(prefix, name);
        self.env.types.insert(sym(&key), resolved.clone());
        // Also register with unprefixed name so field resolution works
        // (e.g., g.words where g: KeywordGroup from a module)
        if prefix.is_some() {
            self.env.types.insert(sym(name), resolved);
        }
        if let Some(derives) = deriving {
            self.register_derive_sigs(derives, name);
        }
    }

    pub(super) fn register_decls(&mut self, decls: &[ast::Decl], prefix: Option<&str>) {
        for decl in decls {
            match decl {
                ast::Decl::Fn { name, params, return_type, effect, r#async, generics, .. } => {
                    self.register_fn_sig(name, params, return_type, effect, r#async, generics, prefix);
                }
                ast::Decl::Type { name, ty, deriving, generics, .. } => {
                    self.register_type_decl(name, ty, deriving, generics, prefix);
                    // Track protocol conformances declared via `: ProtocolName`
                    if let Some(derives) = deriving {
                        for d in derives {
                            self.env.type_protocols
                                .entry(sym(name))
                                .or_insert_with(std::collections::HashSet::new)
                                .insert(sym(d));
                        }
                    }
                }
                ast::Decl::Protocol { name, generics, methods, .. } => {
                    self.register_protocol_decl(name, generics, methods);
                }
                ast::Decl::Impl { trait_, for_, methods, .. } => {
                    self.register_impl_decl(trait_, for_, methods);
                }
                ast::Decl::TopLet { name, ty, value, .. } => {
                    let rt = ty.as_ref().map(|te| self.resolve_type_expr(te)).unwrap_or_else(|| self.infer_literal_type(value));
                    let key = Self::prefixed_key(prefix, name);
                    self.env.top_lets.insert(sym(&key), rt);
                }
                _ => {}
            }
        }
        // After all decls are registered, validate protocol implementations
        self.validate_protocol_impls();
    }
}
