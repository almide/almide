// ── eval.rs, part 5: blocks and the tail-call spine walker ──
//
// include!-spliced into `eval.rs` at module level (#1856). `eval_block` and
// `eval_body_spine` — the iterative walk of a body's tail spine that hands
// transferable calls back to `run_callable`'s trampoline (lib_run.rs).

impl<'a> Interpreter<'a> {
    // ── Blocks ──────────────────────────────────────────────────

    fn eval_block(
        &mut self,
        stmts: &[IrStmt],
        tail: Option<&IrExpr>,
        scope: &Scope,
    ) -> Flow {
        // A block introduces a new lexical frame.
        let frame = scope.child();
        for stmt in stmts {
            if let Err(f) = self.exec_stmt(stmt, &frame) {
                return f;
            }
        }
        match tail {
            Some(e) => self.eval_expr(e, &frame),
            None => Flow::val(Value::Unit),
        }
    }

    // ── The tail-call trampoline's spine walker ─────────────────

    /// Walk a function/closure body's TAIL SPINE iteratively — Block tails,
    /// If branches, the effect `Try{Call}` wrapper — and report whether it
    /// ends in a transferable call ([`crate::SpineOutcome::Transfer`]) or in
    /// anything else (evaluated normally, [`crate::SpineOutcome::Done`]).
    /// The engine looping on this is [`crate::Interpreter::run_callable`];
    /// everything NOT on the spine evaluates through the ordinary recursive
    /// `eval_expr`, so semantics change nowhere except stack growth.
    pub(crate) fn eval_body_spine(
        &mut self,
        body: &IrExpr,
        frame: &Scope,
    ) -> crate::SpineOutcome<'a> {
        use crate::SpineOutcome;
        let mut scope = frame.clone();
        let mut cur = body;
        loop {
            match &cur.kind {
                IrExprKind::Block { stmts, expr: Some(tail) } => {
                    let child = scope.child();
                    for s in stmts {
                        if let Err(f) = self.exec_stmt(s, &child) {
                            return SpineOutcome::Done(f);
                        }
                    }
                    scope = child;
                    cur = tail;
                }
                IrExprKind::If { cond, then, else_ } => match self.eval_expr(cond, &scope) {
                    Flow::Value(Value::Bool(true)) => cur = then,
                    Flow::Value(Value::Bool(false)) => cur = else_,
                    Flow::Value(other) => {
                        return SpineOutcome::Done(Flow::Abort(format!(
                            "internal: if-condition is {} not Bool",
                            other.type_name()
                        )))
                    }
                    other => return SpineOutcome::Done(other),
                },
                // The effect wrapper `f(..)!` in tail position: transfer the
                // call and let the engine fold the marker's normalization
                // over the chain's final value (the pending list).
                IrExprKind::Try { expr: inner } | IrExprKind::Unwrap { expr: inner } => {
                    return match self.spine_tail_call(inner, &scope) {
                        Some(SpineOutcome::Transfer { next, next_args, try_marker: None }) => {
                            SpineOutcome::Transfer {
                                next,
                                next_args,
                                try_marker: Some(marker_is_option_identity(&cur.ty)),
                            }
                        }
                        Some(out) => out,
                        None => SpineOutcome::Done(self.eval_expr(cur, &scope)),
                    };
                }
                IrExprKind::Call { .. } => {
                    return match self.spine_tail_call(cur, &scope) {
                        Some(out) => out,
                        None => SpineOutcome::Done(self.eval_expr(cur, &scope)),
                    };
                }
                _ => return SpineOutcome::Done(self.eval_expr(cur, &scope)),
            }
        }
    }

    /// The call half of the spine walk. `None` = "not transferable, and
    /// NOTHING has been evaluated yet" — the caller re-evaluates the node
    /// through the ordinary path. `Some(Done)` = a terminal tier (builtin,
    /// non-Value arg flow) resolved it here. `Some(Transfer)` = tail call to
    /// a lowered fn / closure with its arguments already evaluated.
    ///
    /// Resolution order mirrors `eval_named_call` exactly: builtins first
    /// (they may evaluate args, so once consulted this fn must complete),
    /// then variant ctors / Endian (pure lookups — declined to the normal
    /// path), then the fn table. A `mut`-param callee declines the transfer:
    /// copy-out needs the caller's lvalue, which a transferred frame no
    /// longer has (#1022).
    fn spine_tail_call(
        &mut self,
        expr: &IrExpr,
        scope: &Scope,
    ) -> Option<crate::SpineOutcome<'a>> {
        use crate::{SpineOutcome, TailCallee};
        let IrExprKind::Call { target, args, .. } = &expr.kind else {
            return None;
        };
        match target {
            CallTarget::Named { name } => {
                if let Some(flow) = self.eval_builtin_call(name.as_str(), args, scope) {
                    return Some(SpineOutcome::Done(flow));
                }
                if self.variant_ctor(*name).is_some()
                    || (args.is_empty()
                        && matches!(name.as_str(), "LittleEndian" | "BigEndian"))
                {
                    return None;
                }
                let func = self.fns.get(name).copied()?;
                if func.params.iter().any(|p| p.is_mut) {
                    return None;
                }
                let mut evaled = Vec::with_capacity(args.len());
                for a in args {
                    match self.eval_expr(a, scope) {
                        Flow::Value(v) => evaled.push(v),
                        other => return Some(SpineOutcome::Done(other)),
                    }
                }
                Some(SpineOutcome::Transfer {
                    next: TailCallee::Fn(func),
                    next_args: evaled,
                    try_marker: None,
                })
            }
            CallTarget::Computed { callee } => {
                let f = match self.eval_expr(callee, scope) {
                    Flow::Value(v) => v,
                    other => return Some(SpineOutcome::Done(other)),
                };
                let Value::Closure(clo) = f else {
                    return Some(SpineOutcome::Done(Flow::Abort(format!(
                        "internal: call of non-closure {}",
                        f.type_name()
                    ))));
                };
                let mut evaled = Vec::with_capacity(args.len());
                for a in args {
                    match self.eval_expr(a, scope) {
                        Flow::Value(v) => evaled.push(v),
                        other => return Some(SpineOutcome::Done(other)),
                    }
                }
                Some(SpineOutcome::Transfer {
                    next: TailCallee::Clo(clo),
                    next_args: evaled,
                    try_marker: None,
                })
            }
            // Module / Method tails keep the ordinary path: their dispatch
            // has value-keyed pre-body tiers (container ops, prims, bridge)
            // whose claim order cannot be probed without evaluating.
            _ => None,
        }
    }
}
