/// IR → TsIR lowering pass: all codegen decisions.
///
/// Input:    &IrProgram, LowerOpts
/// Output:   TsIR Program
/// Owns:     Result erasure (ok→value, err→throw), expression/statement lowering,
///           effect fn context, test wrapping
/// Does NOT: string rendering (render_ts.rs), declarations/types (lower_decls.rs)
///
/// Principles:
/// 1. All codegen decisions happen here — render is a pure pattern match.
/// 2. Checker's types are gospel — never re-infer.
/// 3. Result erasure: ok(x)→x, err(e)→throw, Option: Some(x)→x, None→null.

use std::collections::HashSet;
use crate::ir::*;
use crate::types::Ty;
use super::ts_ir::*;
use super::lower_decls::{sanitize, pascal_to_message};

pub struct LowerOpts {
    pub js_mode: bool,
    pub npm_mode: bool,
}

/// Lower an entire IrProgram to a TsIR Program.
pub fn lower(ir: &IrProgram, opts: &LowerOpts) -> Program {
    let ctx = LowerCtx::new(ir, opts);
    ctx.lower_program()
}

pub(super) struct LowerCtx<'a> {
    pub(super) ir: &'a IrProgram,
    pub(super) js_mode: bool,
    npm_mode: bool,
    unit_variants: HashSet<String>,
    generic_unit_ctors: HashSet<String>,
    pub(super) variant_ctors: HashSet<String>,
    pub(super) user_modules: Vec<String>,
}

impl<'a> LowerCtx<'a> {
    fn new(ir: &'a IrProgram, opts: &LowerOpts) -> Self {
        let mut unit_variants = HashSet::new();
        let mut generic_unit_ctors = HashSet::new();
        let mut variant_ctors = HashSet::new();

        for td in &ir.type_decls { Self::collect_variant_info(td, &mut unit_variants, &mut generic_unit_ctors, &mut variant_ctors); }
        for m in &ir.modules {
            for td in &m.type_decls { Self::collect_variant_info(td, &mut unit_variants, &mut generic_unit_ctors, &mut variant_ctors); }
        }
        let user_modules = ir.modules.iter().map(|m| m.name.clone()).collect();
        LowerCtx { ir, js_mode: opts.js_mode, npm_mode: opts.npm_mode,
            unit_variants, generic_unit_ctors, variant_ctors, user_modules }
    }

    fn collect_variant_info(td: &IrTypeDecl, unit: &mut HashSet<String>, generic_unit: &mut HashSet<String>, record: &mut HashSet<String>) {
        if let IrTypeDeclKind::Variant { cases, is_generic, .. } = &td.kind {
            for c in cases {
                match &c.kind {
                    IrVariantKind::Unit => { unit.insert(c.name.clone()); if *is_generic { generic_unit.insert(c.name.clone()); } }
                    IrVariantKind::Record { .. } => { record.insert(c.name.clone()); }
                    _ => {}
                }
            }
        }
    }

    pub(super) fn vt(&self) -> &VarTable { &self.ir.var_table }
    pub(super) fn var_name(&self, id: VarId) -> String { sanitize(&self.vt().get(id).name) }

    pub(super) fn lower_program(self) -> Program {
        let runtime = if self.npm_mode { String::new() } else { crate::emit_ts_runtime::full_runtime(self.js_mode) };

        let module_names: HashSet<String> = self.ir.modules.iter().map(|m| m.name.clone()).collect();
        let mut ns_decls = Vec::new();
        let mut emitted_ns = HashSet::new();
        for m in &self.ir.modules {
            if m.name.contains('.') {
                let parts: Vec<&str> = m.name.split('.').collect();
                for i in 1..parts.len() {
                    let ancestor = parts[..i].join(".");
                    if !module_names.contains(&ancestor) && !emitted_ns.contains(&ancestor) {
                        emitted_ns.insert(ancestor.clone());
                        ns_decls.push(ancestor);
                    }
                }
            }
        }

        let modules: Vec<Module> = self.ir.modules.iter().map(|m| self.lower_module(m)).collect();
        let type_decls: Vec<TypeDecl> = self.ir.type_decls.iter().map(|td| self.lower_type_decl(td)).collect();
        let top_lets: Vec<Stmt> = self.ir.top_lets.iter().map(|tl| {
            let name = self.ir.var_table.get(tl.var).name.clone();
            let val = self.lower_expr(&tl.value, false, false);
            if self.js_mode { Stmt::Var { name, value: val } } else { Stmt::Const { name, value: val } }
        }).collect();

        let mut functions = Vec::new();
        let mut tests = Vec::new();
        let mut has_main = false;
        for f in &self.ir.functions {
            if f.name == "main" { has_main = true; }
            if f.is_test { tests.push(self.lower_test(f)); } else { functions.push(self.lower_fn(f)); }
        }

        Program {
            runtime, namespace_decls: ns_decls, modules, type_decls, top_lets,
            functions, tests, entry_point: if has_main { Some(EntryPoint { js_mode: self.js_mode }) } else { None },
            js_mode: self.js_mode,
        }
    }

    pub(super) fn lower_fn_body(&self, body: &IrExpr, in_effect: bool, in_test: bool) -> FnBody {
        match &body.kind {
            IrExprKind::Block { stmts, expr } => {
                let s: Vec<Stmt> = stmts.iter().map(|s| self.lower_stmt(s, in_effect, in_test)).collect();
                FnBody::Block { stmts: s, tail: expr.as_ref().map(|e| self.lower_expr(e, in_effect, in_test)) }
            }
            IrExprKind::DoBlock { stmts, expr } => {
                FnBody::Block { stmts: self.lower_do_stmts(stmts, expr.as_deref(), in_effect, in_test), tail: None }
            }
            _ => FnBody::Expr(self.lower_expr(body, in_effect, in_test)),
        }
    }

    // ── Expressions ──────────────────────────────────────────────

    pub(super) fn lower_expr(&self, expr: &IrExpr, ie: bool, it: bool) -> Expr {
        match &expr.kind {
            IrExprKind::LitInt { value } => {
                if *value > 9007199254740991 || *value < -9007199254740991 { Expr::BigInt(*value) } else { Expr::Int(*value) }
            }
            IrExprKind::LitFloat { value } => Expr::Float(*value),
            IrExprKind::LitStr { value } => Expr::Str(value.clone()),
            IrExprKind::LitBool { value } => Expr::Bool(*value),
            IrExprKind::Unit => Expr::Undefined,
            IrExprKind::Var { id } => Expr::Var(self.var_name(*id)),

            IrExprKind::BinOp { op, left, right } => self.lower_binop(*op, left, right, ie, it),
            IrExprKind::UnOp { op, operand } => {
                let o = self.lower_expr(operand, ie, it);
                match op {
                    UnOp::Not => Expr::UnOp { op: "!", operand: Box::new(o) },
                    UnOp::NegInt | UnOp::NegFloat => Expr::UnOp { op: "-", operand: Box::new(o) },
                }
            }

            IrExprKind::If { cond, then, else_ } => Expr::Ternary {
                cond: Box::new(self.lower_expr(cond, ie, it)),
                then: Box::new(self.lower_expr_value(then, ie, it)),
                else_: Box::new(self.lower_expr_value(else_, ie, it)),
            },

            IrExprKind::Match { subject, arms } => self.lower_match(subject, arms, ie, it),

            IrExprKind::Block { stmts, expr } => {
                let s: Vec<Stmt> = stmts.iter().map(|s| self.lower_stmt(s, ie, it)).collect();
                Expr::Block { stmts: s, tail: expr.as_ref().map(|e| Box::new(self.lower_expr(e, ie, it))) }
            }
            IrExprKind::DoBlock { stmts, expr } => {
                let s = self.lower_do_stmts(stmts, expr.as_deref(), ie, it);
                if stmts.iter().any(|s| matches!(&s.kind, IrStmtKind::Guard { .. })) {
                    Expr::DoLoop { body: s }
                } else {
                    Expr::Block { stmts: s, tail: None }
                }
            }

            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                let binding = if let Some(tvars) = var_tuple {
                    format!("[{}]", tvars.iter().map(|v| self.var_name(*v)).collect::<Vec<_>>().join(", "))
                } else { self.var_name(*var) };
                let stmts: Vec<Stmt> = body.iter().map(|s| self.lower_stmt(s, ie, it)).collect();
                if let IrExprKind::Range { start, end, inclusive } = &iterable.kind {
                    Expr::ForRange { binding, start: Box::new(self.lower_expr(start, ie, it)),
                        end: Box::new(self.lower_expr(end, ie, it)), inclusive: *inclusive, body: stmts }
                } else {
                    Expr::For { binding, iter: Box::new(self.lower_expr(iterable, ie, it)), body: stmts }
                }
            }
            IrExprKind::While { cond, body } => Expr::While {
                cond: Box::new(self.lower_expr(cond, ie, it)),
                body: body.iter().map(|s| self.lower_stmt(s, ie, it)).collect(),
            },
            IrExprKind::Break => Expr::Break,
            IrExprKind::Continue => Expr::Continue,

            IrExprKind::Call { target, args, .. } => self.lower_call(target, args, ie, it),

            IrExprKind::List { elements } => Expr::Array(elements.iter().map(|e| self.lower_expr(e, ie, it)).collect()),
            IrExprKind::EmptyMap => Expr::Raw("new Map()".into()),
            IrExprKind::MapLiteral { entries } => Expr::MapNew(entries.iter().map(|(k, v)| (self.lower_expr(k, ie, it), self.lower_expr(v, ie, it))).collect()),
            IrExprKind::Record { name, fields } => {
                let fs: Vec<(String, Expr)> = fields.iter().map(|(n, v)| (n.clone(), self.lower_expr(v, ie, it))).collect();
                if let Some(cn) = name.as_ref() { if self.variant_ctors.contains(cn.as_str()) { return Expr::ObjectWithTag { tag: cn.clone(), fields: fs }; } }
                Expr::Object { fields: fs }
            }
            IrExprKind::SpreadRecord { base, fields } => Expr::Spread {
                base: Box::new(self.lower_expr(base, ie, it)),
                fields: fields.iter().map(|(n, v)| (n.clone(), self.lower_expr(v, ie, it))).collect(),
            },
            IrExprKind::Tuple { elements } => Expr::Tuple(elements.iter().map(|e| self.lower_expr(e, ie, it)).collect()),
            IrExprKind::Range { start, end, inclusive } => Expr::RangeArray {
                start: Box::new(self.lower_expr(start, ie, it)),
                end: Box::new(self.lower_expr(end, ie, it)), inclusive: *inclusive,
            },

            IrExprKind::Member { object, field } => {
                if let IrExprKind::Var { id } = &object.kind {
                    Expr::Field(Box::new(Expr::Var(self.map_module(&self.vt().get(*id).name))), sanitize(field))
                } else { Expr::Field(Box::new(self.lower_expr(object, ie, it)), sanitize(field)) }
            }
            IrExprKind::TupleIndex { object, index } => Expr::TupleIdx(Box::new(self.lower_expr(object, ie, it)), *index),
            IrExprKind::IndexAccess { object, index } => {
                if matches!(object.ty, Ty::Map(_, _)) {
                    Expr::MethodCall { recv: Box::new(self.lower_expr(object, ie, it)), method: "get".into(), args: vec![self.lower_expr(index, ie, it)] }
                } else { Expr::Index(Box::new(self.lower_expr(object, ie, it)), Box::new(self.lower_expr(index, ie, it))) }
            }

            IrExprKind::Lambda { params, body } => Expr::Arrow {
                params: params.iter().map(|(v, _)| self.vt().get(*v).name.clone()).collect(),
                body: Box::new(self.lower_expr(body, ie, it)),
            },
            IrExprKind::StringInterp { parts } => Expr::Template { parts: parts.iter().map(|p| match p {
                IrStringPart::Lit { value } => TemplatePart::Lit(value.clone()),
                IrStringPart::Expr { expr } => TemplatePart::Expr(self.lower_expr(expr, ie, it)),
            }).collect() },

            IrExprKind::ResultOk { expr } => self.lower_expr(expr, ie, it),
            IrExprKind::ResultErr { expr } => self.lower_err(expr, ie, it),
            IrExprKind::OptionSome { expr } => self.lower_expr(expr, ie, it),
            IrExprKind::OptionNone => Expr::Null,
            IrExprKind::Try { expr } => self.lower_expr(expr, ie, it),
            IrExprKind::Await { expr } => Expr::Await(Box::new(self.lower_expr(expr, ie, it))),

            IrExprKind::Hole => if self.js_mode { Expr::Null } else { Expr::Raw("null as any".into()) },
            IrExprKind::Todo { message } => Expr::ThrowError(Box::new(Expr::Str(message.clone()))),
        }
    }

    fn lower_expr_value(&self, expr: &IrExpr, ie: bool, it: bool) -> Expr {
        match &expr.kind {
            IrExprKind::Block { stmts, expr: Some(tail) } if stmts.is_empty() => self.lower_expr_value(tail, ie, it),
            IrExprKind::Block { .. } | IrExprKind::DoBlock { .. } => Expr::Iife(Box::new(self.lower_expr(expr, ie, it))),
            _ => self.lower_expr(expr, ie, it),
        }
    }

    fn lower_err(&self, expr: &IrExpr, ie: bool, it: bool) -> Expr {
        let is_variant = match &expr.kind {
            IrExprKind::Call { target: CallTarget::Named { name }, .. } => name.chars().next().map_or(false, |c| c.is_uppercase()),
            IrExprKind::Var { id } => self.vt().get(*id).name.chars().next().map_or(false, |c| c.is_uppercase()),
            _ => false,
        };
        let msg = self.lower_err_msg(expr, ie, it);
        if is_variant {
            let val = self.lower_expr(expr, ie, it);
            if ie { Expr::ThrowStructuredError { msg: Box::new(msg), value: Box::new(val) } }
            else { Expr::New { class: "__Err".into(), args: vec![msg, val] } }
        } else if ie { Expr::ThrowError(Box::new(msg)) }
        else { Expr::New { class: "__Err".into(), args: vec![msg] } }
    }

    fn lower_err_msg(&self, expr: &IrExpr, ie: bool, it: bool) -> Expr {
        match &expr.kind {
            IrExprKind::LitStr { value } => Expr::Str(value.clone()),
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                let cs = if name.chars().next().map_or(false, |c| c.is_uppercase()) { pascal_to_message(name) } else { name.clone() };
                let arg = if !args.is_empty() { self.lower_expr(&args[0], ie, it) } else { Expr::Str(String::new()) };
                Expr::BinOp { op: "+", left: Box::new(Expr::BinOp { op: "+", left: Box::new(Expr::Str(cs)), right: Box::new(Expr::Str(": ".into())) }), right: Box::new(arg) }
            }
            _ => Expr::Call { func: Box::new(Expr::Var("String".into())), args: vec![self.lower_expr(expr, ie, it)] },
        }
    }

    fn lower_binop(&self, op: BinOp, left: &IrExpr, right: &IrExpr, ie: bool, it: bool) -> Expr {
        let l = self.lower_expr(left, ie, it);
        let r = self.lower_expr(right, ie, it);
        match op {
            BinOp::And => Expr::BinOp { op: "&&", left: Box::new(l), right: Box::new(r) },
            BinOp::Or => Expr::BinOp { op: "||", left: Box::new(l), right: Box::new(r) },
            BinOp::Eq => Expr::Call { func: Box::new(Expr::Var("__deep_eq".into())), args: vec![l, r] },
            BinOp::Neq => Expr::UnOp { op: "!", operand: Box::new(Expr::Call { func: Box::new(Expr::Var("__deep_eq".into())), args: vec![l, r] }) },
            BinOp::ConcatStr | BinOp::ConcatList => Expr::Call { func: Box::new(Expr::Var("__concat".into())), args: vec![l, r] },
            BinOp::PowFloat => Expr::Call { func: Box::new(Expr::Raw("Math.pow".into())), args: vec![l, r] },
            BinOp::XorInt => Expr::Call { func: Box::new(Expr::Var("__bigop".into())), args: vec![Expr::Str("^".into()), l, r] },
            BinOp::MulInt => Expr::Call { func: Box::new(Expr::Var("__bigop".into())), args: vec![Expr::Str("*".into()), l, r] },
            BinOp::ModInt => Expr::Call { func: Box::new(Expr::Var("__bigop".into())), args: vec![Expr::Str("%".into()), l, r] },
            BinOp::DivInt => Expr::Call { func: Box::new(Expr::Var("__div".into())), args: vec![l, r] },
            BinOp::AddInt | BinOp::AddFloat => Expr::BinOp { op: "+", left: Box::new(l), right: Box::new(r) },
            BinOp::SubInt | BinOp::SubFloat => Expr::BinOp { op: "-", left: Box::new(l), right: Box::new(r) },
            BinOp::MulFloat => Expr::BinOp { op: "*", left: Box::new(l), right: Box::new(r) },
            BinOp::DivFloat => Expr::BinOp { op: "/", left: Box::new(l), right: Box::new(r) },
            BinOp::ModFloat => Expr::BinOp { op: "%", left: Box::new(l), right: Box::new(r) },
            BinOp::Lt => Expr::BinOp { op: "<", left: Box::new(l), right: Box::new(r) },
            BinOp::Gt => Expr::BinOp { op: ">", left: Box::new(l), right: Box::new(r) },
            BinOp::Lte => Expr::BinOp { op: "<=", left: Box::new(l), right: Box::new(r) },
            BinOp::Gte => Expr::BinOp { op: ">=", left: Box::new(l), right: Box::new(r) },
        }
    }

    fn lower_call(&self, target: &CallTarget, args: &[IrExpr], ie: bool, it: bool) -> Expr {
        let a: Vec<Expr> = args.iter().map(|a| self.lower_expr(a, ie, it)).collect();
        match target {
            CallTarget::Named { name } => {
                if a.is_empty() && self.unit_variants.contains(name) {
                    return if self.generic_unit_ctors.contains(name) {
                        Expr::Call { func: Box::new(Expr::Var(name.clone())), args: vec![] }
                    } else { Expr::Var(name.clone()) };
                }
                Expr::Call { func: Box::new(Expr::Var(sanitize(name))), args: a }
            }
            CallTarget::Module { module, func } => Expr::Call {
                func: Box::new(Expr::Field(Box::new(Expr::Var(self.map_module(module))), sanitize(func))), args: a,
            },
            CallTarget::Method { object, method } => {
                let obj = self.lower_expr(object, ie, it);
                if method == "unwrap_or" && a.len() == 1 {
                    return Expr::Call { func: Box::new(Expr::Var("unwrap_or".into())), args: vec![obj, a.into_iter().next().unwrap()] };
                }
                let mut all = vec![obj]; all.extend(a);
                // Module-qualified UFCS: "list.len" → same path as Module call
                if let Some((module, func)) = method.split_once('.') {
                    Expr::Call {
                        func: Box::new(Expr::Field(Box::new(Expr::Var(self.map_module(module))), sanitize(func))),
                        args: all,
                    }
                } else {
                    Expr::Call { func: Box::new(Expr::Var(sanitize(method))), args: all }
                }
            }
            CallTarget::Computed { callee } => Expr::Call { func: Box::new(self.lower_expr(callee, ie, it)), args: a },
        }
    }

    fn lower_match(&self, subject: &IrExpr, arms: &[IrMatchArm], ie: bool, it: bool) -> Expr {
        let subj = self.lower_expr(subject, ie, it);
        let has_err = arms.iter().any(|a| matches!(&a.pattern, IrPattern::Err { .. }));
        Expr::Match {
            subject: Box::new(subj), has_err_arm: has_err,
            arms: arms.iter().map(|arm| MatchArm {
                pattern: self.lower_pattern(&arm.pattern),
                guard: arm.guard.as_ref().map(|g| self.lower_expr(g, ie, it)),
                body: self.lower_expr_value(&arm.body, ie, it),
            }).collect(),
        }
    }

    fn lower_pattern(&self, pat: &IrPattern) -> Pattern {
        match pat {
            IrPattern::Wildcard => Pattern::Wild,
            IrPattern::Bind { var } => Pattern::Bind(self.var_name(*var)),
            IrPattern::Literal { expr } => Pattern::Literal(self.lower_expr(expr, false, false)),
            IrPattern::None => Pattern::None,
            IrPattern::Some { inner } => Pattern::Some(Box::new(self.lower_pattern(inner))),
            IrPattern::Ok { inner } => Pattern::Ok(Box::new(self.lower_pattern(inner))),
            IrPattern::Err { inner } => Pattern::Err(Box::new(self.lower_pattern(inner))),
            IrPattern::Constructor { name, args } => Pattern::Ctor {
                tag: name.clone(),
                args: args.iter().enumerate().map(|(i, a)| (format!("_{}", i), self.lower_pattern(a))).collect(),
            },
            IrPattern::RecordPattern { name, fields, .. } => Pattern::RecordCtor {
                tag: name.clone(),
                fields: fields.iter().map(|f| (f.name.clone(), f.pattern.as_ref().map(|p| self.lower_pattern(p)))).collect(),
            },
            IrPattern::Tuple { elements } => Pattern::Tuple(elements.iter().map(|e| self.lower_pattern(e)).collect()),
        }
    }

    // ── Statements ───────────────────────────────────────────────

    fn lower_stmt(&self, stmt: &IrStmt, ie: bool, it: bool) -> Stmt {
        match &stmt.kind {
            IrStmtKind::Bind { var, mutability, value, .. } => {
                let name = self.var_name(*var);
                let val = self.lower_expr(value, ie, it);
                if it && !ie && matches!(&value.kind, IrExprKind::Call { .. }) { Stmt::TryCatchBind { name, value: val } }
                else if *mutability == Mutability::Var { Stmt::Let { name, value: val } }
                else { Stmt::Var { name, value: val } }
            }
            IrStmtKind::BindDestructure { pattern, value } => Stmt::VarDestructure {
                pattern: self.destructure_pattern(pattern), value: self.lower_expr(value, ie, it),
            },
            IrStmtKind::Assign { var, value } => Stmt::Assign { target: self.var_name(*var), value: self.lower_expr(value, ie, it) },
            IrStmtKind::IndexAssign { target, index, value } => {
                let name = self.var_name(*target);
                if matches!(self.vt().get(*target).ty, Ty::Map(_, _)) {
                    Stmt::MapSet { target: name, key: self.lower_expr(index, ie, it), value: self.lower_expr(value, ie, it) }
                } else { Stmt::IndexAssign { target: name, index: self.lower_expr(index, ie, it), value: self.lower_expr(value, ie, it) } }
            }
            IrStmtKind::FieldAssign { target, field, value } => Stmt::FieldAssign {
                target: self.var_name(*target), field: field.clone(), value: self.lower_expr(value, ie, it),
            },
            IrStmtKind::Guard { cond, else_ } => self.lower_guard(&self.lower_expr(cond, ie, it), else_, ie, it),
            IrStmtKind::Expr { expr } => Stmt::Expr(self.lower_expr(expr, ie, it)),
            IrStmtKind::Comment { text } => Stmt::Comment(text.clone()),
        }
    }

    fn lower_guard(&self, cond: &Expr, else_: &IrExpr, ie: bool, it: bool) -> Stmt {
        let neg = Expr::UnOp { op: "!", operand: Box::new(cond.clone()) };
        match &else_.kind {
            IrExprKind::Break => Stmt::If { cond: neg, body: vec![Stmt::Expr(Expr::Break)] },
            IrExprKind::Continue => Stmt::If { cond: neg, body: vec![Stmt::Expr(Expr::Continue)] },
            IrExprKind::ResultErr { expr } => Stmt::If { cond: neg, body: vec![Stmt::Expr(self.lower_err(expr, true, it))] },
            IrExprKind::ResultOk { expr } if matches!(&expr.kind, IrExprKind::Unit) => Stmt::If { cond: neg, body: vec![Stmt::Expr(Expr::Break)] },
            IrExprKind::Unit => Stmt::If { cond: neg, body: vec![Stmt::Expr(Expr::Break)] },
            _ => Stmt::If { cond: neg, body: vec![Stmt::Expr(Expr::Return(Some(Box::new(self.lower_expr(else_, ie, it)))))] },
        }
    }

    fn lower_do_stmts(&self, stmts: &[IrStmt], tail: Option<&IrExpr>, ie: bool, it: bool) -> Vec<Stmt> {
        let has_guard = stmts.iter().any(|s| matches!(&s.kind, IrStmtKind::Guard { .. }));
        let mut out = Vec::new();
        for s in stmts {
            out.push(self.lower_stmt(s, ie, it));
            if !has_guard { if let IrStmtKind::Bind { var, .. } = &s.kind { out.push(Stmt::ErrPropagate { name: self.var_name(*var) }); } }
        }
        if let Some(t) = tail {
            if has_guard { out.push(Stmt::Expr(self.lower_expr(t, ie, it))); }
            else { out.push(Stmt::Expr(Expr::Return(Some(Box::new(self.lower_expr(t, ie, it)))))); }
        }
        out
    }

    fn destructure_pattern(&self, pat: &IrPattern) -> String {
        match pat {
            IrPattern::Bind { var } => self.vt().get(*var).name.clone(),
            IrPattern::Wildcard => "_".into(),
            IrPattern::Tuple { elements } => format!("[{}]", elements.iter().map(|p| self.destructure_pattern(p)).collect::<Vec<_>>().join(", ")),
            IrPattern::RecordPattern { fields, .. } => format!("{{ {} }}", fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>().join(", ")),
            _ => "_".into(),
        }
    }

    fn map_module(&self, name: &str) -> String {
        if self.user_modules.contains(&name.to_string()) { name.to_string() }
        else if crate::stdlib::is_stdlib_module(name) { format!("__almd_{}", name) }
        else { name.to_string() }
    }
}
