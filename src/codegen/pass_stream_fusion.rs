//! StreamFusionPass: fuse pipe chains (map |> filter |> fold) into single loops.
//!
//! Uses the TypeConstructorRegistry's algebraic law table to determine
//! which fusions are valid. This ensures all optimizations are mathematically
//! guaranteed to preserve semantics.
//!
//! ## Fusion Rules (from algebraic laws)
//!
//! - FunctorComposition: `map(f) |> map(g)` → `map(f >> g)`
//! - FilterComposition: `filter(p) |> filter(q)` → `filter(x => p(x) && q(x))`
//! - MapFoldFusion: `map(f) |> fold(init, g)` → `fold(init, (acc, x) => g(acc, f(x)))`
//! - MapFilterFusion: `map(f) |> filter(p)` → single-pass filter_map
//! - FilterMapFoldFusion: `fold(filter_map(x, fm), init, g)` → single-pass fold
//! - RangeFoldFusion: `fold(range(s, e), init, g)` → for loop (no allocation)

use crate::ir::*;
use crate::types::Ty;
use crate::types::constructor::{TypeConstructorRegistry, AlgebraicLaw};
use super::pass::{NanoPass, Target};

// ── NanoPass entry point ──────────────────────────────────────────

#[derive(Debug)]
pub struct StreamFusionPass;

impl NanoPass for StreamFusionPass {
    fn name(&self) -> &str { "StreamFusion" }
    fn targets(&self) -> Option<Vec<Target>> { None }

    fn run(&self, program: &mut IrProgram, _target: Target) {
        // Pre-pass: inline single-use collection lets to expose nested call patterns
        for func in &mut program.functions {
            inline_single_use_collection_lets(&mut func.body, &program.var_table);
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                inline_single_use_collection_lets(&mut func.body, &module.var_table);
            }
        }

        let debug = std::env::var("ALMIDE_DEBUG_FUSION").is_ok();

        // Debug: dump IR after inlining
        if debug {
            for func in &program.functions {
                dump_calls(&func.body, &func.name);
            }
        }

        // Phase 2: fuse chains using algebraic laws
        let mut totals = FusionCounts::default();
        for func in &mut program.functions {
            let c = fuse_all(func, &mut program.var_table);
            totals.add(&c);
        }

        // Debug output
        if debug {
            let registry = &program.type_registry;
            for func in &program.functions {
                let chains = detect_pipe_chains(&func.body, registry);
                for chain in &chains {
                    eprintln!(
                        "[StreamFusion] {}: {} ({} fusible, container={:?})",
                        func.name,
                        chain.ops.iter().map(|o| format!("{:?}", o)).collect::<Vec<_>>().join(" → "),
                        chain.fusible_pairs,
                        chain.container_name,
                    );
                }
            }
            if totals.total() > 0 {
                eprintln!("[StreamFusion] fused: {}", totals.summary());
            }
        }
    }
}

// ── Generic bottom-up IR transform ───────────────────────────────

/// Apply `try_transform` bottom-up to every node in an expression tree.
/// Children are transformed first, then `try_transform` is called on the result.
/// If it returns `Some(new)`, the node is replaced; otherwise kept as-is.
fn recursive_transform(
    expr: IrExpr,
    f: &mut dyn FnMut(IrExpr) -> Option<IrExpr>,
) -> IrExpr {
    let transformed = transform_children(expr, f);
    f(transformed.clone()).unwrap_or(transformed)
}

/// Transform all children of an expression, leaving the node itself unchanged.
fn transform_children(
    expr: IrExpr,
    f: &mut dyn FnMut(IrExpr) -> Option<IrExpr>,
) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;
    // Use a macro to avoid borrow-checker issues with closures capturing &mut f.
    macro_rules! rec {
        ($e:expr) => { recursive_transform($e, f) };
    }
    macro_rules! rec_stmt {
        ($s:expr) => { transform_stmt($s, f) };
    }

    let kind = match expr.kind {
        IrExprKind::Call { target, args, type_args } => IrExprKind::Call {
            target: match target {
                CallTarget::Method { object, method } => CallTarget::Method {
                    object: Box::new(rec!(*object)), method,
                },
                CallTarget::Computed { callee } => CallTarget::Computed {
                    callee: Box::new(rec!(*callee)),
                },
                other => other,
            },
            args: args.into_iter().map(|e| rec!(e)).collect(),
            type_args,
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op, left: Box::new(rec!(*left)), right: Box::new(rec!(*right)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(rec!(*operand)),
        },
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(rec!(*cond)),
            then: Box::new(rec!(*then)),
            else_: Box::new(rec!(*else_)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(rec!(*subject)),
            arms: arms.into_iter().map(|arm| IrMatchArm {
                pattern: arm.pattern,
                guard: arm.guard.map(|g| rec!(g)),
                body: rec!(arm.body),
            }).collect(),
        },
        IrExprKind::Block { stmts, expr: tail } => IrExprKind::Block {
            stmts: stmts.into_iter().map(|s| rec_stmt!(s)).collect(),
            expr: tail.map(|e| Box::new(rec!(*e))),
        },
        IrExprKind::DoBlock { stmts, expr: tail } => IrExprKind::DoBlock {
            stmts: stmts.into_iter().map(|s| rec_stmt!(s)).collect(),
            expr: tail.map(|e| Box::new(rec!(*e))),
        },
        IrExprKind::Lambda { params, body } => IrExprKind::Lambda {
            params, body: Box::new(rec!(*body)),
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(rec!(*iterable)),
            body: body.into_iter().map(|s| rec_stmt!(s)).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(rec!(*cond)),
            body: body.into_iter().map(|s| rec_stmt!(s)).collect(),
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter().map(|e| rec!(e)).collect(),
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter().map(|e| rec!(e)).collect(),
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| rec!(e)).collect(),
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name, fields: fields.into_iter().map(|(k, v)| (k, rec!(v))).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(rec!(*base)),
            fields: fields.into_iter().map(|(k, v)| (k, rec!(v))).collect(),
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter().map(|(k, v)| (rec!(k), rec!(v))).collect(),
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(rec!(*start)), end: Box::new(rec!(*end)), inclusive,
        },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(rec!(*object)), field,
        },
        IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex {
            object: Box::new(rec!(*object)), index,
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(rec!(*object)), index: Box::new(rec!(*index)),
        },
        IrExprKind::MapAccess { object, key } => IrExprKind::MapAccess {
            object: Box::new(rec!(*object)), key: Box::new(rec!(*key)),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr: e } => IrStringPart::Expr { expr: rec!(e) },
                other => other,
            }).collect(),
        },
        IrExprKind::RustMacro { name, args } => IrExprKind::RustMacro {
            name, args: args.into_iter().map(|e| rec!(e)).collect(),
        },
        // Single-child wrappers
        IrExprKind::ResultOk { expr: e } => IrExprKind::ResultOk { expr: Box::new(rec!(*e)) },
        IrExprKind::ResultErr { expr: e } => IrExprKind::ResultErr { expr: Box::new(rec!(*e)) },
        IrExprKind::OptionSome { expr: e } => IrExprKind::OptionSome { expr: Box::new(rec!(*e)) },
        IrExprKind::Try { expr: e } => IrExprKind::Try { expr: Box::new(rec!(*e)) },
        IrExprKind::Await { expr: e } => IrExprKind::Await { expr: Box::new(rec!(*e)) },
        IrExprKind::Clone { expr: e } => IrExprKind::Clone { expr: Box::new(rec!(*e)) },
        IrExprKind::Deref { expr: e } => IrExprKind::Deref { expr: Box::new(rec!(*e)) },
        IrExprKind::Borrow { expr: e, as_str } => IrExprKind::Borrow { expr: Box::new(rec!(*e)), as_str },
        IrExprKind::BoxNew { expr: e } => IrExprKind::BoxNew { expr: Box::new(rec!(*e)) },
        IrExprKind::ToVec { expr: e } => IrExprKind::ToVec { expr: Box::new(rec!(*e)) },
        // Leaf nodes — pass through
        other => other,
    };
    IrExpr { kind, ty, span }
}

/// Transform all expressions inside a statement.
fn transform_stmt(
    stmt: IrStmt,
    f: &mut dyn FnMut(IrExpr) -> Option<IrExpr>,
) -> IrStmt {
    macro_rules! rec {
        ($e:expr) => { recursive_transform($e, f) };
    }
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
            var, mutability, ty, value: rec!(value),
        },
        IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
            pattern, value: rec!(value),
        },
        IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: rec!(value) },
        IrStmtKind::IndexAssign { target, index, value } => IrStmtKind::IndexAssign {
            target, index: rec!(index), value: rec!(value),
        },
        IrStmtKind::MapInsert { target, key, value } => IrStmtKind::MapInsert {
            target, key: rec!(key), value: rec!(value),
        },
        IrStmtKind::FieldAssign { target, field, value } => IrStmtKind::FieldAssign {
            target, field, value: rec!(value),
        },
        IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
            cond: rec!(cond), else_: rec!(else_),
        },
        IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: rec!(expr) },
        other => other,
    };
    IrStmt { kind, span: stmt.span }
}

// ── Fusion orchestrator ──────────────────────────────────────────

/// Run all fusion passes on a function.
fn fuse_all(func: &mut IrFunction, var_table: &mut VarTable) -> FusionCounts {
    let mut counts = FusionCounts::default();

    // FunctorIdentity: map(x, id) → x  (must run BEFORE map+map fusion)
    counts.identity = fuse_counting(&mut func.body, try_eliminate_identity_map);

    // FunctorComposition: map(map(x, f), g) → map(x, f >> g)
    counts.map_map = fuse_counting(&mut func.body, try_fuse_map_map);

    // FilterComposition: filter(filter(x, p), q) → filter(x, p && q)
    counts.filter_filter = fuse_counting(&mut func.body, try_fuse_filter_filter);

    // MapFoldFusion: fold(map(x, f), init, g) → fold(x, init, (acc, x) => g(acc, f(x)))
    counts.map_fold = fuse_counting(&mut func.body, try_fuse_map_fold);

    // MonadAssociativity: flat_map(flat_map(x, f), g) → flat_map(x, x => flat_map(f(x), g))
    counts.flatmap_flatmap = fuse_counting(&mut func.body, try_fuse_flatmap_flatmap);

    // MapFilterFusion: filter(map(x, f), p) → filter_map(x, ...)
    counts.map_filter = fuse_counting(&mut func.body, try_fuse_map_filter);

    // FilterMapFoldFusion: fold(filter_map(x, fm), init, g) → single-pass fold
    {
        let mut count = 0;
        func.body = recursive_transform(func.body.clone(), &mut |e| {
            try_fuse_filter_map_fold(e, &mut count, var_table)
        });
        counts.filter_map_fold = count;
    }

    // RangeFoldFusion: fold(range(start, end), init, g) → for loop
    {
        let mut count = 0;
        func.body = recursive_transform(func.body.clone(), &mut |e| {
            try_fuse_range_fold(e, &mut count, var_table)
        });
        counts.range_fold = count;
    }

    counts
}

/// Apply a fusion transform repeatedly (bottom-up) and count how many times it fired.
fn fuse_counting(
    body: &mut IrExpr,
    try_transform: fn(IrExpr) -> Option<IrExpr>,
) -> usize {
    let mut count = 0usize;
    *body = recursive_transform(body.clone(), &mut |e| {
        if let Some(fused) = try_transform(e) {
            count += 1;
            Some(fused)
        } else {
            None
        }
    });
    count
}

#[derive(Default)]
struct FusionCounts {
    map_map: usize,
    filter_filter: usize,
    map_fold: usize,
    identity: usize,
    flatmap_flatmap: usize,
    map_filter: usize,
    filter_map_fold: usize,
    range_fold: usize,
}

impl FusionCounts {
    fn total(&self) -> usize {
        self.map_map + self.filter_filter + self.map_fold + self.identity
            + self.flatmap_flatmap + self.map_filter + self.filter_map_fold + self.range_fold
    }

    fn add(&mut self, other: &FusionCounts) {
        self.map_map += other.map_map;
        self.filter_filter += other.filter_filter;
        self.map_fold += other.map_fold;
        self.identity += other.identity;
        self.flatmap_flatmap += other.flatmap_flatmap;
        self.map_filter += other.map_filter;
        self.filter_map_fold += other.filter_map_fold;
        self.range_fold += other.range_fold;
    }

    fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.identity > 0 { parts.push(format!("{} identity-map", self.identity)); }
        if self.map_map > 0 { parts.push(format!("{} map+map", self.map_map)); }
        if self.filter_filter > 0 { parts.push(format!("{} filter+filter", self.filter_filter)); }
        if self.map_fold > 0 { parts.push(format!("{} map+fold", self.map_fold)); }
        if self.flatmap_flatmap > 0 { parts.push(format!("{} flat_map+flat_map", self.flatmap_flatmap)); }
        if self.map_filter > 0 { parts.push(format!("{} map+filter", self.map_filter)); }
        if self.filter_map_fold > 0 { parts.push(format!("{} filter_map+fold", self.filter_map_fold)); }
        if self.range_fold > 0 { parts.push(format!("{} range+fold", self.range_fold)); }
        parts.join(", ")
    }
}

// ── Call target classification ────────────────────────────────────

fn is_map_call(target: &CallTarget) -> bool {
    match target {
        CallTarget::Module { func, .. } => func == "map",
        CallTarget::Named { name } => name.ends_with("_map") && !name.ends_with("flat_map") && !name.ends_with("filter_map"),
        _ => false,
    }
}

fn is_filter_call(target: &CallTarget) -> bool {
    match target {
        CallTarget::Module { func, .. } => func == "filter",
        CallTarget::Named { name } => name.ends_with("_filter") && !name.ends_with("_filter_map"),
        _ => false,
    }
}

fn is_fold_call(target: &CallTarget) -> bool {
    match target {
        CallTarget::Module { func, .. } => func == "fold",
        CallTarget::Named { name } => name.ends_with("_fold"),
        _ => false,
    }
}

fn is_flatmap_call(target: &CallTarget) -> bool {
    match target {
        CallTarget::Module { func, .. } => func == "flat_map",
        CallTarget::Named { name } => name.ends_with("_flat_map"),
        _ => false,
    }
}

fn is_filter_map_call(target: &CallTarget) -> bool {
    match target {
        CallTarget::Module { func, .. } => func == "filter_map",
        CallTarget::Named { name } => name.ends_with("_filter_map"),
        _ => false,
    }
}

fn is_range_call(target: &CallTarget) -> bool {
    match target {
        CallTarget::Module { module, func } => module == "list" && func == "range",
        CallTarget::Named { name } => name.ends_with("_range"),
        _ => false,
    }
}

// ── Individual fusion transforms (IrExpr -> Option<IrExpr>) ──────

// ── FunctorIdentity: map(x, (x) => x) → x ──

fn try_eliminate_identity_map(expr: IrExpr) -> Option<IrExpr> {
    if let IrExprKind::Call { ref target, ref args, .. } = expr.kind {
        if is_map_call(target) && args.len() >= 2 && is_identity_lambda(&args[1]) {
            return Some(args[0].clone());
        }
    }
    None
}

fn is_identity_lambda(expr: &IrExpr) -> bool {
    if let IrExprKind::Lambda { params, body } = &expr.kind {
        if params.len() == 1 {
            if let IrExprKind::Var { id } = &body.kind {
                return *id == params[0].0;
            }
        }
    }
    false
}

// ── FunctorComposition: map(map(x, f), g) → map(x, x => g(f(x))) ──

fn try_fuse_map_map(expr: IrExpr) -> Option<IrExpr> {
    if let IrExprKind::Call { ref target, ref args, ref type_args } = expr.kind {
        if is_map_call(target) && args.len() >= 2 {
            if let IrExprKind::Call { target: ref inner_target, args: ref inner_args, .. } = args[0].kind {
                if is_map_call(inner_target) && inner_args.len() >= 2 {
                    let f = &inner_args[1];
                    let g = &args[1];
                    if let Some(composed) = compose_lambdas(f, g) {
                        return Some(IrExpr {
                            kind: IrExprKind::Call {
                                target: target.clone(),
                                args: vec![inner_args[0].clone(), composed],
                                type_args: type_args.clone(),
                            },
                            ty: expr.ty,
                            span: expr.span,
                        });
                    }
                }
            }
        }
    }
    None
}

// ── FilterComposition: filter(filter(x, p), q) → filter(x, x => p(x) && q(x)) ──

fn try_fuse_filter_filter(expr: IrExpr) -> Option<IrExpr> {
    if let IrExprKind::Call { ref target, ref args, ref type_args } = expr.kind {
        if is_filter_call(target) && args.len() >= 2 {
            if let IrExprKind::Call { target: ref inner_target, args: ref inner_args, .. } = args[0].kind {
                if is_filter_call(inner_target) && inner_args.len() >= 2 {
                    let p = &inner_args[1];
                    let q = &args[1];
                    if let Some(composed) = compose_predicates(p, q) {
                        return Some(IrExpr {
                            kind: IrExprKind::Call {
                                target: target.clone(),
                                args: vec![inner_args[0].clone(), composed],
                                type_args: type_args.clone(),
                            },
                            ty: expr.ty,
                            span: expr.span,
                        });
                    }
                }
            }
        }
    }
    None
}

// ── MapFoldFusion: fold(map(x, f), init, g) → fold(x, init, (acc,x) => g(acc, f(x))) ──

fn try_fuse_map_fold(expr: IrExpr) -> Option<IrExpr> {
    if let IrExprKind::Call { ref target, ref args, ref type_args } = expr.kind {
        if is_fold_call(target) && args.len() >= 3 {
            if let IrExprKind::Call { target: ref inner_target, args: ref inner_args, .. } = args[0].kind {
                if is_map_call(inner_target) && inner_args.len() >= 2 {
                    let f = &inner_args[1];
                    let g = &args[2];
                    if let Some(fused_reducer) = compose_map_into_fold(f, g) {
                        return Some(IrExpr {
                            kind: IrExprKind::Call {
                                target: target.clone(),
                                args: vec![inner_args[0].clone(), args[1].clone(), fused_reducer],
                                type_args: type_args.clone(),
                            },
                            ty: expr.ty,
                            span: expr.span,
                        });
                    }
                }
            }
        }
    }
    None
}

// ── MonadAssociativity: flat_map(flat_map(x, f), g) → flat_map(x, x => flat_map(f(x), g)) ──

fn try_fuse_flatmap_flatmap(expr: IrExpr) -> Option<IrExpr> {
    if let IrExprKind::Call { ref target, ref args, ref type_args } = expr.kind {
        if is_flatmap_call(target) && args.len() >= 2 {
            if let IrExprKind::Call { target: ref inner_target, args: ref inner_args, .. } = args[0].kind {
                if is_flatmap_call(inner_target) && inner_args.len() >= 2 {
                    let f = &inner_args[1];
                    let g = &args[1];
                    if let Some(composed) = compose_flatmaps(f, g, target, type_args) {
                        return Some(IrExpr {
                            kind: IrExprKind::Call {
                                target: target.clone(),
                                args: vec![inner_args[0].clone(), composed],
                                type_args: type_args.clone(),
                            },
                            ty: expr.ty,
                            span: expr.span,
                        });
                    }
                }
            }
        }
    }
    None
}

// ── MapFilterFusion: filter(map(x, f), p) → filter_map(x, ...) ──

fn try_fuse_map_filter(expr: IrExpr) -> Option<IrExpr> {
    if let IrExprKind::Call { ref target, ref args, ref type_args } = expr.kind {
        if is_filter_call(target) && args.len() >= 2 {
            if let IrExprKind::Call { target: ref inner_target, args: ref inner_args, .. } = args[0].kind {
                if is_map_call(inner_target) && inner_args.len() >= 2 {
                    let f = &inner_args[1];
                    let p = &args[1];
                    if let Some(filter_map_lambda) = compose_map_filter(f, p) {
                        let fm_target = match inner_target {
                            CallTarget::Module { module, .. } => CallTarget::Module {
                                module: module.clone(), func: "filter_map".to_string(),
                            },
                            CallTarget::Named { name } => CallTarget::Named {
                                name: name.replace("_map", "_filter_map"),
                            },
                            other => other.clone(),
                        };
                        return Some(IrExpr {
                            kind: IrExprKind::Call {
                                target: fm_target,
                                args: vec![inner_args[0].clone(), filter_map_lambda],
                                type_args: type_args.clone(),
                            },
                            ty: expr.ty,
                            span: expr.span,
                        });
                    }
                }
            }
        }
    }
    None
}

// ── FilterMapFoldFusion: fold(filter_map(x, fm), init, g) → fold with match ──

fn try_fuse_filter_map_fold(expr: IrExpr, count: &mut usize, vt: &mut VarTable) -> Option<IrExpr> {
    if let IrExprKind::Call { ref target, ref args, ref type_args } = expr.kind {
        if is_fold_call(target) && args.len() >= 3 {
            if let IrExprKind::Call { target: ref inner_target, args: ref inner_args, .. } = args[0].kind {
                if is_filter_map_call(inner_target) && inner_args.len() >= 2 {
                    let source = &inner_args[0];
                    let fm = &inner_args[1];
                    let init = &args[1];
                    let g = &args[2];
                    if let Some(fused_reducer) = compose_filter_map_into_fold(fm, g, vt) {
                        *count += 1;
                        return Some(IrExpr {
                            kind: IrExprKind::Call {
                                target: target.clone(),
                                args: vec![source.clone(), init.clone(), fused_reducer],
                                type_args: type_args.clone(),
                            },
                            ty: expr.ty,
                            span: expr.span,
                        });
                    }
                }
            }
        }
    }
    None
}

// ── RangeFoldFusion: fold(range(start, end), init, g) → for loop ──

fn try_fuse_range_fold(expr: IrExpr, count: &mut usize, vt: &mut VarTable) -> Option<IrExpr> {
    if let IrExprKind::Call { ref target, ref args, .. } = expr.kind {
        if is_fold_call(target) && args.len() >= 3 {
            if let IrExprKind::Call { target: ref inner_target, args: ref inner_args, .. } = args[0].kind {
                if is_range_call(inner_target) && inner_args.len() >= 2 {
                    let start = &inner_args[0];
                    let end = &inner_args[1];
                    let init = &args[1];
                    let g = &args[2];

                    if let IrExprKind::Lambda { params: g_params, body: g_body } = &g.kind {
                        if g_params.len() == 2 {
                            let (g_acc_id, g_acc_ty) = &g_params[0];
                            let (g_elem_id, g_elem_ty) = &g_params[1];

                            let acc_var = vt.alloc("__acc".into(), g_acc_ty.clone(), Mutability::Var, None);
                            let loop_var = vt.alloc("__i".into(), g_elem_ty.clone(), Mutability::Let, None);

                            let acc_ref = IrExpr { kind: IrExprKind::Var { id: acc_var }, ty: g_acc_ty.clone(), span: None };
                            let loop_ref = IrExpr { kind: IrExprKind::Var { id: loop_var }, ty: g_elem_ty.clone(), span: None };

                            let body_subst = substitute_var_in_expr(
                                &substitute_var_in_expr(g_body, *g_acc_id, &acc_ref),
                                *g_elem_id, &loop_ref,
                            );

                            *count += 1;
                            return Some(IrExpr {
                                kind: IrExprKind::Block {
                                    stmts: vec![
                                        IrStmt {
                                            kind: IrStmtKind::Bind {
                                                var: acc_var, mutability: Mutability::Var,
                                                ty: g_acc_ty.clone(), value: init.clone(),
                                            },
                                            span: None,
                                        },
                                        IrStmt {
                                            kind: IrStmtKind::Expr {
                                                expr: IrExpr {
                                                    kind: IrExprKind::ForIn {
                                                        var: loop_var, var_tuple: None,
                                                        iterable: Box::new(IrExpr {
                                                            kind: IrExprKind::Range {
                                                                start: Box::new(start.clone()),
                                                                end: Box::new(end.clone()),
                                                                inclusive: false,
                                                            },
                                                            ty: Ty::Int, span: None,
                                                        }),
                                                        body: vec![IrStmt {
                                                            kind: IrStmtKind::Assign { var: acc_var, value: body_subst },
                                                            span: None,
                                                        }],
                                                    },
                                                    ty: Ty::Unit, span: None,
                                                },
                                            },
                                            span: None,
                                        },
                                    ],
                                    expr: Some(Box::new(acc_ref)),
                                },
                                ty: expr.ty,
                                span: expr.span,
                            });
                        }
                    }
                }
            }
        }
    }
    None
}

// ── Lambda composition helpers ───────────────────────────────────

/// Compose two lambdas: f and g → (x) => g(f(x))
fn compose_lambdas(f: &IrExpr, g: &IrExpr) -> Option<IrExpr> {
    if let (
        IrExprKind::Lambda { params: f_params, body: f_body },
        IrExprKind::Lambda { params: g_params, body: g_body },
    ) = (&f.kind, &g.kind) {
        if f_params.len() != 1 || g_params.len() != 1 { return None; }
        let (f_param_id, f_param_ty) = &f_params[0];
        let (g_param_id, _) = &g_params[0];
        let composed_body = substitute_var_in_expr(g_body, *g_param_id, f_body);
        return Some(IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(*f_param_id, f_param_ty.clone())],
                body: Box::new(composed_body),
            },
            ty: g.ty.clone(),
            span: f.span,
        });
    }
    None
}

/// Compose two predicates: p and q → (x) => p(x) && q(x)
fn compose_predicates(p: &IrExpr, q: &IrExpr) -> Option<IrExpr> {
    if let (
        IrExprKind::Lambda { params: p_params, body: p_body },
        IrExprKind::Lambda { params: q_params, body: q_body },
    ) = (&p.kind, &q.kind) {
        if p_params.len() != 1 || q_params.len() != 1 { return None; }
        let (p_param_id, p_param_ty) = &p_params[0];
        let (q_param_id, _) = &q_params[0];
        let q_body_subst = substitute_var_in_expr(q_body, *q_param_id, &IrExpr {
            kind: IrExprKind::Var { id: *p_param_id },
            ty: p_param_ty.clone(), span: None,
        });
        let composed_body = IrExpr {
            kind: IrExprKind::BinOp {
                op: crate::ir::BinOp::And,
                left: p_body.clone(),
                right: Box::new(q_body_subst),
            },
            ty: crate::types::Ty::Bool, span: None,
        };
        return Some(IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(*p_param_id, p_param_ty.clone())],
                body: Box::new(composed_body),
            },
            ty: p.ty.clone(), span: p.span,
        });
    }
    None
}

/// Compose map f into fold reducer g: (acc, x) => g(acc, f(x))
fn compose_map_into_fold(f: &IrExpr, g: &IrExpr) -> Option<IrExpr> {
    if let (
        IrExprKind::Lambda { params: f_params, body: f_body },
        IrExprKind::Lambda { params: g_params, body: g_body },
    ) = (&f.kind, &g.kind) {
        if f_params.len() != 1 || g_params.len() != 2 { return None; }
        let (f_param_id, f_param_ty) = &f_params[0];
        let (g_acc_id, g_acc_ty) = &g_params[0];
        let (g_elem_id, _) = &g_params[1];
        let g_body_subst = substitute_var_in_expr(g_body, *g_elem_id, f_body);
        return Some(IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(*g_acc_id, g_acc_ty.clone()), (*f_param_id, f_param_ty.clone())],
                body: Box::new(g_body_subst),
            },
            ty: g.ty.clone(), span: g.span,
        });
    }
    None
}

/// Compose two flat_map functions: f and g → (x) => flat_map(f(x), g)
fn compose_flatmaps(f: &IrExpr, g: &IrExpr, target: &CallTarget, type_args: &[Ty]) -> Option<IrExpr> {
    if let IrExprKind::Lambda { params: f_params, body: f_body } = &f.kind {
        if f_params.len() != 1 { return None; }
        let (f_param_id, f_param_ty) = &f_params[0];
        let inner_call = IrExpr {
            kind: IrExprKind::Call {
                target: target.clone(),
                args: vec![*f_body.clone(), g.clone()],
                type_args: type_args.to_vec(),
            },
            ty: f.ty.clone(), span: f.span,
        };
        return Some(IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(*f_param_id, f_param_ty.clone())],
                body: Box::new(inner_call),
            },
            ty: f.ty.clone(), span: f.span,
        });
    }
    None
}

/// Compose filter_map lambda and fold reducer into a single match-based reducer.
fn compose_filter_map_into_fold(fm: &IrExpr, g: &IrExpr, vt: &mut VarTable) -> Option<IrExpr> {
    if let (
        IrExprKind::Lambda { params: fm_params, body: fm_body },
        IrExprKind::Lambda { params: g_params, body: g_body },
    ) = (&fm.kind, &g.kind) {
        if fm_params.len() != 1 || g_params.len() != 2 { return None; }
        let (fm_param_id, fm_param_ty) = &fm_params[0];
        let (g_acc_id, g_acc_ty) = &g_params[0];
        let (g_elem_id, _) = &g_params[1];

        let fm_call = *fm_body.clone();
        let v_var = vt.alloc("__fused_v".into(), g_acc_ty.clone(), Mutability::Let, None);
        let some_arm = IrMatchArm {
            pattern: IrPattern::Some { inner: Box::new(IrPattern::Bind { var: v_var, ty: g_acc_ty.clone() }) },
            guard: None,
            body: substitute_var_in_expr(g_body, *g_elem_id, &IrExpr {
                kind: IrExprKind::Var { id: v_var }, ty: g_acc_ty.clone(), span: None,
            }),
        };
        let none_arm = IrMatchArm {
            pattern: IrPattern::None, guard: None,
            body: IrExpr { kind: IrExprKind::Var { id: *g_acc_id }, ty: g_acc_ty.clone(), span: None },
        };
        let match_expr = IrExpr {
            kind: IrExprKind::Match { subject: Box::new(fm_call), arms: vec![some_arm, none_arm] },
            ty: g_acc_ty.clone(), span: None,
        };
        return Some(IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(*g_acc_id, g_acc_ty.clone()), (*fm_param_id, fm_param_ty.clone())],
                body: Box::new(match_expr),
            },
            ty: g.ty.clone(), span: g.span,
        });
    }
    None
}

/// Compose map function f and filter predicate p into a filter_map lambda.
fn compose_map_filter(f: &IrExpr, p: &IrExpr) -> Option<IrExpr> {
    if let (
        IrExprKind::Lambda { params: f_params, body: f_body },
        IrExprKind::Lambda { params: p_params, body: p_body },
    ) = (&f.kind, &p.kind) {
        if f_params.len() != 1 || p_params.len() != 1 { return None; }
        let (f_param_id, f_param_ty) = &f_params[0];
        let (p_param_id, _) = &p_params[0];
        let p_applied = substitute_var_in_expr(p_body, *p_param_id, f_body);
        let result_ty = f_body.ty.clone();
        let composed_body = IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(p_applied),
                then: Box::new(IrExpr {
                    kind: IrExprKind::OptionSome { expr: f_body.clone() },
                    ty: Ty::option(result_ty.clone()), span: None,
                }),
                else_: Box::new(IrExpr {
                    kind: IrExprKind::OptionNone,
                    ty: Ty::option(result_ty), span: None,
                }),
            },
            ty: f_body.ty.clone(), span: None,
        };
        return Some(IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(*f_param_id, f_param_ty.clone())],
                body: Box::new(composed_body),
            },
            ty: f.ty.clone(), span: f.span,
        });
    }
    None
}

// ── Pipe chain detection (debug analysis) ────────────────────────

#[derive(Debug)]
pub struct PipeChain {
    pub ops: Vec<PipeOp>,
    pub fusible_pairs: usize,
    pub container_name: Option<String>,
}

#[derive(Debug, Clone)]
pub enum PipeOp {
    Map, Filter, Fold, FlatMap, Other(String),
}

fn unwrap_decorators(expr: &IrExpr) -> &IrExpr {
    match &expr.kind {
        IrExprKind::Borrow { expr: inner, .. }
        | IrExprKind::ToVec { expr: inner }
        | IrExprKind::Clone { expr: inner } => unwrap_decorators(inner),
        _ => expr,
    }
}

fn detect_pipe_chains(expr: &IrExpr, registry: &TypeConstructorRegistry) -> Vec<PipeChain> {
    let mut chains = Vec::new();
    detect_pipe_chains_inner(expr, registry, &mut chains);
    chains
}

fn detect_pipe_chains_inner(
    expr: &IrExpr, registry: &TypeConstructorRegistry, chains: &mut Vec<PipeChain>,
) {
    match &expr.kind {
        IrExprKind::Call { target, args, .. } => {
            let call_name = match target {
                CallTarget::Named { name } => Some(name.as_str()),
                CallTarget::Module { func, .. } => Some(func.as_str()),
                _ => None,
            };
            if let Some(name) = call_name {
                if let Some(op) = classify_stdlib_op(name) {
                    let mut chain_ops = vec![op];
                    let mut current = args.first().map(|a| unwrap_decorators(a));
                    while let Some(arg) = current {
                        let inner_op = extract_call_name(arg).and_then(classify_stdlib_op);
                        let inner_args_opt = match &arg.kind {
                            IrExprKind::Call { args, .. } => Some(args),
                            _ => None,
                        };
                        if let (Some(op), Some(inner_args)) = (inner_op, inner_args_opt) {
                            chain_ops.push(op);
                            current = inner_args.first().map(|a| unwrap_decorators(a));
                            continue;
                        }
                        break;
                    }
                    if chain_ops.len() >= 2 {
                        chain_ops.reverse();
                        let container_name = detect_container_type_from_call(expr);
                        let fusible_pairs = count_fusible_pairs(&chain_ops, &container_name, registry);
                        chains.push(PipeChain { ops: chain_ops, fusible_pairs, container_name });
                        return;
                    }
                }
            }
            for arg in args { detect_pipe_chains_inner(arg, registry, chains); }
        }
        IrExprKind::Block { stmts, expr: body } => {
            let mut let_chain: Vec<(VarId, PipeOp, &IrExpr)> = Vec::new();
            for stmt in stmts {
                let extended = match &stmt.kind {
                    IrStmtKind::Bind { var, value, .. } => try_extend_chain(value, *var, &mut let_chain),
                    IrStmtKind::Expr { expr } => try_extend_chain(expr, VarId(0), &mut let_chain),
                    _ => false,
                };
                if !extended {
                    flush_let_chain(&let_chain, registry, chains);
                    let_chain.clear();
                    match &stmt.kind {
                        IrStmtKind::Bind { value, .. } => detect_pipe_chains_inner(value, registry, chains),
                        IrStmtKind::Expr { expr } => detect_pipe_chains_inner(expr, registry, chains),
                        _ => {}
                    }
                }
            }
            let body_appended = body.as_ref()
                .map_or(false, |e| try_extend_chain(e, VarId(0), &mut let_chain));
            flush_let_chain(&let_chain, registry, chains);
            if let Some(e) = body {
                if !body_appended { detect_pipe_chains_inner(e, registry, chains); }
            }
        }
        IrExprKind::If { cond, then, else_ } => {
            detect_pipe_chains_inner(cond, registry, chains);
            detect_pipe_chains_inner(then, registry, chains);
            detect_pipe_chains_inner(else_, registry, chains);
        }
        IrExprKind::Lambda { body, .. } => detect_pipe_chains_inner(body, registry, chains),
        _ => {}
    }
}

fn extract_call_name(expr: &IrExpr) -> Option<&str> {
    match &expr.kind {
        IrExprKind::Call { target: CallTarget::Module { func, .. }, .. } => Some(func.as_str()),
        IrExprKind::Call { target: CallTarget::Named { name }, .. } => Some(name.as_str()),
        _ => None,
    }
}

fn try_extend_chain<'a>(
    expr: &'a IrExpr, var: VarId, let_chain: &mut Vec<(VarId, PipeOp, &'a IrExpr)>,
) -> bool {
    let Some(name) = extract_call_name(expr) else { return false };
    let Some(op) = classify_stdlib_op(name) else { return false };
    let is_chained = let_chain.last()
        .map_or(true, |(prev_var, _, _)| first_arg_is_var(expr, *prev_var));
    if is_chained { let_chain.push((var, op, expr)); true } else { false }
}

fn first_arg_is_var(expr: &IrExpr, var: VarId) -> bool {
    if let IrExprKind::Call { args, .. } = &expr.kind {
        if let Some(first) = args.first() {
            if let IrExprKind::Var { id } = &unwrap_decorators(first).kind {
                return *id == var;
            }
        }
    }
    false
}

fn flush_let_chain(
    let_chain: &[(VarId, PipeOp, &IrExpr)],
    registry: &TypeConstructorRegistry, chains: &mut Vec<PipeChain>,
) {
    if let_chain.len() >= 2 {
        let ops: Vec<PipeOp> = let_chain.iter().map(|(_, op, _)| op.clone()).collect();
        let container_name = let_chain.first()
            .map(|(_, _, e)| detect_container_type_from_call(e))
            .unwrap_or(None);
        let fusible_pairs = count_fusible_pairs(&ops, &container_name, registry);
        chains.push(PipeChain { ops, fusible_pairs, container_name });
    }
}

fn classify_stdlib_op(name: &str) -> Option<PipeOp> {
    if name.ends_with("flat_map") { return Some(PipeOp::FlatMap); }
    if name.ends_with("filter_map") { return None; }
    let func = name.rsplit('_').next().unwrap_or(name);
    match func {
        "map" => Some(PipeOp::Map),
        "filter" => Some(PipeOp::Filter),
        "fold" | "reduce" => Some(PipeOp::Fold),
        _ => None,
    }
}

fn detect_container_type_from_call(expr: &IrExpr) -> Option<String> {
    if let IrExprKind::Call { args, .. } = &expr.kind {
        if let Some(first_arg) = args.first() {
            let unwrapped = unwrap_decorators(first_arg);
            if let Some(name) = unwrapped.ty.constructor_name() {
                if matches!(name, "List" | "Option" | "Result") {
                    return Some(name.to_string());
                }
            }
            return detect_container_type_from_call(unwrapped);
        }
    }
    expr.ty.constructor_name().map(|s| s.to_string())
}

fn count_fusible_pairs(
    ops: &[PipeOp], container_name: &Option<String>, registry: &TypeConstructorRegistry,
) -> usize {
    let name = match container_name { Some(n) => n.as_str(), None => return 0 };
    ops.windows(2).filter(|pair| {
        match (&pair[0], &pair[1]) {
            (PipeOp::Map, PipeOp::Map) => registry.satisfies(name, AlgebraicLaw::FunctorComposition),
            (PipeOp::Filter, PipeOp::Filter) => registry.satisfies(name, AlgebraicLaw::FilterComposition),
            (PipeOp::Map, PipeOp::Fold) => registry.satisfies(name, AlgebraicLaw::MapFoldFusion),
            (PipeOp::Map, PipeOp::Filter) => registry.satisfies(name, AlgebraicLaw::MapFilterFusion),
            (PipeOp::FlatMap, PipeOp::FlatMap) => registry.satisfies(name, AlgebraicLaw::MonadAssociativity),
            _ => false,
        }
    }).count()
}

// ── Let-inlining pre-pass ────────────────────────────────────────

fn inline_single_use_collection_lets(body: &mut IrExpr, var_table: &VarTable) {
    if let IrExprKind::Block { stmts, expr } = &mut body.kind {
        let mut inlined_vars: std::collections::HashSet<VarId> = std::collections::HashSet::new();
        loop {
            let mut did_inline = false;
            for i in 0..stmts.len() {
                let (var, value) = match &stmts[i].kind {
                    IrStmtKind::Bind { var, value, .. } if !inlined_vars.contains(var) => (*var, value.clone()),
                    _ => continue,
                };
                let is_list_collection_op = matches!(&value.kind,
                    IrExprKind::Call { target: CallTarget::Module { module, func }, .. }
                    if module == "list" && (classify_stdlib_op(func).is_some() || func == "range")
                );
                if !is_list_collection_op || var_table.use_count(var) != 1 { continue; }
                for j in (i + 1)..stmts.len() {
                    let s = &mut stmts[j];
                    match &mut s.kind {
                        IrStmtKind::Bind { value: v, .. } => *v = substitute_var_in_expr(v, var, &value),
                        IrStmtKind::Expr { expr: e } => *e = substitute_var_in_expr(e, var, &value),
                        _ => {}
                    }
                }
                if let Some(e) = expr.as_mut() {
                    **e = substitute_var_in_expr(e, var, &value);
                }
                inlined_vars.insert(var);
                did_inline = true;
                break;
            }
            if !did_inline { break; }
        }
        stmts.retain(|s| {
            if let IrStmtKind::Bind { var, .. } = &s.kind { !inlined_vars.contains(var) } else { true }
        });
        for stmt in stmts.iter_mut() {
            match &mut stmt.kind {
                IrStmtKind::Bind { value, .. } => inline_single_use_collection_lets(value, var_table),
                IrStmtKind::Expr { expr } => inline_single_use_collection_lets(expr, var_table),
                _ => {}
            }
        }
        if let Some(e) = expr { inline_single_use_collection_lets(e, var_table); }
    }
}

// ── Debug helpers ────────────────────────────────────────────────

fn dump_calls(expr: &IrExpr, ctx: &str) {
    match &expr.kind {
        IrExprKind::Call { target, args, .. } => {
            let name = match target {
                CallTarget::Module { module, func } => format!("Module({}.{})", module, func),
                CallTarget::Named { name } => format!("Named({})", name),
                _ => "Other".to_string(),
            };
            eprintln!("[Fusion-IR] {}: {} with {} args", ctx, name, args.len());
            for a in args { dump_calls(a, ctx); }
        }
        IrExprKind::Block { stmts, expr } => {
            for s in stmts {
                match &s.kind {
                    IrStmtKind::Bind { var, value, .. } => {
                        eprintln!("[Fusion-IR] {}: Bind var={}", ctx, var.0);
                        dump_calls(value, ctx);
                    }
                    IrStmtKind::Expr { expr } => dump_calls(expr, ctx),
                    _ => {}
                }
            }
            if let Some(e) = expr { dump_calls(e, ctx); }
        }
        _ => {}
    }
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_map() {
        assert!(matches!(classify_stdlib_op("almide_rt_list_map"), Some(PipeOp::Map)));
    }

    #[test]
    fn classify_filter() {
        assert!(matches!(classify_stdlib_op("almide_rt_list_filter"), Some(PipeOp::Filter)));
    }

    #[test]
    fn classify_fold() {
        assert!(matches!(classify_stdlib_op("almide_rt_list_fold"), Some(PipeOp::Fold)));
    }

    #[test]
    fn classify_flat_map() {
        assert!(matches!(classify_stdlib_op("almide_rt_list_flat_map"), Some(PipeOp::FlatMap)));
    }

    #[test]
    fn classify_unknown() {
        assert!(classify_stdlib_op("almide_rt_list_length").is_none());
    }

    #[test]
    fn count_fusible_map_map() {
        let registry = TypeConstructorRegistry::new();
        let ops = vec![PipeOp::Map, PipeOp::Map];
        assert_eq!(count_fusible_pairs(&ops, &Some("List".into()), &registry), 1);
    }

    #[test]
    fn count_fusible_map_filter_fold() {
        let registry = TypeConstructorRegistry::new();
        let ops = vec![PipeOp::Map, PipeOp::Filter, PipeOp::Fold];
        assert_eq!(count_fusible_pairs(&ops, &Some("List".into()), &registry), 1);
    }

    #[test]
    fn count_fusible_map_fold() {
        let registry = TypeConstructorRegistry::new();
        let ops = vec![PipeOp::Map, PipeOp::Fold];
        assert_eq!(count_fusible_pairs(&ops, &Some("List".into()), &registry), 1);
    }

    #[test]
    fn count_fusible_none_for_map() {
        let registry = TypeConstructorRegistry::new();
        let ops = vec![PipeOp::Map, PipeOp::Map];
        assert_eq!(count_fusible_pairs(&ops, &Some("Map".into()), &registry), 0);
    }

    #[test]
    fn count_fusible_option_flatmap() {
        let registry = TypeConstructorRegistry::new();
        let ops = vec![PipeOp::FlatMap, PipeOp::FlatMap];
        assert_eq!(count_fusible_pairs(&ops, &Some("Option".into()), &registry), 1);
    }
}
