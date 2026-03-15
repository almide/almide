/// AST + Types → Typed IR lowering pass.
///
/// Input:    Program + expr_types + TypeEnv
/// Output:   IrProgram
/// Owns:     desugaring (pipe→call, UFCS, interpolation, operators→BinOp), VarId assignment
/// Does NOT: type inference (trusts checker), codegen decisions (trusts codegen)
///
/// Principles:
/// 1. **Checker is the source of truth** — every expression's type comes from
///    expr_types (populated by the constraint-based checker). Lower never
///    guesses types or falls back to Unknown.
/// 2. **No type inference** — lower is a mechanical translation, not a type
///    checker. If a type is missing from expr_types, that's a checker bug.
/// 3. **Desugar once** — pipes, UFCS, string interpolation, operators are
///    desugared here and nowhere else.
/// 4. **VarId for everything** — all variable references become VarId lookups.
///    No string-based variable resolution in codegen.

use std::collections::HashMap;
use crate::ast;
use crate::ir::*;
use crate::types::{Ty, TypeEnv};

// ── Context ─────────────────────────────────────────────────────

pub struct LowerCtx<'a> {
    pub var_table: VarTable,
    scopes: Vec<HashMap<String, VarId>>,
    expr_types: &'a HashMap<crate::ast::ExprId, Ty>,
    env: &'a TypeEnv,
}

impl<'a> LowerCtx<'a> {
    pub fn new(expr_types: &'a HashMap<crate::ast::ExprId, Ty>, env: &'a TypeEnv) -> Self {
        LowerCtx {
            var_table: VarTable::new(),
            scopes: vec![HashMap::new()],
            expr_types,
            env,
        }
    }

    fn push_scope(&mut self) { self.scopes.push(HashMap::new()); }
    fn pop_scope(&mut self) {
        debug_assert!(self.scopes.len() > 1, "scope underflow");
        self.scopes.pop();
    }

    fn define_var(&mut self, name: &str, ty: Ty, mutability: Mutability, span: Option<ast::Span>) -> VarId {
        let id = self.var_table.alloc(name.to_string(), ty, mutability, span);
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), id);
        }
        id
    }

    fn lookup_var(&self, name: &str) -> Option<VarId> {
        for scope in self.scopes.iter().rev() {
            if let Some(&id) = scope.get(name) {
                return Some(id);
            }
        }
        None
    }

    /// Get the type of an expression from the checker's expr_types.
    /// This is the ONLY way to get types — no fallback inference.
    fn expr_ty(&self, expr: &ast::Expr) -> Ty {
        match self.expr_types.get(&expr.id()).cloned() {
            Some(ty) => ty,
            None => {
                // ICE: checker should have assigned a type to every expression
                eprintln!("[ICE] lower: missing type for expr id={}", expr.id().0);
                Ty::Unknown
            }
        }
    }

    /// Resolve a field type on a known object type.
    fn resolve_field_ty(&self, obj_ty: &Ty, field: &str) -> Ty {
        match obj_ty {
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                fields.iter().find(|(n, _)| n == field).map(|(_, t)| t.clone()).unwrap_or(Ty::Unknown)
            }
            Ty::Named(name, _) => {
                if let Some(def) = self.env.types.get(name) {
                    self.resolve_field_ty(def, field)
                } else { Ty::Unknown }
            }
            Ty::TypeVar(tv) => {
                if let Some(bound) = self.env.structural_bounds.get(tv) {
                    self.resolve_field_ty(bound, field)
                } else { Ty::Unknown }
            }
            _ => Ty::Unknown,
        }
    }

    fn mk(&self, kind: IrExprKind, ty: Ty, span: Option<ast::Span>) -> IrExpr {
        IrExpr { kind, ty, span }
    }
}

// ── Public API ──────────────────────────────────────────────────

pub fn lower_program(prog: &ast::Program, expr_types: &HashMap<crate::ast::ExprId, Ty>, env: &TypeEnv) -> IrProgram {
    let mut ctx = LowerCtx::new(expr_types, env);
    let mut functions = Vec::new();
    let mut top_lets = Vec::new();
    let mut type_decls = Vec::new();

    for decl in &prog.decls {
        match decl {
            ast::Decl::Fn { name, params, body: Some(body), effect, r#async, span, generics, extern_attrs, visibility, .. } => {
                let f = lower_fn(&mut ctx, name, params, body, effect, r#async, span, generics, extern_attrs, visibility, None);
                functions.push(f);
            }
            ast::Decl::Type { name, ty, deriving, visibility, generics, .. } => {
                type_decls.push(lower_type_decl(&mut ctx, name, ty, deriving, visibility, generics.as_ref()));
            }
            ast::Decl::TopLet { name, ty: _, value, .. } => {
                let val_ty = ctx.expr_ty(value);
                let var = ctx.define_var(name, val_ty.clone(), Mutability::Let, None);
                let ir_value = lower_expr(&mut ctx, value);
                let kind = classify_top_let_kind(&ir_value);
                top_lets.push(IrTopLet { var, ty: val_ty, value: ir_value, kind });
            }
            ast::Decl::Test { name, body, .. } => {
                let test_fn = lower_test(&mut ctx, name, body);
                functions.push(test_fn);
            }
            ast::Decl::Impl { methods, .. } => {
                for m in methods {
                    if let ast::Decl::Fn { name, params, body: Some(body), effect, r#async, span, generics, extern_attrs, visibility, .. } = m {
                        let f = lower_fn(&mut ctx, name, params, body, effect, r#async, span, generics, extern_attrs, visibility, None);
                        functions.push(f);
                    }
                }
            }
            _ => {}
        }
    }

    let mut program = IrProgram { functions, top_lets, type_decls, var_table: ctx.var_table, modules: Vec::new() };
    compute_use_counts(&mut program);
    program
}

pub fn lower_module(
    name: &str,
    prog: &ast::Program,
    expr_types: &HashMap<crate::ast::ExprId, Ty>,
    env: &TypeEnv,
    versioned_name: Option<String>,
) -> IrModule {
    let ir_prog = lower_program(prog, expr_types, env);
    IrModule {
        name: name.to_string(),
        versioned_name,
        type_decls: ir_prog.type_decls,
        functions: ir_prog.functions,
        top_lets: ir_prog.top_lets,
        var_table: ir_prog.var_table,
    }
}

// ── Function lowering ───────────────────────────────────────────

fn lower_fn(
    ctx: &mut LowerCtx,
    name: &str, params: &[ast::Param], body: &ast::Expr,
    effect: &Option<bool>, r#async: &Option<bool>, span: &Option<ast::Span>,
    generics: &Option<Vec<ast::GenericParam>>, extern_attrs: &[ast::ExternAttr],
    visibility: &ast::Visibility, _module_prefix: Option<&str>,
) -> IrFunction {
    ctx.push_scope();
    let mut ir_params = Vec::new();
    for p in params {
        let ty = resolve_type_expr(&p.ty);
        let var = ctx.define_var(&p.name, ty.clone(), Mutability::Let, span.clone());
        ir_params.push(IrParam {
            var, ty: ty.clone(), name: p.name.clone(),
            borrow: ParamBorrow::Own, open_record: None,
        });
    }

    let ret_ty = if let Some(sig) = ctx.env.functions.get(name) {
        sig.ret.clone()
    } else {
        ctx.expr_ty(body)
    };

    let ir_body = lower_expr(ctx, body);
    ctx.pop_scope();

    let is_effect = effect.unwrap_or(false);
    let is_async = r#async.unwrap_or(false);
    let vis = match visibility {
        ast::Visibility::Public => IrVisibility::Public,
        ast::Visibility::Mod => IrVisibility::Mod,
        ast::Visibility::Local => IrVisibility::Private,
    };

    IrFunction {
        name: name.to_string(), params: ir_params, ret_ty, body: ir_body,
        is_effect, is_async, is_test: false,
        generics: generics.clone(), extern_attrs: extern_attrs.to_vec(), visibility: vis,
    }
}

fn lower_test(ctx: &mut LowerCtx, name: &str, body: &ast::Expr) -> IrFunction {
    ctx.push_scope();
    let ir_body = lower_expr(ctx, body);
    ctx.pop_scope();
    IrFunction {
        name: name.to_string(), params: vec![], ret_ty: Ty::Unit, body: ir_body,
        is_effect: true, is_async: false, is_test: true,
        generics: None, extern_attrs: vec![], visibility: IrVisibility::Public,
    }
}

// ── Expression lowering ─────────────────────────────────────────

fn lower_expr(ctx: &mut LowerCtx, expr: &ast::Expr) -> IrExpr {
    let ty = ctx.expr_ty(expr);
    let span = expr.span();

    match expr {
        // ── Literals ──
        ast::Expr::Int { raw, .. } => {
            let value = raw.parse::<i64>().unwrap_or(0);
            ctx.mk(IrExprKind::LitInt { value }, ty, span)
        }
        ast::Expr::Float { value, .. } => ctx.mk(IrExprKind::LitFloat { value: *value }, ty, span),
        ast::Expr::String { value, .. } => ctx.mk(IrExprKind::LitStr { value: value.clone() }, ty, span),
        ast::Expr::Bool { value, .. } => ctx.mk(IrExprKind::LitBool { value: *value }, ty, span),
        ast::Expr::Unit { .. } => ctx.mk(IrExprKind::Unit, Ty::Unit, span),

        // ── Variables ──
        ast::Expr::Ident { name, .. } => {
            if let Some(var_id) = ctx.lookup_var(name) {
                ctx.mk(IrExprKind::Var { id: var_id }, ty, span)
            } else {
                ctx.mk(IrExprKind::Var { id: VarId(0) }, ty, span) // error recovery
            }
        }
        ast::Expr::TypeName { name, .. } => {
            // Variant constructor used as value (e.g., Red)
            if ctx.env.constructors.contains_key(name) {
                ctx.mk(IrExprKind::Call {
                    target: CallTarget::Named { name: name.clone() },
                    args: vec![], type_args: vec![],
                }, ty, span)
            } else if let Some(var_id) = ctx.lookup_var(name) {
                ctx.mk(IrExprKind::Var { id: var_id }, ty, span)
            } else {
                ctx.mk(IrExprKind::Var { id: VarId(0) }, ty, span)
            }
        }

        // ── Collections ──
        ast::Expr::List { elements, .. } => {
            let elems = elements.iter().map(|e| lower_expr(ctx, e)).collect();
            ctx.mk(IrExprKind::List { elements: elems }, ty, span)
        }
        ast::Expr::MapLiteral { entries, .. } => {
            let pairs = entries.iter().map(|(k, v)| (lower_expr(ctx, k), lower_expr(ctx, v))).collect();
            ctx.mk(IrExprKind::MapLiteral { entries: pairs }, ty, span)
        }
        ast::Expr::EmptyMap { .. } => ctx.mk(IrExprKind::EmptyMap, ty, span),
        ast::Expr::Tuple { elements, .. } => {
            let elems = elements.iter().map(|e| lower_expr(ctx, e)).collect();
            ctx.mk(IrExprKind::Tuple { elements: elems }, ty, span)
        }

        // ── Records ──
        ast::Expr::Record { name, fields, .. } => {
            let fs = fields.iter().map(|f| (f.name.clone(), lower_expr(ctx, &f.value))).collect();
            ctx.mk(IrExprKind::Record { name: name.clone(), fields: fs }, ty, span)
        }
        ast::Expr::SpreadRecord { base, fields, .. } => {
            let ir_base = lower_expr(ctx, base);
            let fs = fields.iter().map(|f| (f.name.clone(), lower_expr(ctx, &f.value))).collect();
            ctx.mk(IrExprKind::SpreadRecord { base: Box::new(ir_base), fields: fs }, ty, span)
        }

        // ── Operators ──
        ast::Expr::Binary { op, left, right, .. } => {
            let l = lower_expr(ctx, left);
            let r = lower_expr(ctx, right);
            let left_ty = &l.ty;
            let bin_op = match (op.as_str(), left_ty) {
                ("+", Ty::Float) => BinOp::AddFloat, ("+", _) => BinOp::AddInt,
                ("-", Ty::Float) => BinOp::SubFloat, ("-", _) => BinOp::SubInt,
                ("*", Ty::Float) => BinOp::MulFloat, ("*", _) => BinOp::MulInt,
                ("/", Ty::Float) => BinOp::DivFloat, ("/", _) => BinOp::DivInt,
                ("%", Ty::Float) => BinOp::ModFloat, ("%", _) => BinOp::ModInt,
                ("**", _) => BinOp::PowFloat,
                ("^", _) => BinOp::XorInt,
                ("++", Ty::String) => BinOp::ConcatStr,
                ("++", _) => BinOp::ConcatList,
                ("==", _) => BinOp::Eq, ("!=", _) => BinOp::Neq,
                ("<", _) => BinOp::Lt, (">", _) => BinOp::Gt,
                ("<=", _) => BinOp::Lte, (">=", _) => BinOp::Gte,
                ("and", _) => BinOp::And, ("or", _) => BinOp::Or,
                _ => BinOp::AddInt,
            };
            ctx.mk(IrExprKind::BinOp { op: bin_op, left: Box::new(l), right: Box::new(r) }, ty, span)
        }
        ast::Expr::Unary { op, operand, .. } => {
            let o = lower_expr(ctx, operand);
            let un_op = match (op.as_str(), &o.ty) {
                ("not", _) => UnOp::Not,
                ("-", Ty::Float) => UnOp::NegFloat,
                _ => UnOp::NegInt,
            };
            ctx.mk(IrExprKind::UnOp { op: un_op, operand: Box::new(o) }, ty, span)
        }

        // ── Control flow ──
        ast::Expr::If { cond, then, else_, .. } => {
            let c = lower_expr(ctx, cond);
            let t = lower_expr(ctx, then);
            let e = lower_expr(ctx, else_);
            ctx.mk(IrExprKind::If { cond: Box::new(c), then: Box::new(t), else_: Box::new(e) }, ty, span)
        }
        ast::Expr::Match { subject, arms, .. } => {
            let s = lower_expr(ctx, subject);
            let ir_arms = arms.iter().map(|arm| {
                ctx.push_scope();
                let pat = lower_pattern(ctx, &arm.pattern, &s.ty);
                let guard = arm.guard.as_ref().map(|g| lower_expr(ctx, g));
                let body = lower_expr(ctx, &arm.body);
                ctx.pop_scope();
                IrMatchArm { pattern: pat, guard, body }
            }).collect();
            ctx.mk(IrExprKind::Match { subject: Box::new(s), arms: ir_arms }, ty, span)
        }
        ast::Expr::Block { stmts, expr, .. } => {
            ctx.push_scope();
            let ir_stmts: Vec<IrStmt> = stmts.iter().map(|s| lower_stmt(ctx, s)).collect();
            let ir_expr = expr.as_ref().map(|e| Box::new(lower_expr(ctx, e)));
            ctx.pop_scope();
            ctx.mk(IrExprKind::Block { stmts: ir_stmts, expr: ir_expr }, ty, span)
        }
        ast::Expr::DoBlock { stmts, expr, .. } => {
            ctx.push_scope();
            let ir_stmts: Vec<IrStmt> = stmts.iter().map(|s| lower_stmt(ctx, s)).collect();
            let ir_expr = expr.as_ref().map(|e| Box::new(lower_expr(ctx, e)));
            ctx.pop_scope();
            ctx.mk(IrExprKind::DoBlock { stmts: ir_stmts, expr: ir_expr }, ty, span)
        }

        // ── Loops ──
        ast::Expr::ForIn { var, var_tuple, iterable, body, .. } => {
            let ir_iter = lower_expr(ctx, iterable);
            ctx.push_scope();
            let elem_ty = match &ir_iter.ty {
                Ty::List(inner) => *inner.clone(),
                Ty::Map(k, v) => Ty::Tuple(vec![*k.clone(), *v.clone()]),
                _ => Ty::Unknown,
            };
            let var_id = ctx.define_var(var, elem_ty, Mutability::Let, span.clone());
            let tuple_vars = var_tuple.as_ref().map(|names| {
                names.iter().map(|n| ctx.define_var(n, Ty::Unknown, Mutability::Let, None)).collect()
            });
            let ir_body: Vec<IrStmt> = body.iter().map(|s| lower_stmt(ctx, s)).collect();
            ctx.pop_scope();
            ctx.mk(IrExprKind::ForIn { var: var_id, var_tuple: tuple_vars, iterable: Box::new(ir_iter), body: ir_body }, ty, span)
        }
        ast::Expr::While { cond, body, .. } => {
            let ir_cond = lower_expr(ctx, cond);
            ctx.push_scope();
            let ir_body: Vec<IrStmt> = body.iter().map(|s| lower_stmt(ctx, s)).collect();
            ctx.pop_scope();
            ctx.mk(IrExprKind::While { cond: Box::new(ir_cond), body: ir_body }, ty, span)
        }
        ast::Expr::Break { .. } => ctx.mk(IrExprKind::Break, Ty::Unit, span),
        ast::Expr::Continue { .. } => ctx.mk(IrExprKind::Continue, Ty::Unit, span),
        ast::Expr::Range { start, end, inclusive, .. } => {
            let s = lower_expr(ctx, start);
            let e = lower_expr(ctx, end);
            ctx.mk(IrExprKind::Range { start: Box::new(s), end: Box::new(e), inclusive: *inclusive }, ty, span)
        }

        // ── Calls ──
        ast::Expr::Call { callee, args, type_args, .. } => {
            lower_call(ctx, callee, args, type_args.as_ref(), ty, span)
        }

        // ── Pipe: desugar `a |> f(b)` → `f(a, b)` ──
        ast::Expr::Pipe { left, right, .. } => {
            let ir_left = lower_expr(ctx, left);
            match right.as_ref() {
                ast::Expr::Call { callee, args, type_args, .. } => {
                    // Pipe inserts left as first argument
                    let mut all_args = vec![ir_left];
                    all_args.extend(args.iter().map(|a| lower_expr(ctx, a)));
                    let target = lower_call_target(ctx, callee);
                    let ta = type_args.as_ref().map(|tas| tas.iter().map(|t| resolve_type_expr(t)).collect()).unwrap_or_default();
                    ctx.mk(IrExprKind::Call { target, args: all_args, type_args: ta }, ty, span)
                }
                _ => {
                    let ir_right = lower_expr(ctx, right);
                    ctx.mk(IrExprKind::Call {
                        target: CallTarget::Computed { callee: Box::new(ir_right) },
                        args: vec![ir_left], type_args: vec![],
                    }, ty, span)
                }
            }
        }

        // ── Lambda ──
        ast::Expr::Lambda { params, body, .. } => {
            ctx.push_scope();
            // Get lambda type from checker to resolve inferred param types
            let lambda_param_tys: Vec<Ty> = match &ty {
                Ty::Fn { params: ptys, .. } => ptys.clone(),
                _ => vec![],
            };
            let ir_params: Vec<(VarId, Ty)> = params.iter().enumerate().map(|(i, p)| {
                let param_ty = p.ty.as_ref().map(|te| resolve_type_expr(te))
                    .or_else(|| lambda_param_tys.get(i).cloned())
                    .unwrap_or(Ty::Unknown);
                let var = ctx.define_var(&p.name, param_ty.clone(), Mutability::Let, None);
                (var, param_ty)
            }).collect();
            let ir_body = lower_expr(ctx, body);
            ctx.pop_scope();
            ctx.mk(IrExprKind::Lambda { params: ir_params, body: Box::new(ir_body) }, ty, span)
        }

        // ── Access ──
        ast::Expr::Member { object, field, .. } => {
            let obj = lower_expr(ctx, object);
            ctx.mk(IrExprKind::Member { object: Box::new(obj), field: field.clone() }, ty, span)
        }
        ast::Expr::TupleIndex { object, index, .. } => {
            let obj = lower_expr(ctx, object);
            ctx.mk(IrExprKind::TupleIndex { object: Box::new(obj), index: *index }, ty, span)
        }
        ast::Expr::IndexAccess { object, index, .. } => {
            let obj = lower_expr(ctx, object);
            let idx = lower_expr(ctx, index);
            ctx.mk(IrExprKind::IndexAccess { object: Box::new(obj), index: Box::new(idx) }, ty, span)
        }

        // ── String interpolation ──
        ast::Expr::InterpolatedString { value, .. } => {
            let parts = lower_interpolation(ctx, value);
            ctx.mk(IrExprKind::StringInterp { parts }, Ty::String, span)
        }

        // ── Result / Option ──
        ast::Expr::Some { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            ctx.mk(IrExprKind::OptionSome { expr: Box::new(inner) }, ty, span)
        }
        ast::Expr::Ok { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            ctx.mk(IrExprKind::ResultOk { expr: Box::new(inner) }, ty, span)
        }
        ast::Expr::Err { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            ctx.mk(IrExprKind::ResultErr { expr: Box::new(inner) }, ty, span)
        }
        ast::Expr::None { .. } => ctx.mk(IrExprKind::OptionNone, ty, span),
        ast::Expr::Try { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            ctx.mk(IrExprKind::Try { expr: Box::new(inner) }, ty, span)
        }
        ast::Expr::Await { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            ctx.mk(IrExprKind::Await { expr: Box::new(inner) }, ty, span)
        }

        // ── Misc ──
        ast::Expr::Paren { expr, .. } => lower_expr(ctx, expr),
        ast::Expr::Hole { .. } => ctx.mk(IrExprKind::Hole, ty, span),
        ast::Expr::Todo { message, .. } => ctx.mk(IrExprKind::Todo { message: message.clone() }, ty, span),
        ast::Expr::Error { .. } => ctx.mk(IrExprKind::Unit, Ty::Unknown, span),
        ast::Expr::Placeholder { .. } => ctx.mk(IrExprKind::Unit, Ty::Unknown, span),
    }
}

// ── Call lowering ───────────────────────────────────────────────

fn lower_call(ctx: &mut LowerCtx, callee: &ast::Expr, args: &[ast::Expr], type_args: Option<&Vec<ast::TypeExpr>>, ty: Ty, span: Option<ast::Span>) -> IrExpr {
    let ir_args: Vec<IrExpr> = args.iter().map(|a| lower_expr(ctx, a)).collect();
    let ta = type_args.map(|tas| tas.iter().map(|t| resolve_type_expr(t)).collect()).unwrap_or_default();
    let target = lower_call_target(ctx, callee);
    ctx.mk(IrExprKind::Call { target, args: ir_args, type_args: ta }, ty, span)
}

fn lower_call_target(ctx: &mut LowerCtx, callee: &ast::Expr) -> CallTarget {
    match callee {
        ast::Expr::Ident { name, .. } | ast::Expr::TypeName { name, .. } => {
            CallTarget::Named { name: name.clone() }
        }
        ast::Expr::Member { object, field, .. } => {
            // Check if this is a module call (e.g., string.trim, list.map)
            if let ast::Expr::Ident { name: module, .. } = object.as_ref() {
                if crate::stdlib::is_stdlib_module(module) || crate::stdlib::is_any_stdlib(module)
                    || ctx.env.user_modules.contains(module)
                {
                    let resolved = ctx.env.module_aliases.get(module).cloned().unwrap_or(module.clone());
                    return CallTarget::Module { module: resolved, func: field.clone() };
                }
            }
            // Method call: obj.method(args) → UFCS
            let ir_obj = lower_expr(ctx, object);
            CallTarget::Method { object: Box::new(ir_obj), method: field.clone() }
        }
        _ => {
            let ir_callee = lower_expr(ctx, callee);
            CallTarget::Computed { callee: Box::new(ir_callee) }
        }
    }
}

// ── Statement lowering ──────────────────────────────────────────

fn lower_stmt(ctx: &mut LowerCtx, stmt: &ast::Stmt) -> IrStmt {
    let span = match stmt {
        ast::Stmt::Let { span, .. } | ast::Stmt::Var { span, .. }
        | ast::Stmt::Assign { span, .. } | ast::Stmt::Guard { span, .. }
        | ast::Stmt::Expr { span, .. } | ast::Stmt::IndexAssign { span, .. }
        | ast::Stmt::FieldAssign { span, .. } | ast::Stmt::LetDestructure { span, .. }
        | ast::Stmt::Error { span, .. } => *span,
        ast::Stmt::Comment { .. } => None,
    };

    let kind = match stmt {
        ast::Stmt::Let { name, value, .. } => {
            let ir_val = lower_expr(ctx, value);
            let val_ty = ir_val.ty.clone();
            let var = ctx.define_var(name, val_ty.clone(), Mutability::Let, span);
            IrStmtKind::Bind { var, mutability: Mutability::Let, ty: val_ty, value: ir_val }
        }
        ast::Stmt::Var { name, value, .. } => {
            let ir_val = lower_expr(ctx, value);
            let val_ty = ir_val.ty.clone();
            let var = ctx.define_var(name, val_ty.clone(), Mutability::Var, span);
            IrStmtKind::Bind { var, mutability: Mutability::Var, ty: val_ty, value: ir_val }
        }
        ast::Stmt::LetDestructure { pattern, value, .. } => {
            let ir_val = lower_expr(ctx, value);
            let ir_pat = lower_pattern(ctx, pattern, &ir_val.ty);
            IrStmtKind::BindDestructure { pattern: ir_pat, value: ir_val }
        }
        ast::Stmt::Assign { name, value, .. } => {
            let ir_val = lower_expr(ctx, value);
            let var = ctx.lookup_var(name).unwrap_or(VarId(0));
            IrStmtKind::Assign { var, value: ir_val }
        }
        ast::Stmt::IndexAssign { target, index, value, .. } => {
            let var = ctx.lookup_var(target).unwrap_or(VarId(0));
            let ir_idx = lower_expr(ctx, index);
            let ir_val = lower_expr(ctx, value);
            IrStmtKind::IndexAssign { target: var, index: ir_idx, value: ir_val }
        }
        ast::Stmt::FieldAssign { target, field, value, .. } => {
            let var = ctx.lookup_var(target).unwrap_or(VarId(0));
            let ir_val = lower_expr(ctx, value);
            IrStmtKind::FieldAssign { target: var, field: field.clone(), value: ir_val }
        }
        ast::Stmt::Guard { cond, else_, .. } => {
            let ir_cond = lower_expr(ctx, cond);
            let ir_else = lower_expr(ctx, else_);
            IrStmtKind::Guard { cond: ir_cond, else_: ir_else }
        }
        ast::Stmt::Expr { expr, .. } => {
            let ir_expr = lower_expr(ctx, expr);
            IrStmtKind::Expr { expr: ir_expr }
        }
        ast::Stmt::Comment { text } => IrStmtKind::Comment { text: text.clone() },
        ast::Stmt::Error { .. } => IrStmtKind::Comment { text: "/* error */".to_string() },
    };

    IrStmt { kind, span }
}

// ── Pattern lowering ────────────────────────────────────────────

fn lower_pattern(ctx: &mut LowerCtx, pat: &ast::Pattern, ty: &Ty) -> IrPattern {
    match pat {
        ast::Pattern::Wildcard => IrPattern::Wildcard,
        ast::Pattern::Ident { name } => {
            let var = ctx.define_var(name, ty.clone(), Mutability::Let, None);
            IrPattern::Bind { var }
        }
        ast::Pattern::Literal { value } => {
            let ir_expr = lower_expr(ctx, value);
            IrPattern::Literal { expr: ir_expr }
        }
        ast::Pattern::Constructor { name, args } => {
            let payload_tys = get_constructor_payload_tys(ctx, name);
            let ir_args = args.iter().enumerate().map(|(i, a)| {
                let arg_ty = payload_tys.get(i).cloned().unwrap_or(Ty::Unknown);
                lower_pattern(ctx, a, &arg_ty)
            }).collect();
            IrPattern::Constructor { name: name.clone(), args: ir_args }
        }
        ast::Pattern::RecordPattern { name, fields, rest } => {
            let ir_fields = fields.iter().map(|f| {
                let field_ty = resolve_record_field_ty(ctx, name, &f.name);
                IrFieldPattern {
                    name: f.name.clone(),
                    pattern: f.pattern.as_ref().map(|p| lower_pattern(ctx, p, &field_ty)),
                }
            }).collect();
            // Bind unmatched fields as variables
            for f in fields {
                if f.pattern.is_none() {
                    let field_ty = resolve_record_field_ty(ctx, name, &f.name);
                    ctx.define_var(&f.name, field_ty, Mutability::Let, None);
                }
            }
            IrPattern::RecordPattern { name: name.clone(), fields: ir_fields, rest: *rest }
        }
        ast::Pattern::Tuple { elements } => {
            let elem_tys = match ty {
                Ty::Tuple(tys) => tys.clone(),
                _ => vec![Ty::Unknown; elements.len()],
            };
            let ir_elems = elements.iter().enumerate().map(|(i, e)| {
                lower_pattern(ctx, e, elem_tys.get(i).unwrap_or(&Ty::Unknown))
            }).collect();
            IrPattern::Tuple { elements: ir_elems }
        }
        ast::Pattern::Some { inner } => {
            let inner_ty = match ty { Ty::Option(t) => *t.clone(), _ => Ty::Unknown };
            IrPattern::Some { inner: Box::new(lower_pattern(ctx, inner, &inner_ty)) }
        }
        ast::Pattern::None => IrPattern::None,
        ast::Pattern::Ok { inner } => {
            let inner_ty = match ty { Ty::Result(t, _) => *t.clone(), _ => Ty::Unknown };
            IrPattern::Ok { inner: Box::new(lower_pattern(ctx, inner, &inner_ty)) }
        }
        ast::Pattern::Err { inner } => {
            let inner_ty = match ty { Ty::Result(_, e) => *e.clone(), _ => Ty::Unknown };
            IrPattern::Err { inner: Box::new(lower_pattern(ctx, inner, &inner_ty)) }
        }
    }
}

fn get_constructor_payload_tys(ctx: &LowerCtx, ctor_name: &str) -> Vec<Ty> {
    if let Some((_, case)) = ctx.env.constructors.get(ctor_name) {
        match &case.payload {
            crate::types::VariantPayload::Tuple(tys) => tys.clone(),
            crate::types::VariantPayload::Record(fs) => fs.iter().map(|(_, t, _)| t.clone()).collect(),
            crate::types::VariantPayload::Unit => vec![],
        }
    } else {
        vec![]
    }
}

fn resolve_record_field_ty(ctx: &LowerCtx, record_name: &str, field_name: &str) -> Ty {
    if let Some(type_def) = ctx.env.types.get(record_name) {
        ctx.resolve_field_ty(type_def, field_name)
    } else if let Some((_, case)) = ctx.env.constructors.get(record_name) {
        if let crate::types::VariantPayload::Record(fs) = &case.payload {
            fs.iter().find(|(n, _, _)| n == field_name).map(|(_, t, _)| t.clone()).unwrap_or(Ty::Unknown)
        } else { Ty::Unknown }
    } else { Ty::Unknown }
}

// ── Type declarations ───────────────────────────────────────────

fn lower_type_decl(ctx: &mut LowerCtx, name: &str, ty: &ast::TypeExpr, deriving: &Option<Vec<String>>, visibility: &ast::Visibility, generics: Option<&Vec<ast::GenericParam>>) -> IrTypeDecl {
    let kind = match ty {
        ast::TypeExpr::Record { fields } => {
            let fs = fields.iter().map(|f| {
                let default = f.default.as_ref().map(|d| lower_expr(ctx, d));
                IrFieldDecl { name: f.name.clone(), ty: resolve_type_expr(&f.ty), default }
            }).collect();
            IrTypeDeclKind::Record { fields: fs }
        }
        ast::TypeExpr::Variant { cases } => {
            let is_generic = matches!(generics, Some(gs) if !gs.is_empty());
            let cs = cases.iter().map(|c| lower_variant_case(ctx, c, name)).collect();
            IrTypeDeclKind::Variant {
                cases: cs, is_generic,
                boxed_args: std::collections::HashSet::new(),
                boxed_record_fields: std::collections::HashSet::new(),
            }
        }
        _ => IrTypeDeclKind::Alias { target: resolve_type_expr(ty) },
    };
    let vis = match visibility {
        ast::Visibility::Public => IrVisibility::Public,
        ast::Visibility::Mod => IrVisibility::Mod,
        ast::Visibility::Local => IrVisibility::Private,
    };
    IrTypeDecl { name: name.to_string(), kind, deriving: deriving.clone(), generics: generics.cloned(), visibility: vis }
}

fn lower_variant_case(ctx: &mut LowerCtx, case: &ast::VariantCase, _parent: &str) -> IrVariantDecl {
    match case {
        ast::VariantCase::Unit { name } => IrVariantDecl { name: name.clone(), kind: IrVariantKind::Unit },
        ast::VariantCase::Tuple { name, fields } => {
            let tys = fields.iter().map(|f| resolve_type_expr(f)).collect();
            IrVariantDecl { name: name.clone(), kind: IrVariantKind::Tuple { fields: tys } }
        }
        ast::VariantCase::Record { name, fields } => {
            let fs = fields.iter().map(|f| {
                let default = f.default.as_ref().map(|d| lower_expr(ctx, d));
                IrFieldDecl { name: f.name.clone(), ty: resolve_type_expr(&f.ty), default }
            }).collect();
            IrVariantDecl { name: name.clone(), kind: IrVariantKind::Record { fields: fs } }
        }
    }
}

// ── String interpolation ────────────────────────────────────────

fn lower_interpolation(ctx: &mut LowerCtx, template: &str) -> Vec<IrStringPart> {
    let mut parts = Vec::new();
    let mut lit = String::new();
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
            if !lit.is_empty() {
                parts.push(IrStringPart::Lit { value: std::mem::take(&mut lit) });
            }
            i += 2; // skip ${
            let mut depth = 1;
            let mut expr_str = String::new();
            while i < chars.len() && depth > 0 {
                if chars[i] == '{' { depth += 1; }
                if chars[i] == '}' { depth -= 1; if depth == 0 { break; } }
                expr_str.push(chars[i]);
                i += 1;
            }
            i += 1; // skip }
            // Re-parse and lower the expression
            let tokens = crate::lexer::Lexer::tokenize(&expr_str);
            let mut parser = crate::parser::Parser::new_with_id_offset(tokens, u32::MAX / 2);
            if let Ok(parsed) = parser.parse_single_expr() {
                let mut ir_expr = lower_expr(ctx, &parsed);
                // Fix type for simple vars
                if let IrExprKind::Var { id } = &ir_expr.kind {
                    ir_expr.ty = ctx.var_table.get(*id).ty.clone();
                }
                parts.push(IrStringPart::Expr { expr: ir_expr });
            } else {
                parts.push(IrStringPart::Lit { value: format!("${{{}}}", expr_str) });
            }
        } else {
            lit.push(chars[i]);
            i += 1;
        }
    }
    if !lit.is_empty() {
        parts.push(IrStringPart::Lit { value: lit });
    }
    parts
}

// ── Type expression resolution (standalone, no checker needed) ──

fn resolve_type_expr(te: &ast::TypeExpr) -> Ty {
    match te {
        ast::TypeExpr::Simple { name } => match name.as_str() {
            "Int" => Ty::Int, "Float" => Ty::Float, "String" => Ty::String,
            "Bool" => Ty::Bool, "Unit" => Ty::Unit, "Path" => Ty::String,
            other => Ty::Named(other.to_string(), vec![]),
        },
        ast::TypeExpr::Generic { name, args } => {
            let ra: Vec<Ty> = args.iter().map(resolve_type_expr).collect();
            match name.as_str() {
                "List" => Ty::List(Box::new(ra.first().cloned().unwrap_or_else(|| {
                    eprintln!("[ICE] lower: List[] without type argument");
                    Ty::Unknown
                }))),
                "Option" => Ty::Option(Box::new(ra.first().cloned().unwrap_or_else(|| {
                    eprintln!("[ICE] lower: Option[] without type argument");
                    Ty::Unknown
                }))),
                "Result" if ra.len() >= 2 => Ty::Result(Box::new(ra[0].clone()), Box::new(ra[1].clone())),
                "Map" if ra.len() >= 2 => Ty::Map(Box::new(ra[0].clone()), Box::new(ra[1].clone())),
                _ => Ty::Named(name.clone(), ra),
            }
        },
        ast::TypeExpr::Record { fields } => Ty::Record {
            fields: fields.iter().map(|f| (f.name.clone(), resolve_type_expr(&f.ty))).collect(),
        },
        ast::TypeExpr::OpenRecord { fields } => Ty::OpenRecord {
            fields: fields.iter().map(|f| (f.name.clone(), resolve_type_expr(&f.ty))).collect(),
        },
        ast::TypeExpr::Fn { params, ret } => Ty::Fn {
            params: params.iter().map(resolve_type_expr).collect(),
            ret: Box::new(resolve_type_expr(ret)),
        },
        ast::TypeExpr::Tuple { elements } => Ty::Tuple(elements.iter().map(resolve_type_expr).collect()),
        ast::TypeExpr::Variant { cases } => {
            let cs = cases.iter().map(|c| match c {
                ast::VariantCase::Unit { name } => crate::types::VariantCase { name: name.clone(), payload: crate::types::VariantPayload::Unit },
                ast::VariantCase::Tuple { name, fields } => crate::types::VariantCase {
                    name: name.clone(),
                    payload: crate::types::VariantPayload::Tuple(fields.iter().map(resolve_type_expr).collect()),
                },
                ast::VariantCase::Record { name, fields } => crate::types::VariantCase {
                    name: name.clone(),
                    payload: crate::types::VariantPayload::Record(fields.iter().map(|f| (f.name.clone(), resolve_type_expr(&f.ty), f.default.clone())).collect()),
                },
            }).collect();
            Ty::Variant { name: String::new(), cases: cs }
        },
        ast::TypeExpr::Newtype { inner } => resolve_type_expr(inner),
        ast::TypeExpr::Union { members } => Ty::union(members.iter().map(resolve_type_expr).collect()),
    }
}
