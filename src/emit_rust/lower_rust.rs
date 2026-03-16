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
/// 5. **< 500 lines per file** — split into lower_types.rs + lower_rust_expr.rs.

use std::collections::HashMap;
use almide::ir::*;
use almide::types::Ty;
use super::rust_ir::*;
use super::lower_types::{lower_ty_with, collect_anon_records, generate_anon_structs, build_named_records, has_tail_self_call};

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
                generics: td.generics.as_ref().map(|gs| gs.iter()
                    .map(|g| format!("{}: Clone + std::fmt::Debug + PartialEq", g.name))
                    .collect()).unwrap_or_default(),
                derives: rust_derives(td),
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
            let enum_name = &td.name;
            // 自己参照フィールドを Box で包む関数
            let box_if_recursive = |ty: &Ty| -> Type {
                if ty_contains_name(ty, enum_name) {
                    Type::Generic("Box".into(), vec![lty(ty)])
                } else {
                    lty(ty)
                }
            };
            Some(EnumDef {
                name: td.name.clone(),
                variants: cases.iter().map(|c| Variant {
                    name: c.name.clone(),
                    kind: match &c.kind {
                        IrVariantKind::Unit => VariantKind::Unit,
                        IrVariantKind::Tuple { fields } => VariantKind::Tuple(fields.iter().map(|f| box_if_recursive(f)).collect()),
                        IrVariantKind::Record { fields } => VariantKind::Struct(fields.iter().map(|f| (f.name.clone(), box_if_recursive(&f.ty))).collect()),
                    },
                }).collect(),
                generics: td.generics.as_ref().map(|gs| gs.iter()
                    .map(|g| format!("{}: Clone + std::fmt::Debug + PartialEq", g.name))
                    .collect()).unwrap_or_default(),
                derives: rust_derives(td),
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

    // Collect lazy top-level let VarIds
    let lazy_top_let_ids: std::collections::HashSet<VarId> = ir.top_lets.iter()
        .filter(|tl| matches!(tl.kind, almide::ir::TopLetKind::Lazy))
        .map(|tl| tl.var)
        .collect();

    // Top-level lets
    let top_lets_ctx = LowerCtx { vt: &ir.var_table, ctors: &ctors, anon: &anon, named: &named, borrow_info: &borrow_info, current_fn: String::new(), param_vars: vec![], result_fns: &result_fns, in_effect: false, auto_try: false, lazy_top_lets: &lazy_top_let_ids, type_decls: &ir.type_decls };
    let top_lets: Vec<TopLet> = ir.top_lets.iter().map(|tl| {
        let name = ir.var_table.get(tl.var).name.clone();
        let ty = lower_ty_with(&anon, &named, &tl.ty);
        let value = top_lets_ctx.lower_expr(&tl.value);
        let is_const = matches!(tl.kind, almide::ir::TopLetKind::Const);
        TopLet { name, ty, value, is_const }
    }).collect();

    // Functions
    let mut functions = Vec::new();
    let mut tests = Vec::new();
    for f in &ir.functions {
        let param_vars: Vec<VarId> = f.params.iter().map(|p| p.var).collect();
        let ctx = LowerCtx { vt: &ir.var_table, ctors: &ctors, anon: &anon, named: &named, borrow_info: &borrow_info, current_fn: f.name.clone(), param_vars, result_fns: &result_fns, in_effect: f.is_effect, auto_try: f.is_effect && !f.is_test, lazy_top_lets: &lazy_top_let_ids, type_decls: &ir.type_decls };
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
    // Runtime: include almide_rt crate sources inline (for almide run single-file mode)
    // Strip #[cfg(test)] blocks and doc comments for inline embedding
    fn strip_test_blocks(src: &str) -> String {
        let mut out = String::new();
        let mut skip = false;
        let mut brace_depth = 0;
        for line in src.lines() {
            if line.trim().starts_with("#[cfg(test)]") { skip = true; continue; }
            if line.trim().starts_with("//!") { continue; } // doc comments
            if skip {
                brace_depth += line.chars().filter(|c| *c == '{').count();
                brace_depth = brace_depth.saturating_sub(line.chars().filter(|c| *c == '}').count());
                if brace_depth == 0 { skip = false; }
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        out
    }
    for (_name, source) in almide::generated::rust_runtime::RUST_RUNTIME_MODULES {
        rt.push_str(&strip_test_blocks(source));
    }

    Program {
        prelude: vec!["#![allow(unused_parens, unused_variables, dead_code, unused_imports, unused_mut, unused_must_use)]".into()],
        structs, enums, top_lets, functions, tests, main, runtime: rt,
    }
}

// ── Context ─────────────────────────────────────────────────────

pub(super) struct LowerCtx<'a> {
    pub(super) vt: &'a VarTable,
    pub(super) ctors: &'a HashMap<String, String>,
    pub(super) anon: &'a HashMap<Vec<String>, String>,
    pub(super) named: &'a HashMap<Vec<String>, String>,
    pub(super) borrow_info: &'a super::borrow::BorrowInfo,
    pub(super) current_fn: String,
    pub(super) param_vars: Vec<VarId>,
    /// Names of functions that return Result (effect fns + explicit Result returns)
    pub(super) result_fns: &'a std::collections::HashSet<String>,
    /// Whether we're currently inside an effect function (can call effect fns)
    pub(super) in_effect: bool,
    /// Whether auto-? (Result unwrapping) is enabled in this context
    /// true for: effect fn bodies, do blocks
    /// false for: test blocks, regular fn
    pub(super) auto_try: bool,
    /// VarIds of lazy top-level let bindings (need deref via `*` when accessed)
    pub(super) lazy_top_lets: &'a std::collections::HashSet<VarId>,
    /// Type declarations for looking up default field values
    pub(super) type_decls: &'a [IrTypeDecl],
}

impl<'a> LowerCtx<'a> {
    pub(super) fn lty(&self, ty: &Ty) -> Type { lower_ty_with(self.anon, self.named, ty) }

    /// Check if a VarId is a function parameter that borrow analysis marked as Borrow.
    pub(super) fn is_borrowed_param(&self, var: VarId) -> bool {
        if let Some(idx) = self.param_vars.iter().position(|v| *v == var) {
            self.borrow_info.ownership(&self.current_fn, idx) == super::borrow::ParamOwnership::Borrow
        } else {
            false
        }
    }

    /// Check if the current function returns Result (either effect fn or explicit Result[T, E]).
    pub(super) fn current_fn_returns_result(&self) -> bool {
        self.result_fns.contains(&self.current_fn)
    }

    /// Check if an IR expression is a variable used only once (safe to move instead of clone).
    pub(super) fn is_single_use_var(&self, e: &IrExpr) -> bool {
        if let IrExprKind::Var { id } = &e.kind {
            self.vt.get(*id).use_count <= 1
        } else {
            false
        }
    }

    /// Find variable names in a pattern that are bound to Box fields (recursive variants).
    pub(super) fn find_boxed_bindings(&self, pat: &IrPattern) -> Vec<String> {
        let mut result = Vec::new();
        if let IrPattern::Constructor { name, args } = pat {
            let enum_name = self.ctors.get(name).cloned().unwrap_or_default();
            if let Some(td) = self.type_decls.iter().find(|td| td.name == enum_name) {
                if let almide::ir::IrTypeDeclKind::Variant { cases, .. } = &td.kind {
                    if let Some(case) = cases.iter().find(|c| c.name == *name) {
                        let fields = match &case.kind {
                            almide::ir::IrVariantKind::Tuple { fields } => fields.clone(),
                            almide::ir::IrVariantKind::Record { fields } => fields.iter().map(|f| f.ty.clone()).collect(),
                            almide::ir::IrVariantKind::Unit => vec![],
                        };
                        for (i, arg) in args.iter().enumerate() {
                            if let Some(field_ty) = fields.get(i) {
                                if ty_contains_name(field_ty, &enum_name) {
                                    if let IrPattern::Bind { var } = arg {
                                        result.push(self.vt.get(*var).name.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        result
    }

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
                    if let Some(t) = tail {
                        let wrapped = if is_result_expr(&t) { *t } else { Expr::Ok(Box::new(*t)) };
                        (stmts, Some(wrapped))
                    } else if stmts.iter().any(|s| stmt_has_result_return(s)) {
                        // Body has return Ok/Err in loops — don't wrap
                        (stmts, None)
                    } else {
                        (stmts, Some(Expr::Ok(Box::new(Expr::Unit))))
                    }
                }
                other => {
                    let w = if is_result_expr(&other) { other } else { Expr::Ok(Box::new(other)) };
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

    // ── Statements ──

    pub(super) fn lower_stmt(&self, s: &IrStmt) -> Stmt {
        match &s.kind {
            IrStmtKind::Bind { var, mutability, value, .. } => {
                let name = crate::emit_common::sanitize(&self.vt.get(*var).name);
                let var_ty = &self.vt.get(*var).ty;
                let val = self.lower_expr(value);
                // 型注釈が必要なケース: Rust の型推論が足りない場合
                let is_map_new = matches!(&value.kind, IrExprKind::Call { target: CallTarget::Module { module, func }, args, .. }
                    if module == "map" && func == "new" && args.is_empty());
                let is_generic_variant = matches!(&value.kind, IrExprKind::Call { args, .. } if args.is_empty())
                    && (matches!(&value.ty, Ty::Named(_, args) if !args.is_empty())
                        || matches!(var_ty, Ty::Named(_, args) if !args.is_empty()));
                let needs_ty = matches!(&value.kind, IrExprKind::List { elements } if elements.is_empty())
                    || matches!(&value.kind, IrExprKind::EmptyMap | IrExprKind::OptionNone | IrExprKind::OptionSome { .. })
                    || is_map_new || is_generic_variant
                    || (matches!(&value.kind, IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. })
                        && matches!(&value.ty, Ty::Result(_, _)))
                    // Named 型の値で型引数がある場合（generic enum/struct）
                    || matches!(var_ty, Ty::Named(_, args) if !args.is_empty());
                let bind_ty = if var_ty.contains_unknown() { &value.ty } else { var_ty };
                let has_unresolved = bind_ty.contains_unknown() || contains_typevar(bind_ty);
                let is_fn_ty = matches!(bind_ty, Ty::Fn { .. });
                let ty_ann = if needs_ty && !has_unresolved && !is_fn_ty { Some(self.lty(bind_ty)) } else { None };
                Stmt::Let { name, ty: ty_ann, mutable: matches!(mutability, Mutability::Var), value: val }
            }
            IrStmtKind::BindDestructure { pattern, value } => {
                let mut pat = self.lower_pat(pattern);
                // Fill empty struct name from value type for record destructuring
                if let Pattern::Struct { name, .. } = &mut pat {
                    if name.is_empty() {
                        if let Ty::Named(n, _) = &value.ty {
                            *name = n.clone();
                        } else if let Ty::Record { fields } = &value.ty {
                            let mut fnames: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
                            fnames.sort();
                            *name = self.anon.get(&fnames).cloned()
                                .or_else(|| self.named.get(&fnames).cloned())
                                .unwrap_or_else(|| "AnonRecord".into());
                        }
                    }
                }
                Stmt::LetPattern { pattern: pat, value: self.lower_expr(value) }
            }
            IrStmtKind::Assign { var, value } => Stmt::Assign {
                target: crate::emit_common::sanitize(&self.vt.get(*var).name), value: self.lower_expr(value),
            },
            IrStmtKind::IndexAssign { target, index, value } => {
                let target_name = crate::emit_common::sanitize(&self.vt.get(*target).name);
                let target_ty = &self.vt.get(*target).ty;
                if matches!(target_ty, Ty::Map(_, _)) {
                    // Map index assign: m[k] = v → m.insert(k, v)
                    let idx = self.lower_expr(index);
                    let val = self.lower_expr(value);
                    Stmt::Expr(Expr::Raw(format!("{}.insert({}, {});",
                        target_name, super::render::expr_str(&idx), super::render::expr_str(&val))))
                } else {
                    Stmt::IndexAssign {
                        target: target_name,
                        index: self.lower_expr(index), value: self.lower_expr(value),
                    }
                }
            }
            IrStmtKind::FieldAssign { target, field, value } => Stmt::FieldAssign {
                target: crate::emit_common::sanitize(&self.vt.get(*target).name),
                field: field.clone(), value: self.lower_expr(value),
            },
            IrStmtKind::Guard { cond, else_ } => {
                let guard_body = match &else_.kind {
                    IrExprKind::Break => Expr::Break,
                    IrExprKind::Continue => Expr::Continue { label: None },
                    // effect fn: ok(()) → break (ループ脱出), ok(value) → return Ok(value)
                    IrExprKind::ResultOk { expr } if self.auto_try && matches!(&expr.ty, Ty::Unit) => Expr::Break,
                    _ if self.auto_try => Expr::Return(Some(Box::new(self.lower_expr(else_)))),
                    // In non-auto-try context (tests, pure fn): don't wrap in Ok/Err
                    IrExprKind::ResultOk { expr } if !self.auto_try => {
                        let inner = self.lower_expr(expr);
                        Expr::Return(Some(Box::new(inner)))
                    }
                    IrExprKind::ResultErr { expr } if !self.auto_try => {
                        let inner = self.lower_expr(expr);
                        // If in a function returning Result, generate return Err(...)
                        // Otherwise (tests, void fn), panic
                        if self.in_effect || self.current_fn_returns_result() {
                            Expr::Return(Some(Box::new(Expr::Err(Box::new(inner)))))
                        } else {
                            Expr::Raw(format!("panic!(\"{{:?}}\", {})", super::render::expr_str(&inner)))
                        }
                    }
                    _ => Expr::Return(Some(Box::new(self.lower_expr(else_)))),
                };
                Stmt::Expr(Expr::If {
                    cond: Box::new(Expr::UnOp { op: "!", operand: Box::new(self.lower_expr(cond)) }),
                    then: Box::new(guard_body),
                    else_: None,
                })
            }
            IrStmtKind::Expr { expr } => Stmt::Expr(self.lower_expr(expr)),
            IrStmtKind::Comment { text } => Stmt::Expr(Expr::Raw(format!("// {}", text))),
        }
    }

    // ── Patterns ──

    pub(super) fn lower_pat(&self, p: &IrPattern) -> Pattern {
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

/// 型が特定の名前の Named 型を**直接**含むか（再帰型検出用）
/// List/Option/Map の中は間接参照なので Box 不要 → 無視
pub(super) fn ty_contains_name(ty: &Ty, name: &str) -> bool {
    match ty {
        Ty::Named(n, args) => n == name || args.iter().any(|a| ty_contains_name(a, name)),
        Ty::Tuple(ts) => ts.iter().any(|t| ty_contains_name(t, name)),
        // List, Option, Map, Result は既に heap 上の indirection なので再帰とみなさない
        _ => false,
    }
}

fn contains_typevar(ty: &Ty) -> bool {
    match ty {
        Ty::TypeVar(_) => true,
        Ty::List(inner) | Ty::Option(inner) => contains_typevar(inner),
        Ty::Result(a, b) | Ty::Map(a, b) => contains_typevar(a) || contains_typevar(b),
        Ty::Tuple(ts) => ts.iter().any(contains_typevar),
        Ty::Named(_, args) => args.iter().any(contains_typevar),
        Ty::Fn { params, ret } => params.iter().any(contains_typevar) || contains_typevar(ret),
        _ => false,
    }
}

/// Check if an expression already produces a Result (Ok/Err), including through
/// if/match/block where all branches are Result-producing.
pub(super) fn is_result_expr(e: &Expr) -> bool {
    match e {
        Expr::Ok(_) | Expr::Err(_) => true,
        Expr::Return(Some(inner)) => is_result_expr(inner),
        Expr::If { then, else_: Some(else_), .. } => is_result_expr(then) && is_result_expr(else_),
        Expr::Match { arms, .. } => !arms.is_empty() && arms.iter().all(|a| is_result_expr(&a.body)),
        Expr::Block { tail: Some(t), .. } => is_result_expr(t),
        // Block with no tail but containing a loop with return Ok/Err (do-block pattern)
        Expr::Block { stmts, tail: None } => stmts.iter().any(|s| stmt_has_result_return(s)),
        _ => false,
    }
}

fn stmt_has_result_return(s: &Stmt) -> bool {
    match s {
        Stmt::Expr(e) => expr_has_result_return(e),
        _ => false,
    }
}

fn expr_has_result_return(e: &Expr) -> bool {
    match e {
        Expr::Return(Some(inner)) => is_result_expr(inner),
        Expr::Block { stmts, tail } => {
            stmts.iter().any(|s| stmt_has_result_return(s))
                || tail.as_ref().map_or(false, |t| expr_has_result_return(t))
        }
        Expr::Loop { body, .. } => body.iter().any(|s| stmt_has_result_return(s)),
        Expr::If { then, else_, .. } => {
            expr_has_result_return(then) || else_.as_ref().map_or(false, |e| expr_has_result_return(e))
        }
        _ => false,
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
    // Repr → Debug
    if conventions.iter().any(|d| d == "Repr") {
        derives.push("Debug".into());
    } else {
        derives.push("Debug".into()); // always derive Debug for now
    }
    // Ord → PartialOrd + Ord
    if conventions.iter().any(|d| d == "Ord") {
        derives.push("PartialOrd".into());
        derives.push("Ord".into());
    }
    // Hash → Hash
    if conventions.iter().any(|d| d == "Hash") {
        derives.push("Hash".into());
    }
    derives
}
