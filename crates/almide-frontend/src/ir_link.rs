// ── IR Linker ────────────────────────────────────────────────────────
//
// Phase 1: Collect used_stdlib_modules across all modules.
// Phase 2: Flatten modules into root (called inside codegen, after
//          UnifyVarTablesPass has merged VarTables).

use almide_ir::*;
use almide_base::intern::sym;
use std::collections::HashSet;

/// Phase 1: Collect stdlib modules from deps + validate exports.
pub fn ir_link(program: &mut IrProgram) {
    // Log export counts for diagnostics (visible in --emit-ast)
    for module in &program.modules {
        let fn_count = module.exports.iter().filter(|e| matches!(e, IrExport::Function { .. })).count();
        let ty_count = module.exports.iter().filter(|e| matches!(e, IrExport::Type { .. })).count();
        let const_count = module.exports.iter().filter(|e| matches!(e, IrExport::Constant { .. })).count();
        if fn_count + ty_count + const_count > 0 {
            // Exports are available for downstream validation
            let _ = (module.name, fn_count, ty_count, const_count);
        }
    }

    let mut stdlib_modules = std::mem::take(&mut program.used_stdlib_modules);
    for module in &program.modules {
        // A module in program.modules was imported (explicitly or auto-import).
        // Its functions will be merged into root by IrLinkFlattenPass, so the
        // corresponding runtime module is needed.  Register the module name
        // so that codegen includes the runtime source without falling back to
        // generated-code text scanning.
        let name = module.name.as_str().to_string();
        if almide_lang::stdlib_info::is_any_stdlib(&name) {
            stdlib_modules.insert(name);
        }
        for func in &module.functions {
            scan_expr_stdlib(&func.body, &mut stdlib_modules);
        }
        for tl in &module.top_lets {
            scan_expr_stdlib(&tl.value, &mut stdlib_modules);
        }
    }
    program.used_stdlib_modules = stdlib_modules;
}

/// Phase 2: Flatten modules into root program.
/// MUST run after UnifyVarTablesPass (VarIds already unified).
/// After this, program.modules is empty. Walker renders flat functions.
pub fn ir_link_flatten(program: &mut IrProgram) {
    if program.modules.is_empty() {
        return;
    }

    let modules = std::mem::take(&mut program.modules);

    let mut emitted_types: HashSet<String> = program.type_decls.iter()
        .map(|td| td.name.as_str().to_string())
        .collect();

    for module in modules {
        let mod_ident = module.versioned_name
            .map(|v| v.to_string().replace('.', "_"))
            .unwrap_or_else(|| module.name.to_string().replace('.', "_"));

        // Merge type declarations (deduplicate by name)
        for td in module.type_decls {
            let name = td.name.as_str().to_string();
            if !emitted_types.contains(&name) {
                emitted_types.insert(name);
                program.type_decls.push(td);
            }
        }

        // Merge functions with prefixed names
        for mut func in module.functions {
            let clean_name = func.name.as_str()
                .replace(' ', "_").replace('-', "_").replace('.', "_");
            let prefixed = format!("almide_rt_{}_{}", mod_ident, clean_name);
            func.name = sym(&prefixed);
            program.functions.push(func);
        }

        // Merge top_lets (already prefixed by lower_module)
        for tl in module.top_lets {
            program.top_lets.push(tl);
        }
    }
}

/// Scan an expression tree for stdlib module references.
///
/// Router: dispatches to a group helper by expr kind. Each helper handles an
/// independent subset of `IrExprKind` and returns whether it matched — `used`
/// is a write-only accumulator (no arm ever reads back what an earlier arm
/// wrote), so grouping is behavior-preserving. Mirrors the split used for
/// `scan_expr` in `lower/mod_p2.rs`.
fn scan_expr_stdlib(expr: &IrExpr, used: &mut HashSet<String>) {
    if scan_expr_stdlib_calls(expr, used) { return; }
    if scan_expr_stdlib_control(expr, used) { return; }
    scan_expr_stdlib_containers(expr, used);
}

// Call-like nodes: the only arms that can add a module name to `used`.
fn scan_expr_stdlib_calls(expr: &IrExpr, used: &mut HashSet<String>) -> bool {
    match &expr.kind {
        IrExprKind::Call { target, args, .. } => {
            if let CallTarget::Module { module, .. } = target {
                used.insert(module.to_string());
            }
            if let CallTarget::Method { object, .. } = target {
                scan_expr_stdlib(object, used);
            }
            for a in args { scan_expr_stdlib(a, used); }
            true
        }
        IrExprKind::RuntimeCall { symbol, args } => {
            if let Some(rest) = symbol.as_str().strip_prefix("almide_rt_") {
                if let Some(pos) = rest.find('_') {
                    used.insert(rest[..pos].to_string());
                }
            }
            for a in args { scan_expr_stdlib(a, used); }
            true
        }
        _ => false,
    }
}

// Control-flow nodes: recurse into sub-blocks/statements/arms.
fn scan_expr_stdlib_control(expr: &IrExpr, used: &mut HashSet<String>) -> bool {
    if scan_expr_stdlib_block_like(expr, used) { return true; }
    scan_expr_stdlib_loop_like(expr, used)
}

// Block/If/Match: nodes that carry statement lists or arms.
fn scan_expr_stdlib_block_like(expr: &IrExpr, used: &mut HashSet<String>) -> bool {
    match &expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            for s in stmts { scan_stmt_stdlib(s, used); }
            if let Some(e) = tail { scan_expr_stdlib(e, used); }
            true
        }
        IrExprKind::If { cond, then, else_ } => {
            scan_expr_stdlib(cond, used);
            scan_expr_stdlib(then, used);
            scan_expr_stdlib(else_, used);
            true
        }
        IrExprKind::Match { subject, arms } => {
            scan_expr_stdlib(subject, used);
            for arm in arms {
                if let Some(g) = &arm.guard { scan_expr_stdlib(g, used); }
                scan_expr_stdlib(&arm.body, used);
            }
            true
        }
        _ => false,
    }
}

// Lambda/ForIn/While: nodes with a body and (for loops) an iterable/cond.
fn scan_expr_stdlib_loop_like(expr: &IrExpr, used: &mut HashSet<String>) -> bool {
    match &expr.kind {
        IrExprKind::Lambda { body, .. } => { scan_expr_stdlib(body, used); true }
        IrExprKind::ForIn { iterable, body, .. } => {
            scan_expr_stdlib(iterable, used);
            for s in body { scan_stmt_stdlib(s, used); }
            true
        }
        IrExprKind::While { cond, body } => {
            scan_expr_stdlib(cond, used);
            for s in body { scan_stmt_stdlib(s, used); }
            true
        }
        _ => false,
    }
}

// Plain container/wrapper nodes: straight recursive descent, no module names
// to record here.
fn scan_expr_stdlib_containers(expr: &IrExpr, used: &mut HashSet<String>) {
    if scan_expr_stdlib_wrappers(expr, used) { return; }
    scan_expr_stdlib_collections(expr, used);
}

// Single/dual-child wrapper nodes: UnOp, unwrap-like variants, UnwrapOr,
// IndexAccess/Range — each recurses directly into its 1-2 sub-expressions.
fn scan_expr_stdlib_wrappers(expr: &IrExpr, used: &mut HashSet<String>) -> bool {
    match &expr.kind {
        IrExprKind::UnOp { operand, .. } => { scan_expr_stdlib(operand, used); true }
        IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::Unwrap { expr: e }
        | IrExprKind::Try { expr: e } | IrExprKind::ToOption { expr: e }
        | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e }
        | IrExprKind::Member { object: e, .. } => { scan_expr_stdlib(e, used); true }
        IrExprKind::UnwrapOr { expr: e, fallback } => {
            scan_expr_stdlib(e, used);
            scan_expr_stdlib(fallback, used);
            true
        }
        IrExprKind::IndexAccess { object, index } | IrExprKind::Range { start: object, end: index, .. } => {
            scan_expr_stdlib(object, used);
            scan_expr_stdlib(index, used);
            true
        }
        _ => false,
    }
}

// Collection-literal nodes: recurse over a list/map of sub-expressions.
fn scan_expr_stdlib_collections(expr: &IrExpr, used: &mut HashSet<String>) {
    if scan_expr_stdlib_seq_literals(expr, used) { return; }
    scan_expr_stdlib_keyed_literals(expr, used);
}

// BinOp/List/Tuple/Fan/Record: sequence-shaped literals and BinOp.
fn scan_expr_stdlib_seq_literals(expr: &IrExpr, used: &mut HashSet<String>) -> bool {
    match &expr.kind {
        IrExprKind::BinOp { left, right, .. } => {
            scan_expr_stdlib(left, used);
            scan_expr_stdlib(right, used);
            true
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements }
        | IrExprKind::Fan { exprs: elements } => {
            for e in elements { scan_expr_stdlib(e, used); }
            true
        }
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields { scan_expr_stdlib(v, used); }
            true
        }
        _ => false,
    }
}

// StringInterp/SpreadRecord/MapLiteral: keyed/mixed literal forms.
fn scan_expr_stdlib_keyed_literals(expr: &IrExpr, used: &mut HashSet<String>) {
    match &expr.kind {
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr } = p { scan_expr_stdlib(expr, used); }
            }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            scan_expr_stdlib(base, used);
            for (_, v) in fields { scan_expr_stdlib(v, used); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries { scan_expr_stdlib(k, used); scan_expr_stdlib(v, used); }
        }
        _ => {}
    }
}

fn scan_stmt_stdlib(stmt: &IrStmt, used: &mut HashSet<String>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => scan_expr_stdlib(value, used),
        IrStmtKind::Expr { expr } => scan_expr_stdlib(expr, used),
        IrStmtKind::Guard { cond, else_ } => {
            scan_expr_stdlib(cond, used);
            scan_expr_stdlib(else_, used);
        }
        _ => {}
    }
}
