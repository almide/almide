/// Almide type checker — inserted between parser and emitter.
/// Every error includes an actionable hint so LLMs can auto-repair.

use crate::ast;
use crate::diagnostic::Diagnostic;
use crate::stdlib;
use crate::types::{Ty, TypeEnv, FnSig, VariantCase, VariantPayload};

pub struct Checker {
    pub env: TypeEnv,
    pub diagnostics: Vec<Diagnostic>,
}

fn err(msg: String, hint: &str, ctx: &str) -> Diagnostic {
    Diagnostic::error(msg, hint, ctx)
}

fn err_s(msg: String, hint: String, ctx: String) -> Diagnostic {
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

    /// Register an imported module's exported functions and types.
    pub fn register_module(&mut self, mod_name: &str, prog: &ast::Program) {
        self.env.user_modules.insert(mod_name.to_string());
        for decl in &prog.decls {
            match decl {
                ast::Decl::Fn { name, params, return_type, effect, r#async, .. } => {
                    let param_tys: Vec<(String, Ty)> = params.iter()
                        .map(|p| (p.name.clone(), self.resolve_type_expr(&p.ty)))
                        .collect();
                    let ret = self.resolve_type_expr(return_type);
                    let is_effect = effect.unwrap_or(false) || r#async.unwrap_or(false);
                    let key = format!("{}.{}", mod_name, name);
                    self.env.functions.insert(key, FnSig { params: param_tys, ret, is_effect });
                }
                ast::Decl::Type { name, ty, .. } => {
                    let resolved = self.resolve_type_expr(ty);
                    let key = format!("{}.{}", mod_name, name);
                    self.env.types.insert(key, resolved);
                }
                _ => {}
            }
        }
    }

    pub fn check_program(&mut self, prog: &ast::Program) -> Vec<Diagnostic> {
        self.collect_declarations(prog);
        for decl in &prog.decls {
            self.check_decl(decl);
        }
        self.diagnostics.clone()
    }

    fn collect_declarations(&mut self, prog: &ast::Program) {
        for decl in &prog.decls {
            match decl {
                ast::Decl::Type { name, ty, .. } => {
                    let mut resolved = self.resolve_type_expr(ty);
                    if let Ty::Variant { name: ref mut vname, ref cases } = resolved {
                        *vname = name.clone();
                        for case in cases {
                            self.env.constructors.insert(case.name.clone(), (name.clone(), case.clone()));
                        }
                    }
                    self.env.types.insert(name.clone(), resolved);
                }
                ast::Decl::Fn { name, params, return_type, effect, r#async, .. } => {
                    let param_tys: Vec<(String, Ty)> = params.iter()
                        .map(|p| (p.name.clone(), self.resolve_type_expr(&p.ty)))
                        .collect();
                    let ret = self.resolve_type_expr(return_type);
                    let is_effect = effect.unwrap_or(false) || r#async.unwrap_or(false);
                    if is_effect { self.env.effect_fns.insert(name.clone()); }
                    self.env.functions.insert(name.clone(), FnSig { params: param_tys, ret, is_effect });
                }
                _ => {}
            }
        }
    }

    fn check_decl(&mut self, decl: &ast::Decl) {
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
                // Effect fn: codegen auto-wraps body in Ok() and appends ? to calls,
                // so body returning T is valid when declared return is Result[T, E].
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

    fn resolve_type_expr(&self, te: &ast::TypeExpr) -> Ty {
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

    // ── Expression type checking ───────────────────────────────────────

    fn check_expr(&mut self, expr: &ast::Expr) -> Ty {
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
                // In non-effect context, ok(()) is just Unit
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
                        self.diagnostics.push(err(
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
                    self.diagnostics.push(err(
                        format!("if condition has type {} but expected Bool", ct.display()),
                        "The condition must be a Bool expression", "if expression",
                    ));
                }
                let tt = self.check_expr(then);
                let et = self.check_expr(else_);
                if !tt.compatible(&et) {
                    self.diagnostics.push(err(
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
                            self.diagnostics.push(err(
                                format!("match guard has type {} but expected Bool", gt.display()),
                                "Guard conditions must be Bool", "match arm",
                            ));
                        }
                    }
                    let at = self.check_expr(&arm.body);
                    if let Some(ref mut prev) = result_ty {
                        // Allow mixing Result and non-Result types in match arms
                        // (e.g. err(e) => err(e), ok(x) => plain_value)
                        let compat = prev.compatible(&at)
                            || match (prev.clone(), &at) {
                                (Ty::Result(ok_ty, _), non_result) if !matches!(non_result, Ty::Result(_, _)) => ok_ty.compatible(non_result),
                                (_, Ty::Result(ok_ty, _)) if !matches!(prev.clone(), Ty::Result(_, _)) => prev.compatible(&ok_ty),
                                _ => false,
                            };
                        if !compat {
                            self.diagnostics.push(err(
                                format!("match arm has type {} but previous arms have type {}", at.display(), prev.display()),
                                "All match arms must have the same type", "match expression",
                            ));
                        }
                        // Widen the result type to the more specific one
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
                let ty = expr.as_ref().map(|e| self.check_expr(e)).unwrap_or(Ty::Unit);
                self.env.in_do_block = prev_do;
                self.env.pop_scope();
                // do blocks use guard for flow control, their actual type depends on context
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
                        self.diagnostics.push(err_s(
                            format!("cannot iterate over type {}", it.display()),
                            "for...in requires a List or Map".into(),
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
                let left_ty = self.check_expr(left);
                // Pipe passes left as first arg to right's call
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
                            self.diagnostics.push(err(
                                format!("'not' expects Bool but got {}", ot.display()),
                                "Use 'not' only with Bool values", "unary not",
                            ));
                        }
                        Ty::Bool
                    }
                    "-" => {
                        if !ot.compatible(&Ty::Int) && !ot.compatible(&Ty::Float) {
                            self.diagnostics.push(err(
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
                        self.diagnostics.push(err(
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

    // ── Call checking ──────────────────────────────────────────────────

    fn check_call(&mut self, callee: &ast::Expr, args: &[ast::Expr]) -> Ty {
        let arg_tys: Vec<Ty> = args.iter().map(|a| self.check_expr(a)).collect();

        if let ast::Expr::Member { object, field } = callee {
            if let ast::Expr::Ident { name: module } = object.as_ref() {
                return self.check_module_call(module, field, &arg_tys);
            }
        }

        if let ast::Expr::Ident { name } = callee {
            return self.check_direct_call(name, &arg_tys);
        }

        if let ast::Expr::TypeName { name } = callee {
            return self.check_constructor_call(name, &arg_tys);
        }

        let ct = self.check_expr(callee);
        match &ct {
            Ty::Fn { ret, .. } => *ret.clone(),
            _ => Ty::Unknown,
        }
    }

    fn check_direct_call(&mut self, name: &str, arg_tys: &[Ty]) -> Ty {
        match name {
            "println" | "eprintln" => {
                if arg_tys.len() != 1 {
                    self.diagnostics.push(err_s(
                        format!("{}() takes exactly 1 argument but got {}", name, arg_tys.len()),
                        format!("Use {}(\"message\")", name),
                        format!("{}()", name),
                    ));
                } else if !arg_tys[0].compatible(&Ty::String) {
                    self.diagnostics.push(err_s(
                        format!("{}() requires String but got {}", name, arg_tys[0].display()),
                        "Use int.to_string(n) to convert to String first".into(),
                        format!("{}()", name),
                    ));
                }
                return Ty::Unit;
            }
            "assert" => {
                if arg_tys.len() == 1 && !arg_tys[0].compatible(&Ty::Bool) {
                    self.diagnostics.push(err(
                        format!("assert() requires Bool but got {}", arg_tys[0].display()),
                        "Pass a boolean condition to assert()", "assert()",
                    ));
                }
                return Ty::Unit;
            }
            "assert_eq" | "assert_ne" => return Ty::Unit,
            "ok" => return Ty::Result(Box::new(arg_tys.first().cloned().unwrap_or(Ty::Unit)), Box::new(Ty::Unknown)),
            "err" => return Ty::Result(Box::new(Ty::Unknown), Box::new(arg_tys.first().cloned().unwrap_or(Ty::Unknown))),
            "some" => return Ty::Option(Box::new(arg_tys.first().cloned().unwrap_or(Ty::Unknown))),
            _ => {}
        }

        if let Some(sig) = self.env.functions.get(name).cloned() {
            if sig.is_effect && !self.env.in_effect {
                self.diagnostics.push(err_s(
                    format!("cannot call effect function '{}' from a pure function", name),
                    "Mark the calling function as 'effect fn' to allow side effects".into(),
                    format!("call to {}()", name),
                ));
            }
            if arg_tys.len() != sig.params.len() {
                let expected = sig.params.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect::<Vec<_>>().join(", ");
                self.diagnostics.push(err_s(
                    format!("function '{}' expects {} argument(s) but got {}", name, sig.params.len(), arg_tys.len()),
                    format!("Expected: {}({})", name, expected),
                    format!("call to {}()", name),
                ));
            } else {
                for (i, ((pname, pty), aty)) in sig.params.iter().zip(arg_tys.iter()).enumerate() {
                    if !pty.compatible(aty) {
                        self.diagnostics.push(err_s(
                            format!("argument '{}' (position {}) expects {} but got {}", pname, i + 1, pty.display(), aty.display()),
                            format!("Pass a value of type {}", pty.display()),
                            format!("call to {}()", name),
                        ));
                    }
                }
            }
            return sig.ret.clone();
        }

        if self.env.constructors.contains_key(name) {
            return self.check_constructor_call(name, arg_tys);
        }

        Ty::Unknown
    }

    fn check_constructor_call(&mut self, name: &str, arg_tys: &[Ty]) -> Ty {
        if let Some((type_name, case)) = self.env.constructors.get(name).cloned() {
            match &case.payload {
                VariantPayload::Unit => {
                    if !arg_tys.is_empty() {
                        self.diagnostics.push(err_s(
                            format!("constructor '{}' takes no arguments but got {}", name, arg_tys.len()),
                            format!("Use {} without parentheses", name),
                            format!("constructor {}", name),
                        ));
                    }
                }
                VariantPayload::Tuple(expected) => {
                    if arg_tys.len() != expected.len() {
                        let exp = expected.iter().map(|t| t.display()).collect::<Vec<_>>().join(", ");
                        self.diagnostics.push(err_s(
                            format!("constructor '{}' expects {} argument(s) but got {}", name, expected.len(), arg_tys.len()),
                            format!("{}({})", name, exp),
                            format!("constructor {}", name),
                        ));
                    }
                }
                VariantPayload::Record(_) => {}
            }
            return Ty::Named(type_name);
        }
        Ty::Unknown
    }

    fn check_module_call(&mut self, module: &str, func: &str, arg_tys: &[Ty]) -> Ty {
        if let Some(sig) = self.lookup_stdlib(module, func) {
            let min_params = stdlib::min_params(module, func).unwrap_or(sig.params.len());
            if arg_tys.len() < min_params || arg_tys.len() > sig.params.len() {
                let usage = sig.params.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect::<Vec<_>>().join(", ");
                self.diagnostics.push(err_s(
                    format!("{}.{}() expects {} argument(s) but got {}", module, func, sig.params.len(), arg_tys.len()),
                    format!("Usage: {}.{}({})", module, func, usage),
                    format!("{}.{}()", module, func),
                ));
            } else {
                for (i, ((pname, pty), aty)) in sig.params.iter().zip(arg_tys.iter()).enumerate() {
                    if !pty.compatible(aty) {
                        self.diagnostics.push(err_s(
                            format!("{}.{}() argument '{}' (position {}) expects {} but got {}", module, func, pname, i + 1, pty.display(), aty.display()),
                            format!("Pass a value of type {}", pty.display()),
                            format!("{}.{}()", module, func),
                        ));
                    }
                }
            }
            if sig.is_effect && !self.env.in_effect {
                self.diagnostics.push(err_s(
                    format!("{}.{}() is an effect function and cannot be called from a pure function", module, func),
                    "Mark the calling function as 'effect fn'".into(),
                    format!("{}.{}()", module, func),
                ));
            }
            // In effect fn context, codegen auto-appends `?` to Result-returning calls,
            // so the effective return type is the Ok variant.
            let ret = sig.ret.clone();
            if self.env.in_effect {
                if let Ty::Result(ok_ty, _) = &ret {
                    return *ok_ty.clone();
                }
            }
            return ret;
        }

        // Check user-defined modules
        if self.env.user_modules.contains(module) {
            let key = format!("{}.{}", module, func);
            if let Some(sig) = self.env.functions.get(&key).cloned() {
                if arg_tys.len() != sig.params.len() {
                    let usage = sig.params.iter().map(|(n, t)| format!("{}: {}", n, t.display())).collect::<Vec<_>>().join(", ");
                    self.diagnostics.push(err_s(
                        format!("{}.{}() expects {} argument(s) but got {}", module, func, sig.params.len(), arg_tys.len()),
                        format!("Usage: {}.{}({})", module, func, usage),
                        format!("{}.{}()", module, func),
                    ));
                } else {
                    for (i, ((pname, pty), aty)) in sig.params.iter().zip(arg_tys.iter()).enumerate() {
                        if !pty.compatible(aty) {
                            self.diagnostics.push(err_s(
                                format!("{}.{}() argument '{}' (position {}) expects {} but got {}", module, func, pname, i + 1, pty.display(), aty.display()),
                                format!("Pass a value of type {}", pty.display()),
                                format!("{}.{}()", module, func),
                            ));
                        }
                    }
                }
                if sig.is_effect && !self.env.in_effect {
                    self.diagnostics.push(err_s(
                        format!("{}.{}() is an effect function and cannot be called from a pure function", module, func),
                        "Mark the calling function as 'effect fn'".into(),
                        format!("{}.{}()", module, func),
                    ));
                }
                return sig.ret.clone();
            }
        }

        Ty::Unknown
    }

    fn check_member_access(&mut self, obj_ty: &Ty, field: &str) -> Ty {
        let resolved = self.env.resolve_named(obj_ty);
        match &resolved {
            Ty::Record { fields } => {
                for (name, ty) in fields {
                    if name == field { return ty.clone(); }
                }
                let avail = fields.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ");
                self.diagnostics.push(err_s(
                    format!("record has no field '{}'", field),
                    format!("Available fields: {}", avail),
                    format!("field access .{}", field),
                ));
                Ty::Unknown
            }
            Ty::Unknown => Ty::Unknown,
            _ => Ty::Unknown,
        }
    }

    fn check_binary_op(&mut self, op: &str, left: &Ty, right: &Ty) -> Ty {
        if matches!(left, Ty::Unknown) || matches!(right, Ty::Unknown) {
            return match op {
                "==" | "!=" | "<" | ">" | "<=" | ">=" | "and" | "or" => Ty::Bool,
                "++" => left.clone(),
                _ => Ty::Unknown,
            };
        }
        match op {
            "+" | "-" | "*" | "/" | "%" => {
                if left.compatible(&Ty::Int) && right.compatible(&Ty::Int) { Ty::Int }
                else if (left.compatible(&Ty::Float) || left.compatible(&Ty::Int))
                    && (right.compatible(&Ty::Float) || right.compatible(&Ty::Int)) { Ty::Float }
                else {
                    self.diagnostics.push(err_s(
                        format!("operator '{}' requires numeric types but got {} and {}", op, left.display(), right.display()),
                        "Use Int or Float values with arithmetic operators".into(),
                        format!("operator '{}'", op),
                    ));
                    Ty::Unknown
                }
            }
            "^" => {
                if left.compatible(&Ty::Int) && right.compatible(&Ty::Int) { Ty::Int }
                else {
                    self.diagnostics.push(err(
                        format!("'^' (XOR) requires Int but got {} and {}", left.display(), right.display()),
                        "XOR only works on Int values", "operator '^'",
                    ));
                    Ty::Unknown
                }
            }
            "++" => {
                if left.compatible(&Ty::String) && right.compatible(&Ty::String) { Ty::String }
                else if matches!(left, Ty::List(_)) && left.compatible(right) { left.clone() }
                else {
                    self.diagnostics.push(err(
                        format!("'++' requires String or List but got {} and {}", left.display(), right.display()),
                        "Use '++' for String or List concatenation", "operator '++'",
                    ));
                    Ty::Unknown
                }
            }
            "==" | "!=" | "<" | ">" | "<=" | ">=" => Ty::Bool,
            "and" | "or" => {
                if !left.compatible(&Ty::Bool) {
                    self.diagnostics.push(err_s(
                        format!("'{}' requires Bool but left side is {}", op, left.display()),
                        "Use Bool values with logical operators".into(),
                        format!("operator '{}'", op),
                    ));
                }
                if !right.compatible(&Ty::Bool) {
                    self.diagnostics.push(err_s(
                        format!("'{}' requires Bool but right side is {}", op, right.display()),
                        "Use Bool values with logical operators".into(),
                        format!("operator '{}'", op),
                    ));
                }
                Ty::Bool
            }
            _ => Ty::Unknown,
        }
    }

    fn check_stmt(&mut self, stmt: &ast::Stmt) {
        match stmt {
            ast::Stmt::Let { name, ty, value } => {
                let vt = self.check_expr(value);
                // In do blocks, auto-unwrap Result types
                let vt = if self.env.in_do_block {
                    match vt {
                        Ty::Result(ok, _) => *ok,
                        other => other,
                    }
                } else { vt };
                let dt = if let Some(te) = ty {
                    let t = self.resolve_type_expr(te);
                    if !t.compatible(&vt) {
                        self.diagnostics.push(err_s(
                            format!("cannot assign {} to variable '{}' of type {}", vt.display(), name, t.display()),
                            "Change the type annotation or the value".into(),
                            format!("let {} = ...", name),
                        ));
                    }
                    t
                } else { vt };
                self.env.define_var(name, dt);
            }
            ast::Stmt::Var { name, ty, value } => {
                let vt = self.check_expr(value);
                let dt = if let Some(te) = ty {
                    let t = self.resolve_type_expr(te);
                    if !t.compatible(&vt) {
                        self.diagnostics.push(err_s(
                            format!("cannot assign {} to variable '{}' of type {}", vt.display(), name, t.display()),
                            "Change the type annotation or the value".into(),
                            format!("var {} = ...", name),
                        ));
                    }
                    t
                } else { vt };
                self.env.define_var(name, dt);
            }
            ast::Stmt::LetDestructure { fields, value } => {
                let vt = self.check_expr(value);
                let resolved = self.env.resolve_named(&vt);
                match &resolved {
                    Ty::Record { fields: rec_fields } => {
                        for fname in fields {
                            let ft = rec_fields.iter().find(|(n, _)| n == fname)
                                .map(|(_, t)| t.clone())
                                .unwrap_or_else(|| {
                                    let avail = rec_fields.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ");
                                    self.diagnostics.push(err_s(
                                        format!("record has no field '{}'", fname),
                                        format!("Available fields: {}", avail),
                                        format!("let {{ {} }} = ...", fields.join(", ")),
                                    ));
                                    Ty::Unknown
                                });
                            self.env.define_var(fname, ft);
                        }
                    }
                    Ty::Unknown => { for f in fields { self.env.define_var(f, Ty::Unknown); } }
                    _ => {
                        self.diagnostics.push(err_s(
                            format!("cannot destructure type {}", vt.display()),
                            "Destructuring only works on record types".into(),
                            format!("let {{ {} }} = ...", fields.join(", ")),
                        ));
                        for f in fields { self.env.define_var(f, Ty::Unknown); }
                    }
                }
            }
            ast::Stmt::Assign { name, value } => {
                let vt = self.check_expr(value);
                if let Some(var_ty) = self.env.lookup_var(name).cloned() {
                    if !var_ty.compatible(&vt) {
                        self.diagnostics.push(err_s(
                            format!("cannot assign {} to variable '{}' of type {}", vt.display(), name, var_ty.display()),
                            "Assignment must match the variable's declared type".into(),
                            format!("{} = ...", name),
                        ));
                    }
                }
            }
            ast::Stmt::Guard { cond, else_ } => {
                let ct = self.check_expr(cond);
                if !ct.compatible(&Ty::Bool) {
                    self.diagnostics.push(err(
                        format!("guard condition has type {} but expected Bool", ct.display()),
                        "Guard conditions must be Bool", "guard statement",
                    ));
                }
                self.check_expr(else_);
            }
            ast::Stmt::Expr { expr } => { self.check_expr(expr); }
        }
    }

    fn check_pattern(&mut self, pattern: &ast::Pattern, subject_ty: &Ty) {
        match pattern {
            ast::Pattern::Wildcard => {}
            ast::Pattern::Ident { name } => { self.env.define_var(name, subject_ty.clone()); }
            ast::Pattern::Literal { .. } => {}
            ast::Pattern::Some { inner } => {
                let it = match subject_ty { Ty::Option(t) => *t.clone(), _ => Ty::Unknown };
                self.check_pattern(inner, &it);
            }
            ast::Pattern::None => {}
            ast::Pattern::Ok { inner } => {
                let it = match subject_ty { Ty::Result(t, _) => *t.clone(), _ => Ty::Unknown };
                self.check_pattern(inner, &it);
            }
            ast::Pattern::Err { inner } => {
                let it = match subject_ty { Ty::Result(_, e) => *e.clone(), _ => Ty::Unknown };
                self.check_pattern(inner, &it);
            }
            ast::Pattern::Constructor { name, args } => {
                if let Some((_, case)) = self.env.constructors.get(name).cloned() {
                    if let VariantPayload::Tuple(field_tys) = &case.payload {
                        for (pat, ty) in args.iter().zip(field_tys.iter()) { self.check_pattern(pat, ty); }
                    } else {
                        for pat in args { self.check_pattern(pat, &Ty::Unknown); }
                    }
                } else {
                    for pat in args { self.check_pattern(pat, &Ty::Unknown); }
                }
            }
            ast::Pattern::RecordPattern { fields, .. } => {
                for field in fields {
                    if let Some(ref pat) = field.pattern { self.check_pattern(pat, &Ty::Unknown); }
                    else { self.env.define_var(&field.name, Ty::Unknown); }
                }
            }
        }
    }

    // ── Standard library signatures ────────────────────────────────────

    fn register_stdlib(&mut self) {
        for name in stdlib::builtin_effect_fns() {
            self.env.effect_fns.insert(name.to_string());
        }
    }

    fn lookup_stdlib(&self, module: &str, func: &str) -> Option<FnSig> {
        stdlib::lookup_sig(module, func)
    }
}
