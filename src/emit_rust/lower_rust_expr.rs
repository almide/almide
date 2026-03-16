/// Expression + TCO lowering (split from lower_rust.rs).

use almide::ir::*;
use almide::types::Ty;
use super::rust_ir::*;
use super::lower_types::is_copy;
use super::lower_rust::LowerCtx;

/// Check if a RustIR expression contains break or continue (indicating internal loop control flow).
fn expr_has_break_or_continue(e: &Expr) -> bool {
    match e {
        Expr::Break => true,
        Expr::Continue { .. } => true,
        Expr::If { then, else_, .. } => {
            expr_has_break_or_continue(then) || else_.as_ref().map_or(false, |e| expr_has_break_or_continue(e))
        }
        Expr::Match { arms, .. } => arms.iter().any(|a| expr_has_break_or_continue(&a.body)),
        Expr::Block { stmts, tail } => {
            stmts.iter().any(|s| if let Stmt::Expr(e) = s { expr_has_break_or_continue(e) } else { false })
                || tail.as_ref().map_or(false, |t| expr_has_break_or_continue(t))
        }
        _ => false,
    }
}

impl<'a> LowerCtx<'a> {
    // ── Expression ──

    pub(super) fn lower_expr(&self, e: &IrExpr) -> Expr {
        match &e.kind {
            IrExprKind::LitInt { value } => Expr::Int(*value),
            IrExprKind::LitFloat { value } => Expr::Float(*value),
            IrExprKind::LitStr { value } => Expr::Str(value.clone()),
            IrExprKind::LitBool { value } => Expr::Bool(*value),
            IrExprKind::Unit => Expr::Unit,
            IrExprKind::Var { id } => {
                let info = self.vt.get(*id);
                let var = Expr::Var(crate::emit_common::sanitize(&info.name));
                // Lazy top-level lets are LazyLock statics: deref with * to get inner value
                if self.lazy_top_lets.contains(id) {
                    let deref = Expr::Raw(format!("(*{})", crate::emit_common::sanitize(&info.name)));
                    if info.use_count > 1 && !is_copy(&info.ty) {
                        Expr::Clone(Box::new(deref))
                    } else {
                        deref
                    }
                } else {
                    // Clone if: used more than once AND not a Copy type
                    if info.use_count > 1 && !is_copy(&info.ty) {
                        Expr::Clone(Box::new(var))
                    } else { var }
                }
            }

            IrExprKind::BinOp { op, left, right } => {
                let l = self.lower_expr(left);
                let r = self.lower_expr(right);
                match op {
                    BinOp::PowFloat => {
                        if matches!(&left.ty, Ty::Int) {
                            Expr::MethodCall { recv: Box::new(l), method: "pow".into(), args: vec![Expr::Raw(format!("{} as u32", super::render::expr_str(&r)))] }
                        } else {
                            Expr::MethodCall { recv: Box::new(l), method: "powf".into(), args: vec![r] }
                        }
                    }
                    BinOp::ConcatStr | BinOp::ConcatList => Expr::Call { func: "AlmideConcat::concat".into(), args: vec![l, r] },
                    _ => {
                        let op_str = match op {
                            BinOp::AddInt | BinOp::AddFloat => "+", BinOp::SubInt | BinOp::SubFloat => "-",
                            BinOp::MulInt | BinOp::MulFloat => "*", BinOp::DivInt | BinOp::DivFloat => "/",
                            BinOp::ModInt | BinOp::ModFloat => "%", BinOp::XorInt => "^",
                            BinOp::Eq => "==", BinOp::Neq => "!=",
                            BinOp::Lt => "<", BinOp::Gt => ">", BinOp::Lte => "<=", BinOp::Gte => ">=",
                            BinOp::And => "&&", BinOp::Or => "||",
                            _ => "+",
                        };
                        Expr::BinOp { op: op_str, left: Box::new(l), right: Box::new(r) }
                    }
                }
            }
            IrExprKind::UnOp { op, operand } => {
                let o = self.lower_expr(operand);
                Expr::UnOp { op: match op { UnOp::Not => "!", _ => "-" }, operand: Box::new(o) }
            }

            IrExprKind::If { cond, then, else_ } => Expr::If {
                cond: Box::new(self.lower_expr(cond)),
                then: Box::new(self.lower_expr(then)),
                else_: Some(Box::new(self.lower_expr(else_))),
            },
            IrExprKind::Match { subject, arms } => {
                let has_result_arms = arms.iter().any(|a| matches!(&a.pattern,
                    IrPattern::Ok { .. } | IrPattern::Err { .. }
                    | IrPattern::Some { .. } | IrPattern::None));
                let subj = if has_result_arms {
                    // Match on Result — don't auto-try the subject
                    match self.lower_expr(subject) {
                        Expr::Try(inner) => *inner, // strip auto-?
                        other => other,
                    }
                } else {
                    self.lower_expr(subject)
                };
                // String subjects need .as_str() to match against string literal patterns
                let has_str_pat = arms.iter().any(|a| matches!(&a.pattern, IrPattern::Literal { expr } if matches!(&expr.kind, IrExprKind::LitStr { .. })));
                let subj = if has_str_pat && matches!(&subject.ty, Ty::String) {
                    Expr::MethodCall { recv: Box::new(subj), method: "as_str".into(), args: vec![] }
                } else { subj };
                Expr::Match {
                    subject: Box::new(subj),
                    arms: arms.iter().map(|a| {
                        let pat = self.lower_pat(&a.pattern);
                        let body = self.lower_expr(&a.body);
                        // Insert deref stmts for boxed recursive variant bindings
                        let deref_vars = self.find_boxed_bindings(&a.pattern);
                        let body = if deref_vars.is_empty() {
                            body
                        } else {
                            let mut stmts: Vec<Stmt> = deref_vars.iter().map(|v| {
                                Stmt::Let { name: v.clone(), ty: None, value: Expr::Raw(format!("*{}", v)), mutable: false }
                            }).collect();
                            match body {
                                Expr::Block { stmts: mut existing, tail } => { stmts.extend(existing); Expr::Block { stmts, tail } }
                                other => Expr::Block { stmts, tail: Some(Box::new(other)) }
                            }
                        };
                        MatchArm { pat, guard: a.guard.as_ref().map(|g| self.lower_expr(g)), body }
                    }).collect(),
                }
            }
            IrExprKind::Block { stmts, expr } => Expr::Block {
                stmts: stmts.iter().map(|s| self.lower_stmt(s)).collect(),
                tail: expr.as_ref().map(|e| Box::new(self.lower_expr(e))),
            },
            IrExprKind::DoBlock { stmts, expr } => {
                let has_guard = stmts.iter().any(|s| matches!(&s.kind, IrStmtKind::Guard { .. }));
                if has_guard {
                    // Wrap in loop { ... } so guard's break/continue work correctly
                    let mut body_stmts: Vec<Stmt> = stmts.iter().map(|s| self.lower_stmt(s)).collect();
                    if let Some(tail) = expr.as_ref() {
                        let tail_expr = self.lower_expr(tail);
                        // Only insert return for explicit ok()/err() tail expressions.
                        // Other tail expressions (if/else with assignments, break, etc.)
                        // are emitted as-is — the loop exits via guard's return or break.
                        if super::lower_rust::is_result_expr(&tail_expr) {
                            body_stmts.push(Stmt::Expr(Expr::Return(Some(Box::new(tail_expr)))));
                        } else {
                            body_stmts.push(Stmt::Expr(tail_expr));
                        }
                    }
                    // Guards generate `return Ok/Err` (effect) or `break`/`continue` (pure).
                    // In both cases, the loop exits via control flow — no trailing break needed.
                    // (Previously added trailing break for effect context, but this was wrong:
                    //  it caused one-shot behavior when the loop should iterate until a guard returns.)
                    Expr::Block {
                        stmts: vec![Stmt::Expr(Expr::Loop { label: None, body: body_stmts })],
                        tail: None,
                    }
                } else {
                    Expr::Block {
                        stmts: stmts.iter().map(|s| self.lower_stmt(s)).collect(),
                        tail: expr.as_ref().map(|e| Box::new(self.lower_expr(e))),
                    }
                }
            }
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                let iter_expr = self.lower_expr(iterable);
                // Skip clone for: ranges (Copy), list literals (fresh alloc)
                // Always clone variable references — use_count doesn't account for loop repetition
                let needs_clone = !matches!(&iterable.kind, IrExprKind::Range { .. })
                    && !matches!(&iterable.kind, IrExprKind::List { .. });
                let iter_val = if needs_clone { Expr::Clone(Box::new(iter_expr)) } else { iter_expr };
                let binding = if let Some(tvars) = var_tuple {
                    format!("({})", tvars.iter().map(|v| self.vt.get(*v).name.clone()).collect::<Vec<_>>().join(", "))
                } else {
                    self.vt.get(*var).name.clone()
                };
                Expr::For {
                    var: binding,
                    iter: Box::new(iter_val),
                    body: body.iter().map(|s| self.lower_stmt(s)).collect(),
                }
            }
            IrExprKind::While { cond, body } => Expr::While {
                cond: Box::new(self.lower_expr(cond)),
                body: body.iter().map(|s| self.lower_stmt(s)).collect(),
            },
            IrExprKind::Break => Expr::Break,
            IrExprKind::Continue => Expr::Continue { label: None },
            IrExprKind::Range { start, end, inclusive } => Expr::Range {
                start: Box::new(self.lower_expr(start)), end: Box::new(self.lower_expr(end)),
                inclusive: *inclusive, elem_ty: match &e.ty { Ty::List(inner) => self.lty(inner), _ => Type::I64 },
            },

            IrExprKind::Call { target, args, .. } => {
                let ir_args: Vec<Expr> = args.iter().map(|a| self.lower_expr(a)).collect();
                let call = match target {
                    CallTarget::Named { name } => return self.lower_named_call(name, ir_args),
                    CallTarget::Module { module, func } => {
                        return self.lower_stdlib_call(module, func, ir_args, args, e);
                    }

                    CallTarget::Method { object, method } => {
                        let obj = self.lower_expr(object);
                        let mut all_exprs = vec![obj]; all_exprs.extend(ir_args);
                        if let Some((module, func)) = method.split_once('.') {
                            let is_stdlib = crate::stdlib::is_stdlib_module(module) || crate::stdlib::is_any_stdlib(module);
                            if is_stdlib {
                                // Reconstruct IR args with object prepended
                                let mut all_ir: Vec<&IrExpr> = vec![object];
                                all_ir.extend(args.iter());
                                return self.lower_stdlib_call(module, func, all_exprs, &all_ir.iter().map(|x| (*x).clone()).collect::<Vec<_>>(), e);
                            } else {
                                Expr::Call { func: format!("{}_{}", module.replace('.', "_"), crate::emit_common::sanitize(func)), args: all_exprs }
                            }
                        } else {
                            Expr::Call { func: crate::emit_common::sanitize(method), args: all_exprs }
                        }
                    }
                    CallTarget::Computed { callee } => {
                        let c = self.lower_expr(callee);
                        Expr::Raw(format!("({})({})", super::render::expr_str(&c),
                            ir_args.iter().map(|a| super::render::expr_str(a)).collect::<Vec<_>>().join(", ")))
                    }
                };
                // Auto-? for any call returning Result in effect context
                if self.auto_try && matches!(&e.ty, Ty::Result(_, _)) {
                    Expr::Try(Box::new(call))
                } else {
                    call
                }
            }

            IrExprKind::List { elements } => {
                if elements.is_empty() {
                    // Emit typed empty vec to avoid Rust type inference failure
                    let inner_ty = match &e.ty {
                        Ty::List(inner) if !matches!(inner.as_ref(), Ty::Unknown | Ty::TypeVar(_)) => {
                            let rt = super::lower_types::lower_ty(inner);
                            let mut s = String::new();
                            super::render::render_type(&mut s, &rt);
                            Some(s)
                        }
                        _ => None,
                    };
                    if let Some(ty_str) = inner_ty {
                        Expr::Raw(format!("Vec::<{}>::new()", ty_str))
                    } else {
                        Expr::Vec(vec![]) // fallback: let Rust infer
                    }
                } else {
                    Expr::Vec(elements.iter().map(|e| self.lower_expr(e)).collect())
                }
            }
            IrExprKind::MapLiteral { entries } => Expr::HashMap(entries.iter().map(|(k, v)| (self.lower_expr(k), self.lower_expr(v))).collect()),
            IrExprKind::EmptyMap => {
                if let Ty::Map(k, v) = &e.ty {
                    if !matches!(k.as_ref(), Ty::Unknown | Ty::TypeVar(_)) {
                        let mut ks = String::new();
                        let mut vs = String::new();
                        super::render::render_type(&mut ks, &self.lty(k));
                        super::render::render_type(&mut vs, &self.lty(v));
                        Expr::Raw(format!("HashMap::<{}, {}>::new()", ks, vs))
                    } else {
                        Expr::Raw("HashMap::new()".into())
                    }
                } else {
                    Expr::Raw("HashMap::new()".into())
                }
            }
            IrExprKind::Tuple { elements } => Expr::Tuple(elements.iter().map(|e| self.lower_expr(e)).collect()),
            IrExprKind::Record { name, fields } => {
                let sname = name.as_ref().map(|n| self.ctors.get(n).map(|e| format!("{}::{}", e, n)).unwrap_or(n.clone())).unwrap_or_else(|| {
                    // Check if the expression type is a Named type (e.g., Pair, Stack)
                    if let Ty::Named(type_name, _) = &e.ty {
                        return type_name.clone();
                    }
                    let mut fnames: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
                    fnames.sort();
                    self.anon.get(&fnames).cloned()
                        .or_else(|| self.named.get(&fnames).cloned())
                        .unwrap_or("AnonRecord".into())
                });
                let mut lowered_fields: Vec<(String, Expr)> = fields.iter().map(|(n, v)| (n.clone(), self.lower_expr(v))).collect();
                // Fill in default field values for any missing fields in named records
                if let Some(ctor_name) = name.as_ref() {
                    let provided: std::collections::HashSet<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
                    if let Some(field_decls) = self.find_record_field_decls(ctor_name) {
                        for fd in field_decls {
                            if !provided.contains(fd.name.as_str()) {
                                if let Some(default_expr) = &fd.default {
                                    lowered_fields.push((fd.name.clone(), self.lower_expr(default_expr)));
                                }
                            }
                        }
                    }
                }
                Expr::Struct { name: sname, fields: lowered_fields }
            }
            IrExprKind::SpreadRecord { base, fields } => {
                let base_expr = self.lower_expr(base);
                let base_val = if self.is_single_use_var(base) { base_expr } else { Expr::Clone(Box::new(base_expr)) };
                // Resolve struct name from the expression's type
                let sname = self.resolve_record_name(&e.ty);
                Expr::StructUpdate {
                    name: sname,
                    base: Box::new(base_val),
                    fields: fields.iter().map(|(n, v)| (n.clone(), self.lower_expr(v))).collect(),
                }
            }
            IrExprKind::Member { object, field } => {
                let obj = self.lower_expr(object);
                let field_expr = Expr::Field(Box::new(obj), field.clone());
                if is_copy(&e.ty) || self.is_single_use_var(object) { field_expr }
                else { Expr::Clone(Box::new(field_expr)) }
            }
            IrExprKind::TupleIndex { object, index } => Expr::TupleIdx(Box::new(self.lower_expr(object)), *index),
            IrExprKind::IndexAccess { object, index } => {
                if matches!(&object.ty, Ty::Map(_, _)) {
                    // Map index: m[k] → m.get(&k).cloned()
                    let obj = self.lower_expr(object);
                    let idx = self.lower_expr(index);
                    Expr::Raw(format!("{}.get(&{}).cloned()", super::render::expr_str(&obj), super::render::expr_str(&idx)))
                } else {
                    Expr::Index(Box::new(self.lower_expr(object)), Box::new(self.lower_expr(index)))
                }
            }
            IrExprKind::Lambda { params, body } => Expr::Closure {
                params: params.iter().map(|(var, _)| self.vt.get(*var).name.clone()).collect(),
                body: Box::new(self.lower_expr(body)),
            },
            IrExprKind::StringInterp { parts } => {
                let mut template = String::new();
                let mut args = Vec::new();
                for part in parts {
                    match part {
                        IrStringPart::Lit { value } => {
                            for c in value.chars() { match c { '{' => template.push_str("{{"), '}' => template.push_str("}}"), '"' => template.push_str("\\\""), '\\' => template.push_str("\\\\"), _ => template.push(c) } }
                        }
                        IrStringPart::Expr { expr } => {
                            let debug = matches!(&expr.ty, Ty::List(_) | Ty::Option(_) | Ty::Result(_, _) | Ty::Map(_, _) | Ty::Tuple(_) | Ty::Record { .. } | Ty::Variant { .. });
                            template.push_str(if debug { "{:?}" } else { "{}" });
                            args.push(self.lower_expr(expr));
                        }
                    }
                }
                if args.is_empty() { Expr::Str(template) } else { Expr::Format { template: format!("\"{}\"", template), args } }
            }
            IrExprKind::ResultOk { expr } => Expr::Ok(Box::new(self.lower_expr(expr))),
            IrExprKind::ResultErr { expr } => Expr::Err(Box::new(self.lower_expr(expr))),
            IrExprKind::OptionSome { expr } => Expr::Some(Box::new(self.lower_expr(expr))),
            IrExprKind::OptionNone => {
                // Option[T] の T が判明していれば None::<T> を生成（Rust の型推論を助ける）
                if let Ty::Option(inner) = &e.ty {
                    if !inner.contains_unknown() && !matches!(inner.as_ref(), Ty::TypeVar(_)) {
                        let mut ty_str = String::new();
                        super::render::render_type(&mut ty_str, &self.lty(inner));
                        return Expr::Raw(format!("None::<{}>", ty_str));
                    }
                }
                Expr::None
            }
            IrExprKind::Try { expr } => Expr::Try(Box::new(self.lower_expr(expr))),
            IrExprKind::Await { expr } => self.lower_expr(expr),
            IrExprKind::Hole | IrExprKind::Todo { .. } => Expr::Raw("todo!()".into()),
        }
    }

    fn lower_named_call(&self, name: &str, args: Vec<Expr>) -> Expr {
        match name {
            "println" => Expr::Macro { name: "println".into(), args: vec![Expr::Raw("\"{}\"".into()), args.into_iter().next().unwrap_or(Expr::Unit)] },
            "eprintln" => Expr::Macro { name: "eprintln".into(), args: vec![Expr::Raw("\"{}\"".into()), args.into_iter().next().unwrap_or(Expr::Unit)] },
            "assert_eq" => Expr::Macro { name: "assert_eq".into(), args },
            "assert_ne" => Expr::Macro { name: "assert_ne".into(), args },
            "assert" => {
                if args.len() >= 2 {
                    // assert(cond, msg) → assert!(cond, "{}", msg)
                    let mut a = args;
                    let msg = a.remove(1);
                    a.insert(1, Expr::Raw("\"{}\"".into()));
                    a.insert(2, msg);
                    Expr::Macro { name: "assert".into(), args: a }
                } else {
                    Expr::Macro { name: "assert".into(), args }
                }
            }
            _ if name.starts_with("__encode_list_") || name.starts_with("__decode_list_") => {
                let is_encode = name.starts_with("__encode_list_");
                let type_suffix = if is_encode { &name["__encode_list_".len()..] } else { &name["__decode_list_".len()..] };
                // Primitives have direct runtime helpers
                match type_suffix {
                    "string" | "int" | "float" | "bool" => {
                        Expr::Call { func: format!("almide_rt_{}", crate::emit_common::sanitize(name)), args }
                    }
                    // Named types: pass Type_encode/decode as function argument
                    _ => {
                        let func_ref = if is_encode {
                            format!("{}_encode", crate::emit_common::sanitize(type_suffix))
                        } else {
                            format!("{}_decode", crate::emit_common::sanitize(type_suffix))
                        };
                        let rt_func = if is_encode { "almide_rt_value_encode_list" } else { "almide_rt_value_decode_list" };
                        let mut all_args = args;
                        all_args.push(Expr::Var(func_ref));
                        Expr::Call { func: rt_func.into(), args: all_args }
                    }
                }
            }
            _ => {
                let call = if let Some(enum_name) = self.ctors.get(name) {
                    if args.is_empty() { return Expr::Var(format!("{}::{}", enum_name, name)); }
                    // 再帰型コンストラクタ: 自己参照する引数を Box::new() で包む
                    let boxed_args = self.box_recursive_args(enum_name, name, args);
                    Expr::Call { func: format!("{}::{}", enum_name, name), args: boxed_args }
                } else {
                    let func_name = crate::emit_common::sanitize(name);
                    // Runtime helper functions get almide_rt_ prefix
                    let func_name = if func_name.starts_with("__") {
                        format!("almide_rt_{}", func_name)
                    } else {
                        func_name
                    };
                    Expr::Call { func: func_name, args }
                };
                // Auto-? for calls to result-returning functions in effect context
                if self.auto_try && self.result_fns.contains(name) {
                    Expr::Try(Box::new(call))
                } else {
                    call
                }
            }
        }
    }

    /// 再帰型コンストラクタの引数を Box::new() で包む
    fn box_recursive_args(&self, enum_name: &str, ctor_name: &str, args: Vec<Expr>) -> Vec<Expr> {
        // IR の型定義からコンストラクタのフィールド型を取得
        if let Some(td) = self.type_decls.iter().find(|td| td.name == enum_name) {
            if let almide::ir::IrTypeDeclKind::Variant { cases, .. } = &td.kind {
                if let Some(case) = cases.iter().find(|c| c.name == ctor_name) {
                    if let almide::ir::IrVariantKind::Tuple { fields } = &case.kind {
                        return args.into_iter().enumerate().map(|(i, arg)| {
                            if let Some(field_ty) = fields.get(i) {
                                if super::lower_rust::ty_contains_name(field_ty, enum_name) {
                                    return Expr::Call { func: "Box::new".into(), args: vec![arg] };
                                }
                            }
                            arg
                        }).collect();
                    }
                }
            }
        }
        args
    }

    /// Lower a stdlib module call using the generated dispatch templates.
    /// Falls back to direct call if no template exists.
    fn lower_stdlib_call(&self, module: &str, func: &str, rust_args: Vec<Expr>, ir_args: &[IrExpr], e: &IrExpr) -> Expr {
        let key = format!("{}.{}", module, func);
        // Use generated TOML templates for core stdlib modules
        let use_template = matches!(module, "list" | "string" | "map" | "int" | "float" | "math" | "result" | "option");
        let expr = if use_template {
            let args_str: Vec<String> = rust_args.iter().map(|a| super::render::expr_str(a)).collect();
            let inline_lambda = |param_idx: usize, _body_idx: usize| -> (Vec<String>, String) {
                if let Some(Expr::Closure { params, body }) = rust_args.get(param_idx) {
                    (params.clone(), super::render::expr_str(body))
                } else {
                    (vec!["__x".into()], args_str.get(param_idx).cloned().unwrap_or_default())
                }
            };
            if let Some(code) = almide::generated::emit_rust_calls::gen_generated_call(
                module, func, &args_str, self.in_effect, &inline_lambda,
            ) {
                Expr::Raw(code)
            } else {
                Expr::Call {
                    func: format!("almide_rt_{}_{}", module.replace('.', "_"), crate::emit_common::sanitize(func)),
                    args: rust_args,
                }
            }
        } else {
            Expr::Call {
                func: format!("almide_rt_{}_{}", module.replace('.', "_"), crate::emit_common::sanitize(func)),
                args: rust_args,
            }
        };
        // Template already includes ? when in_effect — don't double-wrap
        if use_template && self.in_effect {
            return expr; // template handles ? insertion
        }
        if self.auto_try && self.result_fns.contains(&key) {
            Expr::Try(Box::new(expr))
        } else if self.auto_try && matches!(&e.ty, Ty::Result(_, _)) {
            Expr::Try(Box::new(expr))
        } else {
            expr
        }
    }

    // ── TCO ──

    pub(super) fn lower_tco(&self, e: &IrExpr, fn_name: &str, params: &[String]) -> Expr {
        match &e.kind {
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } if name == fn_name => {
                let mut stmts = Vec::new();
                for (i, arg) in args.iter().enumerate() {
                    stmts.push(Stmt::Let { name: format!("_tco_tmp_{}", i), ty: None, mutable: false, value: self.lower_expr(arg) });
                }
                for (i, param) in params.iter().enumerate() {
                    stmts.push(Stmt::Assign { target: param.clone(), value: Expr::Var(format!("_tco_tmp_{}", i)) });
                }
                stmts.push(Stmt::Expr(Expr::Continue { label: Some("_tco".into()) }));
                Expr::Block { stmts, tail: None }
            }
            IrExprKind::If { cond, then, else_ } => Expr::If {
                cond: Box::new(self.lower_expr(cond)),
                then: Box::new(self.lower_tco(then, fn_name, params)),
                else_: Some(Box::new(self.lower_tco(else_, fn_name, params))),
            },
            IrExprKind::Match { subject, arms } => Expr::Match {
                subject: Box::new(self.lower_expr(subject)),
                arms: arms.iter().map(|a| MatchArm {
                    pat: self.lower_pat(&a.pattern),
                    guard: a.guard.as_ref().map(|g| self.lower_expr(g)),
                    body: self.lower_tco(&a.body, fn_name, params),
                }).collect(),
            },
            IrExprKind::Block { stmts, expr: Some(tail) } => Expr::Block {
                stmts: stmts.iter().map(|s| self.lower_stmt(s)).collect(),
                tail: Some(Box::new(self.lower_tco(tail, fn_name, params))),
            },
            _ => Expr::Return(Some(Box::new(self.lower_expr(e)))),
        }
    }

    /// Resolve the Rust struct name for a record type.
    /// Checks named records first, then falls back to anonymous record names.
    pub(super) fn resolve_record_name(&self, ty: &Ty) -> String {
        match ty {
            Ty::Named(n, _) => n.clone(),
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                let mut names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
                names.sort();
                if let Some(n) = self.named.get(&names) { return n.clone(); }
                if let Some(n) = self.anon.get(&names) { return n.clone(); }
                "AnonRecord".into()
            }
            _ => "AnonRecord".into(),
        }
    }

    /// Look up the field declarations for a named record constructor.
    /// Handles both standalone Record types and Variant record cases.
    fn find_record_field_decls(&self, ctor_name: &str) -> Option<&'a [IrFieldDecl]> {
        // Check if it's a variant constructor
        if let Some(enum_name) = self.ctors.get(ctor_name) {
            for td in self.type_decls {
                if td.name == *enum_name {
                    if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
                        for case in cases {
                            if case.name == ctor_name {
                                if let IrVariantKind::Record { fields } = &case.kind {
                                    return Some(fields);
                                }
                            }
                        }
                    }
                }
            }
        }
        // Check standalone record types
        for td in self.type_decls {
            if td.name == ctor_name {
                if let IrTypeDeclKind::Record { fields } = &td.kind {
                    return Some(fields);
                }
            }
        }
        None
    }
}
