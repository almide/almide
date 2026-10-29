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
            // A Fn value (#2010 closures): a capturing lambda allocates its
            // env at rc 1; a capture-free lambda and a named fn are pool
            // statics the rc ops no-op on.
            | K::Lambda { .. }
            | K::FnRef { .. }
    )
}

/// The tail expression a fn body RETURNS (through block tails).
pub(crate) fn rc_tail(e: &almide_ir::IrExpr) -> &almide_ir::IrExpr {
    match &e.kind {
        almide_ir::IrExprKind::Block { expr: Some(t), .. } => rc_tail(t),
        _ => e,
    }
}

/// The per-type RC routine choice moved to rc_shape.rs; its free form is
/// still reached as `rc_ownership::rc_droppable_ty`.
pub(crate) use crate::rc_shape::rc_droppable_ty;

impl crate::emitter::Emitter<'_> {
    /// The handle on top of the stack is being COPIED into a block that
    /// will release it (a pair, an Option, a set entry, a list slot): +1
    /// when its type is one a holder owns a credit of, nothing otherwise.
    pub(crate) fn share_handle_top(&mut self, t: SliceTy) {
        if self.elem_is_handle(t) {
            self.rc_inc_top();
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
        match &e.kind {
            // #3406: a dying var's credit moves in (dying_move.rs).
            almide_ir::IrExprKind::Var { .. } if self.owned_call_marks.is_moving(e) => {}
            // A C-319 cell var reads its OCCUPANT out of the cell, and the
            // cell holds one credit on it that the next assign releases
            // (#2010) — so a container storing the read co-owns it exactly
            // as it would a plain local's block. (The cell skip this
            // replaced dated from when an occupant was never released.)
            // A top-let GLOBAL (#2803) is a holder too: the module keeps its
            // one credit for the program's life and never releases it, but
            // the container DOES release what it stores — without the +1 the
            // container's drop freed the global's block while the global
            // still named it, and the next allocation reused it.
            almide_ir::IrExprKind::Var { id } => {
                let vt = match self.locals.get(id) {
                    Some(&(_, vt)) => vt,
                    None => match self.globals.get(&(self.var_space, *id)) {
                        Some(&(_, gt)) => gt,
                        None => return,
                    },
                };
                self.share_handle_top(vt);
            }
            // A control funnel can RETURN a var borrow through its arm
            // tails (`push(out, if c then a else b)`) — the O3 gap.
            // Conservative +1 when the stored type itself is droppable:
            // an over-inc on a fresh arm is a leak, never a dangle.
            // Every other BORROWED droppable value takes +1: a control
            // funnel returning a var borrow through its arm tails (the O3
            // gap), an element / field read, an unwrap of a payload, a
            // native arm's declared View. An OWNED value — a fresh
            // construction, an owned call result, a funnel whose every arm
            // is owned (`rc_owned_result`) — moves in with its credit.
            _ if self.rc_droppable(ty) && !self.rc_owned_result(e) => {
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
    /// forever (64 B per call, measured N=1000 vs N=8000). A variant
    /// constructor is owned too (#2317); a module helper keeps the
    /// conservative +1 unless its arm declares an owned result (a leak is
    /// never a dangle).
    /// The frame's droppable PARAM this expression is, when it is a plain
    /// read of one (`b` in `__arr(b, …)`): its local index.
    pub(crate) fn frame_param_var(&self, e: &almide_ir::IrExpr) -> Option<u32> {
        let almide_ir::IrExprKind::Var { id } = &e.kind else { return None };
        let &(idx, _) = self.locals.get(id)?;
        self.rc_frame_params.contains(&idx).then_some(idx)
    }

    pub(crate) fn rc_owned_result(&self, e: &almide_ir::IrExpr) -> bool {
        if rc_certainly_fresh(&e.kind) || self.owned_call_marks.is_moving(e) {
            return true;
        }
        // `{ let t = …; op(t) }` (arg_temps.rs) and any block: the value
        // is the tail's.
        if let almide_ir::IrExprKind::Block { expr: Some(tail), .. } = &e.kind {
            return self.rc_owned_result(tail);
        }
        // A conditional is owned when EVERY arm's value is: the return
        // route then takes no +1 (an `if i >= 0 then int.to_string(i)
        // else "neg"` tail leaked its result on every call, #2005). Arms
        // that disagree are normalized by `lower_if_arms` — the borrowed
        // arm takes its +1 inside, so each arm hands the join one credit —
        // and the lowering marks the node (#2317). An unmarked `if` with a
        // borrowed arm stays borrowed.
        if let almide_ir::IrExprKind::If { then, else_, .. } = &e.kind {
            return (self.rc_owned_result(then) && self.rc_owned_result(else_))
                || self.owned_call_marks.is_marked(e);
        }
        if let almide_ir::IrExprKind::Match { arms, .. } = &e.kind {
            // lower_arm_body normalizes every value arm to one credit.
            return !arms.is_empty();
        }
        // An extraction (`expr!`) is owned exactly when ITS lowering
        // released the carrier it read the payload out of (#2509,
        // `release_ok_carrier`): the carrier's one credit on the payload
        // moved to the value. Every other extraction borrows the payload
        // from a carrier some other route releases, and stays borrowed.
        // `r?` (Result → Option) is owned on the same terms (#2516): its
        // lowering marks the node exactly when the carrier was owned and the
        // payload's credit moved into the fresh some-cell.
        if matches!(
            &e.kind,
            almide_ir::IrExprKind::Try { .. }
                | almide_ir::IrExprKind::Unwrap { .. }
                | almide_ir::IrExprKind::ToOption { .. }
                // `r ?? fb` is owned when its lowering normalized both arms
                // to one credit (#2970, `own_unwrap_or_join`).
                | almide_ir::IrExprKind::UnwrapOr { .. }
                // `m[k]` is `map.get` (lower_data): owned when the arm was.
                | almide_ir::IrExprKind::MapAccess { .. }
                // `fan { … }` (fan.rs `lower_fan_block`): owned when marked.
                | almide_ir::IrExprKind::Fan { .. }
                // #3406: a moved slot (dying_move.rs).
                | almide_ir::IrExprKind::Member { .. }
        ) {
            return self.owned_call_marks.is_marked(e);
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
                return self.owned_call_marks.is_target_marked(target);
            }
            // A closure call (#2010): the lifted body is lowered by the same
            // `lower_fn` a table fn is — its epilogue hands the caller ONE
            // credit on every path (a named fn's shim forwards the plain
            // fn's owned result; an adapter's ok-carrier is fresh).
            almide_ir::CallTarget::Computed { .. } => return true,
            _ => return false,
        };
        // A variant constructor is OWNED (#2317): a case with a payload is
        // a fresh block at rc 1 (`lower_variant_ctor`), a nullary case is a
        // pool static the rc ops no-op on. Classed borrowed, it took the
        // conservative +1 on every route — `if d == 0 then Leaf else
        // Node(…)` returned two credits and no tree node was ever freed.
        if self.is_variant_ctor(name, e) {
            return true;
        }
        // A bodyless host-op leaf (http_call.rs `lower_host_leaf`) builds
        // every answer fresh at rc 1, and its intercept marks the call.
        // Classed borrowed (a leaf has no table entry), the +1 kept the
        // carrier alive forever: an http call handle never reached its drop
        // glue (#2633), and every framed-http / subprocess Result leaked
        // with its payload (#3623).
        if self.owned_call_marks.is_target_marked(target) {
            return true;
        }
        self.cur_module
            .and_then(|m| self.table.by_name.get(&format!("{m}.{name}")))
            .or_else(|| self.table.by_name.get(name))
            .is_some()
            || self.resolve_qualified(name).is_some()
            || self.resolve_method_suffix(name).is_some()
    }

    /// Does the Named call `name` build a variant case? The same two routes
    /// `lower_call_at` takes: the concrete ctor map, then a generic
    /// instance's case looked up in the call's own annotated type.
    pub(crate) fn is_variant_ctor(&self, name: &str, e: &almide_ir::IrExpr) -> bool {
        self.types.ctors.contains_key(name)
            || matches!(slice_ty_of(&e.ty, self.types), Some(SliceTy::Named(ti))
                if matches!(self.types.def(ti), crate::types_table::NamedDef::Variant(v)
                    if v.cases.iter().any(|c| c.name == name)))
    }

    /// `rc_owned_result` as a value-position `if` can know it BEFORE the
    /// arm is lowered: a nested `if` by its own normalization rule (either
    /// arm owned), a block by its tail. A node that is owned only once its
    /// lowering MARKS it (a module call, an extraction, `r?`, `??`, `m[k]`,
    /// `fan { … }`) may be owned: it predicts owned (#2969 — `if
    /// set.contains(s, x) then s else set.insert(s, x)` predicted borrowed,
    /// so the fresh insert and the borrowed `s` went unnormalized and every
    /// insert kept a second credit). A wrong guess costs nothing: each arm
    /// is judged again after it is lowered, and a borrowed arm takes its
    /// `+1` inside, so the join still hands back exactly one credit.
    fn rc_predict_owned(&self, e: &almide_ir::IrExpr) -> bool {
        match &e.kind {
            IrExprKind::If { then, else_, .. } => self.rc_predict_owned(then) || self.rc_predict_owned(else_),
            IrExprKind::Block { expr: Some(t), .. } => self.rc_predict_owned(t),
            // A module call's ownership is its arm's DECLARATION, known only
            // once it is lowered (arm.rs) — and nearly every droppable arm
            // result is `Owned` (two `View` arms exist). Predicting owned is
            // safe either way: the join then asks each arm what it actually
            // was and gives a borrowed one its +1. Predicting borrowed left
            // `if c then name else list.join(xs, ".")` (path.stem) with a
            // borrowed join over an owned arm — its block leaked (#2977).
            IrExprKind::Call { target: almide_ir::CallTarget::Module { .. }, .. }
            | IrExprKind::Try { .. }
            | IrExprKind::Unwrap { .. }
            | IrExprKind::ToOption { .. }
            | IrExprKind::UnwrapOr { .. }
            | IrExprKind::MapAccess { .. }
            | IrExprKind::Fan { .. } => true,
            _ => self.rc_owned_result(e),
        }
    }

    /// The arms of a value-position `if` of type `ty` (#2317). When the
    /// value is droppable and an arm is owned — a fresh construction, an
    /// owned call result — every arm hands the join exactly ONE credit: a
    /// borrowed arm (a Var, a field read) takes its +1 inside, and the node
    /// is marked owned so no route adds another. Before, `if d == 0 then
    /// Leaf else x` was borrowed as a whole and the consumer's +1 gave the
    /// fresh arm two credits — the `match` arms have had this rule since
    /// #2046. Two borrowed arms stay borrowed and pay nothing.
    ///
    /// The `then` arm decides from a PREDICTION of `else_` (it is lowered
    /// first); the `else_` arm from what `then` actually was. A module call
    /// predicts owned (`rc_predict_owned`), so an `else_` owned only by its
    /// arm's mark normalizes the join too.
    pub(crate) fn lower_if_arms(
        &mut self,
        e: &almide_ir::IrExpr,
        ty: SliceTy,
        tail: bool,
    ) -> Result<(), EmitError> {
        let IrExprKind::If { then, else_, .. } = &e.kind else { return unsup("if-arms") };
        let normalize = self.rc_droppable(ty) && (self.rc_predict_owned(then) || self.rc_predict_owned(else_));
        // #4277: the site is anchored to this `if` and its `else`, so the RC
        // check reads the recorder's first arm as the shipped then-arm.
        let site = crate::witness::rc::site_id();
        self.if_sites.insert(e as *const _, site);
        let if_at = self.f.byte_len();
        self.f.instructions().if_(wasm_encoder::BlockType::Result(ty.val_type()));
        self.branch_depth += 1;
        // The witness (#2756) sees each arm and the credit it hands the join.
        self.witness_branch_open_at(site);
        let arms = (|| {
            self.witness_branch_arm();
            self.in_tail = tail;
            self.lower(then, Some(ty))?;
            let then_owned = self.rc_owned_result(then);
            if normalize && !then_owned {
                self.rc_inc_top();
            }
            if self.rc_droppable(ty) && (normalize || then_owned) {
                self.witness_arm_value(then);
            }
            self.f.instructions().else_();
            crate::witness::rc::site_window(site, if_at, self.f.byte_len());
            self.witness_branch_arm_at(site);
            self.in_tail = tail;
            self.lower(else_, Some(ty))?;
            let join = normalize || (then_owned && self.rc_droppable(ty));
            if join && !self.rc_owned_result(else_) {
                self.rc_inc_top();
            }
            if join {
                self.witness_arm_value(else_);
            }
            Ok(join)
        })();
        self.witness_branch_close();
        self.branch_depth -= 1;
        if arms? {
            self.owned_call_marks.mark(e);
        }
        self.f.instructions().end();
        Ok(())
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
