/// Call type checking — resolves function calls, builtins, variant constructors.

use std::collections::HashMap;
use crate::ast;
use crate::types::Ty;
use super::types::InferTy;
use super::Checker;

impl Checker {
    pub(crate) fn check_call(&mut self, callee: &mut ast::Expr, args: &mut [ast::Expr]) -> InferTy {
        let arg_tys: Vec<InferTy> = args.iter_mut().map(|a| self.infer_expr(a)).collect();
        match callee {
            ast::Expr::Ident { name, .. } => self.check_named_call(name, &arg_tys),
            ast::Expr::TypeName { name, .. } => {
                if let Some((type_name, _)) = self.env.constructors.get(name).cloned() {
                    InferTy::Concrete(Ty::Named(type_name, vec![]))
                } else { InferTy::Concrete(Ty::Named(name.clone(), vec![])) }
            }
            _ => {
                let ct = self.infer_expr(callee);
                let ret = self.fresh_var();
                self.constrain(ct, InferTy::Fn { params: arg_tys, ret: Box::new(ret.clone()) }, "function call");
                ret
            }
        }
    }

    fn check_named_call(&mut self, name: &str, arg_tys: &[InferTy]) -> InferTy {
        match name {
            "println" | "eprintln" | "assert" | "assert_eq" | "assert_ne" => InferTy::Concrete(Ty::Unit),
            "ok" => {
                let ok_ty = arg_tys.first().cloned().unwrap_or(InferTy::Concrete(Ty::Unit));
                let err_ty = match &self.env.current_ret {
                    Some(Ty::Result(_, e)) => InferTy::from_ty(e),
                    _ => InferTy::Concrete(Ty::String),
                };
                InferTy::Result(Box::new(ok_ty), Box::new(err_ty))
            }
            "err" => {
                let err_ty = arg_tys.first().cloned().unwrap_or(InferTy::Concrete(Ty::String));
                let ok_ty = match &self.env.current_ret {
                    Some(Ty::Result(o, _)) => InferTy::from_ty(o),
                    _ => InferTy::Concrete(Ty::Unit),
                };
                InferTy::Result(Box::new(ok_ty), Box::new(err_ty))
            }
            "some" => InferTy::Option(Box::new(arg_tys.first().cloned().unwrap_or(self.fresh_var()))),
            "unwrap_or" if arg_tys.len() >= 2 => {
                let concrete = arg_tys[0].to_ty(&self.solutions);
                match &concrete {
                    Ty::Option(inner) => InferTy::from_ty(inner),
                    Ty::Result(ok, _) => InferTy::from_ty(ok),
                    _ => arg_tys[1].clone(),
                }
            }
            _ => {
                if let Some(sig) = self.env.functions.get(name).cloned() {
                    let mut bindings = HashMap::new();
                    let concrete_args: Vec<Ty> = arg_tys.iter().map(|a| a.to_ty(&self.solutions)).collect();
                    for ((_, pty), aty) in sig.params.iter().zip(concrete_args.iter()) {
                        if let Ty::TypeVar(tv) = pty {
                            if let Some(bound) = sig.structural_bounds.get(tv) {
                                let resolved = self.env.resolve_named(aty);
                                if bound.compatible(&resolved) || bound.compatible(aty) { bindings.insert(tv.clone(), aty.clone()); }
                            } else { crate::types::unify(pty, aty, &mut bindings); }
                        } else { crate::types::unify(pty, aty, &mut bindings); }
                    }
                    let ret = if bindings.is_empty() { sig.ret.clone() } else { crate::types::substitute(&sig.ret, &bindings) };
                    InferTy::from_ty(&ret)
                } else if let Some((type_name, _)) = self.env.constructors.get(name).cloned() {
                    InferTy::Concrete(Ty::Named(type_name, vec![]))
                } else if let Some(ty) = self.env.lookup_var(name).cloned() {
                    match &ty { Ty::Fn { ret, .. } => InferTy::from_ty(ret), _ => InferTy::from_ty(&ty) }
                } else {
                    self.diagnostics.push(super::err(format!("undefined function '{}'", name), "Check the function name", format!("call to {}()", name)));
                    InferTy::Concrete(Ty::Unknown)
                }
            }
        }
    }
}
