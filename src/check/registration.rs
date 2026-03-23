/// Declaration registration: collecting function signatures, type declarations,
/// protocol declarations, and protocol validation into the type environment.

use std::collections::HashMap;
use crate::ast;
use crate::types::{Ty, FnSig, ProtocolDef, ProtocolMethodSig};
use super::Checker;
use super::err;

impl Checker {
    /// Collect structural bounds from generic params: Record → OpenRecord conversion.
    pub(super) fn collect_structural_bounds(&self, generics: &Option<Vec<ast::GenericParam>>) -> HashMap<String, Ty> {
        let mut sb = HashMap::new();
        let gs = match generics { Some(gs) => gs, None => return sb };
        for g in gs {
            let bte = match &g.structural_bound { Some(bte) => bte, None => continue };
            let bt = self.resolve_type_expr(bte);
            sb.insert(g.name.clone(), match bt { Ty::Record { fields } => Ty::OpenRecord { fields }, o => o });
        }
        sb
    }

    /// Build a prefixed key: "module.name" or just "name".
    pub(super) fn prefixed_key(prefix: Option<&str>, name: &str) -> String {
        prefix.map(|p| format!("{}.{}", p, name)).unwrap_or_else(|| name.to_string())
    }

    /// Collect protocol bounds from generic params: TypeVar name → list of protocol names.
    pub(super) fn collect_protocol_bounds(&self, generics: &Option<Vec<ast::GenericParam>>) -> HashMap<String, Vec<String>> {
        let mut pb = HashMap::new();
        let gs = match generics { Some(gs) => gs, None => return pb };
        for g in gs {
            if let Some(bounds) = &g.bounds {
                if !bounds.is_empty() {
                    pb.insert(g.name.clone(), bounds.clone());
                }
            }
        }
        pb
    }

    pub(super) fn register_fn_sig(&mut self, name: &str, params: &[ast::Param], return_type: &ast::TypeExpr,
                        effect: &Option<bool>, r#async: &Option<bool>, generics: &Option<Vec<ast::GenericParam>>, prefix: Option<&str>) {
        let gnames: Vec<String> = generics.as_ref().map(|gs| gs.iter().map(|g| g.name.clone()).collect()).unwrap_or_default();
        let sb = self.collect_structural_bounds(generics);
        let pb = self.collect_protocol_bounds(generics);
        for gn in &gnames { self.env.types.insert(gn.clone(), Ty::TypeVar(gn.clone())); }
        let ptys: Vec<(String, Ty)> = params.iter().map(|p| (p.name.clone(), self.resolve_type_expr(&p.ty))).collect();
        let ret = self.resolve_type_expr(return_type);
        for gn in &gnames { self.env.types.remove(gn); }
        let is_effect = effect.unwrap_or(false) || r#async.unwrap_or(false);
        let key = Self::prefixed_key(prefix, name);
        if prefix.is_none() && is_effect { self.env.effect_fns.insert(name.to_string()); }
        let min_p = params.iter().take_while(|p| p.default.is_none()).count();
        self.env.functions.insert(key.clone(), FnSig { params: ptys, ret, is_effect, generics: gnames, structural_bounds: sb, protocol_bounds: pb });
        if min_p < params.len() {
            self.env.fn_min_params.insert(key, min_p);
        }
    }

    pub(super) fn validate_protocols(&mut self, derives: &[String], type_name: &str) {
        for d in derives {
            if !self.env.protocols.contains_key(d.as_str()) {
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
        let type_ty = Ty::Named(type_name.to_string(), vec![]);
        let value_ty = Ty::Named("Value".to_string(), vec![]);
        let empty_sb = std::collections::HashMap::new();
        let empty_pb = std::collections::HashMap::new();
        for d in derives {
            match d.as_str() {
                "Eq" => {
                    let fn_key = format!("{}.eq", type_name);
                    if !self.env.functions.contains_key(&fn_key) {
                        self.env.functions.insert(fn_key, FnSig { params: vec![("a".into(), type_ty.clone()), ("b".into(), type_ty.clone())], ret: Ty::Bool, is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone() });
                    }
                }
                "Repr" => {
                    let fn_key = format!("{}.repr", type_name);
                    if !self.env.functions.contains_key(&fn_key) {
                        self.env.functions.insert(fn_key, FnSig { params: vec![("v".into(), type_ty.clone())], ret: Ty::String, is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone() });
                    }
                }
                "Codec" => {
                    let encode_key = format!("{}.encode", type_name);
                    if !self.env.functions.contains_key(&encode_key) {
                        self.env.functions.insert(encode_key, FnSig { params: vec![("v".into(), type_ty.clone())], ret: value_ty.clone(), is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone() });
                    }
                    let decode_key = format!("{}.decode", type_name);
                    if !self.env.functions.contains_key(&decode_key) {
                        self.env.functions.insert(decode_key, FnSig { params: vec![("v".into(), value_ty.clone())], ret: Ty::result(type_ty.clone(), Ty::String), is_effect: false, generics: vec![], structural_bounds: empty_sb.clone(), protocol_bounds: empty_pb.clone() });
                    }
                }
                _ => {}
            }
        }
    }

    /// Register a user-defined protocol declaration into env.protocols.
    pub(super) fn register_protocol_decl(&mut self, name: &str, generics: &Option<Vec<ast::GenericParam>>, methods: &[ast::ProtocolMethod]) {
        let gnames: Vec<String> = generics.as_ref()
            .map(|gs| gs.iter().map(|g| g.name.clone()).collect())
            .unwrap_or_default();

        // Temporarily register `Self` as a TypeVar so resolve_type_expr handles it
        self.env.types.insert("Self".to_string(), Ty::TypeVar("Self".to_string()));
        for gn in &gnames {
            self.env.types.insert(gn.clone(), Ty::TypeVar(gn.clone()));
        }

        let method_sigs: Vec<ProtocolMethodSig> = methods.iter().map(|m| {
            let params: Vec<(String, Ty)> = m.params.iter()
                .map(|p| (p.name.clone(), self.resolve_type_expr(&p.ty)))
                .collect();
            let ret = self.resolve_type_expr(&m.return_type);
            ProtocolMethodSig {
                name: m.name.clone(),
                params,
                ret,
                is_effect: m.effect,
            }
        }).collect();

        self.env.types.remove("Self");
        for gn in &gnames {
            self.env.types.remove(gn);
        }

        self.env.protocols.insert(name.to_string(), ProtocolDef {
            name: name.to_string(),
            generics: gnames,
            methods: method_sigs,
        });
    }

    /// Validate that types declaring `: ProtocolName` have all required convention methods.
    /// Called after all declarations are registered so all `Type.method` functions are available.
    pub(super) fn validate_protocol_impls(&mut self) {
        // Snapshot type_protocols to avoid borrow conflict
        let type_protocols: Vec<(String, Vec<String>)> = self.env.type_protocols.iter()
            .map(|(ty, protos)| (ty.clone(), protos.iter().cloned().collect()))
            .collect();

        for (type_name, protocol_names) in &type_protocols {
            for proto_name in protocol_names {
                let proto_def = match self.env.protocols.get(proto_name) {
                    Some(p) => p.clone(),
                    None => continue, // Unknown protocol — already reported by validate_protocols
                };

                for method_sig in &proto_def.methods {
                    let fn_key = format!("{}.{}", type_name, method_sig.name);
                    if !self.env.functions.contains_key(&fn_key) {
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
                                            let display_ty = if *t == Ty::TypeVar("Self".to_string()) {
                                                type_name.clone()
                                            } else {
                                                t.display()
                                            };
                                            format!("{}: {}", n, display_ty)
                                        })
                                        .collect::<Vec<_>>()
                                        .join(", "),
                                    {
                                        let ret = &method_sig.ret;
                                        if *ret == Ty::TypeVar("Self".to_string()) {
                                            type_name.clone()
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

    pub(super) fn register_type_decl(&mut self, name: &str, ty: &ast::TypeExpr, deriving: &Option<Vec<String>>,
                           generics: &Option<Vec<ast::GenericParam>>, prefix: Option<&str>) {
        if let Some(derives) = deriving {
            self.validate_protocols(derives, name);
        }
        let gnames: Vec<String> = generics.as_ref().map(|gs| gs.iter().map(|g| g.name.clone()).collect()).unwrap_or_default();
        for gn in &gnames { self.env.types.insert(gn.clone(), Ty::TypeVar(gn.clone())); }
        let mut resolved = self.resolve_type_expr(ty);
        for gn in &gnames { self.env.types.remove(gn); }
        if prefix.is_none() {
            if let Ty::Variant { name: ref mut vn, ref cases } = resolved {
                *vn = name.to_string();
                for case in cases { self.env.constructors.insert(case.name.clone(), (name.to_string(), case.clone())); }
            }
        }
        let key = Self::prefixed_key(prefix, name);
        self.env.types.insert(key.clone(), resolved.clone());
        // Also register with unprefixed name so field resolution works
        // (e.g., g.words where g: KeywordGroup from a module)
        if prefix.is_some() {
            self.env.types.insert(name.to_string(), resolved);
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
                                .entry(name.clone())
                                .or_insert_with(std::collections::HashSet::new)
                                .insert(d.clone());
                        }
                    }
                }
                ast::Decl::Protocol { name, generics, methods, .. } => {
                    self.register_protocol_decl(name, generics, methods);
                }
                ast::Decl::TopLet { name, ty, value, .. } => {
                    let rt = ty.as_ref().map(|te| self.resolve_type_expr(te)).unwrap_or_else(|| self.infer_literal_type(value));
                    let key = Self::prefixed_key(prefix, name);
                    self.env.top_lets.insert(key, rt);
                }
                _ => {}
            }
        }
        // After all decls are registered, validate protocol implementations
        self.validate_protocol_impls();
    }
}
