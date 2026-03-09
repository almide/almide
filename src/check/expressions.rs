use crate::ast::{self, ResolvedType};
use crate::types::Ty;
use super::{Checker, err};

fn ty_to_resolved(ty: &Ty) -> ResolvedType {
    match ty {
        Ty::Int => ResolvedType::Int,
        Ty::Float => ResolvedType::Float,
        Ty::String => ResolvedType::String,
        Ty::Bool => ResolvedType::Bool,
        Ty::Unit => ResolvedType::Unit,
        Ty::List(_) => ResolvedType::List,
        Ty::Option(_) => ResolvedType::Option,
        Ty::Result(_, _) => ResolvedType::Result,
        Ty::Map(_, _) => ResolvedType::Record,
        Ty::Record { .. } => ResolvedType::Record,
        Ty::Variant { .. } => ResolvedType::Variant,
        Ty::Fn { .. } => ResolvedType::Fn,
        Ty::Tuple(_) => ResolvedType::Tuple,
        Ty::Named(_) => ResolvedType::Named,
        Ty::Unknown => ResolvedType::Unknown,
    }
}

impl Checker {
    pub(crate) fn check_expr(&mut self, expr: &mut ast::Expr) -> Ty {
        // Update current line from expression span for precise error positions
        let prev_line = self.current_decl_line;
        if let Some(span) = expr.span() {
            self.current_decl_line = Some(span.line);
        }
        let ty = self.check_expr_inner(expr);
        expr.set_resolved_type(ty_to_resolved(&ty));
        self.current_decl_line = prev_line;
        ty
    }

    fn check_expr_inner(&mut self, expr: &mut ast::Expr) -> Ty {
        match expr {
            ast::Expr::Int { .. } => Ty::Int,
            ast::Expr::Float { .. } => Ty::Float,
            ast::Expr::String { .. } => Ty::String,
            ast::Expr::InterpolatedString { value, span, .. } => {
                self.check_interpolated_string(value, span.as_ref());
                Ty::String
            }
            ast::Expr::Bool { .. } => Ty::Bool,
            ast::Expr::Unit { .. } => Ty::Unit,
            ast::Expr::None { .. } => Ty::Option(Box::new(Ty::Unknown)),
            ast::Expr::Hole { .. } | ast::Expr::Todo { .. } | ast::Expr::Placeholder { .. } => Ty::Unknown,
            ast::Expr::Some { expr: inner, .. } => Ty::Option(Box::new(self.check_expr(inner))),
            ast::Expr::Ok { expr: inner, .. } => {
                let inner_ty = self.check_expr(inner);
                if !self.env.in_effect && matches!(inner_ty, Ty::Unit) {
                    Ty::Unit
                } else {
                    Ty::Result(Box::new(inner_ty), Box::new(Ty::Unknown))
                }
            }
            ast::Expr::Err { expr: inner, .. } => Ty::Result(Box::new(Ty::Unknown), Box::new(self.check_expr(inner))),

            ast::Expr::Ident { name, .. } => {
                if let Some(ty) = self.env.lookup_var(name).cloned() {
                    self.env.used_vars.insert(name.clone());
                    return ty;
                }
                if let Some(sig) = self.env.functions.get(name) {
                    return Ty::Fn { params: sig.params.iter().map(|(_, t)| t.clone()).collect(), ret: Box::new(sig.ret.clone()) };
                }
                if matches!(name.as_str(), "println" | "eprintln") {
                    return Ty::Fn { params: vec![Ty::String], ret: Box::new(Ty::Unit) };
                }
                Ty::Unknown
            }

            ast::Expr::TypeName { name, .. } => {
                if self.env.constructors.contains_key(name) { return Ty::Unknown; }
                Ty::Named(name.clone())
            }

            ast::Expr::List { elements, .. } => {
                if elements.is_empty() { return Ty::List(Box::new(Ty::Unknown)); }
                let first_ty = self.check_expr(&mut elements[0]);
                for (i, elem) in elements.iter_mut().enumerate().skip(1) {
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

            ast::Expr::Record { fields, .. } => Ty::Record {
                fields: fields.iter_mut().map(|f| (f.name.clone(), self.check_expr(&mut f.value))).collect(),
            },

            ast::Expr::SpreadRecord { base, fields, .. } => {
                let bt = self.check_expr(base);
                for f in fields.iter_mut() { self.check_expr(&mut f.value); }
                bt
            }

            ast::Expr::If { cond, then, else_, .. } => {
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

            ast::Expr::Match { subject, arms, .. } => {
                // Suppress auto-unwrap when matching on ok/err (caller handles Result explicitly)
                let has_result_arms = arms.iter().any(|a| matches!(&a.pattern, ast::Pattern::Ok { .. } | ast::Pattern::Err { .. }));
                let prev_skip = self.env.skip_auto_unwrap;
                if has_result_arms {
                    self.env.skip_auto_unwrap = true;
                }
                let st = self.check_expr(subject);
                self.env.skip_auto_unwrap = prev_skip;
                let mut result_ty: Option<Ty> = None;
                for arm in arms.iter_mut() {
                    self.env.push_scope();
                    self.check_pattern(&arm.pattern, &st);
                    if let Some(ref mut guard) = arm.guard {
                        let gt = self.check_expr(guard);
                        if !gt.compatible(&Ty::Bool) {
                            self.push_diagnostic(err(
                                format!("match guard has type {} but expected Bool", gt.display()),
                                "Guard conditions must be Bool", "match arm",
                            ));
                        }
                    }
                    let at = self.check_expr(&mut arm.body);
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
                // Exhaustiveness check
                self.check_match_exhaustiveness(&st, arms);
                result_ty.unwrap_or(Ty::Unknown)
            }

            ast::Expr::Block { stmts, expr, .. } => {
                self.env.push_scope();
                for s in stmts.iter_mut() { self.check_stmt(s); }
                let ty = expr.as_mut().map(|e| self.check_expr(e)).unwrap_or(Ty::Unit);
                self.warn_unused_vars_in_scope("block");
                self.env.pop_scope();
                ty
            }

            ast::Expr::DoBlock { stmts, expr, .. } => {
                self.env.push_scope();
                let prev_do = self.env.in_do_block;
                self.env.in_do_block = true;
                for s in stmts.iter_mut() { self.check_stmt(s); }
                let _ty = expr.as_mut().map(|e| self.check_expr(e)).unwrap_or(Ty::Unit);
                self.warn_unused_vars_in_scope("do block");
                self.env.in_do_block = prev_do;
                self.env.pop_scope();
                Ty::Unknown
            }

            ast::Expr::Range { start, end, .. } => {
                let st = self.check_expr(start);
                let et = self.check_expr(end);
                if !matches!(st, Ty::Int | Ty::Unknown) {
                    self.push_diagnostic(err(
                        format!("range start must be Int, got {}", st.display()),
                        "range requires Int operands",
                        "start..end".to_string(),
                    ));
                }
                if !matches!(et, Ty::Int | Ty::Unknown) {
                    self.push_diagnostic(err(
                        format!("range end must be Int, got {}", et.display()),
                        "range requires Int operands",
                        "start..end".to_string(),
                    ));
                }
                Ty::List(Box::new(Ty::Int))
            }

            ast::Expr::ForIn { var, var_tuple, iterable, body, .. } => {
                let it = self.check_expr(iterable);
                self.env.push_scope();
                let elem_ty = match &it {
                    Ty::List(inner) => *inner.clone(),
                    Ty::Map(k, _) => *k.clone(),
                    _ if matches!(it, Ty::Unknown) => Ty::Unknown,
                    _ => {
                        self.push_diagnostic(err(
                            format!("cannot iterate over type {}", it.display()),
                            "for...in requires a List, Map, or Range",
                            format!("for {} in ...", var),
                        ));
                        Ty::Unknown
                    }
                };
                if let Some(names) = var_tuple {
                    // Tuple destructuring: define each name
                    match &elem_ty {
                        Ty::Tuple(tys) => {
                            for (i, name) in names.iter().enumerate() {
                                let ty = tys.get(i).cloned().unwrap_or(Ty::Unknown);
                                self.env.define_var(name, ty);
                            }
                        }
                        _ => {
                            for name in names {
                                self.env.define_var(name, Ty::Unknown);
                            }
                        }
                    }
                } else {
                    self.env.define_var(var, elem_ty);
                }
                for s in body.iter_mut() { self.check_stmt(s); }
                self.env.pop_scope();
                Ty::Unit
            }

            ast::Expr::Lambda { params, body, .. } => {
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

            ast::Expr::Call { callee, args, .. } => self.check_call(callee, args),

            ast::Expr::Member { object, field, .. } => {
                // Track module usage for unused import detection
                if let ast::Expr::Ident { name, .. } = object.as_ref() {
                    if crate::stdlib::is_stdlib_module(name) || self.env.user_modules.contains(name) {
                        self.env.used_modules.insert(name.clone());
                    }
                }
                let ot = self.check_expr(object);
                self.check_member_access(&ot, field)
            }

            ast::Expr::TupleIndex { object, index, .. } => {
                let ot = self.check_expr(object);
                match &ot {
                    Ty::Tuple(elements) => {
                        if *index < elements.len() {
                            elements[*index].clone()
                        } else {
                            self.push_diagnostic(err(
                                format!("tuple index {} is out of bounds (tuple has {} elements)", index, elements.len()),
                                format!("Valid indices are 0..{}", elements.len() - 1),
                                "tuple index",
                            ));
                            Ty::Unknown
                        }
                    }
                    _ => Ty::Unknown,
                }
            }

            ast::Expr::Pipe { left, right, .. } => {
                let _left_ty = self.check_expr(left);
                if let ast::Expr::Call { callee, args, .. } = right.as_mut() {
                    let mut all_args = vec![left.as_ref().clone()];
                    all_args.extend(args.iter().cloned());
                    self.check_call(callee, &mut all_args)
                } else {
                    self.check_expr(right)
                }
            }

            ast::Expr::Binary { op, left, right, .. } => {
                let lt = self.check_expr(left);
                let rt = self.check_expr(right);
                self.check_binary_op(op, &lt, &rt)
            }

            ast::Expr::Unary { op, operand, .. } => {
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

            ast::Expr::Paren { expr: inner, .. } => self.check_expr(inner),
            ast::Expr::Tuple { elements, .. } => {
                let tys: Vec<Ty> = elements.iter_mut().map(|e| self.check_expr(e)).collect();
                Ty::Tuple(tys)
            }

            ast::Expr::Try { expr: inner, .. } => {
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

            ast::Expr::Await { expr: inner, .. } => {
                let it = self.check_expr(inner);
                match &it {
                    Ty::Result(ok, _) => *ok.clone(),
                    _ => it,
                }
            }
        }
    }

    /// Validate interpolated expressions inside `"...${expr}..."` strings.
    fn check_interpolated_string(&mut self, value: &str, span: Option<&ast::Span>) {
        let mut chars = value.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '$' && chars.peek() == Some(&'{') {
                chars.next(); // skip {
                let mut expr_str = String::new();
                let mut depth = 1;
                while let Some(ch) = chars.next() {
                    if ch == '{' { depth += 1; }
                    if ch == '}' { depth -= 1; if depth == 0 { break; } }
                    expr_str.push(ch);
                }
                // Parse the interpolated expression
                let tokens = crate::lexer::Lexer::tokenize(&expr_str);
                let mut parser = crate::parser::Parser::new(tokens);
                match parser.parse_single_expr() {
                    Ok(mut parsed_expr) => {
                        // Type-check the expression
                        self.check_expr(&mut parsed_expr);
                    }
                    Err(_) => {
                        let line = span.map(|s| s.line).unwrap_or(0);
                        self.push_diagnostic(err(
                            format!("invalid expression in string interpolation: ${{{}}}", expr_str),
                            "Check the syntax of the expression inside ${{...}}",
                            format!("string at line {}", line),
                        ));
                    }
                }
            }
        }
    }
}
