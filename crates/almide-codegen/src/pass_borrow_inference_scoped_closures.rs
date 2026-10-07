// Included by pass_borrow_inference.rs: a `let`-bound closure that cannot
// outlive its fn is a SCOPE, not a closure value (#3455).
//
// The problem. `let f = (i) => list.get(xs, i) ?? 0` read `xs` from inside a
// closure, every occurrence inside a closure counts as consuming (`consumes`:
// the `move` capture takes the value), so `xs` was owned and every caller
// cloned its list to make the call. A lambda handed straight to a HOF did not
// pay this: a fused chain step (#2286) and a lambda at a non-escaping fn slot
// (#2288) are walked as scopes. A NAMED lambda with the same lifetime did.
//
// References (../almide-references). rustc captures a non-`move` closure's
// upvars by reference (`rustc_hir_typeck/src/upvar.rs:700-702`,
// `adjust_for_non_move_closure`), and Swift captures a non-escaping closure's
// variables by address (`lib/SIL/IR/TypeLowering.cpp:194`,
// `CaptureKind::StorageAddress`), proving non-escape in the optimizer
// (`ComputeEscapeEffects.swift`) where the type does not declare it. The
// Perceus family (Lean `InferBorrow.lean`, Koka `Parc.hs`) owns every capture
// and removes closures by specialisation instead; for a bound closure called
// by name that never fires, so the Rust/Swift rule is the one that applies.
//
// The rule. A lambda `let`-bound to `f` is a scope when, under the round's
// signatures:
// - every occurrence of `f` is one `fn_param_escapes` (#2288, the same rule
//   a fn-typed param's `&dyn Fn` verdict and the certifier's C5 read) lets
//   through — called, borrowed, cloned, handed to a non-escaping fn slot —
//   outside every `fan` arm, and `f` is never written;
// - every variable it captures is never written anywhere in the fn, is no
//   `var`, never sits in a `fan` arm or under `&mut`, and — when it is a
//   heap value — has no consuming occurrence anywhere (a move of it while
//   `f` borrows it is E0505; a capture by another closure is consuming).
// A scope lambda's body is walked like a chain step: depth 0, in a loop. So
// its reads are borrows, the params it reads stay `&T`, and once the verdict
// is final `commit_scoped_closures` spells it `Borrow { Lambda }` — the node
// every later pass already reads as a non-escaping scope (`CaptureClone`
// adds no `__cap` bind, `RustLowering` boxes nothing, the renderer emits a
// non-`move` `&|..| ..`). A TCO-bound fn is left out: its params become the
// loop's reassigned state.
//
// Alternatives. A separate pass before `BorrowInsertion` cannot see the
// signature table, so a call handing `f` to a user fn's `&dyn Fn` slot would
// read as an escape. Treating every bound lambda as a scope and repairing in
// `CaptureClone` would decide ownership from a later pass, the descent the
// fixed point forbids. The decision therefore lives in the fixed point, and
// it is monotone: a scope only makes occurrences less consuming.
//
// Compatibility: none. The program's output is identical; only the Rust a
// non-escaping, read-only bound closure lowers to changes (and so does the
// signature of the fn holding it). `ALMIDE_SCOPED_CLOSURE_OFF=1` is the
// ablation.

/// The round's oracle plus the let-bound closures this fn's walk treats as
/// scopes.
struct WithScoped<'a> {
    scope: &'a Scope<'a>,
    binders: &'a HashSet<VarId>,
}

impl SlotOracle for WithScoped<'_> {
    fn call_slot(&self, target: &CallTarget, index: usize, arg: &IrExpr) -> SlotMode {
        self.scope.call_slot(target, index, arg)
    }
    fn runtime_slot(&self, symbol: Sym, index: usize, arg: &IrExpr) -> SlotMode {
        self.scope.runtime_slot(symbol, index, arg)
    }
    fn scoped_closure(&self, binder: VarId) -> bool {
        self.binders.contains(&binder)
    }
}

/// A `let`-bound lambda and what it captures (`heap`: the capture's type is
/// one a move would take, `almide_ir::top_let_storage::capture_clone_wrap`).
struct BoundLambda {
    binder: VarId,
    captures: Vec<(VarId, bool)>,
}

/// Every `let`-bound lambda in `body`, and every `var` binder.
fn bound_lambdas(body: &IrExpr) -> (Vec<BoundLambda>, HashSet<VarId>) {
    use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
    #[derive(Default)]
    struct Scan { found: Vec<BoundLambda>, vars: HashSet<VarId> }
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) { walk_expr(self, e); }
        fn visit_stmt(&mut self, s: &IrStmt) {
            if let IrStmtKind::Bind { var, mutability, value, .. } = &s.kind {
                match (mutability, &value.kind) {
                    (Mutability::Var, _) => { self.vars.insert(*var); }
                    (_, IrExprKind::Lambda { params, body, .. }) => {
                        self.found.push(BoundLambda { binder: *var, captures: captures_of(params, body) });
                    }
                    _ => {}
                }
            }
            walk_stmt(self, s);
        }
    }
    let mut scan = Scan::default();
    scan.visit_expr(body);
    (scan.found, scan.vars)
}

/// The variables a lambda captures, with whether each is a heap value.
fn captures_of(params: &[(VarId, Ty)], body: &IrExpr) -> Vec<(VarId, bool)> {
    use almide_ir::visit::{IrVisitor, walk_expr};
    struct Types(HashMap<VarId, bool>);
    impl IrVisitor for Types {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Var { id } = &e.kind {
                self.0.insert(*id, almide_ir::top_let_storage::capture_clone_wrap(&e.ty));
            }
            walk_expr(self, e);
        }
    }
    let bound: HashSet<VarId> = params.iter().map(|(v, _)| *v).collect();
    let mut types = Types(HashMap::new());
    types.visit_expr(body);
    almide_ir::free_vars::free_vars(body, &bound).into_iter()
        .map(|v| (v, types.0.get(&v).copied().unwrap_or(true)))
        .collect()
}

/// Does the lambda bound to `lam.binder` qualify as a scope under `uses`?
fn is_scope(lam: &BoundLambda, uses: &UseSites, written: &HashSet<VarId>, vars: &HashSet<VarId>) -> bool {
    let callable_stays = !written.contains(&lam.binder)
        && uses.of(lam.binder).all(|u| !fn_param_escapes(u) && u.fan_arm.is_none());
    callable_stays && lam.captures.iter().all(|&(c, heap)| {
        !written.contains(&c) && !vars.contains(&c)
            && uses.of(c).all(|u| u.fan_arm.is_none() && !u.in_mut && !(heap && consumes(u)))
    })
}

/// The let-bound closures of `func` that are scopes, and the occurrences of
/// its body walked with them as scopes — the table the param verdict reads.
/// The set is the greatest fixed point: start from every bound lambda, walk
/// with the current set and drop the lambdas that do not qualify under that
/// walk, until none is dropped. A lambda's own captures are read as the scope
/// it would be, and dropping one only adds escapes and consumes, so the set
/// only shrinks; the set returned qualifies under the very walk returned.
fn scoped_uses(func: &IrFunction, scope: &Scope) -> (HashSet<VarId>, UseSites) {
    let walk = |set: &HashSet<VarId>| UseSites::of_fn(func, &WithScoped { scope, binders: set });
    if almide_base::env::flag("ALMIDE_SCOPED_CLOSURE_OFF") || crate::pass_tco::is_tco_candidate(func) {
        return (HashSet::new(), walk(&HashSet::new()));
    }
    let (lambdas, vars) = bound_lambdas(&func.body);
    let mut scoped: HashSet<VarId> = lambdas.iter().map(|l| l.binder).collect();
    loop {
        let uses = walk(&scoped);
        if lambdas.is_empty() {
            return (scoped, uses);
        }
        let written = uses.written();
        let next: HashSet<VarId> = lambdas.iter()
            .filter(|l| scoped.contains(&l.binder) && is_scope(l, &uses, &written, &vars))
            .map(|l| l.binder)
            .collect();
        if next == scoped {
            return (scoped, uses);
        }
        scoped = next;
    }
}

/// Spell each scope lambda's verdict (#3455): `let f = λ` becomes
/// `let f = &λ`, read with the FINAL signatures — the fixed point's last
/// round saw exactly these, so the set is the one the param verdicts rest on.
pub fn commit_scoped_closures(program: &mut IrProgram, sigs: &HashMap<String, Vec<ParamBorrow>>) {
    use almide_ir::visit_mut::{walk_stmt_mut, IrMutVisitor};
    struct Commit<'a>(&'a HashSet<VarId>);
    impl IrMutVisitor for Commit<'_> {
        fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
            walk_stmt_mut(self, s);
            if let IrStmtKind::Bind { var, value, .. } = &mut s.kind
                && self.0.contains(var)
                && matches!(value.kind, IrExprKind::Lambda { .. })
            {
                let lambda = std::mem::take(value);
                *value = IrExpr { ty: lambda.ty.clone(), span: lambda.span, def_id: None,
                    kind: IrExprKind::Borrow { expr: Box::new(lambda), as_str: false, mutable: false } };
            }
        }
    }
    let records = seed_record_names(program);
    let variants = seed_variant_names(program);
    let pending = HashSet::new();
    let round = Round { snapshot: sigs, pending: &pending, records: &records, variants: &variants };
    let commit = |func: &mut IrFunction, module: Option<&str>| {
        if !is_analysed_fn(func) { return HashSet::new(); }
        let name = func.name.to_string();
        let (scoped, _) = scoped_uses(func, &Scope { round: &round, module, current_fn: &name });
        if !scoped.is_empty() {
            Commit(&scoped).visit_expr_mut(&mut func.body);
        }
        scoped
    };
    let mut binders = HashSet::new();
    for f in &mut program.functions { binders.extend(commit(f, None)); }
    for m in &mut program.modules {
        let module = m.name.to_string();
        for f in &mut m.functions { binders.extend(commit(f, Some(&module))); }
    }
    program.codegen_annotations.scope_closure_binders.extend(binders);
}
