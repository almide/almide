// ── IR integrity verification (post-pass) ───────────────────────
//
// Debug-only pass that catches internal compiler errors before codegen.
// Runs after lowering + optimization, before monomorphization.
//
// Checks:
//   1. VarId bounds — every referenced VarId exists in VarTable
//   2. Mutability — only `var` variables appear in Assign/IndexAssign/FieldAssign
//   3. Loop context — Break/Continue only inside ForIn/While
//   4. Operator–type consistency — BinOp variant matches operand types

use super::*;
use crate::types::Ty;

/// An internal compiler error detected by IR verification.
#[derive(Debug)]
pub struct IrVerifyError {
    pub message: String,
    pub fn_name: String,
    pub span: Option<Span>,
}

impl std::fmt::Display for IrVerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IR verify: {} (in {})", self.message, self.fn_name)?;
        if let Some(s) = &self.span {
            write!(f, " at line {}", s.line)?;
        }
        Ok(())
    }
}

struct VerifyCtx<'a> {
    var_table: &'a VarTable,
    fn_name: String,
    in_loop: bool,
    errors: Vec<IrVerifyError>,
}

impl<'a> VerifyCtx<'a> {
    fn err(&mut self, message: String, span: Option<Span>) {
        self.errors.push(IrVerifyError {
            message,
            fn_name: self.fn_name.clone(),
            span,
        });
    }

    fn check_var_id(&mut self, id: VarId, span: Option<Span>) {
        if (id.0 as usize) >= self.var_table.len() {
            self.err(
                format!("VarId({}) out of bounds (table size: {})", id.0, self.var_table.len()),
                span,
            );
        }
    }

    // Note: mutability checking is intentionally omitted here.
    // The optimizer's `demote_unused_mut` pass may have already
    // demoted `Var` to `Let` for variables that are assigned but
    // whose assignments were eliminated by DCE. Checking mutability
    // after optimization would produce false positives.
}

/// Verify IR integrity for the main program. Returns errors found.
/// Intended for debug builds — call after optimization, before monomorphization.
pub fn verify_program(program: &IrProgram) -> Vec<IrVerifyError> {
    let mut errors = Vec::new();

    // Verify type declarations
    verify_type_decls(&program.type_decls, "", &mut errors);

    // Verify main module functions
    for f in &program.functions {
        verify_function(f, &program.var_table, &f.name, &mut errors);
    }
    for tl in &program.top_lets {
        let mut ctx = VerifyCtx {
            var_table: &program.var_table,
            fn_name: "<top-level>".into(),
            in_loop: false,
            errors: Vec::new(),
        };
        ctx.check_var_id(tl.var, None);
        verify_expr(&tl.value, &mut ctx);
        errors.append(&mut ctx.errors);
    }

    // Verify imported modules
    for m in &program.modules {
        verify_type_decls(&m.type_decls, &m.name, &mut errors);
        for f in &m.functions {
            let qual_name = format!("{}.{}", m.name, f.name);
            verify_function(f, &m.var_table, &qual_name, &mut errors);
        }
        for tl in &m.top_lets {
            let mut ctx = VerifyCtx {
                var_table: &m.var_table,
                fn_name: format!("{}.<top-level>", m.name),
                in_loop: false,
                errors: Vec::new(),
            };
            ctx.check_var_id(tl.var, None);
            verify_expr(&tl.value, &mut ctx);
            errors.append(&mut ctx.errors);
        }
    }

    errors
}

fn verify_function(f: &IrFunction, var_table: &VarTable, name: &str, errors: &mut Vec<IrVerifyError>) {
    let mut ctx = VerifyCtx {
        var_table,
        fn_name: name.to_string(),
        in_loop: false,
        errors: Vec::new(),
    };

    // Check parameter VarIds are valid and unique
    let mut seen_param_ids = std::collections::HashSet::new();
    for p in &f.params {
        ctx.check_var_id(p.var, None);
        if !seen_param_ids.insert(p.var.0) {
            ctx.err(format!("duplicate parameter VarId({}) for '{}'", p.var.0, p.name), None);
        }
    }

    verify_expr(&f.body, &mut ctx);
    errors.append(&mut ctx.errors);
}

fn verify_type_decls(decls: &[IrTypeDecl], module: &str, errors: &mut Vec<IrVerifyError>) {
    for decl in decls {
        let loc = if module.is_empty() { decl.name.clone() } else { format!("{}.{}", module, decl.name) };
        match &decl.kind {
            IrTypeDeclKind::Record { fields } => {
                let mut seen = std::collections::HashSet::new();
                for f in fields {
                    if !seen.insert(&f.name) {
                        errors.push(IrVerifyError {
                            message: format!("duplicate field '{}' in record type", f.name),
                            fn_name: loc.clone(),
                            span: None,
                        });
                    }
                }
            }
            IrTypeDeclKind::Variant { cases, .. } => {
                let mut seen = std::collections::HashSet::new();
                for c in cases {
                    if !seen.insert(&c.name) {
                        errors.push(IrVerifyError {
                            message: format!("duplicate variant case '{}'", c.name),
                            fn_name: loc.clone(),
                            span: None,
                        });
                    }
                }
            }
            IrTypeDeclKind::Alias { .. } => {}
        }
    }
}

fn verify_expr(expr: &IrExpr, ctx: &mut VerifyCtx) {
    match &expr.kind {
        // ── Variables ──
        IrExprKind::Var { id } => {
            ctx.check_var_id(*id, expr.span);
        }

        // ── Operators: check type consistency ──
        IrExprKind::BinOp { op, left, right } => {
            verify_binop_types(*op, left, right, ctx, expr.span);
            verify_expr(left, ctx);
            verify_expr(right, ctx);
        }
        IrExprKind::UnOp { op, operand } => {
            verify_unop_types(*op, operand, ctx, expr.span);
            verify_expr(operand, ctx);
        }

        // ── Loop context ──
        IrExprKind::Break | IrExprKind::Continue => {
            if !ctx.in_loop {
                let kind = if matches!(expr.kind, IrExprKind::Break) { "break" } else { "continue" };
                ctx.err(format!("{} outside of loop", kind), expr.span);
            }
        }
        IrExprKind::ForIn { var, var_tuple, iterable, body } => {
            ctx.check_var_id(*var, expr.span);
            if let Some(tuple_vars) = var_tuple {
                for v in tuple_vars {
                    ctx.check_var_id(*v, expr.span);
                }
            }
            verify_expr(iterable, ctx);
            let prev = ctx.in_loop;
            ctx.in_loop = true;
            for s in body { verify_stmt(s, ctx); }
            ctx.in_loop = prev;
        }
        IrExprKind::While { cond, body } => {
            verify_expr(cond, ctx);
            let prev = ctx.in_loop;
            ctx.in_loop = true;
            for s in body { verify_stmt(s, ctx); }
            ctx.in_loop = prev;
        }

        // ── Recursive cases ──
        IrExprKind::If { cond, then, else_ } => {
            verify_expr(cond, ctx);
            verify_expr(then, ctx);
            verify_expr(else_, ctx);
        }
        IrExprKind::Block { stmts, expr: tail } => {
            for s in stmts { verify_stmt(s, ctx); }
            if let Some(t) = tail { verify_expr(t, ctx); }
        }
        // DoBlock with guards acts as a loop (break is valid for early exit)
        IrExprKind::DoBlock { stmts, expr: tail } => {
            let prev = ctx.in_loop;
            ctx.in_loop = true;
            for s in stmts { verify_stmt(s, ctx); }
            if let Some(t) = tail { verify_expr(t, ctx); }
            ctx.in_loop = prev;
        }
        IrExprKind::Match { subject, arms } => {
            verify_expr(subject, ctx);
            for arm in arms {
                verify_pattern(&arm.pattern, ctx, expr.span);
                if let Some(g) = &arm.guard { verify_expr(g, ctx); }
                verify_expr(&arm.body, ctx);
            }
        }
        IrExprKind::Call { target, args, .. } => {
            match target {
                CallTarget::Method { object, .. } => verify_expr(object, ctx),
                CallTarget::Computed { callee } => verify_expr(callee, ctx),
                _ => {}
            }
            for a in args { verify_expr(a, ctx); }
        }
        IrExprKind::Lambda { params, body } => {
            for (var, _) in params {
                ctx.check_var_id(*var, expr.span);
            }
            verify_expr(body, ctx);
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements }
        | IrExprKind::Fan { exprs: elements } => {
            for e in elements { verify_expr(e, ctx); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields { verify_expr(v, ctx); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            verify_expr(base, ctx);
            for (_, v) in fields { verify_expr(v, ctx); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries { verify_expr(k, ctx); verify_expr(v, ctx); }
        }
        IrExprKind::Range { start, end, .. } => {
            verify_expr(start, ctx);
            verify_expr(end, ctx);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            verify_expr(object, ctx);
        }
        IrExprKind::IndexAccess { object, index } => {
            if !is_unresolved(&object.ty) && object.ty.is_map() {
                ctx.err("IndexAccess used on Map type (should be MapAccess)".into(), expr.span);
            }
            verify_expr(object, ctx);
            verify_expr(index, ctx);
        }
        IrExprKind::MapAccess { object, key } => {
            if !is_unresolved(&object.ty) && !object.ty.is_map() {
                ctx.err(
                    format!("MapAccess used on non-Map type '{}'", object.ty.display()),
                    expr.span,
                );
            }
            verify_expr(object, ctx);
            verify_expr(key, ctx);
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr } = p { verify_expr(expr, ctx); }
            }
        }
        IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Await { expr: e }
        | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e }
        | IrExprKind::Borrow { expr: e, .. } | IrExprKind::BoxNew { expr: e }
        | IrExprKind::ToVec { expr: e } => {
            verify_expr(e, ctx);
        }
        IrExprKind::RustMacro { args, .. } => {
            for a in args { verify_expr(a, ctx); }
        }

        // Leaf nodes
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::FnRef { .. }
        | IrExprKind::EmptyMap | IrExprKind::OptionNone | IrExprKind::Hole
        | IrExprKind::Todo { .. } | IrExprKind::RenderedCall { .. } => {}
    }
}

fn verify_stmt(stmt: &IrStmt, ctx: &mut VerifyCtx) {
    match &stmt.kind {
        IrStmtKind::Bind { var, value, .. } => {
            ctx.check_var_id(*var, stmt.span);
            verify_expr(value, ctx);
        }
        IrStmtKind::BindDestructure { pattern, value } => {
            verify_pattern(pattern, ctx, stmt.span);
            verify_expr(value, ctx);
        }
        IrStmtKind::Assign { var, value } => {
            ctx.check_var_id(*var, stmt.span);
            verify_expr(value, ctx);
        }
        IrStmtKind::IndexAssign { target, index, value } => {
            ctx.check_var_id(*target, stmt.span);
            verify_expr(index, ctx);
            verify_expr(value, ctx);
        }
        IrStmtKind::MapInsert { target, key, value } => {
            ctx.check_var_id(*target, stmt.span);
            verify_expr(key, ctx);
            verify_expr(value, ctx);
        }
        IrStmtKind::FieldAssign { target, value, .. } => {
            ctx.check_var_id(*target, stmt.span);
            verify_expr(value, ctx);
        }
        IrStmtKind::Guard { cond, else_ } => {
            verify_expr(cond, ctx);
            verify_expr(else_, ctx);
        }
        IrStmtKind::Expr { expr } => {
            verify_expr(expr, ctx);
        }
        IrStmtKind::Comment { .. } => {}
    }
}

fn verify_pattern(pat: &IrPattern, ctx: &mut VerifyCtx, span: Option<Span>) {
    match pat {
        IrPattern::Bind { var } => ctx.check_var_id(*var, span),
        IrPattern::Constructor { args, .. } => {
            for a in args { verify_pattern(a, ctx, span); }
        }
        IrPattern::RecordPattern { fields, .. } => {
            for f in fields {
                if let Some(p) = &f.pattern { verify_pattern(p, ctx, span); }
            }
        }
        IrPattern::Tuple { elements } => {
            for e in elements { verify_pattern(e, ctx, span); }
        }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
            verify_pattern(inner, ctx, span);
        }
        IrPattern::Literal { expr } => verify_expr(expr, ctx),
        IrPattern::Wildcard | IrPattern::None => {}
    }
}

// ── Operator–type consistency ─────────────────────────────────────

/// Check that a BinOp variant is consistent with its operand types.
/// Only flags clear contradictions (e.g., AddInt on String operands).
fn verify_binop_types(op: BinOp, left: &IrExpr, right: &IrExpr, ctx: &mut VerifyCtx, span: Option<Span>) {
    let lt = &left.ty;
    let rt = &right.ty;

    // Skip if either side is Unknown (error recovery) or TypeVar (generic)
    if is_unresolved(lt) || is_unresolved(rt) { return; }

    let expected = match op {
        BinOp::AddInt | BinOp::SubInt | BinOp::MulInt
        | BinOp::DivInt | BinOp::ModInt | BinOp::PowInt | BinOp::XorInt => Some(Ty::Int),
        BinOp::AddFloat | BinOp::SubFloat | BinOp::MulFloat
        | BinOp::DivFloat | BinOp::PowFloat => Some(Ty::Float),
        BinOp::ConcatStr => Some(Ty::String),
        // ConcatList, Eq, Neq, comparisons, And, Or — operand types vary
        _ => None,
    };

    if let Some(expected_ty) = expected {
        if !ty_matches(lt, &expected_ty) || !ty_matches(rt, &expected_ty) {
            ctx.err(
                format!(
                    "{:?} expects {} operands, got {} and {}",
                    op, expected_ty.display(), lt.display(), rt.display()
                ),
                span,
            );
        }
    }

    // And/Or require Bool
    if matches!(op, BinOp::And | BinOp::Or) {
        if !ty_matches(lt, &Ty::Bool) || !ty_matches(rt, &Ty::Bool) {
            ctx.err(
                format!(
                    "{:?} expects Bool operands, got {} and {}",
                    op, lt.display(), rt.display()
                ),
                span,
            );
        }
    }
}

fn verify_unop_types(op: UnOp, operand: &IrExpr, ctx: &mut VerifyCtx, span: Option<Span>) {
    let t = &operand.ty;
    if is_unresolved(t) { return; }

    let expected = match op {
        UnOp::NegInt => Some(Ty::Int),
        UnOp::NegFloat => Some(Ty::Float),
        UnOp::Not => Some(Ty::Bool),
    };

    if let Some(expected_ty) = expected {
        if !ty_matches(t, &expected_ty) {
            ctx.err(
                format!(
                    "{:?} expects {} operand, got {}",
                    op, expected_ty.display(), t.display()
                ),
                span,
            );
        }
    }
}

fn is_unresolved(ty: &Ty) -> bool {
    matches!(ty, Ty::Unknown | Ty::TypeVar(_))
}

fn ty_matches(actual: &Ty, expected: &Ty) -> bool {
    if is_unresolved(actual) { return true; }
    std::mem::discriminant(actual) == std::mem::discriminant(expected)
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_program(functions: Vec<IrFunction>, var_table: VarTable) -> IrProgram {
        IrProgram {
            functions,
            top_lets: vec![],
            type_decls: vec![],
            var_table,
            modules: vec![],
            type_registry: Default::default(),
            effect_map: Default::default(),
        }
    }

    fn lit_int(v: i64) -> IrExpr {
        IrExpr { kind: IrExprKind::LitInt { value: v }, ty: Ty::Int, span: None }
    }

    fn var_expr(id: VarId, ty: Ty) -> IrExpr {
        IrExpr { kind: IrExprKind::Var { id }, ty, span: None }
    }

    fn make_fn(name: &str, body: IrExpr) -> IrFunction {
        IrFunction {
            name: name.into(),
            params: vec![],
            ret_ty: body.ty.clone(),
            body,
            is_effect: false,
            is_async: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            visibility: IrVisibility::Public,
        }
    }

    #[test]
    fn valid_program_no_errors() {
        let mut vt = VarTable::new();
        let x = vt.alloc("x".into(), Ty::Int, Mutability::Let, None);
        let body = IrExpr {
            kind: IrExprKind::Block {
                stmts: vec![IrStmt {
                    kind: IrStmtKind::Bind { var: x, mutability: Mutability::Let, ty: Ty::Int, value: lit_int(1) },
                    span: None,
                }],
                expr: Some(Box::new(var_expr(x, Ty::Int))),
            },
            ty: Ty::Int,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn detects_var_id_out_of_bounds() {
        let vt = VarTable::new(); // empty table
        let body = var_expr(VarId(99), Ty::Int);
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("VarId(99)"));
    }

    #[test]
    fn assign_checks_var_id_bounds() {
        let vt = VarTable::new(); // empty
        let body = IrExpr {
            kind: IrExprKind::Block {
                stmts: vec![IrStmt {
                    kind: IrStmtKind::Assign { var: VarId(99), value: lit_int(2) },
                    span: None,
                }],
                expr: None,
            },
            ty: Ty::Unit,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("VarId(99)"));
    }

    #[test]
    fn detects_break_outside_loop() {
        let vt = VarTable::new();
        let body = IrExpr { kind: IrExprKind::Break, ty: Ty::Unit, span: None };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("break outside of loop"));
    }

    #[test]
    fn allows_break_inside_loop() {
        let mut vt = VarTable::new();
        let i = vt.alloc("i".into(), Ty::Int, Mutability::Let, None);
        let body = IrExpr {
            kind: IrExprKind::ForIn {
                var: i,
                var_tuple: None,
                iterable: Box::new(IrExpr {
                    kind: IrExprKind::Range {
                        start: Box::new(lit_int(0)),
                        end: Box::new(lit_int(10)),
                        inclusive: false,
                    },
                    ty: Ty::Int, // simplified
                    span: None,
                }),
                body: vec![IrStmt {
                    kind: IrStmtKind::Expr {
                        expr: IrExpr { kind: IrExprKind::Break, ty: Ty::Unit, span: None },
                    },
                    span: None,
                }],
            },
            ty: Ty::Unit,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert!(errors.is_empty());
    }

    #[test]
    fn detects_binop_type_mismatch() {
        let vt = VarTable::new();
        let body = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::AddInt,
                left: Box::new(IrExpr { kind: IrExprKind::LitStr { value: "a".into() }, ty: Ty::String, span: None }),
                right: Box::new(lit_int(1)),
            },
            ty: Ty::Int,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("AddInt"));
    }

    #[test]
    fn skips_unknown_types_in_binop() {
        let vt = VarTable::new();
        let body = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::AddInt,
                left: Box::new(IrExpr { kind: IrExprKind::Hole, ty: Ty::Unknown, span: None }),
                right: Box::new(lit_int(1)),
            },
            ty: Ty::Int,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert!(errors.is_empty());
    }

    #[test]
    fn detects_continue_outside_loop() {
        let vt = VarTable::new();
        let body = IrExpr { kind: IrExprKind::Continue, ty: Ty::Unit, span: None };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("continue outside of loop"));
    }

    #[test]
    fn verifies_pattern_var_ids() {
        let mut vt = VarTable::new();
        let _x = vt.alloc("x".into(), Ty::Int, Mutability::Let, None);
        // Pattern references VarId(99) which doesn't exist
        let body = IrExpr {
            kind: IrExprKind::Match {
                subject: Box::new(lit_int(1)),
                arms: vec![IrMatchArm {
                    pattern: IrPattern::Bind { var: VarId(99) },
                    guard: None,
                    body: lit_int(2),
                }],
            },
            ty: Ty::Int,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("VarId(99)"));
    }

    #[test]
    fn verifies_module_functions() {
        let mut main_vt = VarTable::new();
        let _x = main_vt.alloc("x".into(), Ty::Int, Mutability::Let, None);

        let mod_vt = VarTable::new(); // empty
        let mod_body = var_expr(VarId(99), Ty::Int); // out of bounds in module table

        let prog = IrProgram {
            functions: vec![make_fn("main", lit_int(0))],
            top_lets: vec![],
            type_decls: vec![],
            var_table: main_vt,
            modules: vec![IrModule {
                name: "mymod".into(),
                versioned_name: None,
                type_decls: vec![],
                functions: vec![make_fn("helper", mod_body)],
                top_lets: vec![],
                var_table: mod_vt,
            }],
            type_registry: Default::default(),
            effect_map: Default::default(),
        };
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].fn_name == "mymod.helper");
    }

    #[test]
    fn detects_unop_type_mismatch() {
        let vt = VarTable::new();
        let body = IrExpr {
            kind: IrExprKind::UnOp {
                op: UnOp::NegInt,
                operand: Box::new(IrExpr {
                    kind: IrExprKind::LitBool { value: true },
                    ty: Ty::Bool,
                    span: None,
                }),
            },
            ty: Ty::Int,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("NegInt"));
    }

    #[test]
    fn detects_duplicate_record_fields() {
        let vt = VarTable::new();
        let prog = IrProgram {
            functions: vec![],
            top_lets: vec![],
            type_decls: vec![IrTypeDecl {
                name: "Bad".into(),
                kind: IrTypeDeclKind::Record {
                    fields: vec![
                        IrFieldDecl { name: "x".into(), ty: Ty::Int, default: None, alias: None },
                        IrFieldDecl { name: "x".into(), ty: Ty::String, default: None, alias: None },
                    ],
                },
                deriving: None,
                generics: None,
                visibility: IrVisibility::Public,
            }],
            var_table: vt,
            modules: vec![],
            type_registry: Default::default(),
            effect_map: Default::default(),
        };
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("duplicate field 'x'"));
    }

    #[test]
    fn detects_duplicate_variant_cases() {
        let vt = VarTable::new();
        let prog = IrProgram {
            functions: vec![],
            top_lets: vec![],
            type_decls: vec![IrTypeDecl {
                name: "Bad".into(),
                kind: IrTypeDeclKind::Variant {
                    cases: vec![
                        IrVariantDecl { name: "A".into(), kind: IrVariantKind::Unit },
                        IrVariantDecl { name: "A".into(), kind: IrVariantKind::Unit },
                    ],
                    is_generic: false,
                    boxed_args: HashSet::new(),
                    boxed_record_fields: HashSet::new(),
                },
                deriving: None,
                generics: None,
                visibility: IrVisibility::Public,
            }],
            var_table: vt,
            modules: vec![],
            type_registry: Default::default(),
            effect_map: Default::default(),
        };
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("duplicate variant case 'A'"));
    }

    #[test]
    fn detects_duplicate_param_var_ids() {
        let mut vt = VarTable::new();
        let x = vt.alloc("x".into(), Ty::Int, Mutability::Let, None);
        let f = IrFunction {
            name: "bad".into(),
            params: vec![
                IrParam { var: x, ty: Ty::Int, name: "a".into(), borrow: ParamBorrow::Own, open_record: None, default: None },
                IrParam { var: x, ty: Ty::Int, name: "b".into(), borrow: ParamBorrow::Own, open_record: None, default: None },
            ],
            ret_ty: Ty::Int,
            body: lit_int(0),
            is_effect: false,
            is_async: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            visibility: IrVisibility::Public,
        };
        let prog = make_program(vec![f], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("duplicate parameter VarId"));
    }

    #[test]
    fn allows_break_inside_do_block() {
        let vt = VarTable::new();
        let body = IrExpr {
            kind: IrExprKind::DoBlock {
                stmts: vec![IrStmt {
                    kind: IrStmtKind::Expr {
                        expr: IrExpr { kind: IrExprKind::Break, ty: Ty::Unit, span: None },
                    },
                    span: None,
                }],
                expr: None,
            },
            ty: Ty::Unit,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert!(errors.is_empty());
    }

    #[test]
    fn detects_index_access_on_map() {
        let vt = VarTable::new();
        let map_ty = Ty::Applied(crate::types::TypeConstructorId::Map, vec![Ty::String, Ty::Int]);
        let body = IrExpr {
            kind: IrExprKind::IndexAccess {
                object: Box::new(IrExpr { kind: IrExprKind::EmptyMap, ty: map_ty, span: None }),
                index: Box::new(IrExpr { kind: IrExprKind::LitStr { value: "k".into() }, ty: Ty::String, span: None }),
            },
            ty: Ty::Int,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("IndexAccess used on Map"));
    }

    #[test]
    fn detects_map_access_on_non_map() {
        let vt = VarTable::new();
        let list_ty = Ty::Applied(crate::types::TypeConstructorId::List, vec![Ty::Int]);
        let body = IrExpr {
            kind: IrExprKind::MapAccess {
                object: Box::new(IrExpr { kind: IrExprKind::List { elements: vec![] }, ty: list_ty, span: None }),
                key: Box::new(lit_int(0)),
            },
            ty: Ty::Int,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("MapAccess used on non-Map"));
    }

    #[test]
    fn allows_map_access_on_map() {
        let vt = VarTable::new();
        let map_ty = Ty::Applied(crate::types::TypeConstructorId::Map, vec![Ty::String, Ty::Int]);
        let body = IrExpr {
            kind: IrExprKind::MapAccess {
                object: Box::new(IrExpr { kind: IrExprKind::EmptyMap, ty: map_ty, span: None }),
                key: Box::new(IrExpr { kind: IrExprKind::LitStr { value: "k".into() }, ty: Ty::String, span: None }),
            },
            ty: Ty::Int,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        assert!(verify_program(&prog).is_empty());
    }

    #[test]
    fn pow_int_type_consistency() {
        let vt = VarTable::new();
        // PowInt with Int operands — should pass
        let body = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::PowInt,
                left: Box::new(lit_int(2)),
                right: Box::new(lit_int(3)),
            },
            ty: Ty::Int,
            span: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        assert!(verify_program(&prog).is_empty());

        // PowInt with Float operand — should fail
        let vt2 = VarTable::new();
        let body2 = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::PowInt,
                left: Box::new(IrExpr { kind: IrExprKind::LitFloat { value: 2.0 }, ty: Ty::Float, span: None }),
                right: Box::new(lit_int(3)),
            },
            ty: Ty::Int,
            span: None,
        };
        let prog2 = make_program(vec![make_fn("main", body2)], vt2);
        let errors = verify_program(&prog2);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("PowInt"));
    }
}
