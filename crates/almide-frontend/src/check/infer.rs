/// Expression type inference — Pass 1 of the constraint-based checker.
/// Walks the AST, populates TypeMap (ExprId→Ty), collects constraints.

use almide_lang::ast;
use almide_lang::ast::ExprKind;
use almide_base::intern::{Sym, sym};
use crate::types::{Ty, TypeConstructorId, VariantPayload};
use super::types::resolve_ty;
use super::Checker;

impl Checker {
    pub(crate) fn infer_expr(&mut self, expr: &mut ast::Expr) -> Ty {
        if let Some(span) = expr.span {
            self.current_span = Some(span);
        }
        let ity = self.infer_expr_inner(expr);
        self.type_map.insert(expr.id, ity.clone());
        ity
    }

    fn infer_expr_inner(&mut self, expr: &mut ast::Expr) -> Ty {
        match &mut expr.kind {
            ExprKind::Int { .. } => Ty::Int,
            ExprKind::Float { .. } => Ty::Float,
            ExprKind::String { .. } => Ty::String,
            ExprKind::InterpolatedString { parts, .. } => {
                for part in parts.iter_mut() {
                    if let ast::StringPart::Expr { expr } = part {
                        self.infer_expr(expr);
                    }
                }
                Ty::String
            }
            ExprKind::Bool { .. } => Ty::Bool,
            ExprKind::Unit => Ty::Unit,

            ExprKind::None => Ty::option(self.fresh_var()),

            ExprKind::Ident { name, .. } => {
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
                    // Only suggest `import` for modules that require explicit import
                    // and whose names won't be confused with common variable names.
                    // e.g. `value`, `error`, `string`, `list` are too common as
                    // variable names — suggesting `import value` is misleading.
                    let hint = if crate::stdlib::is_import_suggestable(name) {
                        let desc = crate::stdlib::module_description(name);
                        format!("Add `import {}` (stdlib: {})\nOr run `almide fmt` to auto-add missing imports", name, desc)
                    } else {
                        // "Did you mean?" suggestion from variables, top_lets, and functions in scope
                        let candidates = self.env.all_visible_names();
                        if let Some(suggestion) = almide_base::diagnostic::suggest(name, candidates.iter().map(|s| s.as_str())) {
                            format!("Did you mean `{}`?", suggestion)
                        } else {
                            "Check the variable name".to_string()
                        }
                    };
                    self.emit(super::err(format!("undefined variable '{}'", name), hint, format!("variable {}", name)).with_code("E003"));
                    Ty::Unknown
                }
            }

            ExprKind::TypeName { name, .. } => {
                if let Some((type_name, case)) = self.env.constructors.get(&sym(name)).cloned() {
                    match &case.payload {
                        VariantPayload::Tuple(tys) if !tys.is_empty() => {
                            // Constructor with payload used as value → function type
                            let generic_args = self.instantiate_type_generics(type_name.as_str());
                            let ret = Ty::Named(type_name, generic_args.clone());
                            let params = if generic_args.is_empty() {
                                tys.clone()
                            } else {
                                // Substitute TypeVars with fresh inference vars
                                if let Some(ty_def) = self.env.types.get(&type_name).cloned() {
                                    let mut type_var_names = Vec::new();
                                    crate::types::TypeEnv::collect_typevars(&ty_def, &mut type_var_names);
                                    let subst: std::collections::HashMap<Sym, Ty> = type_var_names.iter()
                                        .zip(generic_args.iter())
                                        .map(|(tv, fresh)| (*tv, fresh.clone()))
                                        .collect();
                                    tys.iter().map(|t| super::calls::subst_ty(t, &subst)).collect()
                                } else {
                                    tys.clone()
                                }
                            };
                            Ty::Fn { params, ret: Box::new(ret) }
                        }
                        _ => Ty::Named(type_name, vec![])
                    }
                }
                else if let Some(ty) = self.env.top_lets.get(&sym(name)).cloned() { ty }
                else { Ty::Named(sym(name), vec![]) }
            }

            ExprKind::List { elements, .. } => {
                if elements.is_empty() { Ty::list(self.fresh_var()) }
                else {
                    let first = self.infer_expr(&mut elements[0]);
                    for elem in elements.iter_mut().skip(1) { let et = self.infer_expr(elem); self.constrain(first.clone(), et, "list element"); }
                    Ty::list(first)
                }
            }

            ExprKind::Tuple { elements, .. } => Ty::Tuple(elements.iter_mut().map(|e| self.infer_expr(e)).collect()),

            ExprKind::Record { name, fields, .. } => {
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
                        let ty = self.type_map.get(&f.value.id).map(|it| resolve_ty(it, &self.uf)).unwrap_or(Ty::Unknown);
                        (sym(&f.name), ty)
                    }).collect();
                    Ty::Record { fields: field_tys }
                }
            }

            ExprKind::SpreadRecord { base, fields, .. } => {
                let base_ty = self.infer_expr(base);
                for f in fields.iter_mut() { self.infer_expr(&mut f.value); }
                base_ty
            }

            ExprKind::Member { object, field, .. } => {
                // Module function used as a first-class value: `string.len`,
                // `list.map`, etc. Detect this BEFORE inferring the object
                // (which would fail because `string` is not a variable) and
                // return the function's type signature.
                if let ExprKind::Ident { name: mod_name, .. } = &object.kind {
                    if let Some(sig) = crate::stdlib::lookup_sig(mod_name, field) {
                        self.type_map.insert(object.id, Ty::Unit); // placeholder; object isn't evaluated
                        return Ty::Fn {
                            params: sig.params.iter().map(|(_, t)| t.clone()).collect(),
                            ret: Box::new(sig.ret.clone()),
                        };
                    }
                    let key = format!("{}.{}", mod_name, field);
                    if let Some(sig) = self.env.functions.get(&sym(&key)).cloned() {
                        self.type_map.insert(object.id, Ty::Unit);
                        return Ty::Fn {
                            params: sig.params.iter().map(|(_, t)| t.clone()).collect(),
                            ret: Box::new(sig.ret.clone()),
                        };
                    }
                    // Cross-module top-level `let` access: `utils.CATEGORY_ORDER`.
                    // Spec Visibility section applies to fn, type, AND let.
                    if let Some(let_ty) = self.env.top_lets.get(&sym(&key)).cloned() {
                        self.type_map.insert(object.id, Ty::Unit);
                        return let_ty;
                    }
                    // Cross-module variant constructor as value: dispatch.Never, binary.ImportFunc
                    if let Some((type_name, case)) = self.env.constructors.get(&sym(field)).cloned() {
                        let resolved_mod = self.env.import_table.resolve(mod_name)
                            .unwrap_or(sym(mod_name));
                        let qualified = format!("{}.{}", resolved_mod.as_str(), type_name.as_str());
                        if self.env.types.contains_key(&sym(&qualified)) {
                            self.type_map.insert(object.id, Ty::Unit);
                            let generic_args = self.instantiate_type_generics(type_name.as_str());
                            return match &case.payload {
                                VariantPayload::Unit => Ty::Named(type_name, generic_args),
                                VariantPayload::Tuple(param_tys) => Ty::Fn {
                                    params: param_tys.clone(),
                                    ret: Box::new(Ty::Named(type_name, generic_args)),
                                },
                                VariantPayload::Record(_) => Ty::Named(type_name, generic_args),
                            };
                        }
                    }
                }
                let obj_ty = self.infer_expr(object);
                let concrete = resolve_ty(&obj_ty, &self.uf);
                let field_ty = self.resolve_field_type(&concrete, field);
                // Almide-friendly diagnostic for list / string field access:
                // LLMs trained on Haskell / Python / Ruby write `xs.head`,
                // `xs.tail`, `xs.length`, `s.length`. In Almide these are
                // stdlib calls — intercept here so rustc never leaks
                // `error[E0609]: no field 'head' on type 'Vec<i64>'`.
                if matches!(field_ty, Ty::Unknown) {
                    let module_and_subs: Option<(&str, Vec<(&str, String)>)> = match &concrete {
                        Ty::Applied(TypeConstructorId::List, _) => Some(("list", vec![
                            ("head", "list.first(xs)  // returns Option[T]".into()),
                            ("tail", "list.drop(xs, 1)".into()),
                            ("length", "list.len(xs)".into()),
                            ("len", "list.len(xs)".into()),
                            ("first", "list.first(xs)".into()),
                            ("last", "list.last(xs)".into()),
                            ("size", "list.len(xs)".into()),
                        ])),
                        Ty::String => Some(("string", vec![
                            ("length", "string.len(s)".into()),
                            ("len", "string.len(s)".into()),
                            ("size", "string.len(s)".into()),
                            ("chars", "string.to_chars(s)".into()),
                        ])),
                        _ => None,
                    };
                    if let Some((module, subs)) = module_and_subs {
                        let hint = if let Some((_, snippet)) = subs.iter().find(|(n, _)| n == field) {
                            format!(
                                "Almide values have no fields — use the `{m}` stdlib module. Replace `x.{f}` with `{snippet}`. No method-call or field-access syntax is supported.",
                                m = module, f = field, snippet = snippet
                            )
                        } else {
                            format!(
                                "Almide values have no fields. Use `{m}.<fn>(x)` (or `x |> {m}.<fn>`) — see docs/stdlib/{m}.md for available functions.",
                                m = module
                            )
                        };
                        self.emit(super::err(
                            format!("no field '{}' on {}", field, module),
                            hint,
                            format!("field access .{}", field),
                        ).with_code("E013"));
                    }
                }
                field_ty
            }

            ExprKind::TupleIndex { object, index, .. } => {
                let obj_ty = self.infer_expr(object);
                match &obj_ty {
                    Ty::Tuple(elems) if *index < elems.len() => elems[*index].clone(),
                    _ => {
                        let concrete = resolve_ty(&obj_ty, &self.uf);
                        match &concrete { Ty::Tuple(elems) if *index < elems.len() => elems[*index].clone(), _ => Ty::Unknown }
                    }
                }
            }

            ExprKind::IndexAccess { object, index, .. } => {
                let obj_ty = self.infer_expr(object);
                self.infer_expr(index);
                let concrete = resolve_ty(&obj_ty, &self.uf);
                match &concrete {
                    Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => args[0].clone(),
                    Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 => Ty::option(args[1].clone()),
                    _ => Ty::Unknown,
                }
            }

            ExprKind::Binary { op, left, right, .. } => {
                let lt = self.infer_expr(left);
                let rt = self.infer_expr(right);
                match op.as_str() {
                    "+" => {
                        let lc = resolve_ty(&lt, &self.uf);
                        let rc = resolve_ty(&rt, &self.uf);
                        self.infer_plus_op(&lc, &rc, lt)
                    }
                    "-" | "*" | "/" | "%" | "^" => {
                        let lc = resolve_ty(&lt, &self.uf);
                        let rc = resolve_ty(&rt, &self.uf);
                        // Matrix operators: *, +, - on Matrix types
                        if lc == Ty::Matrix || rc == Ty::Matrix {
                            Ty::Matrix
                        } else {
                            let is_numeric = |t: &Ty| matches!(t, Ty::Int | Ty::Float | Ty::Unknown | Ty::TypeVar(_));
                            if !is_numeric(&lc) || !is_numeric(&rc) {
                                self.emit(super::err(
                                    format!("operator '{}' requires numeric types but got {} and {}", op, lc.display(), rc.display()),
                                    "Use numeric types (Int or Float)", format!("operator {}", op)));
                            }
                            if lc == Ty::Float || rc == Ty::Float { Ty::Float } else { lt }
                        }
                    }
                    "++" => {
                        self.emit(super::err(
                            format!("operator '++' has been removed. Use '+' for concatenation"),
                            "Replace ++ with +", "operator ++"));
                        lt
                    }
                    "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                        // Check none comparison: only valid with Option types
                        let left_is_none = matches!(left.kind, ExprKind::None);
                        let right_is_none = matches!(right.kind, ExprKind::None);
                        if right_is_none && !left_is_none {
                            let lc = resolve_ty(&lt, &self.uf);
                            if !lc.is_option() && !matches!(lc, Ty::Unknown | Ty::TypeVar(_)) {
                                self.emit(super::err(
                                    format!("cannot compare {} with none — only Option types support none comparison", lc.display()),
                                    "Use Option type or check with is_ok()/is_err() for Result", "comparison with none"));
                            }
                        }
                        if left_is_none && !right_is_none {
                            let rc = resolve_ty(&rt, &self.uf);
                            if !rc.is_option() && !matches!(rc, Ty::Unknown | Ty::TypeVar(_)) {
                                self.emit(super::err(
                                    format!("cannot compare none with {} — only Option types support none comparison", rc.display()),
                                    "Use Option type or check with is_ok()/is_err() for Result", "comparison with none"));
                            }
                        }
                        // Unify left/right types so TypeVars in none/err/constructors get resolved
                        self.unify_infer(&lt, &rt);
                        Ty::Bool
                    }
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
                    _ => lt,
                }
            }

            ExprKind::Unary { op, operand, .. } => {
                let t = self.infer_expr(operand);
                match op.as_str() { "not" => Ty::Bool, _ => t }
            }

            ExprKind::If { cond, then, else_, .. } => {
                self.infer_expr(cond);
                let then_ty = self.infer_expr(then);
                let else_ty = self.infer_expr(else_);
                self.constrain(then_ty.clone(), else_ty, "if branches");
                then_ty
            }

            ExprKind::Match { subject, arms, .. } => {
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

            ExprKind::Block { stmts, expr, .. } => {
                self.env.push_scope();
                for stmt in stmts.iter_mut() { self.check_stmt(stmt); }
                let ty = if let Some(e) = expr { self.infer_expr(e) } else { Ty::Unit };
                self.env.pop_scope();
                ty
            }

            ExprKind::Fan { exprs, .. } => {
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

            ExprKind::Call { callee, args, named_args, type_args, .. } => {
                self.infer_call(callee, args, named_args, type_args)
            }

            ExprKind::Pipe { left, right, .. } => {
                self.infer_pipe(left, right)
            }

            ExprKind::Compose { left, right, .. } => {
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

            ExprKind::Lambda { params, body, .. } => {
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

            ExprKind::ForIn { var, var_tuple, iterable, body, .. } => {
                self.infer_for_in(var, var_tuple, iterable, body)
            }

            ExprKind::While { cond, body, .. } => {
                self.infer_expr(cond);
                self.env.push_scope();
                for stmt in body.iter_mut() { self.check_stmt(stmt); }
                self.env.pop_scope();
                Ty::Unit
            }

            ExprKind::Range { start, end, .. } => { let st = self.infer_expr(start); self.infer_expr(end); Ty::list(st) }

            ExprKind::Some { expr, .. } => { let inner = self.infer_expr(expr); Ty::option(inner) }
            ExprKind::Ok { expr, .. } => {
                let ok_ty = self.infer_expr(expr);
                let err_ty = match &self.env.current_ret {
                    Some(Ty::Applied(TypeConstructorId::Result, args)) if args.len() == 2 => args[1].clone(),
                    _ => self.fresh_var(),
                };
                Ty::result(ok_ty, err_ty)
            }
            ExprKind::Err { expr, .. } => {
                let err_ty = self.infer_expr(expr);
                let ok_ty = match &self.env.current_ret {
                    Some(Ty::Applied(TypeConstructorId::Result, args)) if args.len() == 2 => args[0].clone(),
                    _ => self.fresh_var(),
                };
                Ty::result(ok_ty, err_ty)
            }
            ExprKind::Try { expr, .. } => {
                let ty = self.infer_expr(expr);
                match &ty {
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() >= 1 => args[0].clone(),
                    _ => ty,
                }
            }

            ExprKind::Paren { expr, .. } => self.infer_expr(expr),
            ExprKind::Break | ExprKind::Continue => Ty::Unit,
            ExprKind::Hole | ExprKind::Todo { .. } => self.fresh_var(),
            ExprKind::Await { expr, .. } => self.infer_expr(expr),

            // expr! — unwrap with propagation (Option[T] → T, Result[T,E] → T)
            ExprKind::Unwrap { expr: inner, .. } => {
                let t = self.infer_expr(inner);
                let resolved = resolve_ty(&t, &self.uf);
                if let Some(inner_ty) = resolved.option_inner().or_else(|| resolved.result_ok_ty()) {
                    inner_ty
                } else if matches!(&resolved, Ty::Unknown | Ty::TypeVar(_)) {
                    self.fresh_var()
                } else {
                    self.emit(super::err(
                        format!("operator '!' requires Option or Result type but got {}", resolved.display()),
                        "Use '!' only on Option[T] or Result[T, E] values",
                        "operator !",
                    ));
                    Ty::Unknown
                }
            }
            // expr ?? fallback — unwrap with default (Option[T] → T, Result[T,E] → T)
            ExprKind::UnwrapOr { expr: inner, fallback, .. } => {
                let t = self.infer_expr(inner);
                let ft = self.infer_expr(fallback);
                let resolved = resolve_ty(&t, &self.uf);
                let inner_ty = if let Some(ty) = resolved.option_inner().or_else(|| resolved.result_ok_ty()) {
                    ty
                } else if matches!(&resolved, Ty::Unknown | Ty::TypeVar(_)) {
                    ft.clone()
                } else {
                    self.emit(super::err(
                        format!("operator '??' requires Option or Result type but got {}", resolved.display()),
                        "Use '??' only on Option[T] or Result[T, E] values",
                        "operator ??",
                    ));
                    ft.clone()
                };
                self.unify_infer(&inner_ty, &ft);
                inner_ty
            }
            // expr? — to Option (Result[T,E] → Option[T], Option[T] → Option[T])
            ExprKind::ToOption { expr: inner, .. } => {
                let t = self.infer_expr(inner);
                let resolved = resolve_ty(&t, &self.uf);
                if let Some(ok_ty) = resolved.result_ok_ty() {
                    Ty::option(ok_ty)
                } else if resolved.is_option() {
                    resolved.clone()
                } else if matches!(&resolved, Ty::Unknown | Ty::TypeVar(_)) {
                    Ty::option(self.fresh_var())
                } else {
                    self.emit(super::err(
                        format!("operator '?' requires Option or Result type but got {}", resolved.display()),
                        "Use '?' only on Option[T] or Result[T, E] values",
                        "operator ?",
                    ));
                    Ty::Unknown
                }
            }

            // expr?.field — optional chaining: Option[T] → access T.field → Option[FieldType]
            ExprKind::OptionalChain { expr: inner, field, .. } => {
                let t = self.infer_expr(inner);
                let resolved = resolve_ty(&t, &self.uf);
                let inner_ty = if let Some(ty) = resolved.option_inner() {
                    ty
                } else if matches!(&resolved, Ty::Unknown | Ty::TypeVar(_)) {
                    return self.fresh_var();
                } else {
                    self.emit(super::err(
                        format!("operator '?.' requires Option type but got {}", resolved.display()),
                        "Use '?.' only on Option[T] values",
                        "operator ?.",
                    ));
                    return Ty::Unknown;
                };
                // Resolve field type from inner_ty
                match &inner_ty {
                    Ty::Record { fields } | Ty::OpenRecord { fields } => {
                        if let Some((_, field_ty)) = fields.iter().find(|(n, _)| n == field) {
                            Ty::option(field_ty.clone())
                        } else {
                            self.emit(super::err(
                                format!("field '{}' not found on type {}", field, inner_ty.display()),
                                "Check the field name",
                                format!("field {}", field),
                            ));
                            Ty::Unknown
                        }
                    }
                    _ => {
                        let field_ty = self.resolve_field_type(&inner_ty, field);
                        if !matches!(field_ty, Ty::Unknown) {
                            Ty::option(field_ty)
                        } else {
                            self.emit(super::err(
                                format!("cannot access field '{}' on type {}", field, inner_ty.display()),
                                "Optional chaining requires a record type inside Option",
                                format!("field {}", field),
                            ));
                            Ty::Unknown
                        }
                    }
                }
            }

            ExprKind::Error | ExprKind::Placeholder => Ty::Unknown,

            ExprKind::MapLiteral { entries, .. } => {
                if entries.is_empty() { Ty::map_of(self.fresh_var(), self.fresh_var()) }
                else {
                    let kt = self.infer_expr(&mut entries[0].0);
                    let vt = self.infer_expr(&mut entries[0].1);
                    for entry in entries.iter_mut().skip(1) { self.infer_expr(&mut entry.0); self.infer_expr(&mut entry.1); }
                    Ty::map_of(kt, vt)
                }
            }
            ExprKind::EmptyMap => Ty::map_of(self.fresh_var(), self.fresh_var()),
        }
    }

    // ── Extracted inference helpers ──

    fn infer_call(
        &mut self,
        callee: &mut Box<ast::Expr>,
        args: &mut Vec<ast::Expr>,
        named_args: &mut Vec<(almide_base::intern::Sym, ast::Expr)>,
        type_args: &Option<Vec<ast::TypeExpr>>,
    ) -> Ty {
        // Save named arg names, then flatten into positional args temporarily.
        let named_names: Vec<almide_base::intern::Sym> = named_args.iter().map(|(n, _)| *n).collect();
        let named_start = args.len();
        args.extend(std::mem::take(named_args).into_iter().map(|(_, e)| e));
        let resolved_type_args: Option<Vec<crate::types::Ty>> = type_args.as_ref().map(|tas|
            tas.iter().map(|te| self.resolve_type_expr(te)).collect());
        let ret = self.check_call_with_type_args(callee, args, resolved_type_args.as_deref());
        // Restore named args
        let named_exprs: Vec<ast::Expr> = args.drain(named_start..).collect();
        *named_args = named_names.into_iter().zip(named_exprs).collect();
        ret
    }

    fn infer_pipe(&mut self, left: &mut Box<ast::Expr>, right: &mut Box<ast::Expr>) -> Ty {
        // Unwrap postfix operators (??, !, ?) on the RHS so the pipe targets the inner Call.
        // e.g. `xs |> list.find(pred) ?? fallback` → pipe into list.find, then apply ??
        match &mut right.kind {
            ExprKind::UnwrapOr { expr: inner, fallback, .. } => {
                let inner_ty = self.infer_pipe(left, inner);
                let fb_ty = self.infer_expr(fallback);
                self.unify_infer(&inner_ty, &fb_ty);
                // UnwrapOr unwraps Option[T]/Result[T,E] → T
                match &inner_ty {
                    Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => args[0].clone(),
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
                    _ => inner_ty,
                }
            }
            ExprKind::Unwrap { expr: inner, .. } => {
                let inner_ty = self.infer_pipe(left, inner);
                // Annotate the inner expression with its resolved type so the lowering
                // can construct the correct IR type (e.g., Result[List[T], List[E]] for
                // result.collect rather than hardcoding Result[T, String]).
                self.type_map.insert(inner.id, inner_ty.clone());
                match &inner_ty {
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
                    Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => args[0].clone(),
                    _ => inner_ty,
                }
            }
            ExprKind::Try { expr: inner, .. } => {
                let inner_ty = self.infer_pipe(left, inner);
                match &inner_ty {
                    Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 =>
                        Ty::Applied(TypeConstructorId::Option, vec![args[0].clone()]),
                    _ => Ty::Applied(TypeConstructorId::Option, vec![inner_ty]),
                }
            }
            _ => self.infer_pipe_direct(left, right),
        }
    }

    fn infer_pipe_direct(&mut self, left: &mut Box<ast::Expr>, right: &mut Box<ast::Expr>) -> Ty {
        let left_ty = self.infer_expr(left);
        // Resolve TypeVars eagerly via UnionFind — earlier pipes in the chain
        // have already been unified (constrain() calls unify_infer immediately),
        // so the concrete type is available now. Without this, chained UFCS like
        // `xs |> list.map(f) |> list.join(",")` sees a raw TypeVar for the
        // intermediate result, causing module resolution to fail.
        let left_ty = super::types::resolve_ty(&left_ty, &self.uf);
        match &mut right.kind {
            ExprKind::Call { callee, args, .. } => {
                // Pipe inserts left as the first argument
                let mut all_arg_tys: Vec<Ty> = vec![left_ty];
                all_arg_tys.extend(args.iter_mut().map(|a| self.infer_expr(a)));
                // Resolve module calls for pipe (e.g. xs |> list.filter(f))
                match &mut callee.kind {
                    ExprKind::Ident { name, .. } => self.check_named_call(name, &all_arg_tys),
                    ExprKind::Member { object, field, .. } => {
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
            ExprKind::Ident { name, .. } => {
                let all_arg_tys = vec![left_ty];
                self.check_named_call(name, &all_arg_tys)
            }
            // Pipe RHS is a module-qualified function (e.g. `5 |> int.abs`)
            ExprKind::Member { object, field, .. } => {
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
        var_tuple: &Option<Vec<almide_base::intern::Sym>>,
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
            ast::Stmt::Let { name, ty, value, span } => {
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
                if let Some(s) = span {
                    self.env.var_decl_locs.insert(sym(name), (s.line, s.col));
                }
                self.env.define_var(name, final_ty);
            }
            ast::Stmt::Var { name, ty, value, span } => {
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
                if let Some(s) = span {
                    self.env.var_decl_locs.insert(sym(name), (s.line, s.col));
                }
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
                    let mut diag = super::err(
                        format!("cannot reassign immutable binding '{}'", name),
                        hint, format!("{} = ...", name)).with_code("E009");
                    if let Some(&(line, col)) = self.env.var_decl_locs.get(&sym(name)) {
                        diag = diag.with_secondary(line, Some(col), format!("'{}' declared here", name));
                    }
                    self.emit(diag);
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
                // Normalize module-qualified names: "binary.Unreachable" → "Unreachable"
                let bare_name = name.as_str().rsplit_once('.').map(|(_, b)| sym(b)).unwrap_or(*name);
                let payload_tys: Vec<Ty> = match &resolved {
                    Ty::Variant { cases, .. } => cases.iter()
                        .find(|c| c.name == bare_name)
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
                                VariantPayload::Record(fs) => Some(fs.iter().map(|(n, t)| (*n, t.clone())).collect()),
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
                let resolved = resolve_ty(ty, &self.uf);
                if let Ty::Tuple(tys) = &resolved {
                    for (i, e) in elements.iter().enumerate() { self.bind_pattern(e, tys.get(i).unwrap_or(&Ty::Unknown)); }
                } else if super::types::is_inference_var(&resolved).is_some() {
                    // Type is an unresolved inference var (e.g., lambda parameter).
                    // Create fresh vars for each element and constrain: ?N = (?a, ?b, ...).
                    // When the outer call context later resolves ?N, the element vars
                    // get their correct types through the constraint chain.
                    let elem_vars: Vec<Ty> = elements.iter().map(|_| self.fresh_var()).collect();
                    self.constrain(resolved, Ty::Tuple(elem_vars.clone()), "tuple destructure");
                    for (e, ev) in elements.iter().zip(elem_vars.iter()) { self.bind_pattern(e, ev); }
                } else {
                    for e in elements { self.bind_pattern(e, &Ty::Unknown); }
                }
            }
            ast::Pattern::List { elements } => {
                let resolved = resolve_ty(ty, &self.uf);
                let elem_ty = match &resolved {
                    Ty::Applied(TypeConstructorId::List, args) if !args.is_empty() => args[0].clone(),
                    _ => Ty::Unknown,
                };
                for e in elements { self.bind_pattern(e, &elem_ty); }
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
        // When one side is List and the other is TypeVar, unify the TypeVar with the List type
        if is_unknown_ty(lc) && is_concat_ty(rc) {
            self.unify_infer(&lt, rc);
            let resolved_lt = resolve_ty(&lt, &self.uf);
            // Now unify element types if both resolved to List
            if let (Ty::Applied(TypeConstructorId::List, la), Ty::Applied(TypeConstructorId::List, ra)) = (&resolved_lt, rc) {
                if let (Some(le), Some(re)) = (la.first(), ra.first()) {
                    self.unify_infer(le, re);
                }
            }
            return resolve_ty(&lt, &self.uf);
        }
        if (is_concat_ty(lc) && (is_concat_ty(rc) || is_unknown_ty(rc)))
            || (is_concat_ty(rc) && is_unknown_ty(lc)) {
            // Unify element types for list concatenation: List[?0] + List[Int] → ?0 = Int
            if let (Ty::Applied(TypeConstructorId::List, la), Ty::Applied(TypeConstructorId::List, ra)) = (lc, rc) {
                if let (Some(le), Some(re)) = (la.first(), ra.first()) {
                    self.unify_infer(le, re);
                }
            }
            return resolve_ty(&lt, &self.uf);
        }
        // Matrix addition
        if *lc == Ty::Matrix || *rc == Ty::Matrix {
            return Ty::Matrix;
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
    fn resolve_module_call(&mut self, object: &ast::Expr, field: &str) -> Option<String> {
        if let ExprKind::Ident { name: module, .. } = &object.kind {
            if let Some(canonical) = self.env.import_table.resolve(module) {
                self.env.import_table.mark_used(module);
                let key = format!("{}.{}", canonical, field);
                self.check_fn_visibility(&canonical, field, &key);
                return Some(key);
            }
            // Check if Ident.field is a Type.method (protocol implementation)
            let key = format!("{}.{}", module, field);
            if self.env.functions.contains_key(&sym(&key)) {
                return Some(key);
            }
        }
        // Detect dot-chain submodule access (for pipe context)
        if let Some(dotted) = self.env.import_table.resolve_dotted_path(&object.kind) {
            let key = format!("{}.{}", dotted, field);
            if self.env.functions.contains_key(&sym(&key)) {
                let last_seg = dotted.rsplit('.').next().unwrap_or(&dotted);
                self.emit(super::err(
                    format!("dot-chain submodule access is no longer supported"),
                    format!("Add `import {}` and call `{}.{}()` instead", dotted, last_seg, field),
                    format!("call to {}.{}", dotted, field),
                ));
                return Some(key);
            }
        }
        // TypeName.method (e.g. Val.double in pipe)
        if let ExprKind::TypeName { name: type_name, .. } = &object.kind {
            let key = format!("{}.{}", type_name, field);
            if self.env.functions.contains_key(&sym(&key)) {
                return Some(key);
            }
        }
        None
    }

    /// Reject cross-module access to `mod fn` / `local fn` functions.
    ///
    /// A function has `Public` visibility by default — we only store entries
    /// for restricted (`Mod` / `Local`) declarations in `env.fn_visibility`.
    /// If the caller's own module (`self_module_name`) matches the callee's
    /// canonical module, the call is intra-module and all visibilities are
    /// allowed. Otherwise only `Public` is reachable.
    pub(super) fn check_fn_visibility(&mut self, callee_module: &str, field: &str, key: &str) {
        let vis = match self.env.fn_visibility.get(&sym(key)) {
            Some(v) => *v,
            None => return,
        };
        // Intra-module access (same package) is always allowed, regardless of
        // whether it's `mod fn` or `local fn`. This matches the spec for
        // `mod fn` and is a pragmatic relaxation for `local fn` (strict
        // same-file enforcement needs per-fn file tracking — TODO).
        if let Some(self_mod) = self.env.self_module_name {
            if self_mod.as_str() == callee_module {
                return;
            }
        }
        let (kind, scope_hint) = match vis {
            ast::Visibility::Mod => (
                "mod fn",
                "accessible only within the same project",
            ),
            ast::Visibility::Local => (
                "local fn",
                "accessible only within the same file",
            ),
            ast::Visibility::Public => return,
        };
        self.emit(super::err(
            format!("function '{}.{}' is not accessible", callee_module, field),
            format!("'{}' is declared as `{}` ({})", field, kind, scope_hint),
            format!("call to {}.{}", callee_module, field),
        ).with_code("E420"));
    }

}

/// Collect all Ident names referenced in an expression (shallow, for var capture check).
fn collect_idents(expr: &ast::Expr, out: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::Ident { name, .. } => out.push(name.to_string()),
        ExprKind::Call { callee, args, .. } => {
            collect_idents(callee, out);
            for a in args { collect_idents(a, out); }
        }
        ExprKind::Member { object, .. } | ExprKind::TupleIndex { object, .. }
        | ExprKind::IndexAccess { object, .. } => collect_idents(object, out),
        ExprKind::Binary { left, right, .. } | ExprKind::Pipe { left, right, .. } | ExprKind::Compose { left, right, .. } => {
            collect_idents(left, out); collect_idents(right, out);
        }
        ExprKind::Unary { operand, .. } | ExprKind::Paren { expr: operand, .. }
        | ExprKind::Some { expr: operand, .. } | ExprKind::Ok { expr: operand, .. }
        | ExprKind::Err { expr: operand, .. } | ExprKind::Try { expr: operand, .. } => {
            collect_idents(operand, out);
        }
        ExprKind::If { cond, then, else_, .. } => {
            collect_idents(cond, out); collect_idents(then, out); collect_idents(else_, out);
        }
        ExprKind::List { elements, .. } | ExprKind::Tuple { elements, .. } => {
            for e in elements { collect_idents(e, out); }
        }
        ExprKind::Lambda { body, .. } => collect_idents(body, out),
        ExprKind::InterpolatedString { parts, .. } => {
            for p in parts { if let ast::StringPart::Expr { expr } = p { collect_idents(expr, out); } }
        }
        ExprKind::Record { fields, .. } => { for f in fields { collect_idents(&f.value, out); } }
        _ => {} // literals, none, unit, etc.
    }
}
