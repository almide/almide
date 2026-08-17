/// IR optimization passes.
///
/// Pipeline position: Lower -> IR -> **optimize()** -> mono() -> codegen
///
/// Pass 1: Constant folding -- evaluate compile-time-known expressions.
/// Pass 2: Dead code elimination -- remove unused bindings with pure values.
/// Pass 3: Constant propagation -- replace vars bound to literals with the literal.

mod branch_lift;
mod dce;
mod optional_chain;
mod propagate;

use almide_ir::*;

// ── Public entry point ──────────────────────────────────────────

/// Run all optimization passes on an IR program.
/// Requires use-counts to be computed (done by `lower_program`).
pub fn optimize_program(program: &mut IrProgram) {
    // ALMIDE_DISABLE_OPT skips the PERF passes — fold, DCE, propagate — so the
    // perf ratchet can grow an ablation leg (#1466/#1487: the measured delta
    // IS the optimizer's contribution). Three passes are exempt on purpose,
    // because they are not optimizations: optional-chain and branch-lift are
    // lowering ENABLERS the v1 wasm renderer depends on, and the unsigned
    // re-fold is CORRECTNESS (a literal pair under a UInt64 `/`/`%` reaching
    // the emitters folds SIGNED on the native leg — the #872 divergence).
    // Ablation must never change what a program PRINTS (verified: the full
    // 403-file suite passes byte-identical under ablation). It may cost a
    // file its wasm-leg lowering — dead code that DCE deletes can contain a
    // walling shape, which then walls honestly and falls back (measured: 12
    // fallbacks normally, 13 ablated) — which is the correct trade for a
    // measurement mode: an honest wall, never a changed answer.
    let ablate = std::env::var_os("ALMIDE_DISABLE_OPT").is_some();

    if !ablate {
        // Pass 1: constant folding (bottom-up rewrite)
        constant_fold(program);

        // Recompute use-counts after folding may have eliminated references
        compute_use_counts(program);

        // Pass 2: dead code elimination
        dce::eliminate_dead_code(program);

        // Pass 3: constant propagation (replace vars bound to literals with the literal)
        propagate::constant_propagate(program);
    }

    // Pass 3b: RE-FOLD the unsigned 64-bit lane (#872). Propagation is what
    // first puts two literals under a `UInt64` `/`/`%` (`let big: UInt64 =
    // <u64::MAX>; big / 2`), and pass 1 ran before it — so the pair reached
    // the emitters, where the native leg's own constant handling folded it
    // SIGNED (`-1 / 2 = 0`, `-1 % 10 = -1`) while the wasm leg kept the
    // runtime `i64.div_u` and printed the right answer: a cross-target
    // divergence in the band the lane exists for. Folding it HERE, unsigned,
    // settles the value before any emitter sees it. Scoped to `UInt64` so the
    // signed corpus keeps byte-identical output.
    refold_unsigned_lane(program);

    // Recompute after propagation may have reduced use-counts
    compute_use_counts(program);

    // Pass 4: dead code elimination again (propagation may create new dead bindings)
    if !ablate {
        dce::eliminate_dead_code(program);
    }

    // Pass 5a: optional-chain desugar — `p?.f` → a call to a synthesized tail-match
    // helper (`optional_chain_synth_N`), the shape both backends prove out in every
    // position (bind, call argument, `??` operand). Runs AFTER DCE (only surviving
    // chains synthesize a helper) and BEFORE branch-lift (same shared-cut-point
    // discipline; the produced call is not a branch, so order is for clarity only).
    optional_chain::desugar_optional_chains(program);

    // Pass 5: branch-lift — hoist heap-typed `let`-bound `if`/`match` into tail
    // helper functions so the v1 trust-spine wasm renderer can lower them (it
    // walls on a let-bound heap-result branch; the tail helper is a proven shape).
    // Runs AFTER DCE so it only rewrites binds that survived; the synthesized
    // helper's call site keeps the captured vars live (use-counts recomputed below).
    branch_lift::lift_heap_branch_binds(program);

    // Recompute use-counts for downstream consumers (mono, borrow analysis). This
    // also re-counts the `Var` args the branch-lift inserted at each helper call.
    compute_use_counts(program);
}

// ── Pass 1: Constant Folding ────────────────────────────────────

fn constant_fold(program: &mut IrProgram) {
    for f in &mut program.functions {
        fold_expr(&mut f.body);
    }
    for m in &mut program.modules {
        for f in &mut m.functions {
            fold_expr(&mut f.body);
        }
    }
    // Top-lets fold in DECLARATION order (root first, then each module), with
    // every earlier IMMUTABLE top-let that folded to a scalar literal
    // substituted into later initializers (#809): `let DOUBLE_MAX = MAX_COUNT
    // * 9223372036854775807` must fold to the WRAPPED literal here — left as
    // a runtime expression it lands in a Rust `const` whose const-eval
    // REJECTS the overflow that Almide defines as two's-complement wrap (both
    // targets wrap at runtime; `fold_expr` already folds with wrapping ops).
    // Mutable `var` top-lets never substitute, and only Int/Float/Bool
    // literals do (a substituted String would clone its allocation site).
    // The env is PER REGION: the main program and each module are PRIVATE
    // VarId numbering regions (each numbers from 0), so one shared env let a
    // module's folded literal stand in for a SAME-NUMBERED unrelated var in a
    // later region — ceangal's scroll `BOUNCING = 2` / `FRICTION = 0.035`
    // replaced view's `_transparent` / `_white` Color refs inside `_default`
    // (a silent wrong value; order-dependent on module link order). A
    // cross-module top-let read goes through a synthesized ref in the
    // READER's own region resolved by NAME later, never by a raw foreign id,
    // so per-region scoping loses no legitimate #809 chain.
    let fold_top_let = |tl: &mut IrTopLet,
                            env: &mut std::collections::HashMap<VarId, IrExprKind>| {
        subst_const_vars(&mut tl.value, env);
        fold_expr(&mut tl.value);
        if !tl.mutable
            && matches!(
                tl.value.kind,
                IrExprKind::LitInt { .. }
                    | IrExprKind::LitFloat { .. }
                    | IrExprKind::LitBool { .. }
            )
        {
            env.insert(tl.var, tl.value.kind.clone());
        }
    };
    let mut env: std::collections::HashMap<VarId, IrExprKind> =
        std::collections::HashMap::new();
    for tl in &mut program.top_lets {
        fold_top_let(tl, &mut env);
    }
    for m in &mut program.modules {
        let mut env: std::collections::HashMap<VarId, IrExprKind> =
            std::collections::HashMap::new();
        for tl in &mut m.top_lets {
            fold_top_let(tl, &mut env);
        }
    }
}

/// Replace every `Var` reference to an already-folded earlier top-let with its
/// literal, bottom-up through `map_children` (the wildcard-free traversal
/// primitive — a hand-rolled match would silently drop future node kinds).
/// VarIds are unique WITHIN a region (the caller scopes `env` per region —
/// main vs each module — because regions each number from 0), so no
/// shadowing can capture a substitution.
fn subst_const_vars(slot: &mut IrExpr, env: &std::collections::HashMap<VarId, IrExprKind>) {
    if env.is_empty() {
        return;
    }
    fn go(mut expr: IrExpr, env: &std::collections::HashMap<VarId, IrExprKind>) -> IrExpr {
        expr = expr.map_children(&mut |e| go(e, env));
        if let IrExprKind::Var { id } = &expr.kind {
            if let Some(lit) = env.get(id) {
                expr.kind = lit.clone();
            }
        }
        expr
    }
    let placeholder =
        IrExpr { kind: IrExprKind::Unit, ty: slot.ty.clone(), span: None, def_id: None };
    let taken = std::mem::replace(slot, placeholder);
    *slot = go(taken, env);
}

fn fold_expr(expr: &mut IrExpr) {
    // Recurse first (bottom-up)
    match &mut expr.kind {
        IrExprKind::BinOp { .. } | IrExprKind::UnOp { .. } => fold_expr_binop(expr),
        IrExprKind::Block { .. } | IrExprKind::If { .. }
        | IrExprKind::ForIn { .. } | IrExprKind::While { .. } => fold_expr_control(expr),
        IrExprKind::Match { .. } => fold_expr_match(expr),
        IrExprKind::Call { .. } => fold_expr_call(expr),
        IrExprKind::List { .. } | IrExprKind::Tuple { .. } | IrExprKind::Record { .. }
        | IrExprKind::SpreadRecord { .. } | IrExprKind::Range { .. } | IrExprKind::IndexAccess { .. }
        | IrExprKind::MapAccess { .. } | IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
        | IrExprKind::MapLiteral { .. } | IrExprKind::StringInterp { .. } => fold_expr_containers(expr),
        IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. } | IrExprKind::OptionSome { .. }
        | IrExprKind::Try { .. } => fold_expr_wrap(expr),
        IrExprKind::Lambda { body, .. } => fold_expr(body),
        _ => {}
    }

    // Now try to fold this node
    let replacement = try_fold(expr);
    if let Some(new_expr) = replacement {
        *expr = new_expr;
    }
}

/// BinOp / UnOp: recurse into operands.
fn fold_expr_binop(expr: &mut IrExpr) {
    match &mut expr.kind {
        IrExprKind::BinOp { left, right, .. } => {
            fold_expr(left);
            fold_expr(right);
        }
        IrExprKind::UnOp { operand, .. } => fold_expr(operand),
        _ => unreachable!(),
    }
}

/// Block / If / ForIn / While: recurse into control-flow subexpressions and bodies.
fn fold_expr_control(expr: &mut IrExpr) {
    match &mut expr.kind {
        IrExprKind::Block { stmts, expr: tail } => {
            for s in stmts { fold_stmt(s); }
            if let Some(t) = tail { fold_expr(t); }
        }
        IrExprKind::If { cond, then, else_ } => {
            fold_expr(cond);
            fold_expr(then);
            fold_expr(else_);
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            fold_expr(iterable);
            for s in body { fold_stmt(s); }
        }
        IrExprKind::While { cond, body } => {
            fold_expr(cond);
            for s in body { fold_stmt(s); }
        }
        _ => unreachable!(),
    }
}

/// Match: recurse into subject, guards, and arm bodies.
fn fold_expr_match(expr: &mut IrExpr) {
    let IrExprKind::Match { subject, arms } = &mut expr.kind else { unreachable!() };
    fold_expr(subject);
    for a in arms {
        if let Some(g) = &mut a.guard { fold_expr(g); }
        fold_expr(&mut a.body);
    }
}

/// Call: recurse into the receiver (if any) and arguments.
fn fold_expr_call(expr: &mut IrExpr) {
    let IrExprKind::Call { target, args, .. } = &mut expr.kind else { unreachable!() };
    if let CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } = target {
        fold_expr(object);
    }
    for a in args { fold_expr(a); }
}

/// List/Tuple/Record/SpreadRecord/Range/IndexAccess/MapAccess/Member/TupleIndex/MapLiteral/StringInterp:
/// recurse into each child expression.
fn fold_expr_containers(expr: &mut IrExpr) {
    match &mut expr.kind {
        IrExprKind::List { .. } | IrExprKind::Tuple { .. } | IrExprKind::Record { .. }
        | IrExprKind::SpreadRecord { .. } | IrExprKind::MapLiteral { .. } => fold_expr_containers_literals(expr),
        IrExprKind::Range { .. } | IrExprKind::IndexAccess { .. } | IrExprKind::MapAccess { .. }
        | IrExprKind::Member { .. } | IrExprKind::TupleIndex { .. }
        | IrExprKind::StringInterp { .. } => fold_expr_containers_access(expr),
        _ => unreachable!(),
    }
}

/// List/Tuple/Record/SpreadRecord/MapLiteral: recurse into each element/entry.
fn fold_expr_containers_literals(expr: &mut IrExpr) {
    match &mut expr.kind {
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { fold_expr(e); }
        }
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields { fold_expr(v); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            fold_expr(base);
            for (_, v) in fields { fold_expr(v); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries { fold_expr(k); fold_expr(v); }
        }
        _ => unreachable!(),
    }
}

/// Range/IndexAccess/MapAccess/Member/TupleIndex/StringInterp: recurse into each accessed sub-expression.
fn fold_expr_containers_access(expr: &mut IrExpr) {
    match &mut expr.kind {
        IrExprKind::Range { start, end, .. } => {
            fold_expr(start);
            fold_expr(end);
        }
        IrExprKind::IndexAccess { object, index } => {
            fold_expr(object);
            fold_expr(index);
        }
        IrExprKind::MapAccess { object, key } => {
            fold_expr(object);
            fold_expr(key);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            fold_expr(object);
        }
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr: e } = p { fold_expr(e); }
            }
        }
        _ => unreachable!(),
    }
}

/// ResultOk/ResultErr/OptionSome/Try/Await: recurse into the wrapped expression.
fn fold_expr_wrap(expr: &mut IrExpr) {
    let (IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
        | IrExprKind::OptionSome { expr: e } | IrExprKind::Try { expr: e }
) = &mut expr.kind else { unreachable!() };
    fold_expr(e);
}

/// Try to reduce an expression to a simpler form.
/// Returns Some(replacement) if the node can be folded.
fn try_fold(expr: &IrExpr) -> Option<IrExpr> {
    match &expr.kind {
        // ── Arithmetic / string / bool on literals ──
        IrExprKind::BinOp { op, left, right } => {
            // The RESULT type anchors the unsigned lane, not the operand's:
            // a literal operand carries the checker's default `Int` even when
            // the binding is `UInt64`, so keying on `left.ty` silently missed
            // the propagated-literal fold (#872 — `u64::MAX / 2` folded signed
            // to 0 on the native path, where propagation runs before this).
            let arith_ty =
                if matches!(expr.ty, almide_lang::types::Ty::UInt64) { &expr.ty } else { &left.ty };
            let folded_kind = try_fold_binop(*op, &left.kind, &right.kind, arith_ty);
            folded_kind.map(|kind| IrExpr { kind, ty: expr.ty.clone(), span: expr.span, def_id: None })
        }

        // ── Unary on literals ──
        IrExprKind::UnOp { op, operand } => {
            let folded_kind = try_fold_unop(*op, &operand.kind);
            folded_kind.map(|kind| IrExpr { kind, ty: expr.ty.clone(), span: expr.span, def_id: None })
        }

        // ── if true then a else b -> a,  if false then a else b -> b ──
        IrExprKind::If { cond, then, else_ } => {
            match &cond.kind {
                IrExprKind::LitBool { value: true }  => Some(then.as_ref().clone()),
                IrExprKind::LitBool { value: false } => Some(else_.as_ref().clone()),
                _ => None,
            }
        }

        _ => None,
    }
}

/// Fold a `BinOp` whose operands are both literals of the same kind, per-operator.
/// Fold `UInt64`-typed integer BinOps whose operands are BOTH literals, using
/// the UNSIGNED reading of the i64 slot (#872). Runs after constant
/// propagation, which is what creates such pairs.
fn refold_unsigned_lane(program: &mut IrProgram) {
    struct V;
    impl almide_ir::IrMutVisitor for V {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            almide_ir::visit_mut::walk_expr_mut(self, expr);
            if !matches!(expr.ty, almide_lang::types::Ty::UInt64) {
                return;
            }
            let IrExprKind::BinOp { op, left, right } = &expr.kind else { return };
            let (IrExprKind::LitInt { value: a }, IrExprKind::LitInt { value: b }) =
                (&left.kind, &right.kind)
            else {
                return;
            };
            if let Some(k) = try_fold_binop_int(*op, *a, *b, &expr.ty) {
                expr.kind = k;
            }
        }
    }
    use almide_ir::IrMutVisitor;
    for f in &mut program.functions {
        V.visit_expr_mut(&mut f.body);
    }
    for tl in &mut program.top_lets {
        V.visit_expr_mut(&mut tl.value);
    }
    for m in &mut program.modules {
        for f in &mut m.functions {
            V.visit_expr_mut(&mut f.body);
        }
        for tl in &mut m.top_lets {
            V.visit_expr_mut(&mut tl.value);
        }
    }
}

fn try_fold_binop(
    op: BinOp,
    left: &IrExprKind,
    right: &IrExprKind,
    left_ty: &almide_lang::types::Ty,
) -> Option<IrExprKind> {
    match (left, right) {
        (IrExprKind::LitInt { value: a }, IrExprKind::LitInt { value: b }) => {
            try_fold_binop_int(op, *a, *b, left_ty)
        }
        (IrExprKind::LitFloat { value: a }, IrExprKind::LitFloat { value: b }) => try_fold_binop_float(op, *a, *b),
        (IrExprKind::LitStr { value: a }, IrExprKind::LitStr { value: b }) => try_fold_binop_str(op, a, b),
        (IrExprKind::LitBool { value: a }, IrExprKind::LitBool { value: b }) => try_fold_binop_bool(op, *a, *b),
        _ => None,
    }
}

fn try_fold_binop_int(
    op: BinOp,
    a: i64,
    b: i64,
    left_ty: &almide_lang::types::Ty,
) -> Option<IrExprKind> {
    if matches!(left_ty, almide_lang::types::Ty::UInt64) {
        return fold_uint64_binop(op, a as u64, b as u64);
    }
    fold_signed_binop(op, a, b, left_ty)
}

/// The UNSIGNED 64-bit lane (#872): a `UInt64` operand's slot is a u64 BIT
/// PATTERN, so the fold must divide/remainder unsigned — the signed fold turned
/// `u64::MAX / 2` into `-1 / 2 = 0` and `u64::MAX % 10` into `-1`, a compile-time
/// wrong value the runtime lane (`IntOp::DivU`) never sees because the fold fires
/// first. Add/sub/mul wrap identically in two's complement, so they are folded on
/// the signed reinterpretation.
fn fold_uint64_binop(op: BinOp, ua: u64, ub: u64) -> Option<IrExprKind> {
    let (a, b) = (ua as i64, ub as i64);
    let int = |value: i64| Some(IrExprKind::LitInt { value });
    let bool_ = |value: bool| Some(IrExprKind::LitBool { value });
    match op {
        BinOp::AddInt => int(a.wrapping_add(b)),
        BinOp::SubInt => int(a.wrapping_sub(b)),
        BinOp::MulInt => int(a.wrapping_mul(b)),
        BinOp::DivInt if ub != 0 => int((ua / ub) as i64),
        BinOp::ModInt if ub != 0 => int((ua % ub) as i64),
        BinOp::Lt => bool_(ua < ub),
        BinOp::Lte => bool_(ua <= ub),
        BinOp::Gt => bool_(ua > ub),
        BinOp::Gte => bool_(ua >= ub),
        _ => None,
    }
}

/// The SIGNED lane, narrowed to the declared width (see [`narrow_to_width`]).
///
/// #1117: this lane was missing the comparisons the UInt64 one already folds.
/// `guard 1 > 2 else …` reached the mir guard desugar as an unfolded BinOp cond,
/// sidestepping the const-cond fold there.
fn fold_signed_binop(
    op: BinOp,
    a: i64,
    b: i64,
    left_ty: &almide_lang::types::Ty,
) -> Option<IrExprKind> {
    let int = |v: i64| Some(IrExprKind::LitInt { value: narrow_to_width(v, left_ty) });
    let bool_ = |value: bool| Some(IrExprKind::LitBool { value });
    match op {
        BinOp::AddInt => int(a.wrapping_add(b)),
        BinOp::SubInt => int(a.wrapping_sub(b)),
        BinOp::MulInt => int(a.wrapping_mul(b)),
        BinOp::DivInt if b != 0 => int(a / b),
        BinOp::ModInt if b != 0 => int(a % b),
        BinOp::Lt => bool_(a < b),
        BinOp::Lte => bool_(a <= b),
        BinOp::Gt => bool_(a > b),
        BinOp::Gte => bool_(a >= b),
        _ => None,
    }
}

/// Wrap a folded value into the two's-complement range of `ty`.
///
/// The fold happens at i64 width, but the RESULT is emitted as a literal of the
/// declared type: `let a: Int8 = 127; let b: Int8 = 1; a + b` folded to `128`
/// and rendered `128i8`, which rustc rejects outright — a program that `check`
/// accepted and could not build, while the wasm leg (which never sees a Rust
/// literal) printed the correct `-128` (#901). The runtime path already wraps
/// (#889); this is the same rule applied at fold time, so the two agree.
/// Canonical `Int` is already i64-wide, so it is returned unchanged.
fn narrow_to_width(v: i64, ty: &almide_lang::types::Ty) -> i64 {
    use almide_lang::types::Ty;
    match ty {
        Ty::Int8 => v as i8 as i64,
        Ty::Int16 => v as i16 as i64,
        Ty::Int32 => v as i32 as i64,
        Ty::UInt8 => v as u8 as i64,
        Ty::UInt16 => v as u16 as i64,
        Ty::UInt32 => v as u32 as i64,
        _ => v,
    }
}

fn try_fold_binop_float(op: BinOp, a: f64, b: f64) -> Option<IrExprKind> {
    match op {
        BinOp::AddFloat => Some(IrExprKind::LitFloat { value: a + b }),
        BinOp::SubFloat => Some(IrExprKind::LitFloat { value: a - b }),
        BinOp::MulFloat => Some(IrExprKind::LitFloat { value: a * b }),
        BinOp::DivFloat if b != 0.0 => Some(IrExprKind::LitFloat { value: a / b }),
        _ => None,
    }
}

fn try_fold_binop_str(op: BinOp, a: &str, b: &str) -> Option<IrExprKind> {
    match op {
        BinOp::ConcatStr => Some(IrExprKind::LitStr { value: format!("{}{}", a, b) }),
        _ => None,
    }
}

fn try_fold_binop_bool(op: BinOp, a: bool, b: bool) -> Option<IrExprKind> {
    match op {
        BinOp::And => Some(IrExprKind::LitBool { value: a && b }),
        BinOp::Or  => Some(IrExprKind::LitBool { value: a || b }),
        _ => None,
    }
}

/// Fold a `UnOp` whose operand is a literal, per-operator.
fn try_fold_unop(op: UnOp, operand: &IrExprKind) -> Option<IrExprKind> {
    match (op, operand) {
        (UnOp::NegInt,   IrExprKind::LitInt   { value }) => Some(IrExprKind::LitInt   { value: -value }),
        (UnOp::NegFloat, IrExprKind::LitFloat { value }) => Some(IrExprKind::LitFloat { value: -value }),
        (UnOp::Not,      IrExprKind::LitBool  { value }) => Some(IrExprKind::LitBool  { value: !value }),
        _ => None,
    }
}

fn fold_stmt(stmt: &mut IrStmt) {
    match &mut stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => fold_expr(value),
        IrStmtKind::IndexAssign { index, value, .. } => {
            fold_expr(index);
            fold_expr(value);
        }
        IrStmtKind::MapInsert { key, value, .. } => {
            fold_expr(key);
            fold_expr(value);
        }
        IrStmtKind::ListSwap { a, b, .. } => {
            fold_expr(a);
            fold_expr(b);
        }
        IrStmtKind::ListReverse { end, .. } | IrStmtKind::ListRotateLeft { end, .. } => {
            fold_expr(end);
        }
        IrStmtKind::ListCopySlice { len, .. } => {
            fold_expr(len);
        }
        IrStmtKind::Guard { cond, else_ } => {
            fold_expr(cond);
            fold_expr(else_);
        }
        IrStmtKind::Expr { expr } => fold_expr(expr),
        IrStmtKind::Comment { .. } | IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::dce::dce_stmts;
    use almide_lang::types::Ty;

    fn lit_int(v: i64) -> IrExpr {
        IrExpr { kind: IrExprKind::LitInt { value: v }, ty: Ty::Int, span: None, def_id: None, }
    }

    fn lit_str(v: &str) -> IrExpr {
        IrExpr { kind: IrExprKind::LitStr { value: v.to_string() }, ty: Ty::String, span: None, def_id: None }
    }

    fn lit_bool(v: bool) -> IrExpr {
        IrExpr { kind: IrExprKind::LitBool { value: v }, ty: Ty::Bool, span: None, def_id: None }
    }

    #[test]
    fn fold_int_add() {
        let mut e = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::AddInt,
                left: Box::new(lit_int(1)),
                right: Box::new(lit_int(2)),
            },
            ty: Ty::Int,
            span: None, def_id: None,
        };
        fold_expr(&mut e);
        assert!(matches!(e.kind, IrExprKind::LitInt { value: 3 }));
    }

    #[test]
    fn fold_str_concat() {
        let mut e = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::ConcatStr,
                left: Box::new(lit_str("a")),
                right: Box::new(lit_str("b")),
            },
            ty: Ty::String,
            span: None, def_id: None,
        };
        fold_expr(&mut e);
        assert!(matches!(e.kind, IrExprKind::LitStr { ref value } if value == "ab"));
    }

    #[test]
    fn fold_not_true() {
        let mut e = IrExpr {
            kind: IrExprKind::UnOp {
                op: UnOp::Not,
                operand: Box::new(lit_bool(true)),
            },
            ty: Ty::Bool,
            span: None, def_id: None,
        };
        fold_expr(&mut e);
        assert!(matches!(e.kind, IrExprKind::LitBool { value: false }));
    }

    #[test]
    fn fold_if_true() {
        let mut e = IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(lit_bool(true)),
                then: Box::new(lit_int(10)),
                else_: Box::new(lit_int(20)),
            },
            ty: Ty::Int,
            span: None, def_id: None,
        };
        fold_expr(&mut e);
        assert!(matches!(e.kind, IrExprKind::LitInt { value: 10 }));
    }

    #[test]
    fn fold_if_false() {
        let mut e = IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(lit_bool(false)),
                then: Box::new(lit_int(10)),
                else_: Box::new(lit_int(20)),
            },
            ty: Ty::Int,
            span: None, def_id: None,
        };
        fold_expr(&mut e);
        assert!(matches!(e.kind, IrExprKind::LitInt { value: 20 }));
    }

    #[test]
    fn dce_removes_unused_pure_binding() {
        let mut var_table = VarTable::new();
        let x = var_table.alloc("x".into(), Ty::Int, Mutability::Let, None);
        // x has use_count 0

        let mut stmts = vec![
            IrStmt {
                kind: IrStmtKind::Bind {
                    var: x,
                    mutability: Mutability::Let,
                    ty: Ty::Int,
                    value: lit_int(42),
                },
                span: None,
            },
        ];
        dce_stmts(&mut stmts, &var_table);
        assert!(stmts.is_empty());
    }

    #[test]
    fn dce_keeps_used_binding() {
        let mut var_table = VarTable::new();
        let x = var_table.alloc("x".into(), Ty::Int, Mutability::Let, None);
        var_table.increment_use(x);

        let mut stmts = vec![
            IrStmt {
                kind: IrStmtKind::Bind {
                    var: x,
                    mutability: Mutability::Let,
                    ty: Ty::Int,
                    value: lit_int(42),
                },
                span: None,
            },
        ];
        dce_stmts(&mut stmts, &var_table);
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn dce_keeps_impure_unused_binding() {
        let mut var_table = VarTable::new();
        let x = var_table.alloc("x".into(), Ty::Int, Mutability::Let, None);
        // x has use_count 0, but value is a call (impure)

        let mut stmts = vec![
            IrStmt {
                kind: IrStmtKind::Bind {
                    var: x,
                    mutability: Mutability::Let,
                    ty: Ty::Int,
                    value: IrExpr {
                        kind: IrExprKind::Call {
                            target: CallTarget::Named { name: "expensive".into() },
                            args: vec![],
                            type_args: vec![],
                        },
                        ty: Ty::Int,
                        span: None, def_id: None,
                    },
                },
                span: None,
            },
        ];
        dce_stmts(&mut stmts, &var_table);
        assert_eq!(stmts.len(), 1); // kept because call may have side effects
    }
}
