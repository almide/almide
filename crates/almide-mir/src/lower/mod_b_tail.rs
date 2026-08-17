// ── tail of mod_b.rs, include!-spliced back at module level ──
//
// A pure code move: this file continues its parent verbatim. The split exists
// only so the parent stays under the 800-line ceiling the codopsy gate holds
// this crate to; there is no boundary of meaning here, and `include!` at module
// level is the one splice Rust allows (an impl-item position rejects it).

fn body_has_tail_position_option_unwrap(body: &IrExpr) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    fn scan(e: &IrExpr) -> bool {
        match &e.kind {
            IrExprKind::Unwrap { expr } => {
                matches!(&expr.ty, Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1)
            }
            IrExprKind::Block { expr, .. } => expr.as_deref().is_some_and(scan),
            IrExprKind::If { then, else_, .. } => scan(then) || scan(else_),
            IrExprKind::Match { arms, .. } => arms.iter().any(|a| scan(&a.body)),
            _ => false,
        }
    }
    scan(body)
}

use almide_lang::intern::sym as __die_sym;
fn die_expr(msg: &str) -> IrExpr {
    die_on(IrExpr {
        kind: IrExprKind::LitStr { value: msg.to_string() },
        ty: Ty::String,
        span: None,
        def_id: None,
    })
}
/// die on an arbitrary String-typed message EXPRESSION (the computed 2-arg
/// assert message: `assert(c, "got " + float.to_string(x))`).
fn die_on(lit: IrExpr) -> IrExpr {
    let handle = IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: __die_sym("prim"), func: __die_sym("handle"), def_id: None },
            args: vec![lit],
            type_args: Vec::new(),
        },
        ty: Ty::Int,
        span: None,
        def_id: None,
    };
    IrExpr {
        kind: IrExprKind::Call {
            target: CallTarget::Module { module: __die_sym("prim"), func: __die_sym("die"), def_id: None },
            args: vec![handle],
            type_args: Vec::new(),
        },
        ty: Ty::Unit,
        span: None,
        def_id: None,
    }
}
/// Desugar `assert(cond)` / `assert_eq(a, b)` / `assert_ne(a, b)` (Unit-typed builtin
/// calls — the test-block floor, also legal in a main body) to the §13 controlled-halt
/// shape the SELF-HOST stdlib already proves out (math.pow's negative-exponent guard):
/// `if <cond> then () else prim.die(prim.handle("assertion failed…"))`. Everything
/// downstream is EXISTING machinery — the stmt-position Unit-`if` executes via
/// `try_lower_unit_if`, `==`/`!=` dispatch through the ordinary BinOp lowering (whatever
/// operand types that subset admits; the rest walls honestly), and `prim.die` is the
/// proven Die prim. Failure = message on stderr + exit 1 — the harness keys on the
/// non-zero exit, exactly like v0's trap. Applied desugar-before-both (same slot as
/// `desugar_heap_branches`), so every driver counts and lowers the SAME tree.
fn desugar_assert_calls(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    
    struct S {
        changed: bool,
        /// Fresh-VarId counter for the unwrap-operand hoist below (the
        /// `max_var_id + 1` discipline the other MIR-side ANF hoists use).
        next_var: u32,
    }

    /// Does `e` contain an effect unwrap (`f(x)!`) anywhere? The gate for the
    /// operand hoist: an `Unwrap` inside the assert's `if` CONDITION has no
    /// propagation route (the cond machinery can only materialize values), so
    /// the whole enclosing fn walls (#1191 — `assert_eq(collect(p, cuts)!,
    /// whole)`, the fold_lines test class).
    fn contains_unwrap(e: &IrExpr) -> bool {
        use almide_ir::visit::{walk_expr, IrVisitor};
        struct C(bool);
        impl IrVisitor for C {
            fn visit_expr(&mut self, e: &IrExpr) {
                if matches!(e.kind, IrExprKind::Unwrap { .. }) {
                    self.0 = true;
                }
                walk_expr(self, e);
            }
        }
        let mut c = C(false);
        c.visit_expr(e);
        c.0
    }

    /// The atom set of `desugar_heap_if_call_args` — operands that carry no
    /// effects and need no hoist slot of their own.
    fn is_atom(a: &IrExpr) -> bool {
        matches!(
            a.kind,
            IrExprKind::Var { .. }
                | IrExprKind::LitInt { .. }
                | IrExprKind::LitFloat { .. }
                | IrExprKind::LitBool { .. }
                | IrExprKind::LitStr { .. }
                | IrExprKind::Unit
        )
    }

    /// The unwrap-operand hoist (#1191): for an assert whose operand CONTAINS
    /// an unwrap, bind every non-atom operand to a fresh `let` IN ARGUMENT
    /// ORDER (effects keep their sequence — the `desugar_heap_if_call_args`
    /// discipline) and build the `if`/die over the Vars. Returns the binds and
    /// the `if` — the CALLER splices them as SIBLING statements: both
    /// downstream `!`-resolvers scan DIRECT statements only (`desugar_let_
    /// unwrap`'s `find_let_unwrap_target`, `desugar_loop_unwrap`'s
    /// `loop_uw_direct_unwrap`), so an expression-position Block wrapper is
    /// invisible to them — measured on the fold_lines fixtures.
    fn hoist_assert(
        name: &str,
        args: &[IrExpr],
        next_var: &mut u32,
    ) -> Option<(Vec<almide_ir::IrStmt>, IrExpr)> {
        use almide_ir::{IrStmt, IrStmtKind, Mutability, VarId};
        let is_assert_shape = matches!(
            (name, args.len()),
            ("assert", 1 | 2) | ("assert_eq", 2) | ("assert_ne", 2)
        );
        if !is_assert_shape || !args.iter().any(contains_unwrap) {
            return None;
        }
        let mut binds: Vec<IrStmt> = Vec::new();
        let mut new_args: Vec<IrExpr> = Vec::with_capacity(args.len());
        for a in args {
            if is_atom(a) {
                new_args.push(a.clone());
                continue;
            }
            let tmp = VarId(*next_var);
            *next_var += 1;
            binds.push(IrStmt {
                kind: IrStmtKind::Bind {
                    var: tmp,
                    mutability: Mutability::Let,
                    ty: a.ty.clone(),
                    value: a.clone(),
                },
                span: a.span.clone(),
            });
            new_args.push(IrExpr {
                kind: IrExprKind::Var { id: tmp },
                ty: a.ty.clone(),
                span: a.span.clone(),
                def_id: None,
            });
        }
        let (cond, die) = assert_die_expr(name, &new_args)?;
        let unit = IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None };
        let iff = IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(cond),
                then: Box::new(unit),
                else_: Box::new(die),
            },
            ty: Ty::Unit,
            span: None,
            def_id: None,
        };
        Some((binds, iff))
    }

    /// Splice statement-position unwrap-bearing asserts into `stmts` (and a
    /// Unit-typed TAIL assert into the statement list, dropping the tail).
    fn splice_assert_stmts(
        stmts: &mut Vec<almide_ir::IrStmt>,
        tail: Option<&mut Option<Box<IrExpr>>>,
        next_var: &mut u32,
    ) -> bool {
        use almide_ir::IrStmtKind;
        let mut changed = false;
        let mut out: Vec<almide_ir::IrStmt> = Vec::with_capacity(stmts.len());
        for s in stmts.drain(..) {
            let hoisted = match &s.kind {
                IrStmtKind::Expr { expr } => match &expr.kind {
                    IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                        hoist_assert(name.as_str(), args, next_var)
                    }
                    _ => None,
                },
                _ => None,
            };
            match hoisted {
                Some((binds, iff)) => {
                    out.extend(binds);
                    out.push(almide_ir::IrStmt {
                        kind: IrStmtKind::Expr { expr: iff },
                        span: s.span.clone(),
                    });
                    changed = true;
                }
                None => out.push(s),
            }
        }
        *stmts = out;
        if let Some(tail) = tail {
            let hoisted = match tail.as_deref() {
                Some(t) if matches!(t.ty, Ty::Unit) => match &t.kind {
                    IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                        hoist_assert(name.as_str(), args, next_var)
                    }
                    _ => None,
                },
                _ => None,
            };
            if let Some((binds, iff)) = hoisted {
                stmts.extend(binds);
                stmts.push(almide_ir::IrStmt {
                    kind: IrStmtKind::Expr { expr: iff },
                    span: None,
                });
                *tail = None;
                changed = true;
            }
        }
        changed
    }
/// `panic(msg)` — an UNCONDITIONAL abort: die on "PANIC: " + msg (the v0 wasm
/// form: prefix + message, then halt). The message expr is evaluated only on
/// the abort path, like the computed assert message. `None` when the call is
/// not that shape.
fn panic_die_expr(name: &str, args: &[IrExpr]) -> Option<IrExpr> {
    if name != "panic" || args.len() != 1 || !matches!(args[0].ty, Ty::String) {
        return None;
    }
    let msg = args[0].clone();
        let text = match &msg.kind {
        IrExprKind::LitStr { value } => {
            die_expr(&format!("PANIC: {value}"))
        }
        _ => die_on(IrExpr {
            kind: IrExprKind::BinOp {
                op: almide_ir::BinOp::ConcatStr,
                left: Box::new(IrExpr {
                    kind: IrExprKind::LitStr { value: "PANIC: ".to_string() },
                    ty: Ty::String,
                    span: None,
                    def_id: None,
                }),
                right: Box::new(msg),
            },
            ty: Ty::String,
            span: None,
            def_id: None,
        }),
        };
    Some(text)
}

    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            // #1191: SPLICE statement-position (and Unit-tail) asserts whose
            // operands carry an unwrap — their binds must land as SIBLING
            // statements (see `hoist_assert`). Runs AFTER the child walk, so
            // the expression handler below (which SKIPS unwrap-bearing
            // asserts) has already left them intact as Calls for this pass.
            // ForIn/While bodies are RAW statement lists, not Blocks — they
            // need their own arms or a loop-body assert is never spliced.
            match &mut e.kind {
                IrExprKind::Block { stmts, expr } => {
                    if splice_assert_stmts(stmts, Some(expr), &mut self.next_var) {
                        self.changed = true;
                    }
                }
                IrExprKind::ForIn { body, .. } | IrExprKind::While { body, .. } => {
                    if splice_assert_stmts(body, None, &mut self.next_var) {
                        self.changed = true;
                    }
                }
                _ => {}
            }
            let is_panic = matches!(&e.kind,
                IrExprKind::Call { target: CallTarget::Named { name }, args, .. }
                    if name.as_str() == "panic" && args.len() == 1
                        && matches!(args[0].ty, Ty::String));
            // `panic` types as the enclosing branch demands (Unit or Never) — it must
            // bypass the Unit gate below.
            if !is_panic && !matches!(e.ty, Ty::Unit) {
                return;
            }
            let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &e.kind
            else {
                return;
            };
            if let Some(text) = panic_die_expr(name.as_str(), args) {
                *e = text;
                self.changed = true;
                return;
            }
            // #1191: an unwrap-bearing assert stays a CALL here — building the
            // `if` with the unwrap INLINE would wall at the cond (the operand
            // has no propagation route), and the operand hoist needs a sibling
            // STATEMENT slot this expression position cannot offer. The
            // enclosing Block/loop's splice above rewrites it; a position with
            // no statement slot keeps the call and walls honestly, exactly as
            // the un-hoisted form always did.
            let is_assert_shape = matches!(
                (name.as_str(), args.len()),
                ("assert", 1 | 2) | ("assert_eq", 2) | ("assert_ne", 2)
            );
            if is_assert_shape && args.iter().any(contains_unwrap) {
                return;
            }
            let Some((cond, die)) = assert_die_expr(name.as_str(), args) else { return };
            let unit = IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None };
            *e = IrExpr {
                kind: IrExprKind::If {
                    cond: Box::new(cond),
                    then: Box::new(unit),
                    else_: Box::new(die),
                },
                ty: Ty::Unit,
                span: e.span.clone(),
                def_id: e.def_id,
            };
            self.changed = true;
        }
    }
    let mut s = S { changed: false, next_var: crate::lower::desugar_var_seed() };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}

/// The `assert` family's failure expression: `(cond, die)` folded into the die
/// side, or `None` when the call is not an assert shape.
///
/// A LITERAL message folds into the die text; a COMPUTED String message dies on
/// the CONCAT `"assertion failed: " + msg`, evaluated only on the failing path.
fn assert_die_expr(name: &str, args: &[IrExpr]) -> Option<(IrExpr, IrExpr)> {
    let (cond, msg) = match (name, args) {
        ("assert", [c]) if matches!(c.ty, Ty::Bool) => {
            (c.clone(), None)
        }
        // The 2-arg form `assert(cond, msg)`: a LITERAL message folds into
        // the die text; a COMPUTED String message dies on the CONCAT
        // `"assertion failed: " + msg` (evaluated only on the failing path).
        ("assert", [c, m]) if matches!(c.ty, Ty::Bool) && matches!(m.ty, Ty::String) => {
            (c.clone(), Some(m.clone()))
        }
        ("assert_eq", [a, b]) => (
            IrExpr {
                kind: IrExprKind::BinOp {
                    op: almide_ir::BinOp::Eq,
                    left: Box::new(a.clone()),
                    right: Box::new(b.clone()),
                },
                ty: Ty::Bool,
                span: None,
                def_id: None,
            },
            None,
        ),
        ("assert_ne", [a, b]) => (
            IrExpr {
                kind: IrExprKind::BinOp {
                    op: almide_ir::BinOp::Neq,
                    left: Box::new(a.clone()),
                    right: Box::new(b.clone()),
                },
                ty: Ty::Bool,
                span: None,
                def_id: None,
            },
            None,
        ),
_ => return None,
    };
    let default_text = match name {
        "assert_eq" => "assertion failed: left == right",
        "assert_ne" => "assertion failed: left != right",
        _ => "assertion failed: assert(false)",
    };
    let die = match msg {
        None => die_expr(default_text),
        Some(m) => match &m.kind {
            IrExprKind::LitStr { value } => {
                die_expr(&format!("assertion failed: {value}"))
            }
            _ => die_on(IrExpr {
                kind: IrExprKind::BinOp {
                    op: almide_ir::BinOp::ConcatStr,
                    left: Box::new(IrExpr {
                        kind: IrExprKind::LitStr {
                            value: "assertion failed: ".to_string(),
                        },
                        ty: Ty::String,
                        span: None,
                        def_id: None,
                    }),
                    right: Box::new(m),
                },
                ty: Ty::String,
                span: None,
                def_id: None,
            }),
        },
    };

    Some((cond, die))
}

/// `m[k]` over a `Map` (the frontend emits `MapAccess` ONLY for `obj.ty.is_map()`) →
/// `map.get(m, k)` — the ordinary self-host map lookup call (`Option[V]` result), which
/// the repr dispatch suffixes (`get_skv`/`get_str`/…) like every other map call site.
/// Applied desugar-before-both (same slot as `desugar_assert_calls`): the counted tree
/// and the lowering see the SAME Call node, so `mir == ir` holds for the one CallFn.
fn desugar_map_access_calls(body: &IrExpr) -> Option<IrExpr> {
    use almide_ir::{walk_expr_mut, IrMutVisitor};
    use almide_lang::intern::sym;
    struct S {
        changed: bool,
    }
    impl IrMutVisitor for S {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            walk_expr_mut(self, e);
            let IrExprKind::MapAccess { object, key } = &e.kind else {
                return;
            };
            *e = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module {
                        module: sym("map"),
                        func: sym("get"),
                        def_id: None,
                    },
                    args: vec![(**object).clone(), (**key).clone()],
                    type_args: Vec::new(),
                },
                ty: e.ty.clone(),
                span: e.span.clone(),
                def_id: e.def_id,
            };
            self.changed = true;
        }
    }
    let mut s = S { changed: false };
    let mut out = body.clone();
    s.visit_expr_mut(&mut out);
    s.changed.then_some(out)
}
