//! The tree-walking evaluator: `IrExpr` / `IrStmt` / `IrPattern` → `Value`.
//!
//! Every eval step burns one unit of fuel. Codegen-inserted node kinds (Clone,
//! Borrow, IterChain, ClosureCreate, …) are unreachable at the pre-codegen cut
//! point and panic with an explanatory message to document the boundary.

use std::rc::Rc;

use almide_base::intern::Sym;
use almide_ir::{
    BinOp, IrExpr, IrExprKind, IrFieldPattern, IrMatchArm, IrPattern, IrStmt, IrStmtKind,
    IrStringPart, UnOp, VarId,
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
                self.eval_record_literal(name, fields, &expr.ty, scope)
            }
            IrExprKind::SpreadRecord { base, fields } => {
                self.eval_spread_record(base, fields, scope)
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
                self.eval_try_unwrap(expr, scope)
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
                self.eval_optional_chain(expr, *field, scope)
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

    // ── `?` / `!` / `?.field` ───────────────────────────────────

    /// `Try`/`Unwrap` — short-circuit the enclosing fn on Err/None.
    fn eval_try_unwrap(&mut self, expr: &IrExpr, scope: &Scope) -> Flow {
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
            Value::Option(None) => {
                Flow::Return(Value::Result(Err(Box::new(Value::str("none".to_string())))))
            }
            other => Flow::val(other),
        }
    }

    fn eval_optional_chain(&mut self, expr: &IrExpr, field: Sym, scope: &Scope) -> Flow {
        let v = val!(self.eval_expr(expr, scope));
        match v {
            Value::Option(None) => Flow::val(Value::Option(None)),
            Value::Option(Some(inner)) => match self.eval_member(*inner, field) {
                Flow::Value(m) => Flow::val(Value::Option(Some(Box::new(m)))),
                other => other,
            },
            other => match self.eval_member(other, field) {
                Flow::Value(m) => Flow::val(Value::Option(Some(Box::new(m)))),
                other => other,
            },
        }
    }

    // ── Record literal / spread ────────────────────────────────

    fn eval_record_literal(
        &mut self,
        name: &Option<Sym>,
        fields: &[(Sym, IrExpr)],
        ty: &Ty,
        scope: &Scope,
    ) -> Flow {
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
            if let Some((ty_name, crate::dispatch::CtorKind::Record)) = self.variant_ctor(*n) {
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
            match ty {
                Ty::Named(n, _) => resolved_name = Some(*n),
                Ty::Record { .. } | Ty::OpenRecord { .. } => {
                    let mut key: Vec<Sym> = out.iter().map(|(k, _)| *k).collect();
                    key.sort();
                    if let Some((ty_name, decl_order)) = self.named_records.get(&key).cloned() {
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

    fn eval_spread_record(&mut self, base: &IrExpr, fields: &[(Sym, IrExpr)], scope: &Scope) -> Flow {
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

}

include!("eval_p2.rs");
include!("eval_p3.rs");
