use crate::ast;
use crate::types::Ty;
use super::{Checker, err};

impl Checker {
    pub(crate) fn check_expr(&mut self, expr: &ast::Expr) -> Ty {
        match expr {
            ast::Expr::Int { .. } => Ty::Int,
            ast::Expr::Float { .. } => Ty::Float,
            ast::Expr::String { .. } | ast::Expr::InterpolatedString { .. } => Ty::String,
            ast::Expr::Bool { .. } => Ty::Bool,
            ast::Expr::Unit => Ty::Unit,
            ast::Expr::None => Ty::Option(Box::new(Ty::Unknown)),
            ast::Expr::Hole | ast::Expr::Todo { .. } | ast::Expr::Placeholder => Ty::Unknown,
            ast::Expr::Some { expr: inner } => Ty::Option(Box::new(self.check_expr(inner))),
            ast::Expr::Ok { expr: inner } => {
                let inner_ty = self.check_expr(inner);
                if !self.env.in_effect && matches!(inner_ty, Ty::Unit) {
                    Ty::Unit
                } else {
                    Ty::Result(Box::new(inner_ty), Box::new(Ty::Unknown))
                }
            }
            ast::Expr::Err { expr: inner } => Ty::Result(Box::new(Ty::Unknown), Box::new(self.check_expr(inner))),

            ast::Expr::Ident { name } => {
                if let Some(ty) = self.env.lookup_var(name) { return ty.clone(); }
                if let Some(sig) = self.env.functions.get(name) {
                    return Ty::Fn { params: sig.params.iter().map(|(_, t)| t.clone()).collect(), ret: Box::new(sig.ret.clone()) };
                }
                if matches!(name.as_str(), "println" | "eprintln") {
                    return Ty::Fn { params: vec![Ty::String], ret: Box::new(Ty::Unit) };
                }
                Ty::Unknown
            }

            ast::Expr::TypeName { name } => {
                if self.env.constructors.contains_key(name) { return Ty::Unknown; }
                Ty::Named(name.clone())
            }

            ast::Expr::List { elements } => {
                if elements.is_empty() { return Ty::List(Box::new(Ty::Unknown)); }
                let first_ty = self.check_expr(&elements[0]);
                for (i, elem) in elements.iter().enumerate().skip(1) {
                    let et = self.check_expr(elem);
                    if !first_ty.compatible(&et) {
                        self.push_diagnostic(err(
                            format!("list element at index {} has type {} but expected {}", i, et.display(), first_ty.display()),
                            "All list elements must have the same type", "list literal",
                        ));
                    }
                }
                Ty::List(Box::new(first_ty))
            }

            ast::Expr::Record { fields } => Ty::Record {
                fields: fields.iter().map(|f| (f.name.clone(), self.check_expr(&f.value))).collect(),
            },

            ast::Expr::SpreadRecord { base, fields } => {
                let bt = self.check_expr(base);
                for f in fields { self.check_expr(&f.value); }
                bt
            }

            ast::Expr::If { cond, then, else_ } => {
                let ct = self.check_expr(cond);
                if !ct.compatible(&Ty::Bool) {
                    self.push_diagnostic(err(
                        format!("if condition has type {} but expected Bool", ct.display()),
                        "The condition must be a Bool expression", "if expression",
                    ));
                }
                let tt = self.check_expr(then);
                let et = self.check_expr(else_);
                if !tt.compatible(&et) {
                    self.push_diagnostic(err(
                        format!("if branches have different types: then is {}, else is {}", tt.display(), et.display()),
                        "Both branches must have the same type", "if expression",
                    ));
                }
                tt
            }

            ast::Expr::Match { subject, arms } => {
                let st = self.check_expr(subject);
                let mut result_ty: Option<Ty> = None;
                for arm in arms {
                    self.env.push_scope();
                    self.check_pattern(&arm.pattern, &st);
                    if let Some(ref guard) = arm.guard {
                        let gt = self.check_expr(guard);
                        if !gt.compatible(&Ty::Bool) {
                            self.push_diagnostic(err(
                                format!("match guard has type {} but expected Bool", gt.display()),
                                "Guard conditions must be Bool", "match arm",
                            ));
                        }
                    }
                    let at = self.check_expr(&arm.body);
                    if let Some(ref mut prev) = result_ty {
                        let compat = prev.compatible(&at)
                            || match (prev.clone(), &at) {
                                (Ty::Result(ok_ty, _), non_result) if !matches!(non_result, Ty::Result(_, _)) => ok_ty.compatible(non_result),
                                (_, Ty::Result(ok_ty, _)) if !matches!(prev.clone(), Ty::Result(_, _)) => prev.compatible(&ok_ty),
                                _ => false,
                            };
                        if !compat {
                            self.push_diagnostic(err(
                                format!("match arm has type {} but previous arms have type {}", at.display(), prev.display()),
                                "All match arms must have the same type", "match expression",
                            ));
                        }
                        if matches!(at, Ty::Result(_, _)) && !matches!(prev.clone(), Ty::Result(_, _)) {
                            *prev = at;
                        }
                    } else {
                        result_ty = Some(at);
                    }
                    self.env.pop_scope();
                }
                result_ty.unwrap_or(Ty::Unknown)
            }

            ast::Expr::Block { stmts, expr } => {
                self.env.push_scope();
                for s in stmts { self.check_stmt(s); }
                let ty = expr.as_ref().map(|e| self.check_expr(e)).unwrap_or(Ty::Unit);
                self.env.pop_scope();
                ty
            }

            ast::Expr::DoBlock { stmts, expr } => {
                self.env.push_scope();
                let prev_do = self.env.in_do_block;
                self.env.in_do_block = true;
                for s in stmts { self.check_stmt(s); }
                let _ty = expr.as_ref().map(|e| self.check_expr(e)).unwrap_or(Ty::Unit);
                self.env.in_do_block = prev_do;
                self.env.pop_scope();
                Ty::Unknown
            }

            ast::Expr::ForIn { var, iterable, body } => {
                let it = self.check_expr(iterable);
                self.env.push_scope();
                let elem_ty = match &it {
                    Ty::List(inner) => *inner.clone(),
                    Ty::Map(k, _) => *k.clone(),
                    _ if matches!(it, Ty::Unknown) => Ty::Unknown,
                    _ => {
                        self.push_diagnostic(err(
                            format!("cannot iterate over type {}", it.display()),
                            "for...in requires a List or Map",
                            format!("for {} in ...", var),
                        ));
                        Ty::Unknown
                    }
                };
                self.env.define_var(var, elem_ty);
                for s in body { self.check_stmt(s); }
                self.env.pop_scope();
                Ty::Unit
            }

            ast::Expr::Lambda { params, body } => {
                self.env.push_scope();
                let pts: Vec<Ty> = params.iter().map(|p| {
                    let ty = p.ty.as_ref().map(|te| self.resolve_type_expr(te)).unwrap_or(Ty::Unknown);
                    self.env.define_var(&p.name, ty.clone());
                    ty
                }).collect();
                let ret = self.check_expr(body);
                self.env.pop_scope();
                Ty::Fn { params: pts, ret: Box::new(ret) }
            }

            ast::Expr::Call { callee, args } => self.check_call(callee, args),

            ast::Expr::Member { object, field } => {
                let ot = self.check_expr(object);
                self.check_member_access(&ot, field)
            }

            ast::Expr::Pipe { left, right } => {
                let _left_ty = self.check_expr(left);
                if let ast::Expr::Call { callee, args } = right.as_ref() {
                    let mut all_args = vec![left.as_ref().clone()];
                    all_args.extend(args.iter().cloned());
                    self.check_call(callee, &all_args)
                } else {
                    self.check_expr(right)
                }
            }

            ast::Expr::Binary { op, left, right } => {
                let lt = self.check_expr(left);
                let rt = self.check_expr(right);
                self.check_binary_op(op, &lt, &rt)
            }

            ast::Expr::Unary { op, operand } => {
                let ot = self.check_expr(operand);
                match op.as_str() {
                    "not" => {
                        if !ot.compatible(&Ty::Bool) {
                            self.push_diagnostic(err(
                                format!("'not' expects Bool but got {}", ot.display()),
                                "Use 'not' only with Bool values", "unary not",
                            ));
                        }
                        Ty::Bool
                    }
                    "-" => {
                        if !ot.compatible(&Ty::Int) && !ot.compatible(&Ty::Float) {
                            self.push_diagnostic(err(
                                format!("unary '-' expects Int or Float but got {}", ot.display()),
                                "Negation only works on numbers", "unary minus",
                            ));
                        }
                        ot
                    }
                    _ => ot,
                }
            }

            ast::Expr::Paren { expr: inner } => self.check_expr(inner),

            ast::Expr::Try { expr: inner } => {
                let it = self.check_expr(inner);
                match &it {
                    Ty::Result(ok, _) => *ok.clone(),
                    Ty::Unknown => Ty::Unknown,
                    _ => {
                        self.push_diagnostic(err(
                            format!("'try' expects a Result but got {}", it.display()),
                            "Use 'try' only on expressions that return Result[T, E]", "try expression",
                        ));
                        Ty::Unknown
                    }
                }
            }

            ast::Expr::Await { expr: inner } => {
                let it = self.check_expr(inner);
                match &it {
                    Ty::Result(ok, _) => *ok.clone(),
                    _ => it,
                }
            }
        }
    }
}
