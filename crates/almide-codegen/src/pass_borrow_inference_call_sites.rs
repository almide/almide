// ── Phase 2: Insert Borrow nodes at call sites ─────────────────────

pub fn insert_borrows_at_call_sites(program: &mut IrProgram, sigs: &HashMap<String, Vec<ParamBorrow>>) {
    for func in &mut program.functions {
        func.body = rewrite_calls(std::mem::take(&mut func.body), sigs, None);
    }
    for tl in &mut program.top_lets {
        tl.value = rewrite_calls(std::mem::take(&mut tl.value), sigs, None);
    }
    for module in &mut program.modules {
        let mod_name = module.name.to_string();
        for func in &mut module.functions {
            func.body = rewrite_calls(std::mem::take(&mut func.body), sigs, Some(&mod_name));
        }
        for tl in &mut module.top_lets {
            tl.value = rewrite_calls(std::mem::take(&mut tl.value), sigs, Some(&mod_name));
        }
    }
}

/// Wrap `arg` in a `Borrow` node per `borrows`'s `ParamBorrow` decision (or
/// leave it untouched on `None`/`Move`). Shared by the `args` borrow-wrap
/// loops in [`rewrite_calls_call`] and [`rewrite_calls_runtime_call`] — the
/// three non-move `ParamBorrow` variants (`Ref`/`RefSlice`, `RefMut`,
/// `RefStr`) each produce the same `Borrow` shape, just with different
/// `as_str`/`mutable` flags.
fn wrap_borrowed_arg(arg: IrExpr, borrow: Option<&ParamBorrow>) -> IrExpr {
    match borrow {
        Some(ParamBorrow::Ref | ParamBorrow::RefSlice) => {
            let t = arg.ty.clone(); let s = arg.span;
            IrExpr { kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: false, mutable: false }, ty: t, span: s, def_id: None }
        }
        Some(ParamBorrow::RefMut) => {
            let t = arg.ty.clone(); let s = arg.span;
            IrExpr { kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: false, mutable: true }, ty: t, span: s, def_id: None }
        }
        Some(ParamBorrow::RefStr) => {
            let t = arg.ty.clone(); let s = arg.span;
            IrExpr { kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: true, mutable: false }, ty: t, span: s, def_id: None }
        }
        _ => arg,
    }
}

/// `Call { target, args, type_args }` arm of [`rewrite_calls`].
/// Borrow-wrap a `Call`'s already-rewritten `args` per the callee's
/// signature, if any. `is_method_with_self` offsets the lookup by 1: the
/// sig's param list starts at the receiver (spliced in by the walker ahead
/// of `args`), so the IR `args` align to params 1..N, not 0..N.
fn wrap_call_args_for_target(args: Vec<IrExpr>, target: &CallTarget, sigs: &HashMap<String, Vec<ParamBorrow>>, mod_scope: Option<&str>) -> Vec<IrExpr> {
    let (callee_name, is_method_with_self) = match target {
        CallTarget::Named { name } => (Some(name.to_string()), false),
        CallTarget::Module { module, func, .. } => (Some(format!("{}::{}", module, func)), false),
        CallTarget::Method { method, .. } if method.contains('.') => (Some(method.to_string()), true),
        _ => (None, false),
    };
    let Some(name) = callee_name else { return args; };
    // ALMIDE_DBG_BORROW=<substr>: name the two keys this call site consults
    // and what each resolved to (companion of the per-iteration sig dump).
    if let Some(filter) = almide_base::env::var("ALMIDE_DBG_BORROW")
        && name.contains(&filter)
    {
        let direct = sigs.get(&name).cloned();
        let scoped = mod_scope.and_then(|m| sigs.get(&format!("{}::{}", m, name))).cloned();
        eprintln!("[borrow callsite] name={name} scope={mod_scope:?} scoped={scoped:?} direct={direct:?}");
    }
    // For module-scoped calls, look up with "module::func" key first
    let borrows = mod_scope
        .and_then(|m| sigs.get(&format!("{}::{}", m, name)))
        .or_else(|| sigs.get(&name));
    let Some(borrows) = borrows else { return args; };
    let arg_offset = if is_method_with_self { 1 } else { 0 };
    args.into_iter().enumerate()
        .map(|(i, arg)| wrap_borrowed_arg(arg, borrows.get(i + arg_offset)))
        .collect()
}

/// `Method { object, method }` `CallTarget` case of [`rewrite_calls_call`]:
/// a UFCS-dotted method (`"module.func"`) borrow-wraps its receiver per the
/// method's first declared param, same as [`wrap_borrowed_arg`] minus the
/// `RefMut` case (a receiver is never borrow-wrapped mutable here).
fn rewrite_calls_method_target(object: IrExpr, method: Sym, sigs: &HashMap<String, Vec<ParamBorrow>>, mod_scope: Option<&str>) -> CallTarget {
    let mut obj = rewrite_calls(object, sigs, mod_scope);
    if method.contains('.') {
        if let Some(borrows) = sigs.get(method.as_str()) {
            if let Some(b) = borrows.first() {
                match b {
                    ParamBorrow::Ref | ParamBorrow::RefSlice => {
                        let t = obj.ty.clone(); let s = obj.span;
                        obj = IrExpr { kind: IrExprKind::Borrow { expr: Box::new(obj), as_str: false, mutable: false }, ty: t, span: s, def_id: None };
                    }
                    ParamBorrow::RefStr => {
                        let t = obj.ty.clone(); let s = obj.span;
                        obj = IrExpr { kind: IrExprKind::Borrow { expr: Box::new(obj), as_str: true, mutable: false }, ty: t, span: s, def_id: None };
                    }
                    _ => {}
                }
            }
        }
    }
    CallTarget::Method { object: Box::new(obj), method }
}

fn rewrite_calls_call(target: CallTarget, args: Vec<IrExpr>, type_args: Vec<Ty>, sigs: &HashMap<String, Vec<ParamBorrow>>, mod_scope: Option<&str>) -> IrExprKind {
    let args: Vec<IrExpr> = args.into_iter().map(|a| rewrite_calls(a, sigs, mod_scope)).collect();
    let args = wrap_call_args_for_target(args, &target, sigs, mod_scope);
    let target = match target {
        CallTarget::Method { object, method } => rewrite_calls_method_target(*object, method, sigs, mod_scope),
        CallTarget::Computed { callee } => CallTarget::Computed {
            callee: Box::new(rewrite_calls(*callee, sigs, mod_scope)),
        },
        other => other,
    };
    IrExprKind::Call { target, args, type_args }
}

/// `RuntimeCall { symbol, args }` arm of [`rewrite_calls`]. Looks up the
/// borrow signature by the mangled runtime symbol (populated from bundled
/// `@intrinsic` attrs at the top of `infer_borrow_signatures`). On hit,
/// wraps each arg with the corresponding Borrow IR node; on miss, leaves
/// args untouched (walker still has its ty-based fallback).
fn rewrite_calls_runtime_call(symbol: Sym, args: Vec<IrExpr>, sigs: &HashMap<String, Vec<ParamBorrow>>, mod_scope: Option<&str>) -> IrExprKind {
    let args: Vec<IrExpr> = args.into_iter()
        .map(|a| rewrite_calls(a, sigs, mod_scope))
        .collect();
    let args = if let Some(borrows) = sigs.get(symbol.as_str()) {
        args.into_iter().enumerate()
            .map(|(i, arg)| wrap_borrowed_arg(arg, borrows.get(i)))
            .collect()
    } else { args };
    IrExprKind::RuntimeCall { symbol, args }
}

/// Annotate every call in the tree with its callee's borrow signature.
///
/// Only the kinds that need something OTHER than "rewrite every child" get an
/// arm: the two call forms, the statement-bearing nodes (whose bodies go
/// through [`rewrite_calls_stmt`], which deliberately skips the list-peephole
/// statements), and the nodes this rewriter treats as opaque. Everything else
/// rides `map_children` — the single wildcard-free traversal, so a new
/// `IrExprKind` cannot silently skip its calls.
fn rewrite_calls(expr: IrExpr, sigs: &HashMap<String, Vec<ParamBorrow>>, mod_scope: Option<&str>) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;
    let rebuild = |kind| IrExpr { kind, ty: ty.clone(), span, def_id: None };

    let kind = match expr.kind {
        IrExprKind::Call { target, args, type_args } => {
            rewrite_calls_call(target, args, type_args, sigs, mod_scope)
        }
        // RuntimeCall: lowered form of an `@intrinsic` / bundled Module call.
        // Its borrow signature lives in the snapshot keyed by mangled symbol.
        IrExprKind::RuntimeCall { symbol, args } => {
            rewrite_calls_runtime_call(symbol, args, sigs, mod_scope)
        }

        // ── Opaque to this rewriter ──
        //
        // A `TailCall` is already committed and an `RcWrap` is a closure-value
        // box — annotating inside either would change code the walker no
        // longer owns.
        kind @ (IrExprKind::TailCall { .. }
        | IrExprKind::RcWrap { .. }) => kind,
        // An `InlineRust` template splices each arg's TEXT verbatim, so the
        // arg itself takes no slot decoration — but a call nested inside an
        // arg is an ordinary call site the walker still renders, and its own
        // arguments follow its callee's signature. The region window
        // (`almide_region_window(|| __rgn_check(__rgn_make(d)))`, #1961) is
        // one: `__rgn_check` borrows its tree once variants borrow, and the
        // site must spell `&__rgn_make(d)`.
        IrExprKind::InlineRust { template, args } => IrExprKind::InlineRust {
            template,
            args: args.into_iter().map(|(n, a)| (n, rewrite_calls(a, sigs, mod_scope))).collect(),
        },
        // IterChain: the source, every step / collector lambda body, a fold's
        // seed and a take's count are all call sites the walker renders
        // (the pass runs before this one now).
        IrExprKind::IterChain { source, consume, steps, collector } => IrExprKind::IterChain {
            source: Box::new(rewrite_calls(*source, sigs, mod_scope)),
            consume,
            steps: steps.into_iter().map(|s| map_step(s, &mut |e| rewrite_calls(e, sigs, mod_scope))).collect(),
            collector: map_collector(collector, &mut |e| rewrite_calls(e, sigs, mod_scope)),
        },

        // ── Statement-bearing nodes: bodies go through `rewrite_calls_stmt` ──
        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: stmts.into_iter().map(|s| rewrite_calls_stmt(s, sigs, mod_scope)).collect(),
            expr: expr.map(|e| Box::new(rewrite_calls(*e, sigs, mod_scope))),
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var,
            var_tuple,
            iterable: Box::new(rewrite_calls(*iterable, sigs, mod_scope)),
            body: body.into_iter().map(|s| rewrite_calls_stmt(s, sigs, mod_scope)).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(rewrite_calls(*cond, sigs, mod_scope)),
            body: body.into_iter().map(|s| rewrite_calls_stmt(s, sigs, mod_scope)).collect(),
        },

        // ── Everything else: rewrite every child ──
        kind => {
            return rebuild(kind).map_children(&mut |e| rewrite_calls(e, sigs, mod_scope));
        }
    };

    rebuild(kind)
}

fn rewrite_calls_stmt(stmt: IrStmt, sigs: &HashMap<String, Vec<ParamBorrow>>, mod_scope: Option<&str>) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
            var, mutability, ty, value: rewrite_calls(value, sigs, mod_scope),
        },
        IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: rewrite_calls(value, sigs, mod_scope) },
        IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: rewrite_calls(expr, sigs, mod_scope) },
        IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
            cond: rewrite_calls(cond, sigs, mod_scope), else_: rewrite_calls(else_, sigs, mod_scope),
        },
        IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
            pattern, value: rewrite_calls(value, sigs, mod_scope),
        },
        // Assign-with-computed-subexpr kinds: descend into the index/key/value so a
        // stdlib call there (e.g. `m[string.take(s, i)] = …`) gets its borrow args
        // annotated — else the call's `&str`/`&[T]` arg renders as an owned value
        // and rustc rejects it (#415). The earlier `other => other` skipped these.
        IrStmtKind::IndexAssign { target, index, value } => IrStmtKind::IndexAssign {
            target, index: rewrite_calls(index, sigs, mod_scope), value: rewrite_calls(value, sigs, mod_scope),
        },
        IrStmtKind::MapInsert { target, key, value } => IrStmtKind::MapInsert {
            target, key: rewrite_calls(key, sigs, mod_scope), value: rewrite_calls(value, sigs, mod_scope),
        },
        IrStmtKind::FieldAssign { target, field, value } => IrStmtKind::FieldAssign {
            target, field, value: rewrite_calls(value, sigs, mod_scope),
        },
        // Explicit-preserve: no rewrite-relevant expr children (or handled elsewhere).
        kind @ (IrStmtKind::Comment { .. }
            | IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. }
            | IrStmtKind::ListSwap { .. } | IrStmtKind::ListReverse { .. }
            | IrStmtKind::ListRotateLeft { .. } | IrStmtKind::ListCopySlice { .. }) => kind,
    };
    IrStmt { kind, span: stmt.span }
}

// ── Phase 3: Hoist conflicting reads from &mut call args ──────────

/// When a call has `&mut var_x` (or `&mut var_x.field…`) as one arg and
/// another arg reads `var_x`, Rust's borrow checker rejects the overlapping
/// borrows. This phase hoists the conflicting read args into
/// `let __hoist = <expr>` bindings before the call, replacing them with
/// `Var(__hoist)`.
///
/// A `var` GLOBAL root is stricter (#2946): the walker renders the call
/// inside `G.with(|c| … c.borrow_mut() …)`, so ANY sibling that can run
/// user code — a user fn call, which may read `G` through its own body —
/// would re-borrow the cell while it is mutably borrowed and panic. Those
/// siblings are hoisted too, so they evaluate before the borrow is taken.
pub fn hoist_conflicting_reads(program: &mut IrProgram) {
    let globals: HashSet<VarId> = program.top_lets.iter()
        .chain(program.modules.iter().flat_map(|m| m.top_lets.iter()))
        .filter(|tl| tl.mutable)
        .map(|tl| tl.var)
        .collect();
    let IrProgram { functions, modules, var_table, .. } = program;
    let mut cx = HoistCx { vt: var_table, globals: &globals };
    for func in functions.iter_mut() {
        func.body = hoist_expr(std::mem::take(&mut func.body), &mut cx);
    }
    for module in modules.iter_mut() {
        for func in module.functions.iter_mut() {
            func.body = hoist_expr(std::mem::take(&mut func.body), &mut cx);
        }
    }
}

/// The hoist's state: the var table it allocates `__hoist` temps in, and the
/// mutable top-level vars whose `&mut` places the walker renders through the
/// global's cell.
struct HoistCx<'a> {
    vt: &'a mut VarTable,
    globals: &'a HashSet<VarId>,
}

impl HoistCx<'_> {
    /// Must the sibling `arg` of a `&mut` place rooted at `root` be hoisted?
    fn must_hoist(&self, arg: &IrExpr, root: VarId) -> bool {
        reads_var(arg, root) || (self.globals.contains(&root) && may_run_user_code(arg))
    }
}

/// Does `arg` run user code when it is evaluated — a user fn call, or a
/// closure handed to a call that may invoke it? A bare lambda argument is not
/// evaluated by being passed.
fn may_run_user_code(arg: &IrExpr) -> bool {
    struct Finder(bool);
    impl IrVisitor for Finder {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.0 { return; }
            if matches!(e.kind, IrExprKind::Call { .. } | IrExprKind::Lambda { .. } | IrExprKind::ClosureCreate { .. }) {
                self.0 = true;
                return;
            }
            walk_expr(self, e);
        }
    }
    if matches!(arg.kind, IrExprKind::Lambda { .. } | IrExprKind::ClosureCreate { .. }) {
        return false;
    }
    let mut f = Finder(false);
    f.visit_expr(arg);
    f.0
}

/// Does `arg` read `var` anywhere (closure bodies included)? A sibling of a
/// `&mut var` argument that does is the conflicting read the hoist moves out.
fn reads_var(arg: &IrExpr, var: VarId) -> bool {
    UseSites::of_expr(arg, Site::Operand, &super::use_kind::ExplicitBorrows).occurs(var)
}

/// Find the root VarId of a `&mut` place argument: `&mut x`, `&mut x.f`,
/// `&mut x.f.0`, … (#2946 — a field place was not recognised, so a sibling
/// reading the root was never hoisted).
fn find_mut_borrow_var(arg: &IrExpr) -> Option<VarId> {
    let IrExprKind::Borrow { expr, mutable: true, .. } = &arg.kind else { return None };
    let mut cur: &IrExpr = expr;
    loop {
        match &cur.kind {
            IrExprKind::Var { id } => return Some(*id),
            IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => cur = object,
            // Clone insertion spells a tuple projection's object `t.clone().0`.
            IrExprKind::Clone { expr: object } => cur = object,
            _ => return None,
        }
    }
}

/// Hoist one conflicting read arg into a `let __hoist` binding, preserving a
/// by-ref calling convention: a `&expr` arg hoists its INNER expr and passes
/// `&__hoist`. Hoisting the whole `Borrow` bound a reference where the Bind
/// path expects an owned value — for a mutable global that emitted
/// `let __hoist: AlmideRcCow<Vec<u8>> = &G.with(…)`, invalid Rust on both the
/// binding and the call site (#955, `bytes.copy_from(g, g, …)`). Binding the
/// inner expr instead lets the Bind renderer apply its owned-value glue
/// (a global read binds through `AlmideRcCow::from(…)`), and `&__hoist`
/// deref-coerces at the call.
fn hoist_one_arg(arg: IrExpr, hoisted: &mut Vec<IrStmt>, cx: &mut HoistCx<'_>) -> IrExpr {
    let arg_ty = arg.ty.clone();
    let (inner, rewrap) = match arg.kind {
        IrExprKind::Borrow { expr, as_str, mutable: false } => (*expr, Some(as_str)),
        kind => (IrExpr { kind, ty: arg_ty.clone(), span: arg.span, def_id: arg.def_id }, None),
    };
    let tmp_ty = inner.ty.clone();
    // A unique name per temporary: one call can hoist several args, and the
    // native renderer emits locals by name, so a shared `__hoist` made every
    // later `let` shadow the earlier ones and each arg read the last value
    // (#3049).
    let tmp = cx.vt.alloc(sym(&format!("__hoist_{}", cx.vt.len())), tmp_ty.clone(), Mutability::Let, None);
    hoisted.push(IrStmt {
        kind: IrStmtKind::Bind { var: tmp, mutability: Mutability::Let, ty: tmp_ty.clone(), value: inner },
        span: None,
    });
    let var = IrExpr { kind: IrExprKind::Var { id: tmp }, ty: tmp_ty, span: None, def_id: None };
    match rewrap {
        Some(as_str) => IrExpr {
            kind: IrExprKind::Borrow { expr: Box::new(var), as_str, mutable: false },
            ty: arg_ty, span: None, def_id: None,
        },
        None => var,
    }
}

fn hoist_expr(expr: IrExpr, cx: &mut HoistCx<'_>) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        IrExprKind::Call { target, args, type_args } => {
            return hoist_call(target, args, type_args, ty, span, cx)
        }
        IrExprKind::RuntimeCall { symbol, args } => {
            return hoist_runtime_call(symbol, args, ty, span, cx)
        }

        // Recurse into all compound expressions
        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: stmts.into_iter().map(|s| hoist_stmt(s, cx)).collect(),
            expr: expr.map(|e| Box::new(hoist_expr(*e, cx))),
        },
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(hoist_expr(*cond, cx)),
            then: Box::new(hoist_expr(*then, cx)),
            else_: Box::new(hoist_expr(*else_, cx)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(hoist_expr(*subject, cx)),
            arms: arms.into_iter().map(|a| IrMatchArm {
                pattern: a.pattern,
                guard: a.guard.map(|g| hoist_expr(g, cx)),
                body: hoist_expr(a.body, cx),
            }).collect(),
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(hoist_expr(*iterable, cx)),
            body: body.into_iter().map(|s| hoist_stmt(s, cx)).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(hoist_expr(*cond, cx)),
            body: body.into_iter().map(|s| hoist_stmt(s, cx)).collect(),
        },
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params, body: Box::new(hoist_expr(*body, cx)), lambda_id,
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op, left: Box::new(hoist_expr(*left, cx)), right: Box::new(hoist_expr(*right, cx)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(hoist_expr(*operand, cx)),
        },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(hoist_expr(*expr, cx)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(hoist_expr(*expr, cx)) },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(hoist_expr(*expr, cx)) },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(hoist_expr(*expr, cx)) },
        IrExprKind::Unwrap { expr } => IrExprKind::Unwrap { expr: Box::new(hoist_expr(*expr, cx)) },
        IrExprKind::UnwrapOr { expr, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(hoist_expr(*expr, cx)), fallback: Box::new(hoist_expr(*fallback, cx)),
        },
        // Explicit-preserve: nodes this hoist pass does NOT descend into. The
        // &mut-conflict hoist only fires at Call / RuntimeCall sites and the
        // compound forms above; everything else is returned unchanged, exactly
        // as the original `other => other` did (zero behaviour change).
        kind @ (IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
            | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
            | IrExprKind::Unit | IrExprKind::Var { .. } | IrExprKind::FnRef { .. }
            | IrExprKind::Fan { .. } | IrExprKind::Break | IrExprKind::Continue
            | IrExprKind::TailCall { .. } | IrExprKind::List { .. }
            | IrExprKind::MapLiteral { .. } | IrExprKind::EmptyMap
            | IrExprKind::Record { .. } | IrExprKind::SpreadRecord { .. }
            | IrExprKind::Tuple { .. } | IrExprKind::Range { .. }
            | IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
            | IrExprKind::IndexAccess { .. } | IrExprKind::MapAccess { .. }
            | IrExprKind::StringInterp { .. } | IrExprKind::OptionNone
            | IrExprKind::ToOption { .. } | IrExprKind::OptionalChain { .. }
            | IrExprKind::Clone { .. }
            | IrExprKind::Deref { .. } | IrExprKind::Borrow { .. }
            | IrExprKind::BoxNew { .. } | IrExprKind::RcWrap { .. }
            | IrExprKind::RustMacro { .. } | IrExprKind::ToVec { .. }
            | IrExprKind::RenderedCall { .. } | IrExprKind::InlineRust { .. }
            | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
            | IrExprKind::Hole
            | IrExprKind::Todo { .. }) => kind,
        IrExprKind::IterChain { source, consume, steps, collector } => IrExprKind::IterChain {
            source: Box::new(hoist_expr(*source, cx)),
            consume,
            steps: steps.into_iter().map(|s| map_step(s, &mut |e| hoist_expr(e, cx))).collect(),
            collector: map_collector(collector, &mut |e| hoist_expr(e, cx)),
        },
    };

    IrExpr { kind, ty, span, def_id: None }
}

/// Apply `f` to a chain step's expression child (its lambda or count).
fn map_step(step: IterStep, f: &mut dyn FnMut(IrExpr) -> IrExpr) -> IterStep {
    match step {
        IterStep::Map { lambda } => IterStep::Map { lambda: Box::new(f(*lambda)) },
        IterStep::Filter { lambda } => IterStep::Filter { lambda: Box::new(f(*lambda)) },
        IterStep::FlatMap { lambda } => IterStep::FlatMap { lambda: Box::new(f(*lambda)) },
        IterStep::FilterMap { lambda } => IterStep::FilterMap { lambda: Box::new(f(*lambda)) },
        IterStep::Take { n } => IterStep::Take { n: Box::new(f(*n)) },
        IterStep::Enumerate => IterStep::Enumerate,
    }
}

/// Apply `f` to a collector's expression children (its lambda and seed).
fn map_collector(collector: IterCollector, f: &mut dyn FnMut(IrExpr) -> IrExpr) -> IterCollector {
    match collector {
        IterCollector::Fold { init, lambda } => IterCollector::Fold { init: Box::new(f(*init)), lambda: Box::new(f(*lambda)) },
        IterCollector::Any { lambda } => IterCollector::Any { lambda: Box::new(f(*lambda)) },
        IterCollector::All { lambda } => IterCollector::All { lambda: Box::new(f(*lambda)) },
        IterCollector::Find { lambda } => IterCollector::Find { lambda: Box::new(f(*lambda)) },
        IterCollector::Count { lambda } => IterCollector::Count { lambda: Box::new(f(*lambda)) },
        IterCollector::Collect => IterCollector::Collect,
        IterCollector::Sum { float } => IterCollector::Sum { float },
        IterCollector::Len => IterCollector::Len,
    }
}

/// A `Call` site: hoist its args and its Method/Computed target first, then let
/// [`hoist_call_if_needed`] decide whether an `&mut` conflict forces a hoist.
fn hoist_call(
    target: CallTarget,
    args: Vec<IrExpr>,
    type_args: Vec<almide_lang::types::Ty>,
    ty: almide_lang::types::Ty,
    span: Option<almide_base::span::Span>,
    cx: &mut HoistCx<'_>,
) -> IrExpr {
    let args: Vec<IrExpr> = args.into_iter().map(|a| hoist_expr(a, cx)).collect();
    let target = match target {
        CallTarget::Method { object, method } => {
            CallTarget::Method { object: Box::new(hoist_expr(*object, cx)), method }
        }
        CallTarget::Computed { callee } => {
            CallTarget::Computed { callee: Box::new(hoist_expr(*callee, cx)) }
        }
        other => other,
    };
    hoist_call_if_needed(target, args, type_args, ty, span, cx)
}

/// A `RuntimeCall` site: the [`hoist_call_if_needed`] rule, applied to the
/// symbol form. Every arg that READS the `&mut`-borrowed var is hoisted to a
/// preceding let so the borrow no longer overlaps; the `&mut` arg itself stays.
fn hoist_runtime_call(
    symbol: almide_base::intern::Sym,
    args: Vec<IrExpr>,
    ty: almide_lang::types::Ty,
    span: Option<almide_base::span::Span>,
    cx: &mut HoistCx<'_>,
) -> IrExpr {
    let args: Vec<IrExpr> = args.into_iter().map(|a| hoist_expr(a, cx)).collect();
    let Some(mut_id) = args.iter().find_map(find_mut_borrow_var) else {
        return IrExpr { kind: IrExprKind::RuntimeCall { symbol, args }, ty, span, def_id: None };
    };
    let mut hoisted_stmts: Vec<IrStmt> = Vec::new();
    let new_args: Vec<IrExpr> = args
        .into_iter()
        .map(|arg| {
            if find_mut_borrow_var(&arg).is_some() {
                arg // keep the &mut arg as-is
            } else if cx.must_hoist(&arg, mut_id) {
                hoist_one_arg(arg, &mut hoisted_stmts, cx)
            } else {
                arg
            }
        })
        .collect();
    let call = IrExpr {
        kind: IrExprKind::RuntimeCall { symbol, args: new_args },
        ty: ty.clone(),
        span,
        def_id: None,
    };
    if hoisted_stmts.is_empty() {
        return call;
    }
    IrExpr {
        kind: IrExprKind::Block { stmts: hoisted_stmts, expr: Some(Box::new(call)) },
        ty,
        span,
        def_id: None,
    }
}

fn hoist_call_if_needed(target: CallTarget, args: Vec<IrExpr>, type_args: Vec<almide_lang::types::Ty>,
    ty: almide_lang::types::Ty, span: Option<almide_base::span::Span>, cx: &mut HoistCx<'_>) -> IrExpr
{
    let mut_var = args.iter().find_map(find_mut_borrow_var);
    if let Some(mut_id) = mut_var {
        let mut hoisted_stmts: Vec<IrStmt> = Vec::new();
        let new_args: Vec<IrExpr> = args.into_iter().map(|arg| {
            if find_mut_borrow_var(&arg).is_some() {
                arg
            } else if cx.must_hoist(&arg, mut_id) {
                hoist_one_arg(arg, &mut hoisted_stmts, cx)
            } else {
                arg
            }
        }).collect();
        if !hoisted_stmts.is_empty() {
            let call = IrExpr {
                kind: IrExprKind::Call { target, args: new_args, type_args },
                ty: ty.clone(), span, def_id: None,
            };
            return IrExpr {
                kind: IrExprKind::Block { stmts: hoisted_stmts, expr: Some(Box::new(call)) },
                ty, span, def_id: None,
            };
        }
        IrExpr { kind: IrExprKind::Call { target, args: new_args, type_args }, ty, span, def_id: None }
    } else {
        IrExpr { kind: IrExprKind::Call { target, args, type_args }, ty, span, def_id: None }
    }
}

fn hoist_stmt(stmt: IrStmt, cx: &mut HoistCx<'_>) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
            var, mutability, ty, value: hoist_expr(value, cx),
        },
        IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: hoist_expr(value, cx) },
        IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: hoist_expr(expr, cx) },
        IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
            cond: hoist_expr(cond, cx), else_: hoist_expr(else_, cx),
        },
        IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
            pattern, value: hoist_expr(value, cx),
        },
        IrStmtKind::IndexAssign { target, index, value } => IrStmtKind::IndexAssign {
            target, index: hoist_expr(index, cx), value: hoist_expr(value, cx),
        },
        IrStmtKind::MapInsert { target, key, value } => IrStmtKind::MapInsert {
            target, key: hoist_expr(key, cx), value: hoist_expr(value, cx),
        },
        IrStmtKind::FieldAssign { target, field, value } => IrStmtKind::FieldAssign {
            target, field, value: hoist_expr(value, cx),
        },
        // Explicit-preserve: stmt kinds with no hoistable child expr, matching
        // the original `other => other` (zero behaviour change).
        kind @ (IrStmtKind::Comment { .. } | IrStmtKind::RcInc { .. }
            | IrStmtKind::RcDec { .. } | IrStmtKind::ListSwap { .. }
            | IrStmtKind::ListReverse { .. } | IrStmtKind::ListRotateLeft { .. }
            | IrStmtKind::ListCopySlice { .. }) => kind,
    };
    IrStmt { kind, span: stmt.span }
}
