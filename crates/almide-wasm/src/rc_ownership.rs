//! RC-3 ownership machinery: the borrow/fresh classifier, the droppable
//! set, and the inc/share/arg guards — split from emitter.rs for the
//! file budget.

use almide_ir::{IrExpr, IrExprKind, VarId};
use wasm_encoder::ValType;

use crate::emitter::Emitter;
use crate::*;

// ── RC-3 ownership helpers ──────────────────────────────────────────────

/// Expressions that certainly PRODUCE a fresh (or pool-static) block —
/// binding one transfers ownership, no inc. Everything else may borrow
/// (var reads, element/field reads, calls into native arms, if/match
/// funnels) and takes the +1; over-inc on a fresh value is a leak,
/// never a dangle.
pub(crate) fn rc_certainly_fresh(k: &almide_ir::IrExprKind) -> bool {
    use almide_ir::IrExprKind as K;
    matches!(
        k,
        K::LitStr { .. }
            | K::StringInterp { .. }
            | K::BinOp { .. }
            | K::List { .. }
            | K::MapLiteral { .. }
            | K::EmptyMap
            | K::Record { .. }
            | K::SpreadRecord { .. }
            | K::Tuple { .. }
            | K::Range { .. }
            | K::ResultOk { .. }
            | K::ResultErr { .. }
            | K::OptionSome { .. }
            // `none` is NULL_ADDR — no block, so "owned" costs nothing and
            // lets an `if c then some(x) else none` tail count as owned.
            | K::OptionNone
    )
}

/// The tail expression a fn body RETURNS (through block tails).
pub(crate) fn rc_tail(e: &almide_ir::IrExpr) -> &almide_ir::IrExpr {
    match &e.kind {
        almide_ir::IrExprKind::Block { expr: Some(t), .. } => rc_tail(t),
        _ => e,
    }
}

impl Emitter<'_> {
    /// The droppable set (#2010 stage 1): every block shape with NO heap
    /// interiors — Str, Bytes, and a List / tuple / record / variant /
    /// Option / Result whose payload slots are all flat. Such a block is
    /// released by `$dec_flat` alone: there is no shared field to
    /// dangle and no glue to recurse into. A shape holding a block (a
    /// `List[String]`, an `(Int, String)`, a `Map`) stays on the bump
    /// graveyard until its typed drop glue exists (#2010 stages 2–4).
    pub(crate) fn rc_droppable(&self, t: SliceTy) -> bool {
        match t {
            SliceTy::Scalar(Scalar::Str | Scalar::Bytes) => true,
            // A List of ANY element (stage 2a): `$dec_flat` frees the SPINE
            // only; the elements keep the credits they hold today. The
            // shallow-drop experiment first diverged 40 corpus fixtures, and
            // every one was one of two defects: `value.object` borrowed the
            // spine it keeps (now Retain), and the matrix byte loaders trusted
            // fresh memory to be zero (now filled). Releasing the elements is
            // stage 2b: typed glue (#2010).
            SliceTy::List(_) => true,
            SliceTy::Option(h) => self.flat_slot(self.types.el(h)),
            SliceTy::Result(a, b) => self.flat_slot(self.types.el(a)) && self.flat_slot(self.types.el(b)),
            SliceTy::Tuple(h) => self.types.tuple_def(h).fields.iter().all(|&(t, _)| self.flat_slot(t)),
            // Records and variants wait for stage 1b: the `mut` param
            // move-mode rewrite (C-132) carries a record through a
            // (result, buffer) tuple and writes it back through the Assign
            // route, whose ownership is not yet audited for a droppable
            // record (mut_param_effect_never_err double-freed the Tally).
            SliceTy::Named(_) => false,
            _ => false,
        }
    }

    /// A slot that holds no heap block: a non-Str/Bytes scalar, a flowing
    /// Unit, or a fn value (a table index).
    fn flat_slot(&self, t: SliceTy) -> bool {
        match t {
            SliceTy::Scalar(s) => !matches!(s, Scalar::Str | Scalar::Bytes),
            SliceTy::Unit | SliceTy::Fn(_) => true,
            _ => false,
        }
    }

    /// +1 the i32 block handle on top of the stack, leaving it there.
    pub(crate) fn rc_inc_top(&mut self) {
        let scr = self.scr_i32_local;
        let mut i = self.f.instructions();
        i.local_set(scr);
        i.local_get(scr).call(F_INC);
        i.local_get(scr);
    }
}

impl Emitter<'_> {
    /// RC-3 share guard for handle STORES into blocks: when the stored
    /// expression reads a droppable-owned LOCAL, the container becomes a
    /// co-owner (+1) — otherwise the local's release would free a block
    /// the container still holds. Fresh values and non-droppable
    /// sources pass through untouched (their owner never decs).
    pub(crate) fn rc_share_guard(&mut self, e: &almide_ir::IrExpr, ty: SliceTy) {
        if ty.val_type() != ValType::I32 {
            return;
        }
        // #1219 stage 1: a Map handle stored into a block witnesses a
        // second holder. Maps stay OFF the droppable set (never dec'd),
        // so the count is MONOTONE — "shared at some point" — which is
        // exactly what the in-place set window asks (rc == 1 ⇒ the
        // var's block is its alone). Binds/assigns copy, so a plain var
        // never shares; a fresh value has no other holder to witness.
        if matches!(ty, SliceTy::Map(..)) {
            if !rc_certainly_fresh(&e.kind) {
                self.rc_inc_top();
            }
            return;
        }
        match &e.kind {
            almide_ir::IrExprKind::Var { id } => {
                if self.cells.contains(id) {
                    return;
                }
                let Some(&(_, vt)) = self.locals.get(id) else { return };
                if self.rc_droppable(vt) {
                    self.rc_inc_top();
                }
            }
            // A control funnel can RETURN a var borrow through its arm
            // tails (`push(out, if c then a else b)`) — the O3 gap.
            // Conservative +1 when the stored type itself is droppable:
            // an over-inc on a fresh arm is a leak, never a dangle.
            almide_ir::IrExprKind::If { .. }
            | almide_ir::IrExprKind::Match { .. }
            | almide_ir::IrExprKind::Block { .. }
            | almide_ir::IrExprKind::Unwrap { .. }
            | almide_ir::IrExprKind::UnwrapOr { .. }
            | almide_ir::IrExprKind::Try { .. }
                if self.rc_droppable(ty) =>
            {
                self.rc_inc_top();
            }
            _ => {}
        }
    }
}

/// Does the expression read `var` anywhere? (The Assign dec-old
/// suppressor: a self-referential rhs means ownership moved through it.)
pub(crate) fn rc_mentions_var(e: &IrExpr, var: VarId) -> bool {
    struct Finder {
        var: VarId,
        found: bool,
    }
    impl almide_ir::visit::IrVisitor for Finder {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Var { id } = &e.kind
                && *id == self.var
            {
                self.found = true;
            }
            if !self.found {
                almide_ir::visit::walk_expr(self, e);
            }
        }
    }
    let mut f = Finder { var, found: false };
    almide_ir::visit::IrVisitor::visit_expr(&mut f, e);
    f.found
}

impl Emitter<'_> {
    /// A value that arrives OWNED — exactly one credit this frame may
    /// release: a certainly-fresh construction, or the result of a call
    /// to a table fn (#1986). A table fn's epilogue hands its caller one
    /// credit on every path (a certainly-fresh tail is the alloc itself,
    /// any other droppable tail takes the ret-inc), so the caller's bind,
    /// assign and return routes must NOT add another — the +1 they took
    /// for a "borrowed" rhs left every returned List/Str/Bytes at rc 1
    /// forever (64 B per call, measured N=1000 vs N=8000). Ctors and
    /// module helpers keep the conservative +1 (a leak is never a
    /// dangle); helper-by-helper conventions are the follow-up.
    /// The frame's droppable PARAM this expression is, when it is a plain
    /// read of one (`b` in `__arr(b, …)`): its local index.
    pub(crate) fn frame_param_var(&self, e: &almide_ir::IrExpr) -> Option<u32> {
        let almide_ir::IrExprKind::Var { id } = &e.kind else { return None };
        let &(idx, _) = self.locals.get(id)?;
        self.rc_frame_params.contains(&idx).then_some(idx)
    }

    pub(crate) fn rc_owned_result(&self, e: &almide_ir::IrExpr) -> bool {
        if rc_certainly_fresh(&e.kind) {
            return true;
        }
        // `{ let t = …; op(t) }` (arg_temps.rs) and any block: the value
        // is the tail's.
        if let almide_ir::IrExprKind::Block { expr: Some(tail), .. } = &e.kind {
            return self.rc_owned_result(tail);
        }
        // A conditional is owned when EVERY arm's value is: the return
        // route then takes no +1 (an `if i >= 0 then int.to_string(i)
        // else "neg"` tail leaked its result on every call, #2005). One
        // borrowed arm makes the whole value borrowed — the per-arm
        // identity is #1996.
        if let almide_ir::IrExprKind::If { then, else_, .. } = &e.kind {
            return self.rc_owned_result(then) && self.rc_owned_result(else_);
        }
        if let almide_ir::IrExprKind::Match { arms, .. } = &e.kind {
            return !arms.is_empty() && arms.iter().all(|a| self.rc_owned_result(&a.body));
        }
        let almide_ir::IrExprKind::Call { target, .. } = &e.kind else {
            return false;
        };
        let name = match target {
            almide_ir::CallTarget::Named { name } => name.as_str(),
            // `prim.alloc_*` is a fresh block at rc 1: the bind OWNS it.
            // (An earlier attempt to treat it as owned was blamed for the
            // regex engine's capture garbage; the culprit was the
            // loop-form double free of #1988, co-landed at the time —
            // owned allocs sent every alloc-ledger watermark DOWN.)
            almide_ir::CallTarget::Module { module, func, .. }
                if module.as_str() == "prim" && func.as_str().starts_with("alloc_") =>
            {
                return true;
            }
            // A module call is owned exactly when its NODE was marked by
            // the dispatch that lowered it: the registry-table path
            // (#1990) or a native arm that declared `Lowered::owned` (#2004).
            almide_ir::CallTarget::Module { .. } => {
                return self.owned_call_marks.contains(&(target as *const almide_ir::CallTarget as usize));
            }
            _ => return false,
        };
        if self.types.ctors.contains_key(name) {
            return false;
        }
        self.cur_module
            .and_then(|m| self.table.by_name.get(&format!("{m}.{name}")))
            .or_else(|| self.table.by_name.get(name))
            .is_some()
            || self.resolve_qualified(name).is_some()
            || self.resolve_method_suffix(name).is_some()
    }

    pub(crate) fn rc_arg_guard(&mut self, e: &almide_ir::IrExpr, ty: SliceTy) {
        // A BORROWED argument (a Var, a field or element read, a native
        // arm's result) takes +1: the callee's epilogue decs its params.
        // An OWNED argument — a fresh literal, or a call result that
        // arrived with its one credit (#1986) — moves INTO the callee:
        // no +1, the callee's dec spends the credit. Guarding an owned
        // result left every `f(g(x))` temporary at rc 1 forever (#2004:
        // `string.len(int.to_string(i))` grew 16 B per call).
        if self.rc_droppable(ty) && !self.rc_owned_result(e) {
            self.rc_inc_top();
        }
    }
}

/// True when the body makes ANY `prim.*` module call — the raw-address
/// origin (`prim.handle`, raw loads). A fn whose body touches prim may
/// hand a tail callee a pointer into a param's block, so the tail-site
/// param release (calls.rs) is fenced off for it.
pub(crate) fn body_uses_prim(body: &almide_ir::IrExpr) -> bool {
    struct Scan {
        hit: bool,
    }
    impl almide_ir::visit::IrVisitor for Scan {
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            if self.hit {
                return;
            }
            if let almide_ir::IrExprKind::Call { target, .. }
            | almide_ir::IrExprKind::TailCall { target, .. } = &e.kind
            {
                if let almide_ir::CallTarget::Module { module, .. } = target
                    && module.as_str() == "prim"
                {
                    self.hit = true;
                    return;
                }
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut s = Scan { hit: false };
    almide_ir::visit::IrVisitor::visit_expr(&mut s, body);
    s.hit
}
