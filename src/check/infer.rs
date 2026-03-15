/// Expression type inference — Pass 1 of the constraint-based checker.
/// Walks the AST, assigns InferTy to each expression, collects constraints.

use crate::ast;
use crate::types::{Ty, VariantPayload};
use super::types::InferTy;
use super::Checker;

impl Checker {
    pub(crate) fn infer_expr(&mut self, expr: &mut ast::Expr) -> InferTy {
        let ity = self.infer_expr_inner(expr);
        self.infer_types.insert(expr.id(), ity.clone());
        ity
    }

    fn infer_expr_inner(&mut self, expr: &mut ast::Expr) -> InferTy {
        match expr {
            ast::Expr::Int { .. } => InferTy::Concrete(Ty::Int),
            ast::Expr::Float { .. } => InferTy::Concrete(Ty::Float),
            ast::Expr::String { .. } | ast::Expr::InterpolatedString { .. } => InferTy::Concrete(Ty::String),
            ast::Expr::Bool { .. } => InferTy::Concrete(Ty::Bool),
            ast::Expr::Unit { .. } => InferTy::Concrete(Ty::Unit),

            ast::Expr::None { .. } => InferTy::Option(Box::new(self.fresh_var())),

            ast::Expr::Ident { name, .. } => {
                self.env.used_vars.insert(name.clone());
                if let Some(ty) = self.env.lookup_var(name).cloned() { InferTy::from_ty(&ty) }
                else if let Some(ty) = self.env.top_lets.get(name).cloned() { InferTy::from_ty(&ty) }
                else {
                    self.diagnostics.push(super::err(format!("undefined variable '{}'", name), "Check the variable name", format!("variable {}", name)));
                    InferTy::Concrete(Ty::Unknown)
                }
            }

            ast::Expr::TypeName { name, .. } => {
                if let Some((type_name, _)) = self.env.constructors.get(name) { InferTy::Concrete(Ty::Named(type_name.clone(), vec![])) }
                else if let Some(ty) = self.env.top_lets.get(name).cloned() { InferTy::from_ty(&ty) }
                else { InferTy::Concrete(Ty::Named(name.clone(), vec![])) }
            }

            ast::Expr::List { elements, .. } => {
                if elements.is_empty() { InferTy::List(Box::new(self.fresh_var())) }
                else {
                    let first = self.infer_expr(&mut elements[0]);
                    for elem in elements.iter_mut().skip(1) { let et = self.infer_expr(elem); self.constrain(first.clone(), et, "list element"); }
                    InferTy::List(Box::new(first))
                }
            }

            ast::Expr::Tuple { elements, .. } => InferTy::Tuple(elements.iter_mut().map(|e| self.infer_expr(e)).collect()),

            ast::Expr::Record { name, fields, .. } => {
                for f in fields.iter_mut() { self.infer_expr(&mut f.value); }
                if let Some(n) = name {
                    // Variant constructor → resolve to parent type name
                    let type_name = self.env.constructors.get(n.as_str())
                        .map(|(vname, _)| vname.clone())
                        .unwrap_or_else(|| n.clone());
                    InferTy::Concrete(Ty::Named(type_name, vec![]))
                }
                else {
                    let field_tys: Vec<(String, Ty)> = fields.iter().map(|f| {
                        let ty = self.infer_types.get(&f.value.id()).map(|it| it.to_ty(&self.solutions)).unwrap_or(Ty::Unknown);
                        (f.name.clone(), ty)
                    }).collect();
                    InferTy::Concrete(Ty::Record { fields: field_tys })
                }
            }

            ast::Expr::SpreadRecord { base, fields, .. } => {
                let base_ty = self.infer_expr(base);
                for f in fields.iter_mut() { self.infer_expr(&mut f.value); }
                base_ty
            }

            ast::Expr::Member { object, field, .. } => {
                let obj_ty = self.infer_expr(object);
                let concrete = obj_ty.to_ty(&self.solutions);
                InferTy::from_ty(&self.resolve_field_type(&concrete, field))
            }

            ast::Expr::TupleIndex { object, index, .. } => {
                let obj_ty = self.infer_expr(object);
                match &obj_ty {
                    InferTy::Tuple(elems) if *index < elems.len() => elems[*index].clone(),
                    _ => {
                        let concrete = obj_ty.to_ty(&self.solutions);
                        match &concrete { Ty::Tuple(elems) if *index < elems.len() => InferTy::from_ty(&elems[*index]), _ => InferTy::Concrete(Ty::Unknown) }
                    }
                }
            }

            ast::Expr::IndexAccess { object, index, .. } => {
                let obj_ty = self.infer_expr(object);
                self.infer_expr(index);
                let concrete = obj_ty.to_ty(&self.solutions);
                match &concrete {
                    Ty::List(inner) => InferTy::from_ty(inner),
                    Ty::Map(_, v) => InferTy::Option(Box::new(InferTy::from_ty(v))),
                    _ => InferTy::Concrete(Ty::Unknown),
                }
            }

            ast::Expr::Binary { op, left, right, .. } => {
                let lt = self.infer_expr(left);
                let rt = self.infer_expr(right);
                match op.as_str() {
                    "+" | "-" | "*" | "/" | "%" => {
                        let lc = lt.to_ty(&self.solutions);
                        let rc = rt.to_ty(&self.solutions);
                        let is_numeric = |t: &Ty| matches!(t, Ty::Int | Ty::Float | Ty::Unknown | Ty::TypeVar(_));
                        if !is_numeric(&lc) || !is_numeric(&rc) {
                            self.diagnostics.push(super::err(
                                format!("operator '{}' requires numeric types but got {} and {}", op, lc.display(), rc.display()),
                                "Use numeric types (Int or Float)", format!("operator {}", op)));
                        }
                        if lc == Ty::Float || rc == Ty::Float { InferTy::Concrete(Ty::Float) } else { lt }
                    }
                    "++" => {
                        let lc = lt.to_ty(&self.solutions);
                        let is_concatable = |t: &Ty| matches!(t, Ty::String | Ty::List(_) | Ty::Unknown | Ty::TypeVar(_));
                        if !is_concatable(&lc) {
                            self.diagnostics.push(super::err(
                                format!("operator '++' requires String or List but got {}", lc.display()),
                                "Use ++ with String or List types", "operator ++"));
                        }
                        lt
                    }
                    "==" | "!=" | "<" | ">" | "<=" | ">=" => InferTy::Concrete(Ty::Bool),
                    "and" | "or" => {
                        let lc = lt.to_ty(&self.solutions);
                        let rc = rt.to_ty(&self.solutions);
                        let is_bool = |t: &Ty| matches!(t, Ty::Bool | Ty::Unknown | Ty::TypeVar(_));
                        if !is_bool(&lc) {
                            self.diagnostics.push(super::err(
                                format!("operator '{}' requires Bool but got {}", op, lc.display()),
                                "Use Bool values with logical operators", format!("operator {}", op)));
                        }
                        if !is_bool(&rc) {
                            self.diagnostics.push(super::err(
                                format!("operator '{}' requires Bool but got {}", op, rc.display()),
                                "Use Bool values with logical operators", format!("operator {}", op)));
                        }
                        InferTy::Concrete(Ty::Bool)
                    }
                    "^" => InferTy::Concrete(Ty::Int),
                    _ => lt,
                }
            }

            ast::Expr::Unary { op, operand, .. } => {
                let t = self.infer_expr(operand);
                match op.as_str() { "not" => InferTy::Concrete(Ty::Bool), _ => t }
            }

            ast::Expr::If { cond, then, else_, .. } => {
                self.infer_expr(cond);
                let then_ty = self.infer_expr(then);
                let else_ty = self.infer_expr(else_);
                self.constrain(then_ty.clone(), else_ty, "if branches");
                then_ty
            }

            ast::Expr::Match { subject, arms, .. } => {
                let subject_ty = self.infer_expr(subject);
                let sc = subject_ty.to_ty(&self.solutions);
                self.check_match_exhaustiveness(&sc, arms);
                let result = self.fresh_var();
                for arm in arms.iter_mut() {
                    self.env.push_scope();
                    let sub_c = subject_ty.to_ty(&self.solutions);
                    self.bind_pattern(&arm.pattern, &sub_c);
                    if let Some(ref mut guard) = arm.guard { self.infer_expr(guard); }
                    let arm_ty = self.infer_expr(&mut arm.body);
                    self.constrain(result.clone(), arm_ty, "match arm");
                    self.env.pop_scope();
                }
                result
            }

            ast::Expr::Block { stmts, expr, .. } | ast::Expr::DoBlock { stmts, expr, .. } => {
                self.env.push_scope();
                for stmt in stmts.iter_mut() { self.check_stmt(stmt); }
                let ty = if let Some(e) = expr { self.infer_expr(e) } else { InferTy::Concrete(Ty::Unit) };
                self.env.pop_scope();
                ty
            }

            ast::Expr::Call { callee, args, named_args, .. } => {
                // Combine positional + named args for type checking
                let mut all_args: Vec<&mut ast::Expr> = args.iter_mut().collect();
                let mut named_exprs: Vec<ast::Expr> = named_args.iter().map(|(_, e)| e.clone()).collect();
                let mut all_flat: Vec<ast::Expr> = args.to_vec();
                all_flat.extend(named_exprs);
                self.check_call(callee, &mut all_flat)
            }

            ast::Expr::Pipe { left, right, .. } => {
                let left_ty = self.infer_expr(left);
                match right.as_mut() {
                    ast::Expr::Call { callee, args, .. } => {
                        // Pipe inserts left as the first argument
                        let mut all_arg_tys: Vec<super::types::InferTy> = vec![left_ty];
                        all_arg_tys.extend(args.iter_mut().map(|a| self.infer_expr(a)));
                        // Resolve module calls for pipe (e.g. xs |> list.filter(f))
                        match callee.as_mut() {
                            ast::Expr::Ident { name, .. } => self.check_named_call(name, &all_arg_tys),
                            ast::Expr::Member { object, field, .. } => {
                                if let ast::Expr::Ident { name: module, .. } = object.as_ref() {
                                    if crate::stdlib::is_stdlib_module(module) || self.env.user_modules.contains(module.as_str()) {
                                        let key = format!("{}.{}", module, field);
                                        return self.check_named_call(&key, &all_arg_tys);
                                    }
                                    if let Some(target) = self.env.module_aliases.get(module.as_str()).cloned() {
                                        let key = format!("{}.{}", target, field);
                                        return self.check_named_call(&key, &all_arg_tys);
                                    }
                                }
                                let ct = self.infer_expr(callee);
                                let ret = self.fresh_var();
                                self.constrain(ct, super::types::InferTy::Fn { params: all_arg_tys, ret: Box::new(ret.clone()) }, "pipe call");
                                ret
                            }
                            _ => {
                                let ct = self.infer_expr(callee);
                                let ret = self.fresh_var();
                                self.constrain(ct, super::types::InferTy::Fn { params: all_arg_tys, ret: Box::new(ret.clone()) }, "pipe call");
                                ret
                            }
                        }
                    }
                    _ => self.infer_expr(right),
                }
            }

            ast::Expr::Lambda { params, body, .. } => {
                self.env.push_scope();
                let param_tys: Vec<InferTy> = params.iter().map(|p| {
                    let ty = p.ty.as_ref().map(|te| InferTy::from_ty(&self.resolve_type_expr(te))).unwrap_or_else(|| self.fresh_var());
                    let concrete = ty.to_ty(&self.solutions);
                    self.env.define_var(&p.name, concrete);
                    ty
                }).collect();
                let ret_ty = self.infer_expr(body);
                self.env.pop_scope();
                InferTy::Fn { params: param_tys, ret: Box::new(ret_ty) }
            }

            ast::Expr::ForIn { var, iterable, body, .. } => {
                let iter_ty = self.infer_expr(iterable);
                self.env.push_scope();
                let elem_ty = match &iter_ty {
                    InferTy::List(inner) => inner.to_ty(&self.solutions),
                    InferTy::Concrete(Ty::List(inner)) => *inner.clone(),
                    _ => Ty::Unknown,
                };
                self.env.define_var(var, elem_ty);
                for stmt in body.iter_mut() { self.check_stmt(stmt); }
                self.env.pop_scope();
                InferTy::Concrete(Ty::Unit)
            }

            ast::Expr::While { cond, body, .. } => {
                self.infer_expr(cond);
                self.env.push_scope();
                for stmt in body.iter_mut() { self.check_stmt(stmt); }
                self.env.pop_scope();
                InferTy::Concrete(Ty::Unit)
            }

            ast::Expr::Range { start, end, .. } => { let st = self.infer_expr(start); self.infer_expr(end); InferTy::List(Box::new(st)) }

            ast::Expr::Some { expr, .. } => { let inner = self.infer_expr(expr); InferTy::Option(Box::new(inner)) }
            ast::Expr::Ok { expr, .. } => {
                let ok_ty = self.infer_expr(expr);
                let err_ty = match &self.env.current_ret {
                    Some(Ty::Result(_, e)) => InferTy::from_ty(e),
                    _ => self.fresh_var(),
                };
                InferTy::Result(Box::new(ok_ty), Box::new(err_ty))
            }
            ast::Expr::Err { expr, .. } => {
                let err_ty = self.infer_expr(expr);
                let ok_ty = match &self.env.current_ret {
                    Some(Ty::Result(o, _)) => InferTy::from_ty(o),
                    _ => self.fresh_var(),
                };
                InferTy::Result(Box::new(ok_ty), Box::new(err_ty))
            }
            ast::Expr::Try { expr, .. } => {
                let ty = self.infer_expr(expr);
                match &ty {
                    InferTy::Result(ok, _) => *ok.clone(),
                    InferTy::Concrete(Ty::Result(ok, _)) => InferTy::from_ty(ok),
                    _ => ty,
                }
            }

            ast::Expr::Paren { expr, .. } => self.infer_expr(expr),
            ast::Expr::Break { .. } | ast::Expr::Continue { .. } => InferTy::Concrete(Ty::Unit),
            ast::Expr::Hole { .. } | ast::Expr::Todo { .. } => self.fresh_var(),
            ast::Expr::Await { expr, .. } => self.infer_expr(expr),
            ast::Expr::Error { .. } | ast::Expr::Placeholder { .. } => InferTy::Concrete(Ty::Unknown),

            ast::Expr::MapLiteral { entries, .. } => {
                if entries.is_empty() { InferTy::Map(Box::new(self.fresh_var()), Box::new(self.fresh_var())) }
                else {
                    let kt = self.infer_expr(&mut entries[0].0);
                    let vt = self.infer_expr(&mut entries[0].1);
                    for entry in entries.iter_mut().skip(1) { self.infer_expr(&mut entry.0); self.infer_expr(&mut entry.1); }
                    InferTy::Map(Box::new(kt), Box::new(vt))
                }
            }
            ast::Expr::EmptyMap { .. } => InferTy::Map(Box::new(self.fresh_var()), Box::new(self.fresh_var())),
        }
    }

    // ── Statement checking ──

    pub(crate) fn check_stmt(&mut self, stmt: &mut ast::Stmt) {
        match stmt {
            ast::Stmt::Let { name, ty, value, .. } => {
                let val_ity = self.infer_expr(value);
                let final_ty = if let Some(te) = ty {
                    let declared = self.resolve_type_expr(te);
                    self.constrain(InferTy::from_ty(&declared), val_ity, format!("let {}", name));
                    declared
                } else {
                    let t = val_ity.to_ty(&self.solutions);
                    // Auto-unwrap Result in do blocks
                    if self.env.in_effect {
                        match t { Ty::Result(ok, _) => *ok, other => other }
                    } else { t }
                };
                self.env.define_var(name, final_ty);
            }
            ast::Stmt::Var { name, ty, value, .. } => {
                let val_ity = self.infer_expr(value);
                let final_ty = if let Some(te) = ty {
                    let declared = self.resolve_type_expr(te);
                    self.constrain(InferTy::from_ty(&declared), val_ity, format!("let {}", name));
                    declared
                } else {
                    let t = val_ity.to_ty(&self.solutions);
                    if self.env.in_effect {
                        match t { Ty::Result(ok, _) => *ok, other => other }
                    } else { t }
                };
                self.env.define_var(name, final_ty);
                self.env.mutable_vars.insert(name.clone());
            }
            ast::Stmt::LetDestructure { pattern, value, .. } => {
                let val_ity = self.infer_expr(value);
                let val_ty = val_ity.to_ty(&self.solutions);
                self.bind_pattern(pattern, &val_ty);
            }
            ast::Stmt::Assign { name, value, .. } => {
                self.infer_expr(value);
                if self.env.lookup_var(name).is_some() && !self.env.mutable_vars.contains(name.as_str()) {
                    let hint = if self.env.param_vars.contains(name.as_str()) {
                        format!("'{}' is a function parameter (immutable). Use a local copy: var {0}_ = {0}", name)
                    } else {
                        format!("Use 'var {0} = ...' instead of 'let {0} = ...' to declare a mutable variable", name)
                    };
                    self.diagnostics.push(super::err(
                        format!("cannot reassign immutable binding '{}'", name),
                        hint, format!("{} = ...", name)));
                }
            }
            ast::Stmt::IndexAssign { index, value, .. } => { self.infer_expr(index); self.infer_expr(value); }
            ast::Stmt::FieldAssign { value, .. } => { self.infer_expr(value); }
            ast::Stmt::Guard { cond, else_, .. } => { self.infer_expr(cond); self.infer_expr(else_); }
            ast::Stmt::Expr { expr, .. } => { self.infer_expr(expr); }
            ast::Stmt::Comment { .. } | ast::Stmt::Error { .. } => {}
        }
    }

    // ── Pattern binding ──

    pub(crate) fn bind_pattern(&mut self, pattern: &ast::Pattern, ty: &Ty) {
        match pattern {
            ast::Pattern::Wildcard => {}
            ast::Pattern::Ident { name } => { self.env.define_var(name, ty.clone()); }
            ast::Pattern::Constructor { args, .. } => { for arg in args { self.bind_pattern(arg, &Ty::Unknown); } }
            ast::Pattern::RecordPattern { fields, .. } => {
                let resolved = self.env.resolve_named(ty);
                let field_tys: Vec<(String, Ty)> = match &resolved {
                    Ty::Record { fields } | Ty::OpenRecord { fields } => fields.clone(),
                    Ty::Variant { cases, .. } => cases.iter().find_map(|c| match &c.payload {
                        VariantPayload::Record(fs) => Some(fs.iter().map(|(n, t, _)| (n.clone(), t.clone())).collect()),
                        _ => None,
                    }).unwrap_or_default(),
                    _ => vec![],
                };
                for f in fields {
                    let ft = field_tys.iter().find(|(n, _)| n == &f.name).map(|(_, t)| t.clone()).unwrap_or(Ty::Unknown);
                    if let Some(ref p) = f.pattern { self.bind_pattern(p, &ft); }
                    else { self.env.define_var(&f.name, ft); }
                }
            }
            ast::Pattern::Tuple { elements } => {
                if let Ty::Tuple(tys) = ty { for (i, e) in elements.iter().enumerate() { self.bind_pattern(e, tys.get(i).unwrap_or(&Ty::Unknown)); } }
            }
            ast::Pattern::Some { inner } => { let it = match ty { Ty::Option(t) => *t.clone(), _ => Ty::Unknown }; self.bind_pattern(inner, &it); }
            ast::Pattern::Ok { inner } => { let it = match ty { Ty::Result(t, _) => *t.clone(), _ => Ty::Unknown }; self.bind_pattern(inner, &it); }
            ast::Pattern::Err { inner } => { let it = match ty { Ty::Result(_, e) => *e.clone(), _ => Ty::Unknown }; self.bind_pattern(inner, &it); }
            ast::Pattern::None | ast::Pattern::Literal { .. } => {}
        }
    }
}
