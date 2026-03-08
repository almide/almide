/// Almide type checker — inserted between parser and emitter.
/// Every error includes an actionable hint so LLMs can auto-repair.

mod expressions;
mod calls;
mod operators;
mod statements;

use crate::ast;
use crate::diagnostic::Diagnostic;
use crate::stdlib;
use crate::types::{Ty, TypeEnv, FnSig, VariantCase, VariantPayload};

pub struct Checker {
    pub env: TypeEnv,
    pub diagnostics: Vec<Diagnostic>,
}

pub(crate) fn err(msg: String, hint: &str, ctx: &str) -> Diagnostic {
    Diagnostic::error(msg, hint, ctx)
}

pub(crate) fn err_s(msg: String, hint: String, ctx: String) -> Diagnostic {
    Diagnostic::error_s(msg, hint, ctx)
}

impl Checker {
    pub fn new() -> Self {
        let mut c = Checker {
            env: TypeEnv::new(),
            diagnostics: Vec::new(),
        };
        c.register_stdlib();
        c
    }

    /// Register function and type declarations into the environment.
    /// When `prefix` is Some, keys are prefixed (e.g. "module.func") for imported modules.
    /// When `prefix` is None, registers as local declarations with variant constructors and effect tracking.
    fn register_decls(&mut self, decls: &[ast::Decl], prefix: Option<&str>) {
        for decl in decls {
            match decl {
                ast::Decl::Fn { name, params, return_type, effect, r#async, .. } => {
                    let param_tys: Vec<(String, Ty)> = params.iter()
                        .map(|p| (p.name.clone(), self.resolve_type_expr(&p.ty)))
                        .collect();
                    let ret = self.resolve_type_expr(return_type);
                    let is_effect = effect.unwrap_or(false) || r#async.unwrap_or(false);
                    let key = match prefix {
                        Some(p) => format!("{}.{}", p, name),
                        None => name.clone(),
                    };
                    if prefix.is_none() && is_effect {
                        self.env.effect_fns.insert(name.clone());
                    }
                    self.env.functions.insert(key, FnSig { params: param_tys, ret, is_effect });
                }
                ast::Decl::Type { name, ty, .. } => {
                    let mut resolved = self.resolve_type_expr(ty);
                    if prefix.is_none() {
                        if let Ty::Variant { name: ref mut vname, ref cases } = resolved {
                            *vname = name.clone();
                            for case in cases {
                                self.env.constructors.insert(case.name.clone(), (name.clone(), case.clone()));
                            }
                        }
                    }
                    let key = match prefix {
                        Some(p) => format!("{}.{}", p, name),
                        None => name.clone(),
                    };
                    self.env.types.insert(key, resolved);
                }
                _ => {}
            }
        }
    }

    /// Register an imported module's exported functions and types.
    pub fn register_module(&mut self, mod_name: &str, prog: &ast::Program) {
        self.env.user_modules.insert(mod_name.to_string());
        self.register_decls(&prog.decls, Some(mod_name));
    }

    pub fn check_program(&mut self, prog: &ast::Program) -> Vec<Diagnostic> {
        self.register_decls(&prog.decls, None);
        for decl in &prog.decls {
            self.check_decl(decl);
        }
        self.diagnostics.clone()
    }

    pub(crate) fn check_decl(&mut self, decl: &ast::Decl) {
        match decl {
            ast::Decl::Fn { name, params, return_type, body, effect, .. } => {
                self.env.push_scope();
                for p in params {
                    let ty = self.resolve_type_expr(&p.ty);
                    self.env.define_var(&p.name, ty);
                }
                let ret_ty = self.resolve_type_expr(return_type);
                let prev_ret = self.env.current_ret.take();
                let prev_effect = self.env.in_effect;
                self.env.current_ret = Some(ret_ty.clone());
                self.env.in_effect = effect.unwrap_or(false);
                let body_ty = self.check_expr(body);
                let is_effect = effect.unwrap_or(false);
                let effective_ret = if is_effect {
                    match &ret_ty {
                        Ty::Result(ok_ty, _) => *ok_ty.clone(),
                        _ => ret_ty.clone(),
                    }
                } else {
                    ret_ty.clone()
                };
                if !body_ty.compatible(&effective_ret) && !body_ty.compatible(&ret_ty) {
                    self.diagnostics.push(err_s(
                        format!("function '{}' declared to return {} but body has type {}", name, ret_ty.display(), body_ty.display()),
                        "Change the return type or fix the body expression".into(),
                        format!("fn {}", name),
                    ));
                }
                self.env.current_ret = prev_ret;
                self.env.in_effect = prev_effect;
                self.env.pop_scope();
            }
            ast::Decl::Test { body, .. } => {
                self.env.push_scope();
                let prev = self.env.in_effect;
                self.env.in_effect = true;
                self.check_expr(body);
                self.env.in_effect = prev;
                self.env.pop_scope();
            }
            _ => {}
        }
    }

    pub(crate) fn resolve_type_expr(&self, te: &ast::TypeExpr) -> Ty {
        match te {
            ast::TypeExpr::Simple { name } => match name.as_str() {
                "Int" => Ty::Int, "Float" => Ty::Float, "String" => Ty::String,
                "Bool" => Ty::Bool, "Unit" => Ty::Unit, "Path" => Ty::String,
                other => Ty::Named(other.to_string()),
            },
            ast::TypeExpr::Generic { name, args } => {
                let ra: Vec<Ty> = args.iter().map(|a| self.resolve_type_expr(a)).collect();
                match name.as_str() {
                    "List" if ra.len() == 1 => Ty::List(Box::new(ra[0].clone())),
                    "Option" if ra.len() == 1 => Ty::Option(Box::new(ra[0].clone())),
                    "Result" if ra.len() == 2 => Ty::Result(Box::new(ra[0].clone()), Box::new(ra[1].clone())),
                    "Map" if ra.len() == 2 => Ty::Map(Box::new(ra[0].clone()), Box::new(ra[1].clone())),
                    "Set" => Ty::List(Box::new(ra.first().cloned().unwrap_or(Ty::Unknown))),
                    _ => Ty::Named(name.clone()),
                }
            }
            ast::TypeExpr::Record { fields } => Ty::Record {
                fields: fields.iter().map(|f| (f.name.clone(), self.resolve_type_expr(&f.ty))).collect(),
            },
            ast::TypeExpr::Fn { params, ret } => Ty::Fn {
                params: params.iter().map(|p| self.resolve_type_expr(p)).collect(),
                ret: Box::new(self.resolve_type_expr(ret)),
            },
            ast::TypeExpr::Newtype { inner } => self.resolve_type_expr(inner),
            ast::TypeExpr::Variant { cases } => {
                let cs: Vec<VariantCase> = cases.iter().map(|c| match c {
                    ast::VariantCase::Unit { name } => VariantCase { name: name.clone(), payload: VariantPayload::Unit },
                    ast::VariantCase::Tuple { name, fields } => VariantCase {
                        name: name.clone(),
                        payload: VariantPayload::Tuple(fields.iter().map(|f| self.resolve_type_expr(f)).collect()),
                    },
                    ast::VariantCase::Record { name, fields } => VariantCase {
                        name: name.clone(),
                        payload: VariantPayload::Record(fields.iter().map(|f| (f.name.clone(), self.resolve_type_expr(&f.ty))).collect()),
                    },
                }).collect();
                Ty::Variant { name: String::new(), cases: cs }
            }
        }
    }

    fn register_stdlib(&mut self) {
        for name in stdlib::builtin_effect_fns() {
            self.env.effect_fns.insert(name.to_string());
        }
    }
}
