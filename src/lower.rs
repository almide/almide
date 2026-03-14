/// AST → Typed IR lowering pass.
///
/// Desugars:
/// - Pipes `a |> f(b)` → `Call(f, [a, b])`
/// - UFCS `x.method(y)` → `ModuleCall("module", "method", [x, y])`
/// - String interpolation → `StringInterp { parts }`
/// - Operators → type-dispatched `BinOp` / `UnOp`
/// - Pattern variables → `VarId` bindings
///
/// Every IR node carries full `Ty` from the checker's `expr_types` map.

use std::collections::HashMap;
use crate::ast;
use crate::ir::*;
use crate::types::{Ty, TypeEnv};

// ── Context ─────────────────────────────────────────────────────

pub struct LowerCtx<'a> {
    pub var_table: VarTable,
    scopes: Vec<HashMap<String, VarId>>,
    expr_types: &'a HashMap<(usize, usize), Ty>,
    env: &'a TypeEnv,
    /// When true, skip span-based type lookups (used for re-parsed string interpolation
    /// expressions whose bogus spans would match wrong entries in expr_types).
    skip_span_lookup: bool,
}

impl<'a> LowerCtx<'a> {
    pub fn new(expr_types: &'a HashMap<(usize, usize), Ty>, env: &'a TypeEnv) -> Self {
        LowerCtx {
            var_table: VarTable::new(),
            scopes: vec![HashMap::new()],
            expr_types,
            skip_span_lookup: false,
            env,
        }
    }

    fn push_scope(&mut self) { self.scopes.push(HashMap::new()); }
    fn pop_scope(&mut self) { self.scopes.pop(); }

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

    fn expr_ty(&self, expr: &ast::Expr) -> Ty {
        // First try the checker's span-based type lookup (skip for re-parsed interpolation exprs)
        if !self.skip_span_lookup {
            let ty = expr.span()
                .and_then(|s| self.expr_types.get(&(s.line, s.col)).cloned())
                .unwrap_or(Ty::Unknown);
            if !matches!(ty, Ty::Unknown) {
                return ty;
            }
        }
        // Fallback: infer type from the expression structure
        self.infer_expr_ty(expr)
    }

    /// Structural type inference when the checker's span-based lookup fails.
    /// This handles cases where the span doesn't match (e.g., cross-module expressions).
    fn infer_expr_ty(&self, expr: &ast::Expr) -> Ty {
        match expr {
            ast::Expr::Ident { name, .. } => {
                if let Some(var_id) = self.lookup_var(name) {
                    return self.var_table.get(var_id).ty.clone();
                }
                Ty::Unknown
            }
            ast::Expr::Member { object, field, .. } => {
                let obj_ty = self.expr_ty(object);
                self.resolve_field_ty(&obj_ty, field)
            }
            ast::Expr::Call { callee, .. } => {
                if let ast::Expr::Member { object, field, .. } = callee.as_ref() {
                    // Try direct module call (e.g., grammar.keyword_groups())
                    if let Some((mod_path, func)) = flatten_module_call(self, object, field) {
                        let key = format!("{}.{}", mod_path, func);
                        if let Some(sig) = self.env.functions.get(&key) {
                            return sig.ret.clone();
                        }
                        // Try stdlib signature
                        if let Some(sig) = crate::stdlib::lookup_sig(&mod_path, &func) {
                            let mut bindings = std::collections::HashMap::new();
                            // No receiver unification for direct module calls
                            return crate::types::substitute(&sig.ret, &bindings);
                        }
                    }
                    // Try UFCS: infer object type, determine module, look up signature
                    let obj_ty = self.expr_ty(object);
                    let module = match &obj_ty {
                        Ty::String => Some("string"),
                        Ty::List(_) => Some("list"),
                        Ty::Map(_, _) => Some("map"),
                        Ty::Int => Some("int"),
                        Ty::Float => Some("float"),
                        _ => None,
                    };
                    if let Some(module) = module {
                        if let Some(sig) = crate::stdlib::lookup_sig(module, field) {
                            let mut bindings = std::collections::HashMap::new();
                            if !sig.params.is_empty() {
                                crate::types::unify(&sig.params[0].1, &obj_ty, &mut bindings);
                            }
                            return crate::types::substitute(&sig.ret, &bindings);
                        }
                    }
                }
                Ty::Unknown
            }
            ast::Expr::String { .. } | ast::Expr::InterpolatedString { .. } => Ty::String,
            ast::Expr::Int { .. } => Ty::Int,
            ast::Expr::Float { .. } => Ty::Float,
            ast::Expr::Bool { .. } => Ty::Bool,
            ast::Expr::List { .. } => Ty::List(Box::new(Ty::Unknown)),
            _ => Ty::Unknown,
        }
    }

    /// Resolve the type of a field access on a known object type.
    fn resolve_field_ty(&self, obj_ty: &Ty, field: &str) -> Ty {
        match obj_ty {
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                for (fname, fty) in fields {
                    if fname == field { return fty.clone(); }
                }
                Ty::Unknown
            }
            Ty::Named(name, _) => {
                // Look up type definition from the checker's environment.
                // Try direct name first, then module-qualified name (e.g., "grammar.TypeName")
                // since cross-module types are stored with module prefix.
                if let Some(def) = self.env.types.get(name) {
                    return self.resolve_field_ty(def, field);
                }
                // Try module-qualified lookup: scan for "*.TypeName"
                let suffix = format!(".{}", name);
                for (key, def) in &self.env.types {
                    if key.ends_with(&suffix) {
                        return self.resolve_field_ty(def, field);
                    }
                }
                Ty::Unknown
            }
            _ => Ty::Unknown,
        }
    }

    fn mk(&self, kind: IrExprKind, ty: Ty, span: Option<ast::Span>) -> IrExpr {
        IrExpr { kind, ty, span }
    }

    /// Check if a name refers to a module (stdlib or user-defined, including aliases).
    fn is_module(&self, name: &str) -> bool {
        crate::stdlib::is_any_stdlib(name) || self.env.user_modules.contains(name)
            || self.env.module_aliases.contains_key(name)
    }
}

// ── Program lowering ────────────────────────────────────────────

pub fn lower_program(prog: &ast::Program, expr_types: &HashMap<(usize, usize), Ty>, env: &TypeEnv) -> IrProgram {
    let mut ctx = LowerCtx::new(expr_types, env);
    let mut functions = Vec::new();
    let mut top_lets = Vec::new();

    for decl in &prog.decls {
        match decl {
            ast::Decl::Fn { name, params, body: Some(body), effect, r#async, span, .. } => {
                functions.push(lower_fn(&mut ctx, name, params, body,
                    effect.unwrap_or(false), r#async.unwrap_or(false), false, *span));
            }
            ast::Decl::Test { name, body, span, .. } => {
                functions.push(lower_fn(&mut ctx, name, &[], body, false, false, true, *span));
            }
            ast::Decl::TopLet { name, value, span, .. } => {
                let ir_value = lower_expr(&mut ctx, value);
                // Prefer checker type; fall back to IR expression's type
                let ty = {
                    let checked = ctx.expr_ty(value);
                    if matches!(checked, Ty::Unknown) { ir_value.ty.clone() } else { checked }
                };
                let var = ctx.define_var(name, ty.clone(), Mutability::Let, *span);
                top_lets.push(IrTopLet { var, ty, value: ir_value });
            }
            ast::Decl::Impl { methods, .. } => {
                for m in methods {
                    if let ast::Decl::Fn { name, params, body: Some(body), effect, r#async, span, .. } = m {
                        functions.push(lower_fn(&mut ctx, name, params, body,
                            effect.unwrap_or(false), r#async.unwrap_or(false), false, *span));
                    }
                }
            }
            _ => {}
        }
    }

    IrProgram { functions, top_lets, var_table: ctx.var_table }
}

fn lower_fn(
    ctx: &mut LowerCtx, name: &str, params: &[ast::Param], body: &ast::Expr,
    is_effect: bool, is_async: bool, is_test: bool, _span: Option<ast::Span>,
) -> IrFunction {
    ctx.push_scope();
    let ir_params: Vec<(VarId, Ty)> = params.iter().map(|p| {
        let ty = resolve_type_expr(&p.ty);
        let var = ctx.define_var(&p.name, ty.clone(), Mutability::Let, None);
        (var, ty)
    }).collect();
    let ir_body = lower_expr(ctx, body);
    // Use declared return type from env if available, else body type
    let ret_ty = ctx.env.functions.get(name)
        .map(|sig| sig.ret.clone())
        .unwrap_or_else(|| ir_body.ty.clone());
    ctx.pop_scope();
    IrFunction { name: name.to_string(), params: ir_params, ret_ty, body: ir_body, is_effect, is_async, is_test }
}

// ── Expression lowering ─────────────────────────────────────────

fn lower_expr(ctx: &mut LowerCtx, expr: &ast::Expr) -> IrExpr {
    let ty = ctx.expr_ty(expr);
    let span = expr.span();

    match expr {
        // ── Literals — use concrete types even when checker has no span ──
        ast::Expr::Int { raw, .. } => {
            let t = if matches!(ty, Ty::Unknown) { Ty::Int } else { ty };
            ctx.mk(IrExprKind::LitInt { value: raw.parse().unwrap_or(0) }, t, span)
        }
        ast::Expr::Float { value: v, .. } => {
            let t = if matches!(ty, Ty::Unknown) { Ty::Float } else { ty };
            ctx.mk(IrExprKind::LitFloat { value: *v }, t, span)
        }
        ast::Expr::String { value: v, .. } => {
            let t = if matches!(ty, Ty::Unknown) { Ty::String } else { ty };
            ctx.mk(IrExprKind::LitStr { value: v.clone() }, t, span)
        }
        ast::Expr::Bool { value: v, .. } => {
            let t = if matches!(ty, Ty::Unknown) { Ty::Bool } else { ty };
            ctx.mk(IrExprKind::LitBool { value: *v }, t, span)
        }
        ast::Expr::Unit { .. } => ctx.mk(IrExprKind::Unit, ty, span),

        // ── String interpolation → parsed parts ──
        ast::Expr::InterpolatedString { value, .. } => {
            let parts = lower_string_interp(ctx, value);
            ctx.mk(IrExprKind::StringInterp { parts }, ty, span)
        }

        // ── Variables ──
        ast::Expr::Ident { name, .. } => {
            if let Some(id) = ctx.lookup_var(name) {
                ctx.mk(IrExprKind::Var { id }, ty, span)
            } else {
                // Could be a free function reference or constructor
                ctx.mk(IrExprKind::Call {
                    target: CallTarget::Named { name: name.clone() },
                    args: vec![],
                    type_args: vec![],
                }, ty, span)
            }
        }
        ast::Expr::TypeName { name, .. } => {
            if let Some(id) = ctx.lookup_var(name) {
                ctx.mk(IrExprKind::Var { id }, ty, span)
            } else {
                // Constructor or type reference
                ctx.mk(IrExprKind::Call {
                    target: CallTarget::Named { name: name.clone() },
                    args: vec![],
                    type_args: vec![],
                }, ty, span)
            }
        }

        // ── Paren (strip) ──
        ast::Expr::Paren { expr: inner, .. } => lower_expr(ctx, inner),

        // ── Operators (type-dispatched) ──
        ast::Expr::Binary { op, left, right, .. } => {
            let left_ir = lower_expr(ctx, left);
            let right_ir = lower_expr(ctx, right);
            match resolve_bin_op(op, &left_ir.ty) {
                Some(bin_op) => {
                    let result_ty = if matches!(ty, Ty::Unknown) { bin_op_result_ty(bin_op) } else { ty };
                    ctx.mk(IrExprKind::BinOp {
                        op: bin_op, left: Box::new(left_ir), right: Box::new(right_ir),
                    }, result_ty, span)
                }
                None => ctx.mk(IrExprKind::Unit, ty, span), // unreachable after checker
            }
        }
        ast::Expr::Unary { op, operand, .. } => {
            let operand_ir = lower_expr(ctx, operand);
            match resolve_un_op(op, &operand_ir.ty) {
                Some(un_op) => {
                    let result_ty = if matches!(ty, Ty::Unknown) { operand_ir.ty.clone() } else { ty };
                    ctx.mk(IrExprKind::UnOp {
                        op: un_op, operand: Box::new(operand_ir),
                    }, result_ty, span)
                }
                None => ctx.mk(IrExprKind::Unit, ty, span),
            }
        }

        // ── Control flow ──
        ast::Expr::If { cond, then, else_, .. } => {
            let c = lower_expr(ctx, cond);
            let t = lower_expr(ctx, then);
            let e = lower_expr(ctx, else_);
            IrExpr { kind: IrExprKind::If {
                cond: Box::new(c), then: Box::new(t), else_: Box::new(e),
            }, ty, span }
        }

        ast::Expr::Match { subject, arms, .. } => {
            let subject_ir = lower_expr(ctx, subject);
            let arms_ir: Vec<IrMatchArm> = arms.iter().map(|arm| {
                ctx.push_scope();
                let pattern = lower_pattern(ctx, &arm.pattern);
                let guard = arm.guard.as_ref().map(|g| lower_expr(ctx, g));
                let body = lower_expr(ctx, &arm.body);
                ctx.pop_scope();
                IrMatchArm { pattern, guard, body }
            }).collect();
            ctx.mk(IrExprKind::Match { subject: Box::new(subject_ir), arms: arms_ir }, ty, span)
        }

        ast::Expr::Block { stmts, expr: tail, .. } => {
            ctx.push_scope();
            let ir_stmts: Vec<IrStmt> = stmts.iter().map(|s| lower_stmt(ctx, s)).collect();
            let ir_tail = tail.as_ref().map(|e| Box::new(lower_expr(ctx, e)));
            ctx.pop_scope();
            ctx.mk(IrExprKind::Block { stmts: ir_stmts, expr: ir_tail }, ty, span)
        }

        ast::Expr::DoBlock { stmts, expr: tail, .. } => {
            ctx.push_scope();
            let ir_stmts: Vec<IrStmt> = stmts.iter().map(|s| lower_stmt(ctx, s)).collect();
            let ir_tail = tail.as_ref().map(|e| Box::new(lower_expr(ctx, e)));
            ctx.pop_scope();
            ctx.mk(IrExprKind::DoBlock { stmts: ir_stmts, expr: ir_tail }, ty, span)
        }

        // ── Loops ──
        ast::Expr::ForIn { var, var_tuple, iterable, body, .. } => {
            let iterable_ir = lower_expr(ctx, iterable);
            ctx.push_scope();
            let elem_ty = match &iterable_ir.ty {
                Ty::List(inner) => *inner.clone(),
                Ty::Map(k, v) => {
                    if var_tuple.is_some() {
                        Ty::Tuple(vec![*k.clone(), *v.clone()])
                    } else {
                        *k.clone()
                    }
                }
                _ => Ty::Unknown,
            };
            let var_tuple_ids = if let Some(names) = var_tuple {
                // Extract element types from the tuple type for each destructured variable
                let tuple_inner = match &elem_ty {
                    Ty::Tuple(tys) => tys.clone(),
                    _ => vec![Ty::Unknown; names.len()],
                };
                let ids: Vec<VarId> = names.iter().enumerate().map(|(i, n)|
                    ctx.define_var(n, tuple_inner.get(i).cloned().unwrap_or(Ty::Unknown), Mutability::Let, None)
                ).collect();
                Some(ids)
            } else {
                None
            };
            // For tuple destructuring, use a synthetic name to avoid shadowing
            // the first destructured variable (var == first tuple element name)
            let var_name = if var_tuple.is_some() {
                format!("__for_tuple_{}", var)
            } else {
                var.clone()
            };
            let var_id = ctx.define_var(&var_name, elem_ty, Mutability::Let, None);
            let body_ir: Vec<IrStmt> = body.iter().map(|s| lower_stmt(ctx, s)).collect();
            ctx.pop_scope();
            ctx.mk(IrExprKind::ForIn {
                var: var_id, var_tuple: var_tuple_ids, iterable: Box::new(iterable_ir), body: body_ir,
            }, ty, span)
        }

        ast::Expr::While { cond, body, .. } => {
            let cond_ir = lower_expr(ctx, cond);
            ctx.push_scope();
            let body_ir: Vec<IrStmt> = body.iter().map(|s| lower_stmt(ctx, s)).collect();
            ctx.pop_scope();
            ctx.mk(IrExprKind::While { cond: Box::new(cond_ir), body: body_ir }, ty, span)
        }

        ast::Expr::Break { .. } => ctx.mk(IrExprKind::Break, ty, span),
        ast::Expr::Continue { .. } => ctx.mk(IrExprKind::Continue, ty, span),

        // ── Pipe (desugar to Call) ──
        ast::Expr::Pipe { left, right, .. } => {
            let left_ir = lower_expr(ctx, left);
            lower_pipe(ctx, left_ir, right, ty, span)
        }

        // ── Calls (with UFCS / module resolution) ──
        ast::Expr::Call { callee, args, type_args, .. } => lower_call(ctx, callee, args, type_args.as_ref(), ty, span),

        // ── Member access (non-call) ──
        ast::Expr::Member { object, field, .. } => {
            let obj_ir = lower_expr(ctx, object);
            ctx.mk(IrExprKind::Member { object: Box::new(obj_ir), field: field.clone() }, ty, span)
        }

        // ── Collections ──
        ast::Expr::List { elements, .. } => {
            let elems: Vec<IrExpr> = elements.iter().map(|e| lower_expr(ctx, e)).collect();
            ctx.mk(IrExprKind::List { elements: elems }, ty, span)
        }

        ast::Expr::EmptyMap { .. } => {
            ctx.mk(IrExprKind::EmptyMap, ty, span)
        }

        ast::Expr::MapLiteral { entries, .. } => {
            let ir_entries: Vec<(IrExpr, IrExpr)> = entries.iter()
                .map(|(k, v)| (lower_expr(ctx, k), lower_expr(ctx, v)))
                .collect();
            ctx.mk(IrExprKind::MapLiteral { entries: ir_entries }, ty, span)
        }

        ast::Expr::Record { name, fields, .. } => {
            let fs: Vec<(String, IrExpr)> = fields.iter()
                .map(|f| (f.name.clone(), lower_expr(ctx, &f.value)))
                .collect();
            ctx.mk(IrExprKind::Record { name: name.clone(), fields: fs }, ty, span)
        }

        ast::Expr::SpreadRecord { base, fields, .. } => {
            let base_ir = lower_expr(ctx, base);
            let fs: Vec<(String, IrExpr)> = fields.iter()
                .map(|f| (f.name.clone(), lower_expr(ctx, &f.value)))
                .collect();
            ctx.mk(IrExprKind::SpreadRecord { base: Box::new(base_ir), fields: fs }, ty, span)
        }

        ast::Expr::Tuple { elements, .. } => {
            let elems: Vec<IrExpr> = elements.iter().map(|e| lower_expr(ctx, e)).collect();
            ctx.mk(IrExprKind::Tuple { elements: elems }, ty, span)
        }

        ast::Expr::Range { start, end, inclusive, .. } => {
            let s = lower_expr(ctx, start);
            let e = lower_expr(ctx, end);
            IrExpr { kind: IrExprKind::Range {
                start: Box::new(s), end: Box::new(e), inclusive: *inclusive,
            }, ty, span }
        }

        // ── Access ──
        ast::Expr::TupleIndex { object, index, .. } => {
            let obj_ir = lower_expr(ctx, object);
            ctx.mk(IrExprKind::TupleIndex { object: Box::new(obj_ir), index: *index }, ty, span)
        }

        ast::Expr::IndexAccess { object, index, .. } => {
            let o = lower_expr(ctx, object);
            let i = lower_expr(ctx, index);
            IrExpr { kind: IrExprKind::IndexAccess {
                object: Box::new(o), index: Box::new(i),
            }, ty, span }
        }

        // ── Lambda ──
        ast::Expr::Lambda { params, body, .. } => {
            ctx.push_scope();
            // Extract inferred param types from checker's lambda type (bidirectional inference)
            let inferred_param_tys = match &ty {
                Ty::Fn { params: fn_params, .. } => Some(fn_params.as_slice()),
                _ => None,
            };
            let ir_params: Vec<(VarId, Ty)> = params.iter().enumerate().map(|(i, p)| {
                let pty = if let Some(te) = &p.ty {
                    resolve_type_expr(te)
                } else if let Some(fn_params) = inferred_param_tys {
                    fn_params.get(i).cloned().unwrap_or(Ty::Unknown)
                } else {
                    Ty::Unknown
                };
                let var = ctx.define_var(&p.name, pty.clone(), Mutability::Let, None);
                (var, pty)
            }).collect();
            let body_ir = lower_expr(ctx, body);
            ctx.pop_scope();
            ctx.mk(IrExprKind::Lambda {
                params: ir_params, body: Box::new(body_ir),
            }, ty, span)
        }

        // ── Result / Option ──
        ast::Expr::Ok { expr: inner, .. } => {
            let v = lower_expr(ctx, inner);
            IrExpr { kind: IrExprKind::ResultOk { expr: Box::new(v) }, ty, span }
        }
        ast::Expr::Err { expr: inner, .. } => {
            let v = lower_expr(ctx, inner);
            IrExpr { kind: IrExprKind::ResultErr { expr: Box::new(v) }, ty, span }
        }
        ast::Expr::Some { expr: inner, .. } => {
            let v = lower_expr(ctx, inner);
            IrExpr { kind: IrExprKind::OptionSome { expr: Box::new(v) }, ty, span }
        }
        ast::Expr::None { .. } => IrExpr { kind: IrExprKind::OptionNone, ty, span },

        ast::Expr::Try { expr: inner, .. } => {
            let v = lower_expr(ctx, inner);
            IrExpr { kind: IrExprKind::Try { expr: Box::new(v) }, ty, span }
        }
        ast::Expr::Await { expr: inner, .. } => {
            let v = lower_expr(ctx, inner);
            IrExpr { kind: IrExprKind::Await { expr: Box::new(v) }, ty, span }
        }

        // ── Misc ──
        ast::Expr::Hole { .. } => ctx.mk(IrExprKind::Hole, ty, span),
        ast::Expr::Todo { message, .. } => ctx.mk(IrExprKind::Todo { message: message.clone() }, ty, span),
        ast::Expr::Placeholder { .. } => ctx.mk(IrExprKind::Hole, ty, span),
        ast::Expr::Error { .. } => ctx.mk(IrExprKind::Unit, Ty::Unknown, span),
    }
}

// ── Statement lowering ──────────────────────────────────────────

fn lower_stmt(ctx: &mut LowerCtx, stmt: &ast::Stmt) -> IrStmt {
    match stmt {
        ast::Stmt::Let { name, ty: declared_ty, value, span, .. } => {
            let ir_value = lower_expr(ctx, value);
            // Prefer declared type annotation over inferred type (avoids Unknown for none, empty list, etc.)
            let ty = declared_ty.as_ref()
                .map(|t| resolve_type_expr(t))
                .unwrap_or_else(|| ir_value.ty.clone());
            let var = ctx.define_var(name, ty.clone(), Mutability::Let, *span);
            IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value: ir_value }, span: *span }
        }
        ast::Stmt::Var { name, ty: declared_ty, value, span, .. } => {
            let ir_value = lower_expr(ctx, value);
            let ty = declared_ty.as_ref()
                .map(|t| resolve_type_expr(t))
                .unwrap_or_else(|| ir_value.ty.clone());
            let var = ctx.define_var(name, ty.clone(), Mutability::Var, *span);
            IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Var, ty, value: ir_value }, span: *span }
        }
        ast::Stmt::LetDestructure { pattern, value, span } => {
            let ir_value = lower_expr(ctx, value);
            let ir_pattern = lower_pattern(ctx, pattern);
            IrStmt { kind: IrStmtKind::BindDestructure { pattern: ir_pattern, value: ir_value }, span: *span }
        }
        ast::Stmt::Assign { name, value, span } => {
            let ir_value = lower_expr(ctx, value);
            if let Some(var) = ctx.lookup_var(name) {
                IrStmt { kind: IrStmtKind::Assign { var, value: ir_value }, span: *span }
            } else {
                // Unresolved assign — wrap as expression
                IrStmt { kind: IrStmtKind::Expr { expr: ir_value }, span: *span }
            }
        }
        ast::Stmt::IndexAssign { target, index, value, span } => {
            let index_ir = lower_expr(ctx, index);
            let value_ir = lower_expr(ctx, value);
            if let Some(var) = ctx.lookup_var(target) {
                IrStmt { kind: IrStmtKind::IndexAssign { target: var, index: index_ir, value: value_ir }, span: *span }
            } else {
                IrStmt { kind: IrStmtKind::Expr { expr: value_ir }, span: *span }
            }
        }
        ast::Stmt::FieldAssign { target, field, value, span } => {
            let value_ir = lower_expr(ctx, value);
            if let Some(var) = ctx.lookup_var(target) {
                IrStmt { kind: IrStmtKind::FieldAssign { target: var, field: field.clone(), value: value_ir }, span: *span }
            } else {
                IrStmt { kind: IrStmtKind::Expr { expr: value_ir }, span: *span }
            }
        }
        ast::Stmt::Guard { cond, else_, span } => {
            let cond_ir = lower_expr(ctx, cond);
            let else_ir = lower_expr(ctx, else_);
            IrStmt { kind: IrStmtKind::Guard { cond: cond_ir, else_: else_ir }, span: *span }
        }
        ast::Stmt::Expr { expr, span } => {
            IrStmt { kind: IrStmtKind::Expr { expr: lower_expr(ctx, expr) }, span: *span }
        }
        ast::Stmt::Comment { text } => {
            IrStmt { kind: IrStmtKind::Comment { text: text.clone() }, span: None }
        }
        ast::Stmt::Error { span } => {
            IrStmt { kind: IrStmtKind::Comment { text: "/* error */".to_string() }, span: *span }
        }
    }
}

// ── Pattern lowering ────────────────────────────────────────────

fn lower_pattern(ctx: &mut LowerCtx, pat: &ast::Pattern) -> IrPattern {
    match pat {
        ast::Pattern::Wildcard => IrPattern::Wildcard,
        ast::Pattern::Ident { name } => {
            let var = ctx.define_var(name, Ty::Unknown, Mutability::Let, None);
            IrPattern::Bind { var }
        }
        ast::Pattern::Literal { value } => {
            let expr = lower_expr(ctx, value);
            IrPattern::Literal { expr }
        }
        ast::Pattern::Constructor { name, args } => {
            let ir_args: Vec<IrPattern> = args.iter().map(|p| lower_pattern(ctx, p)).collect();
            IrPattern::Constructor { name: name.clone(), args: ir_args }
        }
        ast::Pattern::RecordPattern { name, fields, rest } => {
            let ir_fields: Vec<IrFieldPattern> = fields.iter().map(|f| {
                IrFieldPattern {
                    name: f.name.clone(),
                    pattern: f.pattern.as_ref().map(|p| lower_pattern(ctx, p)),
                }
            }).collect();
            // Auto-bind fields without explicit patterns
            for f in fields {
                if f.pattern.is_none() {
                    ctx.define_var(&f.name, Ty::Unknown, Mutability::Let, None);
                }
            }
            IrPattern::RecordPattern { name: name.clone(), fields: ir_fields, rest: *rest }
        }
        ast::Pattern::Tuple { elements } => {
            IrPattern::Tuple { elements: elements.iter().map(|p| lower_pattern(ctx, p)).collect() }
        }
        ast::Pattern::Some { inner } => {
            IrPattern::Some { inner: Box::new(lower_pattern(ctx, inner)) }
        }
        ast::Pattern::None => IrPattern::None,
        ast::Pattern::Ok { inner } => {
            IrPattern::Ok { inner: Box::new(lower_pattern(ctx, inner)) }
        }
        ast::Pattern::Err { inner } => {
            IrPattern::Err { inner: Box::new(lower_pattern(ctx, inner)) }
        }
    }
}

// ── Call resolution (UFCS, modules, constructors) ───────────────

/// Flatten a Member chain into a dotted module path + function name.
/// e.g. `Member(Member(Ident("mylib"), "parser"), "parse")` → Some(("mylib.parser", "parse"))
/// Resolves module aliases (e.g., `m` → `mylib` when `import mylib as m`).
/// Returns None if the chain doesn't resolve to a known module.
fn flatten_module_call(ctx: &LowerCtx, object: &ast::Expr, func: &str) -> Option<(String, String)> {
    // Collect path segments from nested Member expressions
    let mut segments = vec![];
    let mut current = object;
    loop {
        match current {
            ast::Expr::Ident { name, .. } => {
                segments.push(name.as_str());
                break;
            }
            ast::Expr::Member { object: inner, field, .. } => {
                segments.push(field.as_str());
                current = inner;
            }
            _ => return None,
        }
    }
    segments.reverse();

    // If the first segment is a local variable, it's not a module call.
    // Local variables shadow module names (e.g., `let args = env.args(); args.len()`)
    if ctx.lookup_var(segments[0]).is_some() {
        return None;
    }

    // Resolve the first segment through module aliases (e.g., "m" → "mylib")
    let resolved_first = ctx.env.module_aliases.get(segments[0])
        .map(|s| s.as_str())
        .unwrap_or(segments[0]);

    // Build resolved segments with alias expansion
    let mut resolved_segments = vec![resolved_first];
    for s in &segments[1..] {
        resolved_segments.push(s);
    }

    // Try progressively longer prefixes: "mylib", "mylib.parser", etc.
    for i in 1..=resolved_segments.len() {
        let mod_path = resolved_segments[..i].join(".");
        if ctx.is_module(&mod_path) {
            if i == resolved_segments.len() {
                return Some((mod_path, func.to_string()));
            }
            // Try extending: "mylib.parser", "mylib.http.client", etc.
            let full_path = resolved_segments[..].join(".");
            if ctx.is_module(&full_path) {
                return Some((full_path, func.to_string()));
            }
            // Try partial: check each extension level
            for j in (i + 1)..=resolved_segments.len() {
                let candidate = resolved_segments[..j].join(".");
                if ctx.is_module(&candidate) && j == resolved_segments.len() {
                    return Some((candidate, func.to_string()));
                }
            }
        }
    }
    None
}

fn lower_call(ctx: &mut LowerCtx, callee: &ast::Expr, args: &[ast::Expr], type_args: Option<&Vec<crate::ast::TypeExpr>>, ty: Ty, span: Option<ast::Span>) -> IrExpr {
    let ir_args: Vec<IrExpr> = args.iter().map(|a| lower_expr(ctx, a)).collect();

    match callee {
        // module.func(args) or receiver.method(args)
        ast::Expr::Member { object, field, .. } => {
            // Case 1: Module call — flatten Member chain and check for module path
            // Handles single (`string.trim(x)`) and multi-segment (`mylib.parser.parse(x)`)
            if let Some((mod_path, func)) = flatten_module_call(ctx, object, field) {
                return ctx.mk(IrExprKind::Call {
                    target: CallTarget::Module { module: mod_path, func },
                    args: ir_args,
                    type_args: vec![],
                }, ty, span);
            }

            // Case 2: UFCS — `x.trim()` where trim is a stdlib method
            let obj_ty = ctx.expr_ty(object);
            let resolved_type = match &obj_ty {
                Ty::String => Some(ast::ResolvedType::String),
                Ty::Int => Some(ast::ResolvedType::Int),
                Ty::Float => Some(ast::ResolvedType::Float),
                Ty::List(_) => Some(ast::ResolvedType::List),
                Ty::Map(_, _) => Some(ast::ResolvedType::Map),
                Ty::Bool => Some(ast::ResolvedType::Bool),
                _ => None,
            };

            // Try type-based resolution first (compile-time, zero-cost)
            let module = resolved_type
                .and_then(|rt| crate::stdlib::resolve_ufcs_by_type(field, rt))
                .or_else(|| {
                    let candidates = crate::stdlib::resolve_ufcs_candidates(field);
                    if candidates.len() == 1 { Some(candidates[0]) } else { None }
                });

            if let Some(module) = module {
                // UFCS resolved: prepend receiver as first arg
                let obj_ir = lower_expr(ctx, object);
                let mut all_args = vec![obj_ir];
                all_args.extend(ir_args);
                return ctx.mk(IrExprKind::Call {
                    target: CallTarget::Module { module: module.to_string(), func: field.clone() },
                    args: all_args,
                    type_args: vec![],
                }, ty, span);
            }

            // Case 3: Unresolved method — emitter decides UFCS vs method call
            let obj_ir = lower_expr(ctx, object);
            ctx.mk(IrExprKind::Call {
                target: CallTarget::Method { object: Box::new(obj_ir), method: field.clone() },
                args: ir_args,
                type_args: vec![],
            }, ty, span)
        }

        // Constructor call — `Red`, `Node(1, left, right)`
        ast::Expr::TypeName { name, .. } => {
            let ir_type_args: Vec<crate::types::Ty> = type_args.map_or(vec![], |tas| {
                tas.iter().map(|te| crate::lower::resolve_type_expr(te)).collect()
            });
            ctx.mk(IrExprKind::Call {
                target: CallTarget::Named { name: name.clone() },
                args: ir_args,
                type_args: ir_type_args,
            }, ty, span)
        }

        // Free function call — `foo(x)`, `println(x)`
        ast::Expr::Ident { name, .. } => {
            let ir_type_args: Vec<crate::types::Ty> = type_args.map_or(vec![], |tas| {
                tas.iter().map(|te| crate::lower::resolve_type_expr(te)).collect()
            });
            ctx.mk(IrExprKind::Call {
                target: CallTarget::Named { name: name.clone() },
                args: ir_args,
                type_args: ir_type_args,
            }, ty, span)
        }

        // Computed callee — `(some_expr)(args)`
        _ => {
            let callee_ir = lower_expr(ctx, callee);
            ctx.mk(IrExprKind::Call {
                target: CallTarget::Computed { callee: Box::new(callee_ir) },
                args: ir_args,
                type_args: vec![],
            }, ty, span)
        }
    }
}

// ── Pipe desugaring ─────────────────────────────────────────────

fn lower_pipe(ctx: &mut LowerCtx, left: IrExpr, right: &ast::Expr, ty: Ty, span: Option<ast::Span>) -> IrExpr {
    match right {
        // `a |> f(b)` → substitute placeholder or prepend left as first arg
        ast::Expr::Call { callee, args, .. } => {
            let has_placeholder = args.iter().any(|a| matches!(a, ast::Expr::Placeholder { .. }));

            if has_placeholder {
                // `a |> f(_, b)` → `f(a, b)` — substitute placeholder with left
                let ir_args: Vec<IrExpr> = args.iter().map(|a| {
                    if matches!(a, ast::Expr::Placeholder { .. }) {
                        left.clone()
                    } else {
                        lower_expr(ctx, a)
                    }
                }).collect();
                lower_call_with_args(ctx, callee, ir_args, ty, span)
            } else {
                // `a |> f(b)` → `f(a, b)` — prepend left
                let mut ir_args = vec![left];
                ir_args.extend(args.iter().map(|a| lower_expr(ctx, a)));
                lower_call_with_args(ctx, callee, ir_args, ty, span)
            }
        }
        // `a |> f` → `f(a)` — call right as function with left as sole arg
        ast::Expr::Ident { name, .. } | ast::Expr::TypeName { name, .. } => {
            if ctx.lookup_var(name).is_some() {
                // Variable holding a function — use Computed call
                let callee_ir = lower_expr(ctx, right);
                ctx.mk(IrExprKind::Call {
                    target: CallTarget::Computed { callee: Box::new(callee_ir) },
                    args: vec![left],
                    type_args: vec![],
                }, ty, span)
            } else {
                // Named function — use Named call directly
                ctx.mk(IrExprKind::Call {
                    target: CallTarget::Named { name: name.clone() },
                    args: vec![left],
                    type_args: vec![],
                }, ty, span)
            }
        }
        _ => {
            let callee_ir = lower_expr(ctx, right);
            ctx.mk(IrExprKind::Call {
                target: CallTarget::Computed { callee: Box::new(callee_ir) },
                args: vec![left],
                type_args: vec![],
            }, ty, span)
        }
    }
}

/// Like lower_call but with pre-computed IrExpr args (for pipe desugaring).
fn lower_call_with_args(ctx: &mut LowerCtx, callee: &ast::Expr, ir_args: Vec<IrExpr>, ty: Ty, span: Option<ast::Span>) -> IrExpr {
    match callee {
        ast::Expr::Member { object, field, .. } => {
            // Check module call (single and multi-segment)
            if let Some((mod_path, func)) = flatten_module_call(ctx, object, field) {
                return ctx.mk(IrExprKind::Call {
                    target: CallTarget::Module { module: mod_path, func },
                    args: ir_args,
                    type_args: vec![],
                }, ty, span);
            }
            // Check UFCS
            let obj_ty = ctx.expr_ty(object);
            let resolved_type = ty_to_resolved(&obj_ty);
            let module = resolved_type
                .and_then(|rt| crate::stdlib::resolve_ufcs_by_type(field, rt))
                .or_else(|| {
                    let c = crate::stdlib::resolve_ufcs_candidates(field);
                    if c.len() == 1 { Some(c[0]) } else { None }
                });

            if let Some(module) = module {
                let obj_ir = lower_expr(ctx, object);
                let mut all_args = vec![obj_ir];
                all_args.extend(ir_args);
                return ctx.mk(IrExprKind::Call {
                    target: CallTarget::Module { module: module.to_string(), func: field.clone() },
                    args: all_args,
                    type_args: vec![],
                }, ty, span);
            }

            let obj_ir = lower_expr(ctx, object);
            ctx.mk(IrExprKind::Call {
                target: CallTarget::Method { object: Box::new(obj_ir), method: field.clone() },
                args: ir_args,
                type_args: vec![],
            }, ty, span)
        }
        ast::Expr::Ident { name, .. } => {
            ctx.mk(IrExprKind::Call {
                target: CallTarget::Named { name: name.clone() },
                args: ir_args,
                type_args: vec![],
            }, ty, span)
        }
        ast::Expr::TypeName { name, .. } => {
            ctx.mk(IrExprKind::Call {
                target: CallTarget::Named { name: name.clone() },
                args: ir_args,
                type_args: vec![],
            }, ty, span)
        }
        _ => {
            let callee_ir = lower_expr(ctx, callee);
            ctx.mk(IrExprKind::Call {
                target: CallTarget::Computed { callee: Box::new(callee_ir) },
                args: ir_args,
                type_args: vec![],
            }, ty, span)
        }
    }
}

// ── String interpolation parsing ────────────────────────────────

fn lower_string_interp(ctx: &mut LowerCtx, raw: &str) -> Vec<IrStringPart> {
    let mut parts = Vec::new();
    let mut chars = raw.chars().peekable();
    let mut lit = String::new();

    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next(); // skip {
            if !lit.is_empty() {
                parts.push(IrStringPart::Lit { value: std::mem::take(&mut lit) });
            }
            let mut expr_str = String::new();
            let mut depth = 1;
            while let Some(ch) = chars.next() {
                if ch == '{' { depth += 1; }
                if ch == '}' { depth -= 1; if depth == 0 { break; } }
                expr_str.push(ch);
            }
            // Re-parse the expression and lower it.
            // Skip span-based type lookups — re-parsed expressions have bogus spans (1,1)
            // that would match wrong entries in the checker's expr_types map.
            let tokens = crate::lexer::Lexer::tokenize(&expr_str);
            let mut parser = crate::parser::Parser::new(tokens);
            if let Ok(parsed) = parser.parse_single_expr() {
                ctx.skip_span_lookup = true;
                let mut ir_expr = lower_expr(ctx, &parsed);
                ctx.skip_span_lookup = false;
                // Fix type: for simple Var nodes, use the authoritative type from VarTable.
                if let IrExprKind::Var { id } = &ir_expr.kind {
                    ir_expr.ty = ctx.var_table.get(*id).ty.clone();
                }
                parts.push(IrStringPart::Expr { expr: ir_expr });
            } else {
                // Parse failed — keep as literal
                parts.push(IrStringPart::Lit { value: format!("${{{}}}", expr_str) });
            }
        } else {
            lit.push(c);
        }
    }
    if !lit.is_empty() {
        parts.push(IrStringPart::Lit { value: lit });
    }
    parts
}

// ── Helpers ─────────────────────────────────────────────────────

fn resolve_bin_op(op: &str, left_ty: &Ty) -> Option<BinOp> {
    Some(match op {
        "+" => if matches!(left_ty, Ty::Float) { BinOp::AddFloat } else { BinOp::AddInt },
        "-" => if matches!(left_ty, Ty::Float) { BinOp::SubFloat } else { BinOp::SubInt },
        "*" => if matches!(left_ty, Ty::Float) { BinOp::MulFloat } else { BinOp::MulInt },
        "/" => if matches!(left_ty, Ty::Float) { BinOp::DivFloat } else { BinOp::DivInt },
        "%" => if matches!(left_ty, Ty::Float) { BinOp::ModFloat } else { BinOp::ModInt },
        "^" => if matches!(left_ty, Ty::Int) { BinOp::XorInt } else { BinOp::PowFloat },
        "++" => if matches!(left_ty, Ty::List(_)) { BinOp::ConcatList } else { BinOp::ConcatStr },
        "==" => BinOp::Eq,
        "!=" => BinOp::Neq,
        "<" => BinOp::Lt,
        ">" => BinOp::Gt,
        "<=" => BinOp::Lte,
        ">=" => BinOp::Gte,
        "and" => BinOp::And,
        "or" => BinOp::Or,
        _ => return None,
    })
}

/// Infer result type from a BinOp when checker type is unavailable.
fn bin_op_result_ty(op: BinOp) -> Ty {
    match op {
        BinOp::AddInt | BinOp::SubInt | BinOp::MulInt | BinOp::DivInt
        | BinOp::ModInt | BinOp::XorInt => Ty::Int,
        BinOp::AddFloat | BinOp::SubFloat | BinOp::MulFloat | BinOp::DivFloat
        | BinOp::ModFloat | BinOp::PowFloat => Ty::Float,
        BinOp::ConcatStr => Ty::String,
        BinOp::ConcatList => Ty::Unknown, // can't determine element type
        BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte
        | BinOp::And | BinOp::Or => Ty::Bool,
    }
}

fn resolve_un_op(op: &str, operand_ty: &Ty) -> Option<UnOp> {
    Some(match op {
        "-" => if matches!(operand_ty, Ty::Float) { UnOp::NegFloat } else { UnOp::NegInt },
        "not" => UnOp::Not,
        _ => return None,
    })
}

fn ty_to_resolved(ty: &Ty) -> Option<ast::ResolvedType> {
    Some(match ty {
        Ty::Int => ast::ResolvedType::Int,
        Ty::Float => ast::ResolvedType::Float,
        Ty::String => ast::ResolvedType::String,
        Ty::Bool => ast::ResolvedType::Bool,
        Ty::List(_) => ast::ResolvedType::List,
        Ty::Map(_, _) => ast::ResolvedType::Map,
        _ => return None,
    })
}

fn resolve_type_expr(te: &ast::TypeExpr) -> Ty {
    match te {
        ast::TypeExpr::Simple { name } => match name.as_str() {
            "Int" => Ty::Int,
            "Float" => Ty::Float,
            "String" => Ty::String,
            "Bool" => Ty::Bool,
            "Unit" => Ty::Unit,
            other => Ty::Named(other.to_string(), vec![]),
        },
        ast::TypeExpr::Generic { name, args } => match name.as_str() {
            "List" if args.len() == 1 => Ty::List(Box::new(resolve_type_expr(&args[0]))),
            "Option" if args.len() == 1 => Ty::Option(Box::new(resolve_type_expr(&args[0]))),
            "Result" if args.len() == 2 => Ty::Result(
                Box::new(resolve_type_expr(&args[0])),
                Box::new(resolve_type_expr(&args[1])),
            ),
            "Map" if args.len() == 2 => Ty::Map(
                Box::new(resolve_type_expr(&args[0])),
                Box::new(resolve_type_expr(&args[1])),
            ),
            other => Ty::Named(other.to_string(), args.iter().map(resolve_type_expr).collect()),
        },
        ast::TypeExpr::Tuple { elements } => {
            Ty::Tuple(elements.iter().map(resolve_type_expr).collect())
        }
        ast::TypeExpr::Fn { params, ret } => Ty::Fn {
            params: params.iter().map(resolve_type_expr).collect(),
            ret: Box::new(resolve_type_expr(ret)),
        },
        ast::TypeExpr::Record { fields } => Ty::Record {
            fields: fields.iter().map(|f| (f.name.clone(), resolve_type_expr(&f.ty))).collect(),
        },
        ast::TypeExpr::OpenRecord { fields } => Ty::OpenRecord {
            fields: fields.iter().map(|f| (f.name.clone(), resolve_type_expr(&f.ty))).collect(),
        },
        _ => Ty::Unknown,
    }
}
