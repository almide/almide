// ── PerceusOptPass: RETIRED (2026-06-11, #527 F6) ───────────────────────
// The Inc/Dec pair-elimination targeted the PRE-Round-3 "Inc-BEFORE-bind"
// convention. Under the current Inc-AFTER-bind rules its pattern never
// matched (probed: zero firings across the full corpus), and a match would
// have MISATTRIBUTED a Rule-1 protective Inc — removing it plus the
// binding's Dec opens a transient use-after-free window. Deleted rather
// than rewritten: there is no remaining shape for it to optimize.

/// Perceus verification: check that RC operations are correctly balanced.
///
/// Invariant: For each heap-typed variable v,
///   inc_count(v) + 1 (initial alloc) ≥ dec_count(v)
///   AND dec_count(v) ≥ 1 (every alloc has at least one free path)
///
/// Violations are reported as compiler warnings (not errors — the program
/// still runs, it just may leak or double-free).
#[derive(Debug)]
pub struct PerceusVerifyPass;

impl NanoPass for PerceusVerifyPass {
    fn name(&self) -> &str { "PerceusVerify" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Wasm]) }

    fn run(&self, program: IrProgram, _target: Target) -> PassResult {
        for func in &program.functions {
            if func.is_test { continue; }
            // Lean-certified verify: uses perceus_verified::is_heap_type,
            // count_decs, count_incs (mirroring Lean 4 proven definitions)
            verify_function(func, &program.var_table);
        }
        PassResult { program, changed: false }
    }
}

/// Lean-certified verification. THE ACTUAL VERIFY uses
/// perceus_verified::verify_expr (mirrors Lean 4 proofs).
fn verify_function(func: &IrFunction, var_table: &VarTable) {
    perceus_verify_function(func, var_table);
}

/// Run Lean 4-certified Perceus RC verification on a single function.
/// Returns the number of violations found.
pub fn perceus_verify_function(func: &IrFunction, var_table: &VarTable) -> usize {
    let mut returned_vars: HashSet<VarId> = HashSet::new();
    collect_all_tail_vars(&func.body, &mut returned_vars);
    let mut moved_out_vars: HashSet<VarId> = HashSet::new();
    collect_moved_out_vars(&func.body, &mut moved_out_vars);
    let mut env_load_vars_set: HashSet<VarId> = HashSet::new();
    scan_env_loads(&func.body, &mut env_load_vars_set);

    let issues = super::perceus_verified::verify_expr(
        &func.body, var_table, &returned_vars, &moved_out_vars, &env_load_vars_set,
    );
    for (var, msg) in &issues {
        let name = var_table.get(*var).name.as_str();
        eprintln!("[perceus-belt] {}: `{}` (VarId {}) in `{}`",
            msg, name, var.0, func.name.as_str());
    }

    verify_branch_balance(&func.body, &HashSet::new(), &env_load_vars_set, var_table, func.name.as_str());
    issues.len()
}

fn scan_env_loads(expr: &IrExpr, vars: &mut HashSet<VarId>) {
    if let IrExprKind::Block { stmts, expr: tail } = &expr.kind {
        for stmt in stmts {
            if let IrStmtKind::Bind { var, value, ty, .. } = &stmt.kind {
                if is_heap_type(ty) && matches!(&value.kind, IrExprKind::EnvLoad { .. }) {
                    vars.insert(*var);
                }
            }
        }
        if let Some(t) = tail { scan_env_loads(t, vars); }
    }
    if let IrExprKind::If { then, else_, .. } = &expr.kind {
        scan_env_loads(then, vars); scan_env_loads(else_, vars);
    }
    if let IrExprKind::Match { arms, .. } = &expr.kind {
        for arm in arms { scan_env_loads(&arm.body, vars); }
    }
}

pub(crate) fn is_heap_type(ty: &Ty) -> bool {
    // `Ty::Named` is a DECLARED nominal record/variant (`type P = {...}`); its
    // runtime repr is a heap pointer (emit's `ty_to_valtype`/`byte_size` already
    // treat it as i32/4-byte). It must be classified heap so its locals get a
    // scope-end Dec and its alias-binds get an Inc — without this every declared
    // record/variant local leaks (anonymous `Ty::Record` was handled, the nominal
    // `Ty::Named` was not). An opaque alias to a heap type (`type H = String`) is
    // also a heap pointer; an alias to a scalar never reaches codegen as `Named`.
    matches!(ty, Ty::String | Ty::Applied(_, _) | Ty::Record { .. } | Ty::Named(..) | Ty::Unknown | Ty::Fn { .. })
}

/// Does `e`, bound to a heap local, yield a BORROWED ALIAS of an existing owned
/// heap value — as opposed to a freshly-owned allocation?
///
/// This is the exhaustive FRESH-vs-ALIAS classification at the heart of correct
/// reference counting. A local bound to an alias shares a refcount it does not
/// own; without an Inc at bind, its scope-end Dec under-counts and double-frees
/// the value the alias points into (still owned by its container/source). So an
/// alias-bound heap local must acquire its own reference (Inc-after-bind), while
/// a fresh-bound one already owns its single reference and must NOT be Inc'd
/// (that would leak). Returning/moving the alias out is also correct under this
/// rule: the Inc gives the escaping value its own reference, which the consumer's
/// Dec then balances.
///
/// The two directions are asymmetric in cost: a missing Inc on an alias =
/// double-free (a crash/hang), an extra Inc on a fresh value = a leak. The
/// classification is therefore total (no wildcard arm — a newly added
/// `IrExprKind` must be classified deliberately, not silently defaulted).
/// Tail-yielding forms (`match`/`if`/block) recurse into their tails: a value
/// flows out through the tail, so an alias in ANY tail makes the whole
/// expression able to yield an alias. `match` with a literal/data-constant
/// fallback arm stays correct because Inc/Dec are runtime no-ops on data-section
/// constants (`ptr < heap_start`).
/// Runtime calls that return a BORROWED ALIAS of an element of a heap container
/// argument (the stored pointer, no copy) — so a local or container that takes
/// the result must acquire its own reference, exactly like a `Member`/`Index`
/// access. Only the DIRECT-element accessors belong here; the Option-returning
/// lookups surface their alias through a `match` arm instead (see the call site).
fn is_alias_returning_runtime_call(symbol: &str) -> bool {
    matches!(symbol,
        "almide_rt_list_get_or" | "almide_rt_map_get_or"
        // unwrap_or peels the payload pointer straight out of the box —
        // identical to the IR UnwrapOr node classified below. Missing these
        // freed the JSON root while a live alias pointed at it, then a later
        // rc_inc RESURRECTED the freed block (silent corruption, json_gltf).
        | "almide_rt_result_unwrap_or" | "almide_rt_option_unwrap_or"
    )
}

/// Flatten loop-exit nesting so a `continue`/`break` becomes its enclosing
/// block's own TAIL. Two shapes, applied post-order (innermost first):
///   (a) Trailing discarded-exit statement — `Block{ …, Expr(X) ; tail: None }`
///       where `X` always exits — is the no-value form TCO emits when the
///       self-call is a discarded tail. Promote `X` to the block's tail.
///   (b) Tail block ending in an exit — `Block{ A ; tail: Block{ B ; tail: exit }}`
///       — hoist `B` up and make `exit` the outer tail.
/// After this, `insert_decs_before_ret` sees `continue`/`break` as the terminal
/// and emits each heap-local Dec as a same-level statement BEFORE it: reachable
/// (no leak) and at the Bind's level (counted by the flat verifier).
fn flatten_exit_tail_blocks(body: &mut IrExpr) {
    struct Flattener;
    impl IrMutVisitor for Flattener {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e); // children first (innermost nesting collapses first)
            if let IrExprKind::Block { stmts, expr } = &mut e.kind {
                // (a) Promote a trailing always-exit `Expr` statement to the tail.
                #[allow(clippy::collapsible_if)]
                if expr.is_none() {
                    if let Some(IrStmtKind::Expr { expr: last }) = stmts.last().map(|s| &s.kind) {
                        if expr_always_exits(last) {
                            if let Some(IrStmt { kind: IrStmtKind::Expr { expr: x }, .. }) = stmts.pop() {
                                *expr = Some(Box::new(x));
                            }
                        }
                    }
                }
                // (b) Absorb a tail block that ends in a bare exit.
                if let Some(tail) = expr {
                    while tail_block_ends_in_exit(tail) {
                        if let IrExprKind::Block { stmts: inner, expr: inner_tail } = &mut tail.kind {
                            let mut hoisted = std::mem::take(inner);
                            stmts.append(&mut hoisted);
                            *tail = inner_tail.take().expect("checked by tail_block_ends_in_exit");
                        } else {
                            break;
                        }
                    }
                }
            }
        }
    }
    Flattener.visit_expr_mut(body);
}

/// True iff `t` is a `Block` whose own tail is a bare `continue`/`break`.
fn tail_block_ends_in_exit(t: &IrExpr) -> bool {
    matches!(&t.kind, IrExprKind::Block { expr: Some(inner), .. }
        if matches!(inner.kind, IrExprKind::Continue | IrExprKind::Break))
}

/// Unconditionally exits the loop iteration (continue/break), possibly through a
/// trailing block/if/match — but NOT through a nested loop.
fn expr_always_exits(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::Continue | IrExprKind::Break => true,
        IrExprKind::Block { stmts, expr } =>
            expr.as_deref().is_some_and(expr_always_exits)
                || stmts.iter().any(|s| matches!(&s.kind,
                    IrStmtKind::Expr { expr } if expr_always_exits(expr))),
        IrExprKind::If { then, else_, .. } => expr_always_exits(then) && expr_always_exits(else_),
        IrExprKind::Match { arms, .. } =>
            !arms.is_empty() && arms.iter().all(|a| expr_always_exits(&a.body)),
        _ => false,
    }
}

pub(crate) fn yields_borrowed_alias(e: &IrExpr) -> bool {
    use IrExprKind::*;
    match &e.kind {
        // ── Definite aliases: borrow an existing owned reference ──
        Var { .. } | Clone { .. } | Deref { .. }
        | Member { .. } | TupleIndex { .. } | IndexAccess { .. }
        | MapAccess { .. } | OptionalChain { .. } => true,

        // ── Wrapper peels: extract the payload OUT of a Result/Option box ──
        // `r?`/`r!`/`o ?? d` surface the wrapped heap value, which the box owns;
        // a local bound to it shares the box's reference and must acquire its own
        // (else its scope-end Dec frees a value the box — or the container the box
        // borrowed from, e.g. `value.get(v, k)?` aliasing a field of `v` — still
        // holds). This holds whether the box is fresh or itself an alias, so the
        // peel is an alias unconditionally (for a heap payload; scalars are gated
        // out by `is_heap_type`). `UnwrapOr`'s fallback rides the same Inc — a
        // data-constant fallback makes it a runtime no-op, a fresh-heap fallback
        // leaks (the safe direction).
        Unwrap { .. } | ToOption { .. } | Try { .. } | UnwrapOr { .. } => true,

        // ── Direct element/value accessors: borrow an element of a container ──
        // `list.get_or`/`map.get_or` return the stored element POINTER directly
        // (no copy), so the result aliases the container exactly like a Member or
        // IndexAccess. The Option-returning lookups (`list.get`/`first`/`last`/
        // `find`, `map.get`) surface their aliased payload through a `match` arm
        // `Var`, already covered by the tail recursion below — so they are not
        // listed here (dup'ing their fresh Option box would not help the payload).
        RuntimeCall { symbol, .. } => is_alias_returning_runtime_call(symbol.as_str()),

        // The SAME accessors in their pre-StdlibLowering Call{Module} spelling.
        // `?? default` can reach Perceus as `Call { option.unwrap_or }` — the
        // RuntimeCall arm alone left that form classified FRESH, so the
        // refined inner-hoist dropped a load-bearing Inc and the map-owned
        // payload double-freed at teardown (map_insertion_order, group_by +
        // `?? []` extraction).
        Call { target: almide_ir::CallTarget::Module { module, func, .. }, .. } => {
            matches!(
                (module.as_str(), func.as_str()),
                ("option" | "result", "unwrap_or") | ("list" | "map", "get_or")
            )
        }

        // ── Degenerate single-part interpolation: aliases its only part ──
        // The WASM emitter short-circuits a 1-part `"${e}"` and returns `e`'s
        // pointer directly (no fresh alloc — `emit_string_interp`). When that
        // part is a heap String alias (`"${s}"` for a bound `s`), the result
        // shares `s`'s reference, so the binding needs its own +1 or its
        // scope-end Dec double-frees `s`'s buffer at teardown (#622). A literal
        // part, or a non-String part (Int/Bool/Float → a fresh `__int_to_string`
        // etc.), is genuinely fresh and stays classified below.
        StringInterp { parts } if parts.len() == 1 => {
            matches!(&parts[0],
                IrStringPart::Expr { expr }
                    if expr.ty == Ty::String && yields_borrowed_alias(expr))
        }

        // ── Tail-yielding forms: alias iff any tail can alias ──
        Match { arms, .. } => arms.iter().any(|a| yields_borrowed_alias(&a.body)),
        If { then, else_, .. } =>
            yields_borrowed_alias(then) || yields_borrowed_alias(else_),
        Block { expr: Some(tail), .. } => yields_borrowed_alias(tail),
        Block { expr: None, .. } => false,

        // ── Definite fresh allocations: the binding owns a new reference ──
        LitInt { .. } | LitFloat { .. } | LitBool { .. } | LitStr { .. } | Unit
        | OptionNone | Hole | Todo { .. }
        | List { .. } | Record { .. } | MapLiteral { .. } | EmptyMap | Tuple { .. }
        | StringInterp { .. } | SpreadRecord { .. }
        | Call { .. } | RenderedCall { .. } | RustMacro { .. }
        | InlineRust { .. } | TailCall { .. }
        | ResultOk { .. } | ResultErr { .. } | OptionSome { .. }
        | ClosureCreate { .. } | Lambda { .. } | FnRef { .. }
        | BinOp { .. } | UnOp { .. } | Range { .. } | ToVec { .. } | IterChain { .. }
        | Fan { .. } | Await { .. }
        | RcWrap { .. } | BoxNew { .. } | Borrow { .. } | EnvLoad { .. }
        | Break { .. } | Continue { .. } | While { .. } | ForIn { .. } => false,
    }
}


/// Insert RC operations into a function body using FnBody conversion.
/// Block IR → FnBody chain → Perceus rules → FnBody → Block IR.
fn insert_rc_ops(func: &mut IrFunction, var_table: &mut VarTable) -> bool {
    if func.is_test { return false; }

    // TCO loops emit the self-tail-call as a nested block ending in
    // continue/break; without flattening, scope-end Decs land AFTER the
    // nested exit — unreachable dead code (a leak per iteration) that the
    // flat verifier cannot see. Flatten first so the exits become block
    // tails and Decs are emitted same-level, reachable, and counted.
    flatten_exit_tail_blocks(&mut func.body);

    // Mechanism #6 — RETURN-ALIAS DUP: a function whose tail yields a
    // borrowed alias (a param, a pattern-bound payload, a member load …)
    // hands its caller a pointer the caller will own and Dec, while the
    // aliased source keeps its own owner and Dec. The callee must return
    // OWNED: bind the tail, Inc it, return the binding. Call results are
    // (correctly) classified FRESH at bind sites, so this is the only place
    // the missing +1 can be inserted. Mixed fresh/alias match arms take the
    // Inc on whichever value is produced — a fresh arm then leaks one count
    // (the safe direction). Data-section constants make the Inc a no-op.
    if is_heap_type(&func.ret_ty) && yields_borrowed_alias(&func.body) {
        let ret_ty = func.ret_ty.clone();
        let dup_var = var_table.alloc(
            almide_base::intern::sym("__ret_dup"),
            ret_ty.clone(),
            Mutability::Let,
            None,
        );
        let body = std::mem::replace(&mut func.body, IrExpr {
            kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None,
        });
        let span = body.span;
        // NOTE: no hand-built RcInc here — the Bind flows through the
        // ChainHead::VDecl arm whose Rule 1 fires on the IDENTICAL predicate
        // (is_heap && yields_borrowed_alias). The earlier explicit Inc
        // DOUBLE-applied with it: +1 rc leak per call of every
        // alias-returning function (verified by IR dump — two rc_incs on
        // __ret_dup).
        func.body = IrExpr {
            kind: IrExprKind::Block {
                stmts: vec![
                    IrStmt { kind: IrStmtKind::Bind { var: dup_var, mutability: Mutability::Let, ty: ret_ty.clone(), value: body }, span },
                ],
                expr: Some(Box::new(IrExpr { kind: IrExprKind::Var { id: dup_var }, ty: ret_ty, span: None, def_id: None })),
            },
            ty: func.ret_ty.clone(),
            span,
            def_id: None,
        };
    }

    // Apply Perceus recursively to the entire function body
    perceus_expr(&mut func.body, var_table);
    // Note: function parameters use borrow semantics — the CALLER owns the
    // value and Dec's it at scope exit. The callee does NOT Dec parameters.
    // This avoids double-free (caller Dec + callee Dec on same pointer).

    true
}

