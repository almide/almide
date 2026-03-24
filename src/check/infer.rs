/// Expression type inference — Pass 1 of the constraint-based checker.
/// Walks the AST, assigns Ty to each expression, collects constraints.

use crate::ast;
use crate::intern::{Sym, sym};
use crate::types::{Ty, TypeConstructorId, VariantPayload};
use super::types::resolve_ty;
use super::Checker;

impl Checker {
    pub(crate) fn infer_expr(&mut self, expr: &mut ast::Expr) -> Ty {
        // Track current span for diagnostic annotation
        if let Some(span) = expr.span() {
            self.current_span = Some(span);
        }
        let ity = self.infer_expr_inner(expr);
        self.infer_types.insert(expr.id(), ity.clone());
        ity
    }

    fn infer_expr_inner(&mut self, expr: &mut ast::Expr) -> Ty {
        match expr {
            ast::Expr::Int { .. } => Ty::Int,
            ast::Expr::Float { .. } => Ty::Float,
            ast::Expr::String { .. } => Ty::String,
            ast::Expr::InterpolatedString { parts, .. } => {
                for part in parts.iter_mut() {
                    if let ast::StringPart::Expr { expr } = part {
                        self.infer_expr(expr);
                    }
                }
                Ty::String
            }
            ast::Expr::Bool { .. } => Ty::Bool,
            ast::Expr::Unit { .. } => Ty::Unit,

            ast::Expr::None { .. } => Ty::option(self.fresh_var()),

            ast::Expr::Ident { name, .. } => {
                self.env.used_vars.insert(sym(name));
                if let Some(ty) = self.env.lookup_var(name).cloned() { self.instantiate_ty(&ty) }
                else if let Some(ty) = self.env.top_lets.get(&sym(name)).cloned() { self.instantiate_ty(&ty) }
                else if let Some(sig) = self.env.functions.get(&sym(name)).cloned() {
                    Ty::Fn {
                        params: sig.params.iter().map(|(_, t)| t.clone()).collect(),
                        ret: Box::new(sig.ret.clone()),
                    }
                }
                else {
                    self.emit(super::err(format!("undefined variable '{}'", name), "Check the variable name", format!("variable {}", name)).with_code("E003"));
                    Ty::Unknown
                }
            }

            ast::Expr::TypeName { name, .. } => {
                if let Some((type_name, _)) = self.env.constructors.get(&sym(name)) { Ty::Named(*type_name, vec![]) }
                else if let Some(ty) = self.env.top_lets.get(&sym(name)).cloned() { ty }
                else { Ty::Named(sym(name), vec![]) }
            }

            ast::Expr::List { elements, .. } => {
                if elements.is_empty() { Ty::list(self.fresh_var()) }
                else {
                    let first = self.infer_expr(&mut elements[0]);
                    for elem in elements.iter_mut().skip(1) { let et = self.infer_expr(elem); self.constrain(first.clone(), et, "list element"); }
                    Ty::list(first)
                }
            }

            ast::Expr::Tuple { elements, .. } => Ty::Tuple(elements.iter_mut().map(|e| self.infer_expr(e)).collect()),

            ast::Expr::Record { name, fields, .. } => {
                for f in fields.iter_mut() { self.infer_expr(&mut f.value); }
                if let Some(n) = name {
                    // Variant constructor → resolve to parent type name
                    let type_name = self.env.constructors.get(&sym(n))
                        .map(|(vname, _)| *vname)
                        .unwrap_or_else(|| sym(n));
                    Ty::Named(type_name, vec![])
                }
                else {
                    let field_tys: Vec<(Sym, Ty)> = fields.iter().map(|f| {
                        let ty = self.infer_types.get(&f.value.id()).map(|it| resolve_ty(it, &self.uf)).unwrap_or(Ty::Unknown);
                        (sym(&f.name), ty)
                    }).collect();
                    Ty::Record { fields: field_tys }
                }
            }

            ast::Expr::SpreadRecord { base, fields, .. } => {
                let base_ty = self.infer_expr(base);
                for f in fields.iter_mut() { self.infer_expr(&mut f.value); }
                base_ty
            }

            ast::Expr::Member { object, field, .. } => {
                let obj_ty = self.infer_expr(object);
                let concrete = resolve_ty(&obj_ty, &self.uf);
                self.resolve_field_type(&concrete, field)
            }

            ast::Expr::TupleIndex { object, index, .. } => {
                let obj_ty = self.infer_expr(object);
                match &obj_ty {
                    Ty::Tuple(elems) if *index < elems.len() => elems[*index].clone(),
                    _ => {
                        let concrete = resolve_ty(&obj_ty, &self.uf);
                        match &concrete { Ty::Tuple(elems) if *index < elems.len() => elems[*index].clone(), _ => Ty::Unknown }
                    }
                }
            }

            ast::Expr::IndexAccess { object, index, .. } => {
                let obj_ty = self.infer_expr(object);
                self.infer_expr(index);
                let concrete = resolve_ty(&obj_ty, &self.uf);
                match &concrete {
                    Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => args[0].clone(),
                    Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 => Ty::option(args[1].clone()),
                    _ => Ty::Unknown,
                }
            }

            ast::Expr::Binary { op, left, right, .. } => {
                let lt = self.infer_expr(left);
                let rt = self.infer_expr(right);
                match op.as_str() {
                    "+" => {
                        let lc = resolve_ty(&lt, &self.uf);
                        let rc = resolve_ty(&rt, &self.uf);
                        self.infer_plus_op(&lc, &rc, lt)
                    }
                    "-" | "*" | "/" | "%" => {
                        let lc = resolve_ty(&lt, &self.uf);
                        let rc = resolve_ty(&rt, &self.uf);
                        let is_numeric = |t: &Ty| matches!(t, Ty::Int | Ty::Float | Ty::Unknown | Ty::TypeVar(_));
                        if !is_numeric(&lc) || !is_numeric(&rc) {
                            self.emit(super::err(
                                format!("operator '{}' requires numeric types but got {} and {}", op, lc.display(), rc.display()),
                                "Use numeric types (Int or Float)", format!("operator {}", op)));
                        }
                        if lc == Ty::Float || rc == Ty::Float { Ty::Float } else { lt }
                    }
                    "++" => {
                        self.emit(super::err(
                            format!("operator '++' has been removed. Use '+' for concatenation"),
                            "Replace ++ with +", "operator ++"));
                        lt
                    }
                    "==" | "!=" | "<" | ">" | "<=" | ">=" => Ty::Bool,
                    "and" | "or" => {
                        let lc = resolve_ty(&lt, &self.uf);
                        let rc = resolve_ty(&rt, &self.uf);
                        let is_bool = |t: &Ty| matches!(t, Ty::Bool | Ty::Unknown | Ty::TypeVar(_));
                        if !is_bool(&lc) {
                            self.emit(super::err(
                                format!("operator '{}' requires Bool but got {}", op, lc.display()),
                                "Use Bool values with logical operators", format!("operator {}", op)));
                        }
                        if !is_bool(&rc) {
                            self.emit(super::err(
                                format!("operator '{}' requires Bool but got {}", op, rc.display()),
                                "Use Bool values with logical operators", format!("operator {}", op)));
                        }
                        Ty::Bool
                    }
                    "^" => Ty::Int,
                    _ => lt,
                }
            }

            ast::Expr::Unary { op, operand, .. } => {
                let t = self.infer_expr(operand);
                match op.as_str() { "not" => Ty::Bool, _ => t }
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
                let sc = resolve_ty(&subject_ty, &self.uf);
                self.check_match_exhaustiveness(&sc, arms);
                let mut arm_types = Vec::new();
                for arm in arms.iter_mut() {
                    self.env.push_scope();
                    let sub_c = resolve_ty(&subject_ty, &self.uf);
                    self.bind_pattern(&arm.pattern, &sub_c);
                    if let Some(ref mut guard) = arm.guard { self.infer_expr(guard); }
                    let arm_ty = self.infer_expr(&mut arm.body);
                    arm_types.push(arm_ty);
                    self.env.pop_scope();
                }
                // Unify all arm types with each other (not with a shared result var
                // that can be contaminated by external constraints)
                if let Some(first) = arm_types.first().cloned() {
                    for aty in &arm_types[1..] {
                        self.constrain(first.clone(), aty.clone(), "match arm");
                    }
                    first
                } else {
                    Ty::Unit
                }
            }

            ast::Expr::Block { stmts, expr, .. } => {
                self.env.push_scope();
                for stmt in stmts.iter_mut() { self.check_stmt(stmt); }
                let ty = if let Some(e) = expr { self.infer_expr(e) } else { Ty::Unit };
                self.env.pop_scope();
                ty
            }

            ast::Expr::Fan { exprs, .. } => {
                if !self.env.can_call_effect {
                    self.emit(super::err(
                        "fan block can only be used inside an effect fn".to_string(),
                        "Mark the enclosing function as `effect fn`",
                        "fan block".to_string()).with_code("E007"));
                }
                // Check for mutable variable capture
                let mutable_captures: Vec<String> = exprs.iter().flat_map(|e| {
                    let mut idents = Vec::new();
                    collect_idents(e, &mut idents);
                    idents.into_iter().filter(|name| self.env.mutable_vars.contains(&sym(name))).collect::<Vec<_>>()
                }).collect();
                for name in &mutable_captures {
                    self.emit(super::err(
                        format!("cannot capture mutable variable '{}' inside fan block", name),
                        "Use a `let` binding instead of `var` for values shared across fan expressions",
                        "fan block".to_string()).with_code("E008"));
                }
                let tys: Vec<Ty> = exprs.iter_mut().map(|e| {
                    let ty = self.infer_expr(e);
                    // Auto-unwrap Result: fan unwraps Result<T, E> to T
                    let concrete = resolve_ty(&ty, &self.uf);
                    match &concrete {
                        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
                        _ => ty,
                    }
                }).collect();
                match tys.len() {
                    1 => tys.into_iter().next().unwrap(),
                    _ => Ty::Tuple(tys.iter().map(|t| resolve_ty(t, &self.uf)).collect()),
                }
            }

            ast::Expr::Call { callee, args, named_args, type_args, .. } => {
                self.infer_call(callee, args, named_args, type_args)
            }

            ast::Expr::Pipe { left, right, .. } => {
                self.infer_pipe(left, right)
            }

            ast::Expr::Compose { left, right, .. } => {
                let left_ty = self.infer_expr(left);
                let right_ty = self.infer_expr(right);
                // If left is Fn[A] -> B and right is Fn[B] -> C, result is Fn[A] -> C
                let resolved_left = resolve_ty(&left_ty, &self.uf);
                let resolved_right = resolve_ty(&right_ty, &self.uf);
                match (&resolved_left, &resolved_right) {
                    (Ty::Fn { params: a_params, .. }, Ty::Fn { ret: c_ret, .. }) => {
                        Ty::Fn { params: a_params.clone(), ret: c_ret.clone() }
                    }
                    _ => Ty::Unknown,
                }
            }

            ast::Expr::Lambda { params, body, .. } => {
                self.env.push_scope();
                // Lambda has its own return context — don't leak outer function's current_ret
                let saved_ret = self.env.current_ret.take();
                self.env.lambda_depth += 1;
                let param_tys: Vec<Ty> = params.iter().map(|p| {
                    let ty = p.ty.as_ref().map(|te| self.resolve_type_expr(te)).unwrap_or_else(|| self.fresh_var());
                    let concrete = resolve_ty(&ty, &self.uf);
                    self.env.define_var(&p.name, concrete);
                    ty
                }).collect();
                let ret_ty = self.infer_expr(body);
                self.env.lambda_depth -= 1;
                self.env.current_ret = saved_ret;
                self.env.pop_scope();
                Ty::Fn { params: param_tys, ret: Box::new(ret_ty) }
            }

            ast::Expr::ForIn { var, var_tuple, iterable, body, .. } => {
                self.infer_for_in(var, var_tuple, iterable, body)
            }

            ast::Expr::While { cond, body, .. } => {
                self.infer_expr(cond);
                self.env.push_scope();
                for stmt in body.iter_mut() { self.check_stmt(stmt); }
                self.env.pop_scope();
                Ty::Unit
            }

            ast::Expr::Range { start, end, .. } => { let st = self.infer_expr(start); self.infer_expr(end); Ty::list(st) }

            ast::Expr::Some { expr, .. } => { let inner = self.infer_expr(expr); Ty::option(inner) }
            ast::Expr::Ok { expr, .. } => {
                let ok_ty = self.infer_expr(expr);
                let err_ty = match &self.env.current_ret {
                    Some(Ty::Applied(TypeConstructorId::Result, args)) if args.len() == 2 => args[1].clone(),
                    _ => self.fresh_var(),
                };
                Ty::result(ok_ty, err_ty)
            }
            ast::Expr::Err { expr, .. } => {
                let err_ty = self.infer_expr(expr);
                let ok_ty = match &self.env.current_ret {
                    Some(Ty::Applied(TypeConstructorId::Result, args)) if args.len() == 2 => args[0].clone(),
                    _ => self.fresh_var(),
                };
                Ty::result(ok_ty, err_ty)
            }
            ast::Expr::Try { expr, .. } => {
                let ty = self.infer_expr(expr);
                match &ty {
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() >= 1 => args[0].clone(),
                    _ => ty,
                }
            }

            ast::Expr::Paren { expr, .. } => self.infer_expr(expr),
            ast::Expr::Break { .. } | ast::Expr::Continue { .. } => Ty::Unit,
            ast::Expr::Hole { .. } | ast::Expr::Todo { .. } => self.fresh_var(),
            ast::Expr::Await { expr, .. } => self.infer_expr(expr),
            ast::Expr::Error { .. } | ast::Expr::Placeholder { .. } => Ty::Unknown,

            ast::Expr::MapLiteral { entries, .. } => {
                if entries.is_empty() { Ty::map_of(self.fresh_var(), self.fresh_var()) }
                else {
                    let kt = self.infer_expr(&mut entries[0].0);
                    let vt = self.infer_expr(&mut entries[0].1);
                    for entry in entries.iter_mut().skip(1) { self.infer_expr(&mut entry.0); self.infer_expr(&mut entry.1); }
                    Ty::map_of(kt, vt)
                }
            }
            ast::Expr::EmptyMap { .. } => Ty::map_of(self.fresh_var(), self.fresh_var()),
        }
    }

    // ── Extracted inference helpers ──

    fn infer_call(
        &mut self,
        callee: &mut Box<ast::Expr>,
        args: &mut Vec<ast::Expr>,
        named_args: &mut Vec<(String, ast::Expr)>,
        type_args: &Option<Vec<ast::TypeExpr>>,
    ) -> Ty {
        // Combine positional + named args for type checking
        let named_exprs: Vec<ast::Expr> = named_args.iter().map(|(_, e)| e.clone()).collect();
        let mut all_flat: Vec<ast::Expr> = args.to_vec();
        all_flat.extend(named_exprs);
        // 型引数を解決して渡す
        let resolved_type_args: Option<Vec<crate::types::Ty>> = type_args.as_ref().map(|tas|
            tas.iter().map(|te| self.resolve_type_expr(te)).collect());
        self.check_call_with_type_args(callee, &mut all_flat, resolved_type_args.as_deref())
    }

    fn infer_pipe(&mut self, left: &mut Box<ast::Expr>, right: &mut Box<ast::Expr>) -> Ty {
        let left_ty = self.infer_expr(left);
        match right.as_mut() {
            ast::Expr::Call { callee, args, .. } => {
                // Pipe inserts left as the first argument
                let mut all_arg_tys: Vec<Ty> = vec![left_ty];
                all_arg_tys.extend(args.iter_mut().map(|a| self.infer_expr(a)));
                // Resolve module calls for pipe (e.g. xs |> list.filter(f))
                match callee.as_mut() {
                    ast::Expr::Ident { name, .. } => self.check_named_call(name, &all_arg_tys),
                    ast::Expr::Member { object, field, .. } => {
                        let module_key = self.resolve_module_call(object, field);
                        if let Some(key) = module_key {
                            return self.check_named_call(&key, &all_arg_tys);
                        }
                        let ct = self.infer_expr(callee);
                        let ret = self.fresh_var();
                        self.constrain(ct, Ty::Fn { params: all_arg_tys, ret: Box::new(ret.clone()) }, "pipe call");
                        ret
                    }
                    _ => {
                        let ct = self.infer_expr(callee);
                        let ret = self.fresh_var();
                        self.constrain(ct, Ty::Fn { params: all_arg_tys, ret: Box::new(ret.clone()) }, "pipe call");
                        ret
                    }
                }
            }
            // Pipe RHS is a bare function name (e.g. `5 |> double`)
            ast::Expr::Ident { name, .. } => {
                let all_arg_tys = vec![left_ty];
                self.check_named_call(name, &all_arg_tys)
            }
            // Pipe RHS is a module-qualified function (e.g. `5 |> int.abs`)
            ast::Expr::Member { object, field, .. } => {
                let all_arg_tys = vec![left_ty];
                if let Some(key) = self.resolve_module_call(object, field) {
                    return self.check_named_call(&key, &all_arg_tys);
                }
                let ct = self.infer_expr(right);
                let ret = self.fresh_var();
                self.constrain(ct, Ty::Fn { params: all_arg_tys, ret: Box::new(ret.clone()) }, "pipe call");
                ret
            }
            _ => {
                let rt = self.infer_expr(right);
                let ret = self.fresh_var();
                self.constrain(rt, Ty::Fn { params: vec![left_ty], ret: Box::new(ret.clone()) }, "pipe call");
                ret
            }
        }
    }

    fn infer_for_in(
        &mut self,
        var: &str,
        var_tuple: &Option<Vec<String>>,
        iterable: &mut Box<ast::Expr>,
        body: &mut Vec<ast::Stmt>,
    ) -> Ty {
        let iter_ty = self.infer_expr(iterable);
        self.env.push_scope();
        let iter_resolved = resolve_ty(&iter_ty, &self.uf);
        let elem_ty = match &iter_resolved {
            Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => args[0].clone(),
            Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 => Ty::Tuple(vec![args[0].clone(), args[1].clone()]),
            _ => Ty::Unknown,
        };
        if let Some(names) = var_tuple {
            // Destructure tuple: for (a, b) in xs
            if let Ty::Tuple(tys) = &elem_ty {
                for (i, n) in names.iter().enumerate() {
                    self.env.define_var(n, tys.get(i).cloned().unwrap_or(Ty::Unknown));
                }
            } else {
                for n in names { self.env.define_var(n, Ty::Unknown); }
            }
        } else {
            self.env.define_var(var, elem_ty);
        }
        for stmt in body.iter_mut() { self.check_stmt(stmt); }
        self.env.pop_scope();
        Ty::Unit
    }

    // ── Statement checking ──

    pub(crate) fn check_stmt(&mut self, stmt: &mut ast::Stmt) {
        match stmt {
            ast::Stmt::Let { name, ty, value, .. } => {
                let val_ty = self.infer_expr(value);
                let final_ty = if let Some(te) = ty {
                    let declared = self.resolve_type_expr(te);
                    self.constrain(declared.clone(), val_ty, format!("let {}", name));
                    declared
                } else {
                    let t = resolve_ty(&val_ty, &self.uf);
                    // Auto-unwrap Result in effect fns (but not in test blocks)
                    if self.env.auto_unwrap {
                        match t {
                            Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args.into_iter().next().unwrap(),
                            other => other,
                        }
                    } else { t }
                };
                self.env.define_var(name, final_ty);
            }
            ast::Stmt::Var { name, ty, value, .. } => {
                let val_ty = self.infer_expr(value);
                let final_ty = if let Some(te) = ty {
                    let declared = self.resolve_type_expr(te);
                    self.constrain(declared.clone(), val_ty, format!("let {}", name));
                    declared
                } else {
                    let t = resolve_ty(&val_ty, &self.uf);
                    if self.env.auto_unwrap {
                        match t {
                            Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args.into_iter().next().unwrap(),
                            other => other,
                        }
                    } else { t }
                };
                self.env.define_var(name, final_ty);
                self.env.mutable_vars.insert(sym(name));
                self.env.var_lambda_depth.insert(sym(name), self.env.lambda_depth);
            }
            ast::Stmt::LetDestructure { pattern, value, .. } => {
                let val_ty = self.infer_expr(value);
                let val_resolved = resolve_ty(&val_ty, &self.uf);
                self.bind_pattern(pattern, &val_resolved);
            }
            ast::Stmt::Assign { name, value, .. } => {
                self.infer_expr(value);
                if self.env.lookup_var(name).is_some() && !self.env.mutable_vars.contains(&sym(name)) {
                    let hint = if self.env.param_vars.contains(&sym(name)) {
                        format!("'{}' is a function parameter (immutable). Use a local copy: var {0}_ = {0}", name)
                    } else {
                        format!("Use 'var {0} = ...' instead of 'let {0} = ...' to declare a mutable variable", name)
                    };
                    self.emit(super::err(
                        format!("cannot reassign immutable binding '{}'", name),
                        hint, format!("{} = ...", name)).with_code("E009"));
                }
                // Escape analysis: block var mutation inside closures in pure fns
                if self.env.mutable_vars.contains(&sym(name)) && !self.env.can_call_effect {
                    if let Some(&decl_depth) = self.env.var_lambda_depth.get(&sym(name)) {
                        if self.env.lambda_depth > decl_depth {
                            self.emit(super::err(
                                format!("mutable variable '{}' is mutated inside a closure in a pure function — use effect fn instead", name),
                                "Move the mutation out of the closure, or mark the enclosing function as `effect fn`",
                                format!("{} = ...", name)).with_code("E011"));
                        }
                    }
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
            ast::Pattern::Constructor { name, args } => {
                let resolved = self.env.resolve_named(ty);
                let payload_tys: Vec<Ty> = match &resolved {
                    Ty::Variant { cases, .. } => cases.iter()
                        .find(|c| c.name == *name)
                        .map(|c| match &c.payload {
                            VariantPayload::Tuple(tys) => tys.clone(),
                            _ => vec![],
                        })
                        .unwrap_or_default(),
                    _ => vec![],
                };
                for (i, arg) in args.iter().enumerate() {
                    self.bind_pattern(arg, payload_tys.get(i).unwrap_or(&Ty::Unknown));
                }
            }
            ast::Pattern::RecordPattern { name, fields, .. } => {
                let resolved = self.env.resolve_named(ty);
                let field_tys: Vec<(Sym, Ty)> = match &resolved {
                    Ty::Record { fields } | Ty::OpenRecord { fields } => fields.clone(),
                    Ty::Variant { cases, .. } => {
                        // Find the specific case matching the pattern name
                        cases.iter()
                            .find(|c| c.name == *name)
                            .and_then(|c| match &c.payload {
                                VariantPayload::Record(fs) => Some(fs.iter().map(|(n, t, _)| (*n, t.clone())).collect()),
                                _ => None,
                            })
                            .unwrap_or_default()
                    }
                    _ => vec![],
                };
                for f in fields {
                    let ft = field_tys.iter().find(|(n, _)| *n == f.name).map(|(_, t)| t.clone()).unwrap_or(Ty::Unknown);
                    if let Some(ref p) = f.pattern { self.bind_pattern(p, &ft); }
                    else { self.env.define_var(&f.name, ft); }
                }
            }
            ast::Pattern::Tuple { elements } => {
                if let Ty::Tuple(tys) = ty { for (i, e) in elements.iter().enumerate() { self.bind_pattern(e, tys.get(i).unwrap_or(&Ty::Unknown)); } }
            }
            ast::Pattern::Some { inner } => { let it = match ty { Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => args[0].clone(), _ => Ty::Unknown }; self.bind_pattern(inner, &it); }
            ast::Pattern::Ok { inner } => { let it = match ty { Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(), _ => Ty::Unknown }; self.bind_pattern(inner, &it); }
            ast::Pattern::Err { inner } => { let it = match ty { Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[1].clone(), _ => Ty::Unknown }; self.bind_pattern(inner, &it); }
            ast::Pattern::None | ast::Pattern::Literal { .. } => {}
        }
    }

    /// Infer the result type of the + operator (numeric add or string/list concat).
    fn infer_plus_op(&mut self, lc: &Ty, rc: &Ty, lt: Ty) -> Ty {
        let is_concat_ty = |t: &Ty| matches!(t, Ty::String | Ty::Applied(TypeConstructorId::List, _));
        let is_unknown_ty = |t: &Ty| matches!(t, Ty::Unknown | Ty::TypeVar(_));
        if (is_concat_ty(lc) && (is_concat_ty(rc) || is_unknown_ty(rc)))
            || (is_concat_ty(rc) && is_unknown_ty(lc)) {
            return lt; // concat: return same type
        }
        let is_numeric = |t: &Ty| matches!(t, Ty::Int | Ty::Float | Ty::Unknown | Ty::TypeVar(_));
        if !is_numeric(lc) || !is_numeric(rc) {
            self.emit(super::err(
                format!("operator '+' requires numeric, String, or List types but got {} and {}", lc.display(), rc.display()),
                "Use + with numeric types, String, or List", format!("operator +")));
        }
        if *lc == Ty::Float || *rc == Ty::Float { Ty::Float } else { lt }
    }

    /// Resolve a module.func Member expression to a qualified call key.
    fn resolve_module_call(&self, object: &ast::Expr, field: &str) -> Option<String> {
        if let ast::Expr::Ident { name: module, .. } = object {
            if self.env.imported_stdlib.contains(&sym(module)) || self.env.user_modules.contains(&sym(module)) {
                return Some(format!("{}.{}", module, field));
            }
            if let Some(target) = self.env.module_aliases.get(&sym(module)) {
                return Some(format!("{}.{}", target, field));
            }
            // Check if Ident.field is a Type.method (protocol implementation)
            let key = format!("{}.{}", module, field);
            if self.env.functions.contains_key(&sym(&key)) {
                return Some(key);
            }
        }
        // TypeName.method (e.g. Val.double in pipe)
        if let ast::Expr::TypeName { name: type_name, .. } = object {
            let key = format!("{}.{}", type_name, field);
            if self.env.functions.contains_key(&sym(&key)) {
                return Some(key);
            }
        }
        None
    }
}

/// Collect all Ident names referenced in an expression (shallow, for var capture check).
fn collect_idents(expr: &ast::Expr, out: &mut Vec<String>) {
    match expr {
        ast::Expr::Ident { name, .. } => out.push(name.clone()),
        ast::Expr::Call { callee, args, .. } => {
            collect_idents(callee, out);
            for a in args { collect_idents(a, out); }
        }
        ast::Expr::Member { object, .. } | ast::Expr::TupleIndex { object, .. }
        | ast::Expr::IndexAccess { object, .. } => collect_idents(object, out),
        ast::Expr::Binary { left, right, .. } | ast::Expr::Pipe { left, right, .. } | ast::Expr::Compose { left, right, .. } => {
            collect_idents(left, out); collect_idents(right, out);
        }
        ast::Expr::Unary { operand, .. } | ast::Expr::Paren { expr: operand, .. }
        | ast::Expr::Some { expr: operand, .. } | ast::Expr::Ok { expr: operand, .. }
        | ast::Expr::Err { expr: operand, .. } | ast::Expr::Try { expr: operand, .. } => {
            collect_idents(operand, out);
        }
        ast::Expr::If { cond, then, else_, .. } => {
            collect_idents(cond, out); collect_idents(then, out); collect_idents(else_, out);
        }
        ast::Expr::List { elements, .. } | ast::Expr::Tuple { elements, .. } => {
            for e in elements { collect_idents(e, out); }
        }
        ast::Expr::Lambda { body, .. } => collect_idents(body, out),
        ast::Expr::InterpolatedString { parts, .. } => {
            for p in parts { if let ast::StringPart::Expr { expr } = p { collect_idents(expr, out); } }
        }
        ast::Expr::Record { fields, .. } => { for f in fields { collect_idents(&f.value, out); } }
        _ => {} // literals, none, unit, etc.
    }
}
