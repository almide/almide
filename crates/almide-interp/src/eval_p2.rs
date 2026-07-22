impl<'a> Interpreter<'a> {
    // ── Loops ───────────────────────────────────────────────────

    fn eval_for_in(
        &mut self,
        var: VarId,
        var_tuple: Option<&[VarId]>,
        iterable: &IrExpr,
        body: &[IrStmt],
        scope: &Scope,
    ) -> Flow {
        let iter_v = val!(self.eval_expr(iterable, scope));
        // #561: a Range iterates LAZILY — never materialized into a Vec. The
        // eager materialization in `as_iter_items` allocated the entire range
        // up front with no fuel accounting, so `for i in 0..2_000_000_000`
        // OOM-ABORTED the process (uncatchable by the fuzzer oracle) before a
        // single fuel check. Generating each value on demand keeps the loop
        // fuel-bounded: it stops at `Flow::Fuel`, never allocating the range.
        let items: Vec<Value> = match &iter_v {
            Value::Range { start, end, inclusive } => {
                return self.eval_for_in_range(var, var_tuple, *start, *end, *inclusive, body, scope);
            }
            _ => match iter_v.as_iter_items() {
                Some(items) => items,
                None => {
                    return Flow::Abort(format!(
                        "internal: for-in over non-iterable {}",
                        iter_v.type_name()
                    ))
                }
            },
        };
        for item in items {
            if let Err(f) = self.step() {
                return f;
            }
            let frame = scope.child();
            // Destructure tuple binding `for (a, b) in ...`.
            if let Some(vars) = var_tuple {
                match &item {
                    Value::Tuple(elems) if elems.len() == vars.len() => {
                        for (vid, ev) in vars.iter().zip(elems.iter()) {
                            frame.bind(*vid, ev.clone());
                        }
                    }
                    _ => {
                        return Flow::Abort(
                            "internal: for-in tuple destructure shape mismatch".into(),
                        )
                    }
                }
            } else {
                frame.bind(var, item);
            }
            for stmt in body {
                match self.exec_stmt(stmt, &frame) {
                    Ok(()) => {}
                    Err(Flow::Break) => return Flow::val(Value::Unit),
                    Err(Flow::Continue) => break,
                    Err(other) => return other,
                }
            }
        }
        Flow::val(Value::Unit)
    }

    /// #561: lazy `for i in start..end` — generates each Int on demand and
    /// charges fuel per iteration, so an adversarially huge range terminates
    /// as `Flow::Fuel` instead of materializing (and OOM-aborting) the whole
    /// range. A tuple binder over a Range is shape-invalid (Ints aren't
    /// tuples), matching the eager path's mismatch abort.
    fn eval_for_in_range(
        &mut self,
        var: VarId,
        var_tuple: Option<&[VarId]>,
        start: i64,
        end: i64,
        inclusive: bool,
        body: &[IrStmt],
        scope: &Scope,
    ) -> Flow {
        let last = if inclusive { end } else { end - 1 };
        let mut i = start;
        while i <= last {
            if let Err(f) = self.step() {
                return f;
            }
            let frame = scope.child();
            if var_tuple.is_some() {
                return Flow::Abort(
                    "internal: for-in tuple destructure shape mismatch".into(),
                );
            }
            frame.bind(var, Value::Int(i));
            for stmt in body {
                match self.exec_stmt(stmt, &frame) {
                    Ok(()) => {}
                    Err(Flow::Break) => return Flow::val(Value::Unit),
                    Err(Flow::Continue) => break,
                    Err(other) => return other,
                }
            }
            i += 1;
        }
        Flow::val(Value::Unit)
    }

    fn eval_while(&mut self, cond: &IrExpr, body: &[IrStmt], scope: &Scope) -> Flow {
        loop {
            if let Err(f) = self.step() {
                return f;
            }
            let c = val!(self.eval_expr(cond, scope));
            match c {
                Value::Bool(true) => {}
                Value::Bool(false) => return Flow::val(Value::Unit),
                other => {
                    return Flow::Abort(format!(
                        "internal: while-condition is {} not Bool",
                        other.type_name()
                    ))
                }
            }
            let frame = scope.child();
            let mut broke = false;
            for stmt in body {
                match self.exec_stmt(stmt, &frame) {
                    Ok(()) => {}
                    Err(Flow::Break) => {
                        broke = true;
                        break;
                    }
                    Err(Flow::Continue) => break,
                    Err(other) => return other,
                }
            }
            if broke {
                return Flow::val(Value::Unit);
            }
        }
    }

    // ── Statements ──────────────────────────────────────────────

    fn exec_stmt(&mut self, stmt: &IrStmt, scope: &Scope) -> Result<(), Flow> {
        if let Err(f) = self.step() {
            return Err(f);
        }
        match &stmt.kind {
            IrStmtKind::Bind { var, value, .. } => {
                let v = match self.eval_expr(value, scope) {
                    Flow::Value(v) => v,
                    other => return Err(other),
                };
                scope.bind(*var, v);
                Ok(())
            }
            IrStmtKind::BindDestructure { pattern, value } => {
                let v = match self.eval_expr(value, scope) {
                    Flow::Value(v) => v,
                    other => return Err(other),
                };
                let mut binds = Vec::new();
                if self.try_match(pattern, &v, &mut binds) {
                    for (id, val) in binds {
                        scope.bind(id, val);
                    }
                    Ok(())
                } else {
                    Err(Flow::Abort("internal: irrefutable destructure failed".into()))
                }
            }
            IrStmtKind::Assign { var, value } => {
                let v = match self.eval_expr(value, scope) {
                    Flow::Value(v) => v,
                    other => return Err(other),
                };
                if !scope.assign(*var, v) {
                    return Err(Flow::Abort(format!(
                        "internal: assign to unbound variable {:?}",
                        var
                    )));
                }
                Ok(())
            }
            IrStmtKind::IndexAssign { target, index, value } => {
                self.exec_stmt_index_assign(*target, index, value, scope)
            }
            IrStmtKind::MapInsert { target, key, value } => {
                self.exec_stmt_map_insert(*target, key, value, scope)
            }
            IrStmtKind::FieldAssign { target, field, value } => {
                self.exec_stmt_field_assign(*target, *field, value, scope)
            }
            IrStmtKind::Guard { cond, else_ } => {
                let c = match self.eval_expr(cond, scope) {
                    Flow::Value(v) => v,
                    other => return Err(other),
                };
                match c {
                    Value::Bool(true) => Ok(()),
                    Value::Bool(false) => {
                        // The else branch is an early-return expression.
                        match self.eval_expr(else_, scope) {
                            Flow::Value(v) => Err(Flow::Return(v)),
                            other => Err(other),
                        }
                    }
                    other => Err(Flow::Abort(format!(
                        "internal: guard condition is {} not Bool",
                        other.type_name()
                    ))),
                }
            }
            IrStmtKind::Expr { expr } => {
                match self.eval_expr(expr, scope) {
                    Flow::Value(_) => Ok(()),
                    other => Err(other),
                }
            }
            IrStmtKind::Comment { .. } => Ok(()),

            // ── Codegen-inserted statement kinds ──
            // RcInc/RcDec are pure refcount bookkeeping (Perceus, post-cut) —
            // semantic no-ops for values. Degrade to no-op (belt-and-braces) so
            // a future post-Perceus run doesn't panic.
            IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. } => Ok(()),
            IrStmtKind::ListSwap { .. }
            | IrStmtKind::ListReverse { .. }
            | IrStmtKind::ListRotateLeft { .. }
            | IrStmtKind::ListCopySlice { .. } => unreachable!(
                "list peephole stmt is codegen-inserted (PeepholePass); interp runs pre-codegen"
            ),
        }
    }

    // ── exec_stmt's assign-family arms ─────────────────────────

    fn exec_stmt_index_assign(
        &mut self,
        target: VarId,
        index: &IrExpr,
        value: &IrExpr,
        scope: &Scope,
    ) -> Result<(), Flow> {
        let iv = match self.eval_expr(index, scope) {
            Flow::Value(v) => v,
            other => return Err(other),
        };
        let vv = match self.eval_expr(value, scope) {
            Flow::Value(v) => v,
            other => return Err(other),
        };
        let cur = scope
            .get(target)
            .ok_or_else(|| Flow::Abort("internal: index-assign to unbound list".into()))?;
        match (cur, iv) {
            (Value::List(xs), Value::Int(i)) => {
                if i < 0 || (i as usize) >= xs.len() {
                    return Err(Flow::Abort("index out of bounds".into()));
                }
                let mut new = (*xs).clone();
                new[i as usize] = vv;
                scope.assign(target, Value::list(new));
                Ok(())
            }
            _ => Err(Flow::Abort("internal: malformed index-assign".into())),
        }
    }

    fn exec_stmt_map_insert(
        &mut self,
        target: VarId,
        key: &IrExpr,
        value: &IrExpr,
        scope: &Scope,
    ) -> Result<(), Flow> {
        let kv = match self.eval_expr(key, scope) {
            Flow::Value(v) => v,
            other => return Err(other),
        };
        let vv = match self.eval_expr(value, scope) {
            Flow::Value(v) => v,
            other => return Err(other),
        };
        let cur = scope
            .get(target)
            .ok_or_else(|| Flow::Abort("internal: map-insert to unbound map".into()))?;
        match cur {
            Value::Map(entries) => {
                let mut new = (*entries).clone();
                map_insert(&mut new, kv, vv);
                scope.assign(target, Value::Map(Rc::new(new)));
                Ok(())
            }
            _ => Err(Flow::Abort("internal: map-insert on non-Map".into())),
        }
    }

    fn exec_stmt_field_assign(
        &mut self,
        target: VarId,
        field: Sym,
        value: &IrExpr,
        scope: &Scope,
    ) -> Result<(), Flow> {
        let vv = match self.eval_expr(value, scope) {
            Flow::Value(v) => v,
            other => return Err(other),
        };
        let cur = scope
            .get(target)
            .ok_or_else(|| Flow::Abort("internal: field-assign to unbound record".into()))?;
        match cur {
            Value::Record { name, fields } => {
                let mut new = (*fields).clone();
                if let Some(slot) = new.iter_mut().find(|(k, _)| *k == field) {
                    slot.1 = vv;
                } else {
                    new.push((field, vv));
                }
                scope.assign(target, Value::Record { name, fields: Rc::new(new) });
                Ok(())
            }
            _ => Err(Flow::Abort("internal: field-assign on non-Record".into())),
        }
    }
}
