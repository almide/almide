/// IR → TsIR lowering for declarations and types.
///
/// Input:    &IrTypeDecl, &IrFunction (declarations), Ty (types)
/// Output:   TsIR TypeDecl, Function, Type
/// Owns:     type lowering, declaration structure, variant constructor shapes
/// Does NOT: expression/statement lowering (lower_ts.rs)

use crate::ir::*;
use crate::types::Ty;
use super::ts_ir::*;
use super::lower_ts::LowerCtx;

impl<'a> LowerCtx<'a> {
    pub(super) fn lower_type_decl(&self, td: &IrTypeDecl) -> TypeDecl {
        let generics = if self.js_mode { vec![] } else {
            td.generics.as_ref().map(|gs| gs.iter().map(|g| g.name.clone()).collect()).unwrap_or_default()
        };
        match &td.kind {
            IrTypeDeclKind::Record { fields } => {
                let fs = fields.iter().map(|f| (f.name.clone(), lower_ty(&f.ty))).collect();
                TypeDecl::Interface { name: td.name.clone(), generics, fields: fs }
            }
            IrTypeDeclKind::Alias { target } => {
                if let Ty::OpenRecord { fields, .. } = target {
                    let fs = fields.iter().map(|(n, t)| (n.clone(), lower_ty(t))).collect();
                    TypeDecl::Interface { name: td.name.clone(), generics, fields: fs }
                } else {
                    TypeDecl::TypeAlias { name: td.name.clone(), generics, target: lower_ty(target) }
                }
            }
            IrTypeDeclKind::Variant { cases, is_generic, .. } => {
                let ctors = cases.iter().map(|c| VariantCtor {
                    name: c.name.clone(),
                    kind: match &c.kind {
                        IrVariantKind::Unit => {
                            if *is_generic { VariantCtorKind::GenericUnit } else { VariantCtorKind::Const }
                        }
                        IrVariantKind::Tuple { fields } => VariantCtorKind::TupleCtor { arity: fields.len() },
                        IrVariantKind::Record { fields } => VariantCtorKind::RecordCtor {
                            fields: fields.iter().map(|f| f.name.clone()).collect(),
                        },
                    },
                }).collect();
                TypeDecl::VariantCtors(ctors)
            }
        }
    }

    pub(super) fn lower_fn(&self, f: &IrFunction) -> Function {
        if let Some(ext) = f.extern_attrs.iter().find(|a| a.target == "ts") {
            let args: Vec<String> = f.params.iter().filter(|p| p.name != "self").map(|p| sanitize(&p.name)).collect();
            let call = Expr::Raw(format!("{}.{}({})", ext.module, ext.function, args.join(", ")));
            let params = self.lower_params(f);
            let ret = if self.js_mode { None } else { Some(lower_ty(&f.ret_ty)) };
            return Function {
                name: sanitize(&f.name), params, ret,
                body: FnBody::Block { stmts: vec![], tail: Some(call) },
                is_async: f.is_async, is_export: false,
            };
        }

        let params = self.lower_params(f);
        let ret = if self.js_mode { None } else { Some(lower_ty(&f.ret_ty)) };
        let body = self.lower_fn_body(&f.body, f.is_effect, false);
        Function {
            name: if f.name == "main" { "main".to_string() } else { sanitize(&f.name) },
            params, ret, body, is_async: f.is_async, is_export: false,
        }
    }

    pub(super) fn lower_params(&self, f: &IrFunction) -> Vec<Param> {
        f.params.iter().filter(|p| p.name != "self").map(|p| Param {
            name: sanitize(&p.name),
            ty: if self.js_mode { None } else { Some(lower_ty(&p.ty)) },
        }).collect()
    }

    pub(super) fn lower_test(&self, f: &IrFunction) -> Test {
        let body = self.lower_expr(&f.body, false, true);
        Test { name: f.name.clone(), body }
    }

    pub(super) fn lower_module(&self, m: &IrModule) -> Module {
        let type_decls = m.type_decls.iter().map(|td| self.lower_type_decl(td)).collect();
        let functions = m.functions.iter().map(|f| self.lower_fn(f)).collect();
        let exports = m.functions.iter()
            .filter(|f| f.visibility != IrVisibility::Private)
            .map(|f| sanitize(&f.name))
            .collect();
        Module { name: m.name.clone(), type_decls, functions, exports }
    }
}

// ── Type lowering (standalone) ───────────────────────────────────

pub fn lower_ty(ty: &Ty) -> Type {
    match ty {
        Ty::Int | Ty::Float => Type::Number,
        Ty::String => Type::String,
        Ty::Bool => Type::Boolean,
        Ty::Unit => Type::Void,
        Ty::List(inner) => Type::Array(Box::new(lower_ty(inner))),
        Ty::Map(k, v) => Type::Map(Box::new(lower_ty(k)), Box::new(lower_ty(v))),
        Ty::Option(inner) => Type::Nullable(Box::new(lower_ty(inner))),
        Ty::Result(ok, _) => lower_ty(ok),
        Ty::Tuple(elems) => Type::Tuple(elems.iter().map(|e| lower_ty(e)).collect()),
        Ty::Fn { params, ret } => {
            let ps = params.iter().enumerate().map(|(i, p)| (format!("_{}", i), lower_ty(p))).collect();
            Type::Fn { params: ps, ret: Box::new(lower_ty(ret)) }
        }
        Ty::Record { fields } | Ty::OpenRecord { fields, .. } => {
            Type::Object(fields.iter().map(|(n, t)| (n.clone(), lower_ty(t))).collect())
        }
        Ty::Named(name, _) => match name.as_str() { "Path" => Type::String, other => Type::Named(other.into()) },
        Ty::TypeVar(name) => Type::Named(name.clone()),
        Ty::Union(members) => Type::Union(members.iter().map(|m| lower_ty(m)).collect()),
        Ty::Variant { name, .. } => Type::Named(name.clone()),
        Ty::Unknown => Type::Any,
    }
}

// ── Name utilities ───────────────────────────────────────────────

pub(super) fn sanitize(name: &str) -> String {
    crate::emit_common::sanitize(name)
}

