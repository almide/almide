//! The tree-walking evaluator: `IrExpr` / `IrStmt` / `IrPattern` → `Value`.
//!
//! Every eval step burns one unit of fuel. Codegen-inserted node kinds (Clone,
//! Borrow, IterChain, ClosureCreate, …) are unreachable at the pre-codegen cut
//! point and panic with an explanatory message to document the boundary.

use std::rc::Rc;

use almide_base::intern::Sym;
use almide_ir::{
    BinOp, IrExpr, IrExprKind, IrMatchArm, IrPattern, IrStmt, IrStmtKind, IrStringPart, UnOp,
    VarId,
};
use almide_lang::types::Ty;

use crate::env::Scope;
use crate::value::{Closure, Value, VariantPayload};
use crate::{Flow, Interpreter};

/// Helper: short-circuit a `Flow` that is not a plain value out of an
/// expression evaluator. Returns the inner `Value`, or propagates the signal.
macro_rules! val {
    ($flow:expr) => {
        match $flow {
            Flow::Value(v) => v,
            other => return other,
        }
    };
}

impl<'a> Interpreter<'a> {
    pub(crate) fn eval_expr(&mut self, expr: &IrExpr, scope: &Scope) -> Flow {
        if let Err(f) = self.step() {
            return f;
        }
        match &expr.kind {
            // ── Literals ──
            IrExprKind::LitInt { value } => Flow::val(Value::Int(*value)),
            IrExprKind::LitFloat { value } => Flow::val(Value::Float(*value)),
            IrExprKind::LitStr { value } => Flow::val(Value::str(value.clone())),
            IrExprKind::LitBool { value } => Flow::val(Value::Bool(*value)),
            IrExprKind::Unit => Flow::val(Value::Unit),

            // ── Variables ──
            IrExprKind::Var { id } => match scope.get(*id) {
                Some(v) => Flow::val(v),
                None => Flow::Abort(format!(
                    "internal: unbound variable {:?} ({})",
                    id,
                    self.var_name(*id)
                )),
            },
            // A named function used as a value: wrap it as a closure-like value
            // by capturing its name; application looks it up. We model it as a
            // closure whose body re-dispatches to the named fn.
            IrExprKind::FnRef { name } => self.fn_ref_value(*name, scope),

            // ── Operators ──
            IrExprKind::BinOp { op, left, right } => self.eval_binop(*op, left, right, scope),
            IrExprKind::UnOp { op, operand } => {
                let v = val!(self.eval_expr(operand, scope));
                self.eval_unop(*op, v)
            }

            // ── Control flow ──
            IrExprKind::If { cond, then, else_ } => {
                let c = val!(self.eval_expr(cond, scope));
                match c {
                    Value::Bool(true) => self.eval_expr(then, scope),
                    Value::Bool(false) => self.eval_expr(else_, scope),
                    other => Flow::Abort(format!(
                        "internal: if-condition is {} not Bool",
                        other.type_name()
                    )),
                }
            }
            IrExprKind::Match { subject, arms } => self.eval_match(subject, arms, scope),
            IrExprKind::Block { stmts, expr } => self.eval_block(stmts, expr.as_deref(), scope),
            // Fan block: evaluate each expr SEQUENTIALLY in source order — the
            // deterministic mode both backends collapse to (WASM has no threads;
            // native's `fan_effect`/`fan_expr` join in handle order). Both
            // backends materialize the results as a TUPLE, not a list:
            //   - native walker `render_fan` joins as `(j0, j1, ...)` for >1 expr,
            //     and a bare `j0` for exactly one expr;
            //   - WASM `emit_wasm/expressions.rs::Fan` builds a tuple (`>1`) or
            //     emits the single value bare (`==1`).
            // Each Result-typed spawn body auto-unwraps with `?` semantics at the
            // join (`handle.join().unwrap()?`): on Ok the inner value is taken, on
            // Err the enclosing fn short-circuits. At THIS pre-codegen cut point
            // the auto-`?` is still an explicit `Try`/`Unwrap` node wrapping each
            // Result-typed fan expr (the `strip_fan_auto_try` codegen pass that
            // removes it runs post-cut), so evaluating the expr already performs
            // the unwrap and propagates an `Err` as `Flow::Return` — exactly the
            // backends' join-point `?`. We therefore just evaluate and collect.
            IrExprKind::Fan { exprs } => {
                let mut out = Vec::with_capacity(exprs.len());
                for e in exprs {
                    out.push(val!(self.eval_expr(e, scope)));
                }
                // Single-expr fan is the bare value (no 1-tuple), matching both
                // backends; multi-expr is a tuple.
                if out.len() == 1 {
                    Flow::val(out.into_iter().next().unwrap())
                } else {
                    Flow::val(Value::tuple(out))
                }
            }

            // ── Loops ──
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                self.eval_for_in(*var, var_tuple.as_deref(), iterable, body, scope)
            }
            IrExprKind::While { cond, body } => self.eval_while(cond, body, scope),
            IrExprKind::Break => Flow::Break,
            IrExprKind::Continue => Flow::Continue,

            // ── Calls ──
            IrExprKind::Call { target, args, .. } => self.eval_call(target, args, scope),
            // TailCall is codegen-inserted (TailCallMarkPass, post-cut) but we
            // treat it == Call defensively.
            IrExprKind::TailCall { target, args } => self.eval_call(target, args, scope),

            // ── Collections ──
            IrExprKind::List { elements } => {
                let mut out = Vec::with_capacity(elements.len());
                for e in elements {
                    out.push(val!(self.eval_expr(e, scope)));
                }
                Flow::val(Value::list(out))
            }
            IrExprKind::Tuple { elements } => {
                let mut out = Vec::with_capacity(elements.len());
                for e in elements {
                    out.push(val!(self.eval_expr(e, scope)));
                }
                Flow::val(Value::tuple(out))
            }
            IrExprKind::MapLiteral { entries } => {
                let mut out: Vec<(Value, Value)> = Vec::with_capacity(entries.len());
                for (k, v) in entries {
                    let kv = val!(self.eval_expr(k, scope));
                    let vv = val!(self.eval_expr(v, scope));
                    map_insert(&mut out, kv, vv);
                }
                Flow::val(Value::Map(Rc::new(out)))
            }
            IrExprKind::EmptyMap => Flow::val(Value::Map(Rc::new(Vec::new()))),
            IrExprKind::Record { name, fields } => {
                let mut out = Vec::with_capacity(fields.len());
                for (k, v) in fields {
                    out.push((*k, val!(self.eval_expr(v, scope))));
                }
                // A record-shaped node whose `name` is a registered
                // record-variant constructor builds a `Variant` (so it
                // equality- / pattern-matches as a variant). A plain record
                // type stays a `Record`. Empirically (probe /tmp/repr_probe),
                // both display identically as `Name { f: v }`.
                if let Some(n) = name {
                    if let Some((ty_name, crate::dispatch::CtorKind::Record)) =
                        self.variant_ctor(*n)
                    {
                        return Flow::val(Value::Variant {
                            ty: Some(ty_name),
                            ctor: *n,
                            payload: VariantPayload::Record(out),
                        });
                    }
                }
                // Recover the displayed shape exactly as the codegen walker does
                // (walker/expressions.rs:511-530, walker/types.rs:111). A record
                // LITERAL carries no inline `name` when its nominal type comes
                // from an annotation/inference — the name must be recovered from
                // the expression's type. Three cases, in the walker's order:
                //   1. `expr.ty == Ty::Named(n, _)`  → the nominal name `n`,
                //      fields in literal (declaration) order.
                //   2. `expr.ty == Ty::Record/OpenRecord` whose field-name set
                //      matches a registered NAMED record type (e.g. a nested
                //      list element `[{ val: 2, kids: [] }]` whose element type
                //      was inferred structurally) → that type's name, fields
                //      reordered to the type's DECLARATION order.
                //   3. A genuinely ANONYMOUS record → no name; the native
                //      synthesized struct stores fields in SORTED name order, so
                //      sort here to match the backends' repr.
                let resolved_name;
                if let Some(n) = name {
                    resolved_name = Some(*n);
                } else {
                    match &expr.ty {
                        Ty::Named(n, _) => resolved_name = Some(*n),
                        Ty::Record { .. } | Ty::OpenRecord { .. } => {
                            let mut key: Vec<Sym> = out.iter().map(|(k, _)| *k).collect();
                            key.sort();
                            if let Some((ty_name, decl_order)) =
                                self.named_records.get(&key).cloned()
                            {
                                // Case 2: reorder fields to declaration order.
                                let mut reordered = Vec::with_capacity(out.len());
                                for field in &decl_order {
                                    if let Some(pos) = out.iter().position(|(k, _)| k == field) {
                                        reordered.push(out.swap_remove(pos));
                                    }
                                }
                                reordered.extend(out.drain(..));
                                out = reordered;
                                resolved_name = Some(ty_name);
                            } else {
                                // Case 3: true anonymous record → sorted fields.
                                out.sort_by(|a, b| a.0.cmp(&b.0));
                                resolved_name = None;
                            }
                        }
                        _ => resolved_name = None,
                    }
                }
                Flow::val(Value::Record { name: resolved_name, fields: Rc::new(out) })
            }
            IrExprKind::SpreadRecord { base, fields } => {
                let base_v = val!(self.eval_expr(base, scope));
                let (name, mut merged) = match base_v {
                    Value::Record { name, fields } => (name, (*fields).clone()),
                    other => {
                        return Flow::Abort(format!(
                            "internal: spread base is {} not Record",
                            other.type_name()
                        ))
                    }
                };
                for (k, v) in fields {
                    let vv = val!(self.eval_expr(v, scope));
                    if let Some(slot) = merged.iter_mut().find(|(fk, _)| fk == k) {
                        slot.1 = vv;
                    } else {
                        merged.push((*k, vv));
                    }
                }
                Flow::val(Value::Record { name, fields: Rc::new(merged) })
            }
            IrExprKind::Range { start, end, inclusive } => {
                let s = val!(self.eval_expr(start, scope));
                let e = val!(self.eval_expr(end, scope));
                match (s, e) {
                    (Value::Int(s), Value::Int(e)) => {
                        Flow::val(Value::Range { start: s, end: e, inclusive: *inclusive })
                    }
                    _ => Flow::Abort("internal: range bounds must be Int".into()),
                }
            }

            // ── Access ──
            IrExprKind::Member { object, field } => {
                let o = val!(self.eval_expr(object, scope));
                self.eval_member(o, *field)
            }
            IrExprKind::TupleIndex { object, index } => {
                let o = val!(self.eval_expr(object, scope));
                match o {
                    Value::Tuple(items) | Value::List(items) => match items.get(*index) {
                        Some(v) => Flow::val(v.clone()),
                        None => Flow::Abort("index out of bounds".into()),
                    },
                    other => Flow::Abort(format!(
                        "internal: tuple-index on {} ",
                        other.type_name()
                    )),
                }
            }
            IrExprKind::IndexAccess { object, index } => {
                let o = val!(self.eval_expr(object, scope));
                let i = val!(self.eval_expr(index, scope));
                self.eval_index(o, i)
            }
            IrExprKind::MapAccess { object, key } => {
                let o = val!(self.eval_expr(object, scope));
                let k = val!(self.eval_expr(key, scope));
                match o {
                    Value::Map(entries) => {
                        let found = entries.iter().find(|(ek, _)| ek == &k);
                        Flow::val(Value::Option(found.map(|(_, v)| Box::new(v.clone()))))
                    }
                    other => Flow::Abort(format!(
                        "internal: map-access on {}",
                        other.type_name()
                    )),
                }
            }

            // ── Functions ──
            IrExprKind::Lambda { params, body, .. } => {
                let clo = Closure {
                    params: params.iter().map(|(v, _)| *v).collect(),
                    body: Rc::new((**body).clone()),
                    captured: scope.clone(),
                };
                Flow::val(Value::Closure(Rc::new(clo)))
            }

            // ── Strings ──
            IrExprKind::StringInterp { parts } => self.eval_string_interp(parts, scope),

            // ── Result / Option ──
            IrExprKind::ResultOk { expr } => {
                let v = val!(self.eval_expr(expr, scope));
                Flow::val(Value::Result(Ok(Box::new(v))))
            }
            IrExprKind::ResultErr { expr } => {
                let v = val!(self.eval_expr(expr, scope));
                Flow::val(Value::Result(Err(Box::new(v))))
            }
            IrExprKind::OptionSome { expr } => {
                let v = val!(self.eval_expr(expr, scope));
                Flow::val(Value::Option(Some(Box::new(v))))
            }
            IrExprKind::OptionNone => Flow::val(Value::Option(None)),
            // `?` / `!` — short-circuit the enclosing fn on Err/None.
            IrExprKind::Try { expr } | IrExprKind::Unwrap { expr } => {
                let v = val!(self.eval_expr(expr, scope));
                match v {
                    Value::Result(Ok(inner)) => Flow::val(*inner),
                    Value::Result(Err(e)) => Flow::Return(Value::Result(Err(e))),
                    Value::Option(Some(inner)) => Flow::val(*inner),
                    // #556: `expr!` on a None propagates an Err whose message is
                    // "none" on BOTH backends (the codegen lowers Option `!` to
                    // `ok_or("none")?`). Returning a bare Option(None) made the
                    // main-error path print the Rust-internal "called
                    // Option::unwrap() on a None value" — a wrong third vote
                    // against the native==wasm "Error: none".
                    Value::Option(None) => Flow::Return(Value::Result(Err(Box::new(Value::str("none".to_string()))))),
                    other => Flow::val(other),
                }
            }
            // `??` — unwrap with a fallback value.
            IrExprKind::UnwrapOr { expr, fallback } => {
                let v = val!(self.eval_expr(expr, scope));
                match v {
                    Value::Option(Some(inner)) => Flow::val(*inner),
                    Value::Option(None) => self.eval_expr(fallback, scope),
                    Value::Result(Ok(inner)) => Flow::val(*inner),
                    Value::Result(Err(_)) => self.eval_expr(fallback, scope),
                    other => Flow::val(other),
                }
            }
            // `?` Result→Option (identity for Option).
            IrExprKind::ToOption { expr } => {
                let v = val!(self.eval_expr(expr, scope));
                match v {
                    Value::Result(Ok(inner)) => Flow::val(Value::Option(Some(inner))),
                    Value::Result(Err(_)) => Flow::val(Value::Option(None)),
                    opt @ Value::Option(_) => Flow::val(opt),
                    other => Flow::val(other),
                }
            }
            // `?.field` — optional chaining.
            IrExprKind::OptionalChain { expr, field } => {
                let v = val!(self.eval_expr(expr, scope));
                match v {
                    Value::Option(None) => Flow::val(Value::Option(None)),
                    Value::Option(Some(inner)) => match self.eval_member(*inner, *field) {
                        Flow::Value(m) => Flow::val(Value::Option(Some(Box::new(m)))),
                        other => other,
                    },
                    other => match self.eval_member(other, *field) {
                        Flow::Value(m) => Flow::val(Value::Option(Some(Box::new(m)))),
                        other => other,
                    },
                }
            }
            // The interp is synchronous: await is identity over the value.
            IrExprKind::Await { expr } => self.eval_expr(expr, scope),

            // ── Misc ──
            IrExprKind::Hole => Flow::Abort(
                "internal: evaluated a Hole (intrinsic-stub body reached as an expr)".into(),
            ),
            IrExprKind::Todo { message } => Flow::Abort(message.clone()),

            // ── Codegen-inserted: UNREACHABLE at the pre-codegen cut point ──
            IrExprKind::RuntimeCall { .. } => unreachable!(
                "RuntimeCall is codegen-inserted (IntrinsicLowering); interp runs pre-codegen"
            ),
            IrExprKind::Clone { .. } => {
                unreachable!("Clone is codegen-inserted (CloneInsertion); interp runs pre-codegen")
            }
            IrExprKind::Deref { .. } => {
                unreachable!("Deref is codegen-inserted (BoxDeref); interp runs pre-codegen")
            }
            IrExprKind::Borrow { .. } => unreachable!(
                "Borrow is codegen-inserted (BorrowInsertion); interp runs pre-codegen"
            ),
            IrExprKind::BoxNew { .. } => {
                unreachable!("BoxNew is codegen-inserted (BoxDeref); interp runs pre-codegen")
            }
            IrExprKind::RcWrap { .. } => unreachable!(
                "RcWrap is codegen-inserted (ClosureConversion); interp runs pre-codegen"
            ),
            IrExprKind::RustMacro { .. } => unreachable!(
                "RustMacro is codegen-inserted (BuiltinLowering); interp runs pre-codegen"
            ),
            IrExprKind::ToVec { .. } => {
                unreachable!("ToVec is codegen-inserted; interp runs pre-codegen")
            }
            IrExprKind::RenderedCall { .. } => unreachable!(
                "RenderedCall is codegen-inserted (StdlibLowering); interp runs pre-codegen"
            ),
            IrExprKind::InlineRust { .. } => unreachable!(
                "InlineRust is codegen-inserted (StdlibLowering); interp runs pre-codegen"
            ),
            IrExprKind::ClosureCreate { .. } => unreachable!(
                "ClosureCreate is codegen-inserted (ClosureConversion); interp runs pre-codegen"
            ),
            IrExprKind::EnvLoad { .. } => unreachable!(
                "EnvLoad is codegen-inserted (ClosureConversion); interp runs pre-codegen"
            ),
            IrExprKind::IterChain { .. } => unreachable!(
                "IterChain is codegen-inserted (StdlibLowering); interp runs pre-codegen"
            ),
        }
    }

    // ── Member / index access ──────────────────────────────────

    fn eval_member(&mut self, object: Value, field: Sym) -> Flow {
        match object {
            Value::Record { fields, .. } => {
                match fields.iter().find(|(k, _)| *k == field) {
                    Some((_, v)) => Flow::val(v.clone()),
                    None => Flow::Abort(format!("internal: no field `{}` on record", field)),
                }
            }
            Value::Variant { payload: VariantPayload::Record(fields), .. } => {
                match fields.iter().find(|(k, _)| *k == field) {
                    Some((_, v)) => Flow::val(v.clone()),
                    None => Flow::Abort(format!("internal: no field `{}` on variant", field)),
                }
            }
            other => Flow::Abort(format!(
                "internal: member access `.{}` on {}",
                field,
                other.type_name()
            )),
        }
    }

    fn eval_index(&mut self, object: Value, index: Value) -> Flow {
        let i = match index {
            Value::Int(i) => i,
            other => {
                return Flow::Abort(format!(
                    "internal: list index is {} not Int",
                    other.type_name()
                ))
            }
        };
        match object {
            Value::List(xs) => {
                if i < 0 || (i as usize) >= xs.len() {
                    // Matches the codegen OOB contract: abort + exit 1.
                    Flow::Abort("index out of bounds".into())
                } else {
                    Flow::val(xs[i as usize].clone())
                }
            }
            Value::Str(s) => {
                // String indexing returns the byte? Almide indexes strings via
                // string.* fns; a bare index on a String is unusual. Treat as
                // unsupported to avoid a wrong third vote.
                let _ = s;
                Flow::Unsupported("string index access".into())
            }
            other => Flow::Abort(format!(
                "internal: index access on {}",
                other.type_name()
            )),
        }
    }

    // ── String interpolation ───────────────────────────────────

    fn eval_string_interp(&mut self, parts: &[IrStringPart], scope: &Scope) -> Flow {
        let mut out = String::new();
        for part in parts {
            match part {
                IrStringPart::Lit { value } => out.push_str(value),
                IrStringPart::Expr { expr } => {
                    let v = val!(self.eval_expr(expr, scope));
                    // A bare top-level String stays raw; everything else routes
                    // through the bare-display path (which for compounds is
                    // `almide_repr`, for scalars is plain Display).
                    out.push_str(&v.display_bare());
                }
            }
        }
        Flow::val(Value::str(out))
    }

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
        let items = match iter_v.as_iter_items() {
            Some(items) => items,
            None => {
                return Flow::Abort(format!(
                    "internal: for-in over non-iterable {}",
                    iter_v.type_name()
                ))
            }
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
                let iv = match self.eval_expr(index, scope) {
                    Flow::Value(v) => v,
                    other => return Err(other),
                };
                let vv = match self.eval_expr(value, scope) {
                    Flow::Value(v) => v,
                    other => return Err(other),
                };
                let cur = scope.get(*target).ok_or_else(|| {
                    Flow::Abort("internal: index-assign to unbound list".into())
                })?;
                match (cur, iv) {
                    (Value::List(xs), Value::Int(i)) => {
                        if i < 0 || (i as usize) >= xs.len() {
                            return Err(Flow::Abort("index out of bounds".into()));
                        }
                        let mut new = (*xs).clone();
                        new[i as usize] = vv;
                        scope.assign(*target, Value::list(new));
                        Ok(())
                    }
                    _ => Err(Flow::Abort("internal: malformed index-assign".into())),
                }
            }
            IrStmtKind::MapInsert { target, key, value } => {
                let kv = match self.eval_expr(key, scope) {
                    Flow::Value(v) => v,
                    other => return Err(other),
                };
                let vv = match self.eval_expr(value, scope) {
                    Flow::Value(v) => v,
                    other => return Err(other),
                };
                let cur = scope.get(*target).ok_or_else(|| {
                    Flow::Abort("internal: map-insert to unbound map".into())
                })?;
                match cur {
                    Value::Map(entries) => {
                        let mut new = (*entries).clone();
                        map_insert(&mut new, kv, vv);
                        scope.assign(*target, Value::Map(Rc::new(new)));
                        Ok(())
                    }
                    _ => Err(Flow::Abort("internal: map-insert on non-Map".into())),
                }
            }
            IrStmtKind::FieldAssign { target, field, value } => {
                let vv = match self.eval_expr(value, scope) {
                    Flow::Value(v) => v,
                    other => return Err(other),
                };
                let cur = scope.get(*target).ok_or_else(|| {
                    Flow::Abort("internal: field-assign to unbound record".into())
                })?;
                match cur {
                    Value::Record { name, fields } => {
                        let mut new = (*fields).clone();
                        if let Some(slot) = new.iter_mut().find(|(k, _)| k == field) {
                            slot.1 = vv;
                        } else {
                            new.push((*field, vv));
                        }
                        scope.assign(*target, Value::Record { name, fields: Rc::new(new) });
                        Ok(())
                    }
                    _ => Err(Flow::Abort("internal: field-assign on non-Record".into())),
                }
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

    // ── Match ───────────────────────────────────────────────────

    fn eval_match(&mut self, subject: &IrExpr, arms: &[IrMatchArm], scope: &Scope) -> Flow {
        let subj = val!(self.eval_expr(subject, scope));
        for arm in arms {
            let mut binds = Vec::new();
            if self.try_match(&arm.pattern, &subj, &mut binds) {
                let frame = scope.child();
                for (id, v) in &binds {
                    frame.bind(*id, v.clone());
                }
                // Evaluate the guard (if any) in the arm's frame.
                if let Some(guard) = &arm.guard {
                    match self.eval_expr(guard, &frame) {
                        Flow::Value(Value::Bool(true)) => {}
                        Flow::Value(Value::Bool(false)) => continue,
                        Flow::Value(other) => {
                            return Flow::Abort(format!(
                                "internal: match guard is {} not Bool",
                                other.type_name()
                            ))
                        }
                        other => return other,
                    }
                }
                return self.eval_expr(&arm.body, &frame);
            }
        }
        Flow::Abort("internal: non-exhaustive match (no arm matched)".into())
    }

    /// Attempt to match `value` against `pattern`, accumulating bindings.
    /// Returns `true` on success (bindings valid only then). Implements the
    /// IR-level pattern engine directly — including `List` patterns, since
    /// `ListPatternLoweringPass` runs post-cut so list patterns are still
    /// present.
    fn try_match(
        &mut self,
        pattern: &IrPattern,
        value: &Value,
        binds: &mut Vec<(VarId, Value)>,
    ) -> bool {
        match pattern {
            IrPattern::Wildcard => true,
            IrPattern::Bind { var, .. } => {
                binds.push((*var, value.clone()));
                true
            }
            IrPattern::Literal { expr } => {
                // Evaluate the literal in an empty scope (literals are closed).
                let scope = Scope::root();
                match self.eval_expr(expr, &scope) {
                    Flow::Value(lit) => &lit == value,
                    _ => false,
                }
            }
            IrPattern::Tuple { elements } => match value {
                Value::Tuple(items) if items.len() == elements.len() => elements
                    .iter()
                    .zip(items.iter())
                    .all(|(p, v)| self.try_match(p, v, binds)),
                _ => false,
            },
            IrPattern::List { elements } => match value.as_iter_items() {
                Some(items) if items.len() == elements.len() => elements
                    .iter()
                    .zip(items.iter())
                    .all(|(p, v)| self.try_match(p, v, binds)),
                _ => false,
            },
            IrPattern::Some { inner } => match value {
                Value::Option(Some(v)) => self.try_match(inner, v, binds),
                _ => false,
            },
            IrPattern::None => matches!(value, Value::Option(None)),
            IrPattern::Ok { inner } => match value {
                Value::Result(Ok(v)) => self.try_match(inner, v, binds),
                _ => false,
            },
            IrPattern::Err { inner } => match value {
                Value::Result(Err(v)) => self.try_match(inner, v, binds),
                _ => false,
            },
            IrPattern::Constructor { name, args } => match value {
                Value::Variant { ctor, payload, .. } if ctor.as_str() == name => match payload {
                    VariantPayload::Unit => args.is_empty(),
                    VariantPayload::Tuple(items) if items.len() == args.len() => args
                        .iter()
                        .zip(items.iter())
                        .all(|(p, v)| self.try_match(p, v, binds)),
                    _ => false,
                },
                _ => false,
            },
            IrPattern::RecordPattern { name, fields, rest } => {
                let (obj_name, obj_fields): (Option<Sym>, &Vec<(Sym, Value)>) = match value {
                    Value::Record { name, fields } => (*name, fields),
                    Value::Variant { ctor, payload: VariantPayload::Record(fields), .. } => {
                        (Some(*ctor), fields)
                    }
                    _ => return false,
                };
                // Name must match when the pattern names a constructor.
                if !name.is_empty() {
                    match obj_name {
                        Some(n) if n.as_str() == name => {}
                        _ => return false,
                    }
                }
                if !rest && fields.len() != obj_fields.len() {
                    return false;
                }
                for fp in fields {
                    let fname = fp.name.as_str();
                    let fv = match obj_fields.iter().find(|(k, _)| k.as_str() == fname) {
                        Some((_, v)) => v,
                        None => return false,
                    };
                    match &fp.pattern {
                        // Shorthand `{ x, y }` lowers to explicit `Bind`
                        // sub-patterns (verified via IR dump), so binding is
                        // handled here uniformly.
                        Some(sub) => {
                            if !self.try_match(sub, fv, binds) {
                                return false;
                            }
                        }
                        // A field with no sub-pattern is a structural-only
                        // match (the field must exist, but binds nothing).
                        None => {}
                    }
                }
                true
            }
        }
    }

    // ── Operators ───────────────────────────────────────────────

    fn eval_binop(&mut self, op: BinOp, left: &IrExpr, right: &IrExpr, scope: &Scope) -> Flow {
        // Short-circuit logical operators evaluate the right side lazily.
        match op {
            BinOp::And => {
                let l = val!(self.eval_expr(left, scope));
                return match l {
                    Value::Bool(false) => Flow::val(Value::Bool(false)),
                    Value::Bool(true) => self.eval_expr(right, scope),
                    other => Flow::Abort(format!(
                        "internal: `and` on {}",
                        other.type_name()
                    )),
                };
            }
            BinOp::Or => {
                let l = val!(self.eval_expr(left, scope));
                return match l {
                    Value::Bool(true) => Flow::val(Value::Bool(true)),
                    Value::Bool(false) => self.eval_expr(right, scope),
                    other => Flow::Abort(format!("internal: `or` on {}", other.type_name())),
                };
            }
            _ => {}
        }

        let l = val!(self.eval_expr(left, scope));
        let r = val!(self.eval_expr(right, scope));
        self.apply_binop(op, l, r)
    }

    pub(crate) fn apply_binop(&mut self, op: BinOp, l: Value, r: Value) -> Flow {
        use BinOp::*;
        match op {
            // Integer arithmetic. Native release emits bare `+`/`-`/`*` which
            // WRAP (no panic) — replicate with wrapping ops.
            AddInt => int2(l, r, |a, b| Flow::val(Value::Int(a.wrapping_add(b)))),
            SubInt => int2(l, r, |a, b| Flow::val(Value::Int(a.wrapping_sub(b)))),
            MulInt => int2(l, r, |a, b| Flow::val(Value::Int(a.wrapping_mul(b)))),
            // Total div / mod: `almide_div!` / `almide_mod!` semantics —
            // checked_div/checked_rem, None → abort with the exact native msg.
            DivInt => int2(l, r, |a, b| match a.checked_div(b) {
                Some(v) => Flow::val(Value::Int(v)),
                None => Flow::Abort(div_msg(b)),
            }),
            ModInt => int2(l, r, |a, b| match a.checked_rem(b) {
                Some(v) => Flow::val(Value::Int(v)),
                None => Flow::Abort(div_msg(b)),
            }),
            // base.pow(exp as u32), wrapping in release; negative exp is a
            // type error upstream.
            PowInt => int2(l, r, |a, b| Flow::val(Value::Int(a.wrapping_pow(b as u32)))),

            AddFloat => float2(l, r, |a, b| a + b),
            SubFloat => float2(l, r, |a, b| a - b),
            MulFloat => float2(l, r, |a, b| a * b),
            DivFloat => float2(l, r, |a, b| a / b),
            ModFloat => float2(l, r, |a, b| a % b),
            PowFloat => float2(l, r, |a, b| a.powf(b)),

            ConcatStr => match (l, r) {
                (Value::Str(a), Value::Str(b)) => {
                    Flow::val(Value::str(format!("{}{}", a, b)))
                }
                (a, b) => Flow::Abort(format!(
                    "internal: string concat on {} and {}",
                    a.type_name(),
                    b.type_name()
                )),
            },
            ConcatList => match (l, r) {
                (Value::List(a), Value::List(b)) => {
                    let mut v = (*a).clone();
                    v.extend((*b).clone());
                    Flow::val(Value::list(v))
                }
                (a, b) => Flow::Abort(format!(
                    "internal: list concat on {} and {}",
                    a.type_name(),
                    b.type_name()
                )),
            },

            Eq => Flow::val(Value::Bool(l == r)),
            Neq => Flow::val(Value::Bool(l != r)),
            Lt | Gt | Lte | Gte => match l.partial_cmp_val(&r) {
                Some(ord) => {
                    let res = match op {
                        Lt => ord == std::cmp::Ordering::Less,
                        Gt => ord == std::cmp::Ordering::Greater,
                        Lte => ord != std::cmp::Ordering::Greater,
                        Gte => ord != std::cmp::Ordering::Less,
                        _ => unreachable!(),
                    };
                    Flow::val(Value::Bool(res))
                }
                // #556 F2: a None here is the NaN case (Float partial_cmp) —
                // both backends return IEEE false for every NaN comparison
                // (`<`/`>`/`<=`/`>=`), so the interp must too, NOT abort. A
                // genuine type-mismatch ordering can't reach here: the checker
                // rejects it, and codegen never emits cross-type compares.
                None => Flow::val(Value::Bool(false)),
            },

            And | Or => unreachable!("short-circuited above"),

            // Matrix ops would dispatch to the runtime matrix bridge; not yet
            // implemented in this phase.
            MulMatrix | AddMatrix | SubMatrix | ScaleMatrix => {
                Flow::Unsupported("matrix arithmetic".into())
            }
        }
    }

    fn eval_unop(&mut self, op: UnOp, v: Value) -> Flow {
        match (op, v) {
            (UnOp::NegInt, Value::Int(n)) => Flow::val(Value::Int(n.wrapping_neg())),
            (UnOp::NegFloat, Value::Float(f)) => Flow::val(Value::Float(-f)),
            (UnOp::Not, Value::Bool(b)) => Flow::val(Value::Bool(!b)),
            (op, v) => Flow::Abort(format!(
                "internal: unop {:?} on {}",
                op,
                v.type_name()
            )),
        }
    }

    // ── Helpers ─────────────────────────────────────────────────

    fn var_name(&self, id: VarId) -> String {
        if (id.0 as usize) < self.program.var_table.len() {
            self.program.var_table.get(id).name.to_string()
        } else {
            format!("v{}", id.0)
        }
    }
}

/// Insert / overwrite a key in an insertion-ordered map entry vec, matching the
/// compact-ordered-dict: existing key updates in place; new key appends.
pub(crate) fn map_insert(entries: &mut Vec<(Value, Value)>, key: Value, value: Value) {
    if let Some(slot) = entries.iter_mut().find(|(k, _)| k == &key) {
        slot.1 = value;
    } else {
        entries.push((key, value));
    }
}

fn int2(l: Value, r: Value, f: impl FnOnce(i64, i64) -> Flow) -> Flow {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => f(a, b),
        (a, b) => Flow::Abort(format!(
            "internal: int op on {} and {}",
            a.type_name(),
            b.type_name()
        )),
    }
}

fn float2(l: Value, r: Value, f: impl FnOnce(f64, f64) -> f64) -> Flow {
    match (l, r) {
        (Value::Float(a), Value::Float(b)) => Flow::val(Value::Float(f(a, b))),
        (a, b) => Flow::Abort(format!(
            "internal: float op on {} and {}",
            a.type_name(),
            b.type_name()
        )),
    }
}

/// The exact native abort message for a failing checked int div/mod.
fn div_msg(divisor: i64) -> String {
    if divisor == 0 {
        "division by zero".to_string()
    } else {
        "integer overflow".to_string()
    }
}
