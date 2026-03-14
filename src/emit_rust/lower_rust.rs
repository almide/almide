/// IR → RustIR lowering pass: all codegen decisions.
///
/// Input:    &IrProgram
/// Output:   RustIR Program
/// Owns:     clone insertion, auto-? insertion, Ok-wrap, TCO, type annotations, variant qualification
/// Does NOT: string rendering (render.rs), borrow inference (borrow.rs)
///
/// Principles:
/// 1. **Single responsibility** — all codegen decisions happen here.
/// 2. **Checker's types are gospel** — never re-infer types.
/// 3. **No state flags** — function context from IrFunction fields.
/// 4. **Output is decision-free** — RustIR renders by pure pattern matching.
/// 5. **< 1000 lines per file** — split into lower_types.rs for type/anon logic.

use std::collections::HashMap;
use almide::ir::*;
use almide::types::Ty;
use super::rust_ir::*;
use super::lower_types::{self, lower_ty_with, is_copy, collect_anon_records, generate_anon_structs, build_named_records, has_tail_self_call};

/// Lower an entire IrProgram to a RustIR Program.
pub fn lower(ir: &IrProgram) -> Program {
    // Pre-compute maps
    let anon = collect_anon_records(ir);
    let named = build_named_records(ir);
    let mut ctors: HashMap<String, String> = HashMap::new();
    for td in &ir.type_decls {
        if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
            for c in cases { ctors.insert(c.name.clone(), td.name.clone()); }
        }
    }
    let lty = |ty: &Ty| lower_ty_with(&anon, &named, ty);

    // Type declarations
    let mut structs = generate_anon_structs(&anon);
    for td in &ir.type_decls {
        match &td.kind {
            IrTypeDeclKind::Record { fields } => structs.push(StructDef {
                name: td.name.clone(),
                fields: fields.iter().map(|f| (f.name.clone(), lty(&f.ty))).collect(),
                generics: vec![], derives: rust_derives(td),
                is_pub: true,
            }),
            IrTypeDeclKind::Variant { cases, .. } => {
                // handled below
            }
            _ => {}
        }
    }
    let enums: Vec<EnumDef> = ir.type_decls.iter().filter_map(|td| {
        if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
            Some(EnumDef {
                name: td.name.clone(),
                variants: cases.iter().map(|c| Variant {
                    name: c.name.clone(),
                    kind: match &c.kind {
                        IrVariantKind::Unit => VariantKind::Unit,
                        IrVariantKind::Tuple { fields } => VariantKind::Tuple(fields.iter().map(&lty).collect()),
                        IrVariantKind::Record { fields } => VariantKind::Struct(fields.iter().map(|f| (f.name.clone(), lty(&f.ty))).collect()),
                    },
                }).collect(),
                generics: vec![], derives: rust_derives(td),
                is_pub: true,
            })
        } else { None }
    }).collect();

    // Borrow analysis
    let borrow_info = super::borrow::analyze(ir);

    // Collect result-returning function names (for auto-? in effect context)
    let result_fns: std::collections::HashSet<String> = ir.functions.iter()
        .filter(|f| f.is_effect || matches!(&f.ret_ty, Ty::Result(_, _)))
        .map(|f| f.name.clone())
        .collect();

    // Functions
    let mut functions = Vec::new();
    let mut tests = Vec::new();
    for f in &ir.functions {
        let ctx = LowerCtx { vt: &ir.var_table, ctors: &ctors, anon: &anon, named: &named, result_fns: &result_fns, in_effect: f.is_effect };
        let rf = ctx.lower_fn(f);
        if f.is_test { tests.push(rf); } else { functions.push(rf); }
    }

    // Main wrapper
    let main = ir.functions.iter().find(|f| f.name == "main").map(|f| {
        let call = Expr::Call { func: "almide_main".into(), args: vec![] };
        let body = if f.is_effect {
            vec![Stmt::Expr(Expr::If {
                cond: Box::new(Expr::MethodCall { recv: Box::new(call), method: "is_err".into(), args: vec![] }),
                then: Box::new(Expr::Block { stmts: vec![Stmt::Expr(Expr::Raw("std::process::exit(1)".into()))], tail: None }),
                else_: None,
            })]
        } else {
            vec![Stmt::Expr(call)]
        };
        Function { name: "main".into(), generics: vec![], params: vec![], ret: Type::Unit, body, tail: None, attrs: vec![], is_pub: false }
    });

    // Runtime
    let mut rt = String::new();
    rt.push_str("use std::collections::HashMap;\n");
    rt.push_str("trait AlmideConcat<Rhs> { type Output; fn concat(self, rhs: Rhs) -> Self::Output; }\n");
    rt.push_str("impl AlmideConcat<String> for String { type Output = String; #[inline(always)] fn concat(self, rhs: String) -> String { format!(\"{}{}\", self, rhs) } }\n");
    rt.push_str("impl AlmideConcat<&str> for String { type Output = String; #[inline(always)] fn concat(self, rhs: &str) -> String { format!(\"{}{}\", self, rhs) } }\n");
    rt.push_str("impl AlmideConcat<String> for &str { type Output = String; #[inline(always)] fn concat(self, rhs: String) -> String { format!(\"{}{}\", self, rhs) } }\n");
    rt.push_str("impl AlmideConcat<&str> for &str { type Output = String; #[inline(always)] fn concat(self, rhs: &str) -> String { format!(\"{}{}\", self, rhs) } }\n");
    rt.push_str("impl<T: Clone> AlmideConcat<Vec<T>> for Vec<T> { type Output = Vec<T>; #[inline(always)] fn concat(self, rhs: Vec<T>) -> Vec<T> { let mut r = self; r.extend(rhs); r } }\n");
    rt.push_str("macro_rules! almide_eq { ($a:expr, $b:expr) => { ($a) == ($b) }; }\n");
    rt.push_str("macro_rules! almide_ne { ($a:expr, $b:expr) => { ($a) != ($b) }; }\n");

    Program {
        prelude: vec!["#![allow(unused_parens, unused_variables, dead_code, unused_imports, unused_mut, unused_must_use)]".into()],
        structs, enums, functions, tests, main, runtime: rt,
    }
}

// ── Context ─────────────────────────────────────────────────────

struct LowerCtx<'a> {
    vt: &'a VarTable,
    ctors: &'a HashMap<String, String>,
    anon: &'a HashMap<Vec<String>, String>,
    named: &'a HashMap<Vec<String>, String>,
    /// Names of functions that return Result (effect fns + explicit Result returns)
    result_fns: &'a std::collections::HashSet<String>,
    /// Whether we're currently inside an effect function
    in_effect: bool,
}

impl<'a> LowerCtx<'a> {
    fn lty(&self, ty: &Ty) -> Type { lower_ty_with(self.anon, self.named, ty) }

    fn lower_fn(&self, f: &IrFunction) -> Function {
        let fn_name = if f.name == "main" { "almide_main".into() }
            else if f.is_test { format!("test_{}", f.name.replace(|c: char| !c.is_alphanumeric() && c != '_', "_")) }
            else { crate::emit_common::sanitize(&f.name) };

        let ret = if f.is_test { Type::Unit }
            else if f.is_effect {
                match &f.ret_ty {
                    Ty::Result(_, _) => self.lty(&f.ret_ty),
                    Ty::Unit => Type::Result(Box::new(Type::Unit), Box::new(Type::Str)),
                    _ => Type::Result(Box::new(self.lty(&f.ret_ty)), Box::new(Type::Str)),
                }
            } else { self.lty(&f.ret_ty) };

        let use_tco = has_tail_self_call(&f.name, &f.body);
        let params: Vec<Param> = f.params.iter().map(|p| Param {
            name: crate::emit_common::sanitize(&p.name), ty: self.lty(&p.ty), mutable: use_tco,
        }).collect();

        let body_expr = if use_tco {
            let param_names: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
            Expr::Loop { label: Some("_tco".into()), body: vec![Stmt::Expr(self.lower_tco(&f.body, &f.name, &param_names))] }
        } else {
            self.lower_expr(&f.body)
        };

        // Wrap body for test/effect
        let (body, tail) = if f.is_test {
            match body_expr {
                Expr::Block { stmts, tail } => (stmts, tail.map(|t| *t)),
                other => (vec![], Some(other)),
            }
        } else if f.is_effect {
            match body_expr {
                Expr::Block { stmts, tail } => {
                    let wrapped = tail.map(|t| match *t {
                        Expr::Ok(_) | Expr::Err(_) => *t,
                        other => Expr::Ok(Box::new(other)),
                    }).unwrap_or(Expr::Ok(Box::new(Expr::Unit)));
                    (stmts, Some(wrapped))
                }
                other => {
                    let w = match &other { Expr::Ok(_) | Expr::Err(_) => other, _ => Expr::Ok(Box::new(other)) };
                    (vec![], Some(w))
                }
            }
        } else {
            match body_expr {
                Expr::Block { stmts, tail } => (stmts, tail.map(|t| *t)),
                other => (vec![], Some(other)),
            }
        };

        let generics = f.generics.as_ref().map(|gs| gs.iter()
            .filter(|g| g.structural_bound.is_none())
            .map(|g| format!("{}: Clone + std::fmt::Debug + PartialEq + PartialOrd", g.name))
            .collect()).unwrap_or_default();

        Function { name: fn_name, generics, params, ret, body, tail,
            attrs: if f.is_test { vec!["#[test]".into()] } else { vec![] }, is_pub: !f.is_test }
    }

    // ── Expression ──

    fn lower_expr(&self, e: &IrExpr) -> Expr {
        match &e.kind {
            IrExprKind::LitInt { value } => Expr::Int(*value),
            IrExprKind::LitFloat { value } => Expr::Float(*value),
            IrExprKind::LitStr { value } => Expr::Str(value.clone()),
            IrExprKind::LitBool { value } => Expr::Bool(*value),
            IrExprKind::Unit => Expr::Unit,
            IrExprKind::Var { id } => Expr::Var(crate::emit_common::sanitize(&self.vt.get(*id).name)),

            IrExprKind::BinOp { op, left, right } => {
                let l = self.lower_expr(left);
                let r = self.lower_expr(right);
                match op {
                    BinOp::PowFloat => Expr::MethodCall { recv: Box::new(l), method: "powf".into(), args: vec![r] },
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
            IrExprKind::Match { subject, arms } => Expr::Match {
                subject: Box::new(self.lower_expr(subject)),
                arms: arms.iter().map(|a| MatchArm {
                    pat: self.lower_pat(&a.pattern),
                    guard: a.guard.as_ref().map(|g| self.lower_expr(g)),
                    body: self.lower_expr(&a.body),
                }).collect(),
            },
            IrExprKind::Block { stmts, expr } => Expr::Block {
                stmts: stmts.iter().map(|s| self.lower_stmt(s)).collect(),
                tail: expr.as_ref().map(|e| Box::new(self.lower_expr(e))),
            },
            IrExprKind::DoBlock { stmts, expr } => Expr::Block {
                stmts: stmts.iter().map(|s| self.lower_stmt(s)).collect(),
                tail: expr.as_ref().map(|e| Box::new(self.lower_expr(e))),
            },
            IrExprKind::ForIn { var, iterable, body, .. } => Expr::For {
                var: self.vt.get(*var).name.clone(),
                iter: Box::new(Expr::Clone(Box::new(self.lower_expr(iterable)))),
                body: body.iter().map(|s| self.lower_stmt(s)).collect(),
            },
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
                match target {
                    CallTarget::Named { name } => self.lower_named_call(name, ir_args),
                    CallTarget::Module { module, func } => Expr::Call {
                        func: format!("{}_{}", module.replace('.', "_"), crate::emit_common::sanitize(func)),
                        args: ir_args,
                    },
                    CallTarget::Method { object, method } => {
                        let obj = self.lower_expr(object);
                        let mut all = vec![obj]; all.extend(ir_args);
                        Expr::Call { func: crate::emit_common::sanitize(method), args: all }
                    }
                    CallTarget::Computed { callee } => {
                        let c = self.lower_expr(callee);
                        Expr::Raw(format!("({})({})", super::render::expr_str(&c),
                            ir_args.iter().map(|a| super::render::expr_str(a)).collect::<Vec<_>>().join(", ")))
                    }
                }
            }

            IrExprKind::List { elements } => Expr::Vec(elements.iter().map(|e| self.lower_expr(e)).collect()),
            IrExprKind::MapLiteral { entries } => Expr::HashMap(entries.iter().map(|(k, v)| (self.lower_expr(k), self.lower_expr(v))).collect()),
            IrExprKind::EmptyMap => Expr::Raw("HashMap::new()".into()),
            IrExprKind::Tuple { elements } => Expr::Tuple(elements.iter().map(|e| self.lower_expr(e)).collect()),
            IrExprKind::Record { name, fields } => {
                let sname = name.as_ref().map(|n| self.ctors.get(n).map(|e| format!("{}::{}", e, n)).unwrap_or(n.clone())).unwrap_or_else(|| {
                    let mut fnames: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
                    fnames.sort();
                    self.anon.get(&fnames).cloned().unwrap_or("AnonRecord".into())
                });
                Expr::Struct { name: sname, fields: fields.iter().map(|(n, v)| (n.clone(), self.lower_expr(v))).collect() }
            }
            IrExprKind::SpreadRecord { base, fields } => Expr::StructUpdate {
                base: Box::new(Expr::Clone(Box::new(self.lower_expr(base)))),
                fields: fields.iter().map(|(n, v)| (n.clone(), self.lower_expr(v))).collect(),
            },
            IrExprKind::Member { object, field } => {
                let obj = self.lower_expr(object);
                if is_copy(&e.ty) { Expr::Field(Box::new(obj), field.clone()) }
                else { Expr::Clone(Box::new(Expr::Field(Box::new(obj), field.clone()))) }
            }
            IrExprKind::TupleIndex { object, index } => Expr::TupleIdx(Box::new(self.lower_expr(object)), *index),
            IrExprKind::IndexAccess { object, index } => Expr::Index(Box::new(self.lower_expr(object)), Box::new(self.lower_expr(index))),
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
            IrExprKind::OptionNone => Expr::None,
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
            "assert" => Expr::Macro { name: "assert".into(), args },
            _ => {
                let call = if let Some(enum_name) = self.ctors.get(name) {
                    if args.is_empty() { return Expr::Var(format!("{}::{}", enum_name, name)); }
                    Expr::Call { func: format!("{}::{}", enum_name, name), args }
                } else {
                    Expr::Call { func: crate::emit_common::sanitize(name), args }
                };
                // Auto-? for calls to result-returning functions in effect context
                if self.in_effect && self.result_fns.contains(name) {
                    Expr::Try(Box::new(call))
                } else {
                    call
                }
            }
        }
    }

    // ── TCO ──

    fn lower_tco(&self, e: &IrExpr, fn_name: &str, params: &[String]) -> Expr {
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

    // ── Statements ──

    fn lower_stmt(&self, s: &IrStmt) -> Stmt {
        match &s.kind {
            IrStmtKind::Bind { var, mutability, value, .. } => {
                let name = crate::emit_common::sanitize(&self.vt.get(*var).name);
                let val = self.lower_expr(value);
                let needs_ty = matches!(&value.kind, IrExprKind::List { elements } if elements.is_empty())
                    || matches!(&value.kind, IrExprKind::EmptyMap | IrExprKind::OptionNone)
                    || (matches!(&value.kind, IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. })
                        && matches!(&value.ty, Ty::Result(_, _)) && !value.ty.contains_unknown());
                let ty_ann = if needs_ty { Some(self.lty(&value.ty)) } else { None };
                Stmt::Let { name, ty: ty_ann, mutable: matches!(mutability, Mutability::Var), value: val }
            }
            IrStmtKind::BindDestructure { pattern, value } => Stmt::LetPattern {
                pattern: self.lower_pat(pattern), value: self.lower_expr(value),
            },
            IrStmtKind::Assign { var, value } => Stmt::Assign {
                target: crate::emit_common::sanitize(&self.vt.get(*var).name), value: self.lower_expr(value),
            },
            IrStmtKind::IndexAssign { target, index, value } => Stmt::IndexAssign {
                target: crate::emit_common::sanitize(&self.vt.get(*target).name),
                index: self.lower_expr(index), value: self.lower_expr(value),
            },
            IrStmtKind::FieldAssign { target, field, value } => Stmt::FieldAssign {
                target: crate::emit_common::sanitize(&self.vt.get(*target).name),
                field: field.clone(), value: self.lower_expr(value),
            },
            IrStmtKind::Guard { cond, else_ } => Stmt::Expr(Expr::If {
                cond: Box::new(Expr::UnOp { op: "!", operand: Box::new(self.lower_expr(cond)) }),
                then: Box::new(Expr::Return(Some(Box::new(self.lower_expr(else_))))),
                else_: None,
            }),
            IrStmtKind::Expr { expr } => Stmt::Expr(self.lower_expr(expr)),
            IrStmtKind::Comment { text } => Stmt::Expr(Expr::Raw(format!("// {}", text))),
        }
    }

    // ── Patterns ──

    fn lower_pat(&self, p: &IrPattern) -> Pattern {
        match p {
            IrPattern::Wildcard => Pattern::Wild,
            IrPattern::Bind { var } => Pattern::Var(self.vt.get(*var).name.clone()),
            IrPattern::Literal { expr } => Pattern::Lit(self.lower_expr(expr)),
            IrPattern::Constructor { name, args } => {
                let qualified = self.ctors.get(name).map(|e| format!("{}::{}", e, name)).unwrap_or(name.clone());
                Pattern::Ctor { name: qualified, args: args.iter().map(|a| self.lower_pat(a)).collect() }
            }
            IrPattern::RecordPattern { name, fields, rest } => {
                let qualified = self.ctors.get(name).map(|e| format!("{}::{}", e, name)).unwrap_or(name.clone());
                Pattern::Struct {
                    name: qualified,
                    fields: fields.iter().map(|f| (f.name.clone(), f.pattern.as_ref().map(|p| self.lower_pat(p)))).collect(),
                    rest: *rest,
                }
            }
            IrPattern::Tuple { elements } => Pattern::Tuple(elements.iter().map(|e| self.lower_pat(e)).collect()),
            IrPattern::Some { inner } => Pattern::Ctor { name: "Some".into(), args: vec![self.lower_pat(inner)] },
            IrPattern::None => Pattern::Var("None".into()),
            IrPattern::Ok { inner } => Pattern::Ctor { name: "Ok".into(), args: vec![self.lower_pat(inner)] },
            IrPattern::Err { inner } => Pattern::Ctor { name: "Err".into(), args: vec![self.lower_pat(inner)] },
        }
    }
}

/// Map Almide derive conventions to Rust #[derive(...)] attributes.
/// Base derives (Clone) are always included. Convention-specific derives are added
/// based on the `deriving` field in IrTypeDecl.
fn rust_derives(td: &IrTypeDecl) -> Vec<String> {
    let mut derives = vec!["Clone".to_string()];
    let conventions = td.deriving.as_deref().unwrap_or_default();
    // Eq → PartialEq + Eq
    if conventions.iter().any(|d| d == "Eq") {
        derives.push("PartialEq".into());
        derives.push("Eq".into());
    } else {
        // Default: PartialEq for backward compatibility (== uses almide_eq! macro)
        derives.push("PartialEq".into());
    }
    // Show → Debug
    if conventions.iter().any(|d| d == "Show") {
        derives.push("Debug".into());
    } else {
        derives.push("Debug".into()); // always derive Debug for now
    }
    // Compare → PartialOrd + Ord
    if conventions.iter().any(|d| d == "Compare") {
        derives.push("PartialOrd".into());
        derives.push("Ord".into());
    }
    // Hash → Hash
    if conventions.iter().any(|d| d == "Hash") {
        derives.push("Hash".into());
    }
    derives
}
