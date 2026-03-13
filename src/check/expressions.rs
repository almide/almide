use crate::ast::{self, ResolvedType};
use crate::types::{Ty, VariantPayload};
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
        Ty::Map(_, _) => ResolvedType::Map,
        Ty::Record { .. } => ResolvedType::Record,
        Ty::Variant { .. } => ResolvedType::Variant,
        Ty::Fn { .. } => ResolvedType::Fn,
        Ty::Tuple(_) => ResolvedType::Tuple,
        Ty::Named(..) => ResolvedType::Named,
        Ty::TypeVar(_) => ResolvedType::Named,
        Ty::Unknown => ResolvedType::Unknown,
    }
}

impl Checker {
    pub(crate) fn check_expr(&mut self, expr: &mut ast::Expr) -> Ty {
        self.check_expr_with(expr, None)
    }

    pub(crate) fn check_expr_with(&mut self, expr: &mut ast::Expr, expected: Option<&Ty>) -> Ty {
        // Update current line/col from expression span for precise error positions
        let prev_line = self.current_decl_line;
        let prev_col = self.current_decl_col;
        if let Some(span) = expr.span() {
            self.current_decl_line = Some(span.line);
            self.current_decl_col = Some(span.col);
        }
        let ty = self.check_expr_inner(expr, expected);
        expr.set_resolved_type(ty_to_resolved(&ty));
        if let Some(span) = expr.span() {
            self.expr_types.insert((span.line, span.col), ty.clone());
        }
        self.current_decl_line = prev_line;
        self.current_decl_col = prev_col;
        ty
    }

    fn check_expr_inner(&mut self, expr: &mut ast::Expr, expected: Option<&Ty>) -> Ty {
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
            ast::Expr::None { .. } => {
                if let Some(Ty::Option(inner)) = expected {
                    Ty::Option(inner.clone())
                } else {
                    Ty::Option(Box::new(Ty::Unknown))
                }
            }
            ast::Expr::Hole { .. } | ast::Expr::Todo { .. } | ast::Expr::Placeholder { .. }
            | ast::Expr::Error { .. } => Ty::Unknown,
            ast::Expr::Some { expr: inner, .. } => Ty::Option(Box::new(self.check_expr(inner))),
            ast::Expr::Ok { expr: inner, .. } => {
                let inner_ty = self.check_expr(inner);
                if !self.env.in_effect && matches!(inner_ty, Ty::Unit) {
                    Ty::Unit
                } else {
                    // Use expected type to infer error half
                    let err_ty = match expected {
                        Some(Ty::Result(_, e)) => *e.clone(),
                        _ => Ty::Unknown,
                    };
                    Ty::Result(Box::new(inner_ty), Box::new(err_ty))
                }
            }
            ast::Expr::Err { expr: inner, .. } => {
                let err_ty = self.check_expr(inner);
                // Use expected type to infer ok half
                let ok_ty = match expected {
                    Some(Ty::Result(o, _)) => *o.clone(),
                    _ => Ty::Unknown,
                };
                Ty::Result(Box::new(ok_ty), Box::new(err_ty))
            }

            ast::Expr::Ident { name, .. } => {
                if let Some(ty) = self.env.lookup_var(name).cloned() {
                    self.env.used_vars.insert(name.clone());
                    return ty;
                }
                // Check top-level let constants
                if let Some(ty) = self.env.top_lets.get(name).cloned() {
                    return ty;
                }
                if let Some(sig) = self.env.functions.get(name) {
                    return Ty::Fn { params: sig.params.iter().map(|(_, t)| t.clone()).collect(), ret: Box::new(sig.ret.clone()) };
                }
                if matches!(name.as_str(), "println" | "eprintln") {
                    return Ty::Fn { params: vec![Ty::String], ret: Box::new(Ty::Unit) };
                }
                // Module names (user modules or aliases) are valid as identifiers in member/call context
                if self.env.user_modules.contains(name) || self.env.module_aliases.contains_key(name)
                    || crate::stdlib::is_stdlib_module(name) {
                    return Ty::Unknown;
                }
                // Don't emit error for names that might be constructors or forward-declared
                if !self.env.constructors.contains_key(name) && !name.starts_with(char::is_uppercase) {
                    let hint = if let Some(suggestion) = self.suggest_similar(name, "variable") {
                        format!("Did you mean '{}'?", suggestion)
                    } else {
                        "Check the variable name and make sure it is defined before this expression".to_string()
                    };
                    self.push_diagnostic(err(
                        format!("undefined variable '{}'", name),
                        hint,
                        format!("{}", name),
                    ));
                }
                Ty::Unknown
            }

            ast::Expr::TypeName { name, .. } => {
                // Check top-level let constants (UPPER_CASE names are parsed as TypeName)
                if let Some(ty) = self.env.top_lets.get(name).cloned() {
                    return ty;
                }
                if self.env.constructors.contains_key(name) { return Ty::Unknown; }
                Ty::Named(name.clone(), vec![])
            }

            ast::Expr::List { elements, .. } => {
                if elements.is_empty() {
                    return if let Some(Ty::List(inner)) = expected {
                        Ty::List(inner.clone())
                    } else {
                        Ty::List(Box::new(Ty::Unknown))
                    };
                }
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

            ast::Expr::EmptyMap { .. } => {
                if let Some(Ty::Map(k, v)) = expected {
                    if !self.env.is_hash(k) {
                        self.push_diagnostic(err(
                            format!("Map key type {} is not hashable", k.display()),
                            "Use String, Int, or Bool as Map keys — Float and function types cannot be keys",
                            "empty map literal",
                        ));
                    }
                    Ty::Map(k.clone(), v.clone())
                } else {
                    self.push_diagnostic(err(
                        "cannot infer Map type from empty literal [:]",
                        "Add a type annotation: let m: Map[K, V] = [:]",
                        "empty map literal",
                    ));
                    Ty::Map(Box::new(Ty::Unknown), Box::new(Ty::Unknown))
                }
            }

            ast::Expr::MapLiteral { entries, .. } => {
                let (first_key, first_val) = &mut entries[0];
                let key_ty = self.check_expr(first_key);
                let val_ty = self.check_expr(first_val);
                for (i, (k, v)) in entries.iter_mut().enumerate().skip(1) {
                    let kt = self.check_expr(k);
                    let vt = self.check_expr(v);
                    if !key_ty.compatible(&kt) {
                        self.push_diagnostic(err(
                            format!("map key at index {} has type {} but expected {}", i, kt.display(), key_ty.display()),
                            "All map keys must have the same type", "map literal",
                        ));
                    }
                    if !val_ty.compatible(&vt) {
                        self.push_diagnostic(err(
                            format!("map value at index {} has type {} but expected {}", i, vt.display(), val_ty.display()),
                            "All map values must have the same type", "map literal",
                        ));
                    }
                }
                if !self.env.is_hash(&key_ty) {
                    self.push_diagnostic(err(
                        format!("Map key type {} is not hashable", key_ty.display()),
                        "Use String, Int, or Bool as Map keys — Float and function types cannot be keys",
                        "map literal",
                    ));
                }
                Ty::Map(Box::new(key_ty), Box::new(val_ty))
            }

            ast::Expr::Record { name, fields, .. } => {
                // Check if this is a variant record constructor
                if let Some(cname) = name.as_ref() {
                    if let Some((vname, case)) = self.env.constructors.get(cname.as_str()).cloned() {
                        if let VariantPayload::Record(expected_fields) = &case.payload {
                            // Type-check each provided field
                            for f in fields.iter_mut() {
                                let actual_ty = self.check_expr(&mut f.value);
                                if let Some((_, expected_ty, _)) = expected_fields.iter().find(|(n, _, _)| n == &f.name) {
                                    if !expected_ty.compatible(&actual_ty) {
                                        let hint = Self::hint_with_conversion(
                                            &format!("In variant constructor {}", cname),
                                            expected_ty, &actual_ty,
                                        );
                                        self.push_diagnostic(err(
                                            format!("field '{}' expects {} but got {}", f.name, expected_ty.display(), actual_ty.display()),
                                            hint, "variant record construction",
                                        ));
                                    }
                                } else {
                                    self.push_diagnostic(err(
                                        format!("unknown field '{}' in variant {}", f.name, cname),
                                        &format!("{} does not have a field '{}'", cname, f.name), "variant record construction",
                                    ));
                                }
                            }
                            // Check for missing fields — fill in defaults or report error
                            for (fname, _, default) in expected_fields {
                                if !fields.iter().any(|f| f.name == *fname) {
                                    if let Some(default_expr) = default {
                                        // Fill in the default value
                                        fields.push(crate::ast::FieldInit {
                                            name: fname.clone(),
                                            value: default_expr.clone(),
                                        });
                                    } else {
                                        self.push_diagnostic(err(
                                            format!("missing field '{}' in variant constructor {}", fname, cname),
                                            &format!("Add field '{}' to the constructor", fname), "variant record construction",
                                        ));
                                    }
                                }
                            }
                            // Look up the full variant type
                            if let Some(ty) = self.env.types.get(&vname) {
                                return ty.clone();
                            }
                            return Ty::Named(vname, vec![]);
                        }
                    }
                }
                // Plain record
                Ty::Record {
                    fields: fields.iter_mut().map(|f| (f.name.clone(), self.check_expr(&mut f.value))).collect(),
                }
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
                let tt = self.check_expr_with(then, expected);
                let et = self.check_expr_with(else_, expected);
                if !tt.compatible(&et) {
                    let mut diag = err(
                        format!("if branches have different types: then is {}, else is {}", tt.display(), et.display()),
                        "Both branches must have the same type", "if expression",
                    );
                    if let Some(then_span) = then.span() {
                        diag.secondary.push(crate::diagnostic::SecondarySpan {
                            line: then_span.line, col: Some(then_span.col),
                            label: format!("this is {}", tt.display()),
                        });
                    }
                    self.push_diagnostic(diag);
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
                let first_arm_span = arms.first().and_then(|a| a.body.span());
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
                    let at = self.check_expr_with(&mut arm.body, expected);
                    if let Some(ref mut prev) = result_ty {
                        let compat = prev.compatible(&at)
                            || match (prev.clone(), &at) {
                                (Ty::Result(ok_ty, _), non_result) if !matches!(non_result, Ty::Result(_, _)) => ok_ty.compatible(non_result),
                                (_, Ty::Result(ok_ty, _)) if !matches!(prev.clone(), Ty::Result(_, _)) => prev.compatible(&ok_ty),
                                _ => false,
                            };
                        if !compat {
                            let mut diag = err(
                                format!("match arm has type {} but previous arms have type {}", at.display(), prev.display()),
                                "All match arms must have the same type", "match expression",
                            );
                            // Show the first arm's location as secondary
                            if let Some(first_span) = first_arm_span {
                                diag.secondary.push(crate::diagnostic::SecondarySpan {
                                    line: first_span.line, col: Some(first_span.col),
                                    label: format!("first arm is {}", prev.display()),
                                });
                            }
                            self.push_diagnostic(diag);
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
                let ty = expr.as_mut().map(|e| self.check_expr_with(e, expected)).unwrap_or(Ty::Unit);
                self.warn_unused_vars_in_scope("block");
                self.env.pop_scope();
                ty
            }

            ast::Expr::DoBlock { stmts, expr, .. } => {
                self.env.push_scope();
                let prev_do = self.env.in_do_block;
                self.env.in_do_block = true;
                for s in stmts.iter_mut() { self.check_stmt(s); }
                let _ty = expr.as_mut().map(|e| self.check_expr_with(e, expected)).unwrap_or(Ty::Unit);
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
                    Ty::Map(k, v) => {
                        if var_tuple.is_some() {
                            Ty::Tuple(vec![*k.clone(), *v.clone()])
                        } else {
                            *k.clone()
                        }
                    }
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
                    let tys = self.resolve_tuple_elements(&elem_ty, names.len(), format!("for ({}) in ...", names.join(", ")));
                    for (name, ty) in names.iter().zip(tys) {
                        self.env.define_var(name, ty);
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
                // Extract expected param/ret types for bidirectional inference
                let (expected_params, expected_ret) = match expected {
                    Some(Ty::Fn { params: ep, ret: er }) => (Some(ep.as_slice()), Some(er.as_ref())),
                    _ => (None, None),
                };
                let pts: Vec<Ty> = params.iter().enumerate().map(|(i, p)| {
                    if let Some(names) = &p.tuple_names {
                        let tuple_ty = p.ty.as_ref().map(|te| self.resolve_type_expr(te)).unwrap_or(Ty::Unknown);
                        let tys = self.resolve_tuple_elements(&tuple_ty, names.len(), format!("fn({}) => ...", p.name));
                        for (name, ty) in names.iter().zip(tys.iter()) {
                            self.env.define_var(name, ty.clone());
                        }
                        Ty::Tuple(tys)
                    } else {
                        let ty = if let Some(te) = &p.ty {
                            self.resolve_type_expr(te)
                        } else if let Some(ep) = expected_params {
                            let inferred = ep.get(i).cloned().unwrap_or(Ty::Unknown);
                            // Only use inferred type if it's concrete (not TypeVar/Unknown)
                            if matches!(inferred, Ty::TypeVar(_) | Ty::Unknown) {
                                Ty::Unknown
                            } else {
                                inferred
                            }
                        } else {
                            Ty::Unknown
                        };
                        self.env.define_var(&p.name, ty.clone());
                        ty
                    }
                }).collect();
                let ret = self.check_expr_with(body, expected_ret);
                self.env.pop_scope();
                Ty::Fn { params: pts, ret: Box::new(ret) }
            }

            ast::Expr::Call { callee, args, .. } => self.check_call(callee, args),

            ast::Expr::Member { object, field, .. } => {
                // Track module usage for unused import detection
                if let ast::Expr::Ident { name, .. } = object.as_ref() {
                    if crate::stdlib::is_stdlib_module(name) || self.env.user_modules.contains(name)
                        || self.env.module_aliases.contains_key(name) {
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
                if (op == "==" || op == "!=")
                    && matches!(&lt, Ty::List(inner) if matches!(inner.as_ref(), Ty::Unknown))
                    && matches!(&rt, Ty::List(inner) if matches!(inner.as_ref(), Ty::Unknown))
                {
                    self.push_diagnostic(err(
                        "cannot compare two empty lists without type annotations",
                        "Add a type annotation to at least one side, e.g., let xs: List[Int] = []",
                        format!("operator '{}'", op),
                    ));
                }
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

            ast::Expr::Paren { expr: inner, .. } => self.check_expr_with(inner, expected),
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

            ast::Expr::IndexAccess { object, index, .. } => {
                let ot = self.check_expr(object);
                let it = self.check_expr(index);
                match &ot {
                    Ty::List(inner) => {
                        if !matches!(it, Ty::Int | Ty::Unknown) {
                            self.push_diagnostic(err(
                                format!("list index must be Int, got {}", it.display()),
                                "Use an Int value for list indexing",
                                "xs[i]",
                            ));
                        }
                        *inner.clone()
                    }
                    Ty::Map(k, v) => {
                        if !it.compatible(k) && !matches!(it, Ty::Unknown) {
                            self.push_diagnostic(err(
                                format!("map key type is {} but got {}", k.display(), it.display()),
                                "Key type must match the Map's key type",
                                "m[key]",
                            ));
                        }
                        Ty::Option(v.clone())
                    }
                    Ty::Unknown => Ty::Unknown,
                    _ => {
                        self.push_diagnostic(err(
                            format!("cannot index into type {}", ot.display()),
                            "Indexing with [] is supported for List[T] and Map[K, V]",
                            "xs[i] or m[key]",
                        ));
                        Ty::Unknown
                    }
                }
            }

            ast::Expr::While { cond, body, .. } => {
                let ct = self.check_expr(cond);
                if !ct.compatible(&Ty::Bool) {
                    self.push_diagnostic(err(
                        format!("while condition has type {} but expected Bool", ct.display()),
                        "The condition must be a Bool expression",
                        "while expression",
                    ));
                }
                self.env.push_scope();
                for s in body.iter_mut() { self.check_stmt(s); }
                self.env.pop_scope();
                Ty::Unit
            }

            ast::Expr::Break { .. } => Ty::Unit,
            ast::Expr::Continue { .. } => Ty::Unit,
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
