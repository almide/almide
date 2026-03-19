/// Expression type inference — Pass 1 of the constraint-based checker.
/// Walks the AST, assigns Ty to each expression, collects constraints.

use crate::ast;
use crate::types::{Ty, TypeConstructorId, VariantPayload};
use super::types::resolve_vars;
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
                self.env.used_vars.insert(name.clone());
                if let Some(ty) = self.env.lookup_var(name).cloned() { self.instantiate_ty(&ty) }
                else if let Some(ty) = self.env.top_lets.get(name).cloned() { self.instantiate_ty(&ty) }
                else if let Some(sig) = self.env.functions.get(name).cloned() {
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
                if let Some((type_name, _)) = self.env.constructors.get(name) { Ty::Named(type_name.clone(), vec![]) }
                else if let Some(ty) = self.env.top_lets.get(name).cloned() { ty }
                else { Ty::Named(name.clone(), vec![]) }
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
                    let type_name = self.env.constructors.get(n.as_str())
                        .map(|(vname, _)| vname.clone())
                        .unwrap_or_else(|| n.clone());
                    Ty::Named(type_name, vec![])
                }
                else {
                    let field_tys: Vec<(String, Ty)> = fields.iter().map(|f| {
                        let ty = self.infer_types.get(&f.value.id()).map(|it| resolve_vars(it, &self.solutions)).unwrap_or(Ty::Unknown);
                        (f.name.clone(), ty)
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
                let concrete = resolve_vars(&obj_ty, &self.solutions);
                self.resolve_field_type(&concrete, field)
            }

            ast::Expr::TupleIndex { object, index, .. } => {
                let obj_ty = self.infer_expr(object);
                match &obj_ty {
                    Ty::Tuple(elems) if *index < elems.len() => elems[*index].clone(),
                    _ => {
                        let concrete = resolve_vars(&obj_ty, &self.solutions);
                        match &concrete { Ty::Tuple(elems) if *index < elems.len() => elems[*index].clone(), _ => Ty::Unknown }
                    }
                }
            }

            ast::Expr::IndexAccess { object, index, .. } => {
                let obj_ty = self.infer_expr(object);
                self.infer_expr(index);
                let concrete = resolve_vars(&obj_ty, &self.solutions);
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
                        let lc = resolve_vars(&lt, &self.solutions);
                        let rc = resolve_vars(&rt, &self.solutions);
                        // + works for: String+String, List+List (concat), numeric+numeric (add)
                        let l_concat = matches!(&lc, Ty::String | Ty::Applied(TypeConstructorId::List, _));
                        let r_concat = matches!(&rc, Ty::String | Ty::Applied(TypeConstructorId::List, _));
                        let l_unknown = matches!(&lc, Ty::Unknown | Ty::TypeVar(_));
                        let r_unknown = matches!(&rc, Ty::Unknown | Ty::TypeVar(_));
                        let is_concat = (l_concat && (r_concat || r_unknown))
                            || (r_concat && l_unknown);
                        if is_concat {
                            lt // concat: return same type
                        } else {
                            let is_numeric = |t: &Ty| matches!(t, Ty::Int | Ty::Float | Ty::Unknown | Ty::TypeVar(_));
                            if !is_numeric(&lc) || !is_numeric(&rc) {
                                self.emit(super::err(
                                    format!("operator '+' requires numeric, String, or List types but got {} and {}", lc.display(), rc.display()),
                                    "Use + with numeric types, String, or List", format!("operator +")));
                            }
                            if lc == Ty::Float || rc == Ty::Float { Ty::Float } else { lt }
                        }
                    }
                    "-" | "*" | "/" | "%" => {
                        let lc = resolve_vars(&lt, &self.solutions);
                        let rc = resolve_vars(&rt, &self.solutions);
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
                        let lc = resolve_vars(&lt, &self.solutions);
                        let rc = resolve_vars(&rt, &self.solutions);
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
                let sc = resolve_vars(&subject_ty, &self.solutions);
                self.check_match_exhaustiveness(&sc, arms);
                let result = self.fresh_var();
                for arm in arms.iter_mut() {
                    self.env.push_scope();
                    let sub_c = resolve_vars(&subject_ty, &self.solutions);
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
                    idents.into_iter().filter(|name| self.env.mutable_vars.contains(name)).collect::<Vec<_>>()
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
                    let concrete = resolve_vars(&ty, &self.solutions);
                    match &concrete {
                        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
                        _ => ty,
                    }
                }).collect();
                match tys.len() {
                    1 => tys.into_iter().next().unwrap(),
                    _ => Ty::Tuple(tys.iter().map(|t| resolve_vars(t, &self.solutions)).collect()),
                }
            }

            ast::Expr::Call { callee, args, named_args, type_args, .. } => {
                self.infer_call(callee, args, named_args, type_args)
            }

            ast::Expr::Pipe { left, right, .. } => {
                self.infer_pipe(left, right)
            }

            ast::Expr::Lambda { params, body, .. } => {
                self.env.push_scope();
                let param_tys: Vec<Ty> = params.iter().map(|p| {
                    let ty = p.ty.as_ref().map(|te| self.resolve_type_expr(te)).unwrap_or_else(|| self.fresh_var());
                    let concrete = resolve_vars(&ty, &self.solutions);
                    self.env.define_var(&p.name, concrete);
                    ty
                }).collect();
                let ret_ty = self.infer_expr(body);
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
        let iter_resolved = resolve_vars(&iter_ty, &self.solutions);
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
                    let t = resolve_vars(&val_ty, &self.solutions);
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
                    let t = resolve_vars(&val_ty, &self.solutions);
                    if self.env.auto_unwrap {
                        match t {
                            Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args.into_iter().next().unwrap(),
                            other => other,
                        }
                    } else { t }
                };
                self.env.define_var(name, final_ty);
                self.env.mutable_vars.insert(name.clone());
            }
            ast::Stmt::LetDestructure { pattern, value, .. } => {
                let val_ty = self.infer_expr(value);
                let val_resolved = resolve_vars(&val_ty, &self.solutions);
                self.bind_pattern(pattern, &val_resolved);
            }
            ast::Stmt::Assign { name, value, .. } => {
                self.infer_expr(value);
                if self.env.lookup_var(name).is_some() && !self.env.mutable_vars.contains(name.as_str()) {
                    let hint = if self.env.param_vars.contains(name.as_str()) {
                        format!("'{}' is a function parameter (immutable). Use a local copy: var {0}_ = {0}", name)
                    } else {
                        format!("Use 'var {0} = ...' instead of 'let {0} = ...' to declare a mutable variable", name)
                    };
                    self.emit(super::err(
                        format!("cannot reassign immutable binding '{}'", name),
                        hint, format!("{} = ...", name)).with_code("E009"));
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
            ast::Pattern::Some { inner } => { let it = match ty { Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => args[0].clone(), _ => Ty::Unknown }; self.bind_pattern(inner, &it); }
            ast::Pattern::Ok { inner } => { let it = match ty { Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(), _ => Ty::Unknown }; self.bind_pattern(inner, &it); }
            ast::Pattern::Err { inner } => { let it = match ty { Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[1].clone(), _ => Ty::Unknown }; self.bind_pattern(inner, &it); }
            ast::Pattern::None | ast::Pattern::Literal { .. } => {}
        }
    }

    /// Resolve a module.func Member expression to a qualified call key.
    fn resolve_module_call(&self, object: &ast::Expr, field: &str) -> Option<String> {
        if let ast::Expr::Ident { name: module, .. } = object {
            if crate::stdlib::is_stdlib_module(module) || self.env.user_modules.contains(module.as_str()) {
                return Some(format!("{}.{}", module, field));
            }
            if let Some(target) = self.env.module_aliases.get(module.as_str()) {
                return Some(format!("{}.{}", target, field));
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
        ast::Expr::Binary { left, right, .. } | ast::Expr::Pipe { left, right, .. } => {
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
