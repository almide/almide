/// The key a call target is looked up under in the pure-fn set: a user fn's
/// bare name, a module fn's `module.func`. Method/Computed dispatch can hide
/// effects, so neither has a key — they are conservatively impure.
fn call_target_key(target: &CallTarget) -> Option<Sym> {
    match target {
        CallTarget::Module { module, func, .. } => {
            Some(almide_base::intern::sym(&format!("{}.{}", module, func)))
        }
        CallTarget::Named { name } => Some(*name),
        CallTarget::Method { .. } | CallTarget::Computed { .. } => None,
    }
}

/// Is a call's destination known to be side-effect free?
fn call_target_is_pure(target: &CallTarget, pure_fns: &HashSet<Sym>) -> bool {
    call_target_key(target).is_some_and(|key| pure_fns.contains(&key))
}

/// May this BinOp trap at runtime? Integer division/modulo trap on a zero
/// divisor and PowInt can overflow-panic; their float duals are total (IEEE
/// inf/NaN, never a trap). A nonzero integer-literal divisor is statically
/// safe. Comparison, boolean, concat and wrapping arithmetic ops are total.
fn binop_may_trap(op: BinOp, right: &IrExpr) -> bool {
    match op {
        BinOp::DivInt | BinOp::ModInt => {
            !matches!(&right.kind, IrExprKind::LitInt { value } if *value != 0)
        }
        BinOp::PowInt => true,
        _ => false,
    }
}

/// Returns true if the expression is SPECULATION-SAFE: no function calls with
/// effects, no I/O, no mutation, AND no possibly-trapping operations. LICM
/// hoisting executes the expression even on paths where the original site
/// would never have run (a zero-trip loop), so a hoisted trap is a new
/// observable behavior — the almide#1424 native/wasm divergence. Trapping
/// shapes (index/map access, non-literal integer div/mod) are therefore
/// impure here even though they have no side effects.
/// Conservative: any function call to a non-speculation-safe target makes the
/// expression impure.
/// Grouped by CHILD SHAPE: apart from calls (which consult the pure-fn set)
/// and the conservatively-impure tail, a node is pure exactly when all of its
/// children are.
fn is_pure(expr: &IrExpr, pure_fns: &HashSet<Sym>) -> bool {
    let pure = |e: &IrExpr| is_pure(e, pure_fns);
    match &expr.kind {
        // ── No children: always pure ──
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::OptionNone
        | IrExprKind::Var { .. } | IrExprKind::FnRef { .. } | IrExprKind::Hole
        | IrExprKind::Break | IrExprKind::Continue | IrExprKind::EmptyMap => true,

        // ── Function calls: pure if the target is known-pure and so are the args ──
        IrExprKind::Call { target, args, .. } => {
            call_target_is_pure(target, pure_fns) && args.iter().all(pure)
        }
        IrExprKind::RustMacro { .. } | IrExprKind::RenderedCall { .. } => false,

        // ── One child ──
        IrExprKind::UnOp { operand: e, .. }
        | IrExprKind::Member { object: e, .. } | IrExprKind::TupleIndex { object: e, .. }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e } | IrExprKind::Clone { expr: e }
        | IrExprKind::Deref { expr: e } | IrExprKind::Borrow { expr: e, .. }
        | IrExprKind::BoxNew { expr: e } | IrExprKind::ToVec { expr: e } => pure(e),

        // ── Two children ──
        IrExprKind::BinOp { op, left: a, right: b } => {
            !binop_may_trap(*op, b) && pure(a) && pure(b)
        }
        IrExprKind::UnwrapOr { expr: a, fallback: b }
        | IrExprKind::Range { start: a, end: b, .. } => pure(a) && pure(b),

        // ── A flat sequence of children ──
        IrExprKind::List { elements: xs } | IrExprKind::Tuple { elements: xs } => {
            xs.iter().all(pure)
        }

        // ── Name-tagged children ──
        IrExprKind::Record { fields, .. } => fields.iter().all(|(_, v)| pure(v)),
        IrExprKind::SpreadRecord { base, fields } => {
            pure(base) && fields.iter().all(|(_, v)| pure(v))
        }

        // ── Shapes with their own traversal ──
        IrExprKind::MapLiteral { entries } => {
            entries.iter().all(|(k, v)| pure(k) && pure(v))
        }
        IrExprKind::StringInterp { parts } => parts.iter().all(|p| match p {
            IrStringPart::Expr { expr } => pure(expr),
            IrStringPart::Lit { .. } => true,
        }),

        // Everything else: conservatively impure. IndexAccess/MapAccess sit
        // here (not in the two-children arm) because they can trap — an
        // out-of-bounds index hoisted above a zero-trip loop is a speculative
        // abort the source never performs (almide#1424). Listed explicitly so
        // a new IrExprKind is a compile error here, not a silently-impure
        // default.
        IrExprKind::If { .. } | IrExprKind::Match { .. }
        | IrExprKind::Block { .. } | IrExprKind::Fan { .. }
        | IrExprKind::ForIn { .. } | IrExprKind::While { .. }
        | IrExprKind::TailCall { .. } | IrExprKind::RuntimeCall { .. }
        | IrExprKind::Lambda { .. } | IrExprKind::Try { .. }
        | IrExprKind::Unwrap { .. } | IrExprKind::ToOption { .. }
        | IrExprKind::OptionalChain { .. }
        | IrExprKind::IndexAccess { .. } | IrExprKind::MapAccess { .. }
        | IrExprKind::RcWrap { .. } | IrExprKind::InlineRust { .. }
        | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
        | IrExprKind::IterChain { .. } | IrExprKind::Todo { .. } => false,
    }
}

/// Returns true if all variable references in the expression are outside the loop
/// (i.e., none of them are in `loop_defined`).
/// Grouped by CHILD SHAPE: apart from `Var` (the actual test) and `Lambda`
/// (whose params shadow the loop's), a node's refs are outside the loop exactly
/// when all of its children's are.
fn refs_are_outside_loop(expr: &IrExpr, loop_defined: &HashSet<VarId>) -> bool {
    let outside = |e: &IrExpr| refs_are_outside_loop(e, loop_defined);
    match &expr.kind {
        IrExprKind::Var { id } => !loop_defined.contains(id),

        // ── One child ──
        IrExprKind::UnOp { operand: e, .. }
        | IrExprKind::Member { object: e, .. } | IrExprKind::TupleIndex { object: e, .. }
        | IrExprKind::OptionalChain { expr: e, .. }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
        | IrExprKind::ResultErr { expr: e } | IrExprKind::Try { expr: e }
        | IrExprKind::Unwrap { expr: e } | IrExprKind::ToOption { expr: e }
        | IrExprKind::Clone { expr: e } | IrExprKind::Deref { expr: e }
        | IrExprKind::Borrow { expr: e, .. } | IrExprKind::BoxNew { expr: e }
        | IrExprKind::ToVec { expr: e } => outside(e),

        // ── Two children ──
        IrExprKind::BinOp { left: a, right: b, .. }
        | IrExprKind::UnwrapOr { expr: a, fallback: b }
        | IrExprKind::IndexAccess { object: a, index: b }
        | IrExprKind::MapAccess { object: a, key: b }
        | IrExprKind::Range { start: a, end: b, .. } => outside(a) && outside(b),

        // ── Three children ──
        IrExprKind::If { cond, then, else_ } => {
            outside(cond) && outside(then) && outside(else_)
        }

        // ── A flat sequence of children ──
        IrExprKind::List { elements: xs } | IrExprKind::Tuple { elements: xs } => {
            xs.iter().all(outside)
        }

        // ── Name-tagged children ──
        IrExprKind::Record { fields, .. } => fields.iter().all(|(_, v)| outside(v)),
        IrExprKind::SpreadRecord { base, fields } => {
            outside(base) && fields.iter().all(|(_, v)| outside(v))
        }

        // Shapes with their own traversal — see [`refs_are_outside_loop_nested`].
        IrExprKind::Call { .. } | IrExprKind::Block { .. } | IrExprKind::MapLiteral { .. }
        | IrExprKind::StringInterp { .. } | IrExprKind::Match { .. }
        | IrExprKind::Lambda { .. } => refs_are_outside_loop_nested(expr, loop_defined),

        // Leaf nodes and nodes whose inner refs aren't tracked here: treated as
        // "all refs outside loop" (true). Listed explicitly so a new IrExprKind
        // is a compile error, not a silent always-true default.
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
        | IrExprKind::Unit | IrExprKind::FnRef { .. } | IrExprKind::Fan { .. }
        | IrExprKind::ForIn { .. } | IrExprKind::While { .. }
        | IrExprKind::Break | IrExprKind::Continue | IrExprKind::TailCall { .. }
        | IrExprKind::RuntimeCall { .. } | IrExprKind::EmptyMap
        | IrExprKind::OptionNone
        | IrExprKind::RcWrap { .. } | IrExprKind::RustMacro { .. }
        | IrExprKind::RenderedCall { .. } | IrExprKind::InlineRust { .. }
        | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
        | IrExprKind::IterChain { .. } | IrExprKind::Hole
        | IrExprKind::Todo { .. } => true,
    }
}

/// The [`refs_are_outside_loop`] shapes that do not fall out of a plain child
/// walk: a call's target and args, a block's statements and tail, a map/interp
/// literal's parts, a match's subject/guards/bodies, and a lambda (whose params
/// are LOCAL, so they are NOT loop-defined for its body — remove them before
/// checking the body's free variables).
fn refs_are_outside_loop_nested(expr: &IrExpr, loop_defined: &HashSet<VarId>) -> bool {
    let outside = |e: &IrExpr| refs_are_outside_loop(e, loop_defined);
    match &expr.kind {
        IrExprKind::Call { target, args, .. } => {
            let target_ok = match target {
                CallTarget::Method { object, .. } => outside(object),
                CallTarget::Computed { callee } => outside(callee),
                CallTarget::Named { .. } | CallTarget::Module { .. } => true,
            };
            target_ok && args.iter().all(outside)
        }
        IrExprKind::Block { stmts, expr } => {
            stmts.iter().all(|s| refs_are_outside_loop_stmt(s, loop_defined))
                && expr.as_ref().is_none_or(|e| outside(e))
        }
        IrExprKind::MapLiteral { entries } => {
            entries.iter().all(|(k, v)| outside(k) && outside(v))
        }
        IrExprKind::StringInterp { parts } => parts.iter().all(|p| match p {
            IrStringPart::Expr { expr } => outside(expr),
            IrStringPart::Lit { .. } => true,
        }),
        IrExprKind::Match { subject, arms } => {
            outside(subject)
                && arms.iter().all(|a| {
                    a.guard.as_ref().is_none_or(|g| outside(g)) && outside(&a.body)
                })
        }
        // Lambda params are local, so they are NOT loop-defined for the body:
        // remove them before checking the body's free variables.
        IrExprKind::Lambda { body, params, .. } => {
            let mut extended = loop_defined.clone();
            for (v, _) in params {
                extended.remove(v);
            }
            refs_are_outside_loop(body, &extended)
        }
        _ => unreachable!("dispatched by refs_are_outside_loop's own arm list"),
    }
}

fn refs_are_outside_loop_stmt(stmt: &IrStmt, loop_defined: &HashSet<VarId>) -> bool {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            refs_are_outside_loop(value, loop_defined)
        }
        IrStmtKind::IndexAssign { index, value, .. } => {
            refs_are_outside_loop(index, loop_defined) && refs_are_outside_loop(value, loop_defined)
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            refs_are_outside_loop(key, loop_defined) && refs_are_outside_loop(value, loop_defined)
        }
        IrStmtKind::ListSwap { a, b, .. } => {
            refs_are_outside_loop(a, loop_defined) && refs_are_outside_loop(b, loop_defined)
        }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
            refs_are_outside_loop(end, loop_defined)
        }
        IrStmtKind::ListCopySlice { len, .. } => {
            refs_are_outside_loop(len, loop_defined)
        }
        IrStmtKind::Guard { cond, else_ } => {
            refs_are_outside_loop(cond, loop_defined) && refs_are_outside_loop(else_, loop_defined)
        }
        IrStmtKind::Expr { expr } => refs_are_outside_loop(expr, loop_defined),
        IrStmtKind::RcInc { var } | IrStmtKind::RcDec { var } => !loop_defined.contains(var),
        IrStmtKind::Comment { .. } => true,
    }
}

/// Check if an `@inline_rust` template contains `&mut` (indicating mutation).
fn has_mut_in_inline_rust(attrs: &[almide_lang::ast::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.name.as_str() == "inline_rust"
            && a.args.first().map_or(false, |arg| {
                matches!(&arg.value, almide_lang::ast::AttrValue::String { value } if value.contains("&mut "))
            })
    })
}

// ── User function purity analysis (fixpoint) ──────────────

/// Analyze all user functions and return the set of names that are
/// SPECULATION-SAFE: the body contains no impure operations AND no
/// possibly-trapping operations (see [`is_pure`] — a hoisted call runs even
/// when the loop is zero-trip, so a callee that can trap must not be hoisted;
/// almide#1424).
///
/// Fixpoint by WORKLIST (#1232): every body is evaluated once against the
/// optimistic all-pure set; a body that evaluates pure was walked in full,
/// so the same walk yields its complete callee list (see [`PurityQuery`]),
/// and that is the only thing its verdict depends on. Removing a function
/// therefore re-evaluates exactly its still-pure callers, and nothing else
/// — the rescan-everything rounds it replaces re-walked every pure body per
/// round. Both reach the same greatest fixpoint: a function is only ever
/// removed when its body is impure against the current set, and every
/// function left at the end has been re-checked after its last callee left.
fn analyze_pure_functions(program: &IrProgram) -> HashSet<Sym> {
    let fn_bodies: Vec<(Sym, &IrExpr)> = program
        .functions
        .iter()
        .chain(program.modules.iter().flat_map(|m| m.functions.iter()))
        .map(|f| (f.name, &f.body))
        .collect();

    // Start: assume all functions are pure
    let mut pure_set: HashSet<Sym> = fn_bodies.iter().map(|(name, _)| *name).collect();
    // callee -> the body indices whose verdict consulted it
    let mut callers: HashMap<Sym, Vec<usize>> = HashMap::new();
    let mut worklist: Vec<Sym> = Vec::new();

    for (i, &(name, body)) in fn_bodies.iter().enumerate() {
        let (pure, callees) = PurityQuery::judge(body, &pure_set);
        if pure {
            for callee in callees {
                callers.entry(callee).or_default().push(i);
            }
        } else if pure_set.remove(&name) {
            worklist.push(name);
        }
    }

    while let Some(removed) = worklist.pop() {
        let Some(dependents) = callers.get(&removed) else { continue };
        for &i in dependents {
            let (name, body) = fn_bodies[i];
            if !pure_set.contains(&name) {
                continue;
            }
            if !PurityQuery::judge(body, &pure_set).0 && pure_set.remove(&name) {
                worklist.push(name);
            }
        }
    }

    pure_set
}

/// One purity evaluation of a body against a fixed pure-fn set, recording
/// every call-target key it consults. A body that evaluates pure has had
/// EVERY node visited (the walk only short-circuits on the first impure
/// node), so its `callees` are complete; an impure body's list is a prefix
/// nobody reads.
struct PurityQuery<'a> {
    pure_fns: &'a HashSet<Sym>,
    callees: HashSet<Sym>,
}

impl PurityQuery<'_> {
    fn judge(body: &IrExpr, pure_fns: &HashSet<Sym>) -> (bool, HashSet<Sym>) {
        let mut q = PurityQuery { pure_fns, callees: HashSet::new() };
        let pure = q.expr(body);
        (pure, q.callees)
    }

    fn call(&mut self, target: &CallTarget) -> bool {
        let Some(key) = call_target_key(target) else { return false };
        self.callees.insert(key);
        self.pure_fns.contains(&key)
    }

    /// Check if an expression is pure given a current set of known-pure user functions.
    /// Similar to `is_pure` but works on immutable IR (no VarTable needed).
    /// Grouped by CHILD SHAPE like [`is_pure`]. This variant covers a WIDER
    /// impure tail: without a VarTable it cannot reason about the loop and
    /// propagation nodes, so those stay conservatively impure here.
    fn expr(&mut self, expr: &IrExpr) -> bool {
        match &expr.kind {
            // ── No children: always pure ──
            IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. }
            | IrExprKind::LitBool { .. } | IrExprKind::Unit | IrExprKind::OptionNone
            | IrExprKind::Var { .. } | IrExprKind::FnRef { .. } | IrExprKind::Hole
            | IrExprKind::Break | IrExprKind::Continue | IrExprKind::EmptyMap => true,

            // ── Function calls ──
            IrExprKind::Call { target, args, .. } => {
                self.call(target) && args.iter().all(|a| self.expr(a))
            }
            IrExprKind::RustMacro { .. } | IrExprKind::RenderedCall { .. } => false,

            // ── One child ──
            IrExprKind::UnOp { operand: e, .. } | IrExprKind::Lambda { body: e, .. }
            | IrExprKind::Member { object: e, .. } | IrExprKind::TupleIndex { object: e, .. }
            | IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e }
            | IrExprKind::ResultErr { expr: e } | IrExprKind::Clone { expr: e }
            | IrExprKind::Deref { expr: e } | IrExprKind::Borrow { expr: e, .. }
            | IrExprKind::BoxNew { expr: e } | IrExprKind::ToVec { expr: e } => self.expr(e),

            // ── Two children ──
            IrExprKind::BinOp { op, left: a, right: b } => {
                !binop_may_trap(*op, b) && self.expr(a) && self.expr(b)
            }
            IrExprKind::UnwrapOr { expr: a, fallback: b }
            | IrExprKind::Range { start: a, end: b, .. } => self.expr(a) && self.expr(b),

            // ── Three children ──
            IrExprKind::If { cond, then, else_ } => {
                self.expr(cond) && self.expr(then) && self.expr(else_)
            }

            // ── A flat sequence of children ──
            IrExprKind::List { elements: xs } | IrExprKind::Tuple { elements: xs } => {
                xs.iter().all(|e| self.expr(e))
            }

            // ── Name-tagged children ──
            IrExprKind::Record { fields, .. } => fields.iter().all(|(_, v)| self.expr(v)),

            // ── Shapes with their own traversal ──
            IrExprKind::Match { subject, arms } => {
                self.expr(subject) && arms.iter().all(|a| self.expr(&a.body))
            }
            IrExprKind::Block { stmts, expr } => {
                stmts.iter().all(|s| self.stmt(s))
                    && expr.as_ref().is_none_or(|e| self.expr(e))
            }
            IrExprKind::StringInterp { parts } => parts.iter().all(|p| match p {
                IrStringPart::Expr { expr } => self.expr(expr),
                IrStringPart::Lit { .. } => true,
            }),

            // ForIn, While, Fan, Await, etc. — conservatively impure. IndexAccess
            // and MapAccess sit here because they can trap: a function whose body
            // indexes is not speculation-safe, so it must not enter the pure set
            // that licenses hoisting (almide#1424). Listed explicitly so a new
            // IrExprKind is a compile error here, not a silently-impure default.
            IrExprKind::Fan { .. } | IrExprKind::ForIn { .. }
            | IrExprKind::While { .. } | IrExprKind::TailCall { .. }
            | IrExprKind::RuntimeCall { .. } | IrExprKind::MapLiteral { .. }
            | IrExprKind::SpreadRecord { .. } | IrExprKind::MapAccess { .. }
            | IrExprKind::IndexAccess { .. }
            | IrExprKind::Try { .. } | IrExprKind::Unwrap { .. }
            | IrExprKind::ToOption { .. } | IrExprKind::OptionalChain { .. }
            | IrExprKind::RcWrap { .. }
            | IrExprKind::InlineRust { .. } | IrExprKind::ClosureCreate { .. }
            | IrExprKind::EnvLoad { .. } | IrExprKind::IterChain { .. }
            | IrExprKind::Todo { .. } => false,
        }
    }

    fn stmt(&mut self, stmt: &IrStmt) -> bool {
        match &stmt.kind {
            IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } => {
                self.expr(value)
            }
            // Assignments are mutations → impure
            IrStmtKind::Assign { .. } | IrStmtKind::IndexAssign { .. }
            | IrStmtKind::FieldAssign { .. } | IrStmtKind::MapInsert { .. }
            | IrStmtKind::ListSwap { .. } | IrStmtKind::ListReverse { .. }
            | IrStmtKind::ListRotateLeft { .. } | IrStmtKind::ListCopySlice { .. } => false,
            IrStmtKind::Expr { expr } => self.expr(expr),
            IrStmtKind::Guard { cond, else_ } => {
                self.expr(cond) && self.expr(else_)
            }
            IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. } => false,
            IrStmtKind::Comment { .. } => true,
        }
    }
}
