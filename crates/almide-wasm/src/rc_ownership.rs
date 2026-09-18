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
    /// A slot whose value is a heap HANDLE the holder owns one credit of
    /// (#2010 stage 2b/2c): Str / Bytes, a List, an Option / Result /
    /// tuple block, a record / variant (2c-ii), a Map / Set (Map stage
    /// b). Value and Fn handles carry no credit the holder releases yet.
    pub(crate) fn elem_is_handle(&self, elem: SliceTy) -> bool {
        match elem {
            SliceTy::Scalar(Scalar::Str | Scalar::Bytes)
            | SliceTy::List(_)
            | SliceTy::Option(_)
            | SliceTy::Result(..)
            | SliceTy::Tuple(_) => true,
            // Stage 2c-ii: records and variants with a layout.
            SliceTy::Named(ti) => self.named_has_layout(ti),
            // Map stage b: the entries array and its entries are credits.
            SliceTy::Map(..) | SliceTy::Set(_) => true,
            _ => false,
        }
    }

    /// Does the type table hold a layout for this Named type (a record or
    /// a variant — `Excluded` names have slots but no layout)?
    fn named_has_layout(&self, ti: u32) -> bool {
        matches!(self.types.def(ti), crate::types_table::NamedDef::Record(_) | crate::types_table::NamedDef::Variant(_))
    }

    /// The handle slots of a fixed-slot block: `(payload offset, dec fn)`
    /// per slot, and for a tagged block (Result, variant) the per-tag
    /// tables under the tag word. Registers the nested glue on the way.
    fn shape_slots(&self, t: SliceTy) -> (Vec<(u32, u32)>, Option<(u32, Vec<(u32, Vec<(u32, u32)>)>)>) {
        let slot = |ft: SliceTy, off: u32| self.elem_is_handle(ft).then(|| (off, self.dec_fn_of(ft)));
        match t {
            SliceTy::Option(h) => (slot(self.types.el(h), almide_layout::OPTION_FIELD).into_iter().collect(), None),
            SliceTy::Result(a, b) => {
                let cases = [(0u32, self.types.el(a)), (1u32, self.types.el(b))]
                    .into_iter()
                    .filter_map(|(tag, pt)| slot(pt, almide_layout::SUM_FIELD).map(|s| (tag, vec![s])))
                    .collect();
                (Vec::new(), Some((almide_layout::SUM_TAG, cases)))
            }
            SliceTy::Tuple(h) => {
                let fields = self.types.tuple_def(h).fields.clone();
                (fields.iter().filter_map(|&(ft, off)| slot(ft, off)).collect(), None)
            }
            SliceTy::Named(ti) => match self.types.def(ti) {
                crate::types_table::NamedDef::Record(def) => {
                    (def.fields.iter().filter_map(|fi| slot(fi.ty, fi.offset)).collect(), None)
                }
                crate::types_table::NamedDef::Variant(def) => {
                    let cases = def
                        .cases
                        .iter()
                        .map(|c| (c.tag, c.fields.iter().filter_map(|fi| slot(fi.ty, fi.offset)).collect::<Vec<_>>()))
                        .filter(|(_, v)| !v.is_empty())
                        .collect();
                    (Vec::new(), Some((almide_layout::SUM_TAG, cases)))
                }
                crate::types_table::NamedDef::Excluded => (Vec::new(), None),
            },
            _ => (Vec::new(), None),
        }
    }

    /// Does a fixed-slot block of type `t` hold any handle slot at all?
    fn shape_has_handles(&self, t: SliceTy) -> bool {
        let has = |ft: SliceTy| self.elem_is_handle(ft);
        match t {
            SliceTy::Option(h) => has(self.types.el(h)),
            SliceTy::Result(a, b) => has(self.types.el(a)) || has(self.types.el(b)),
            SliceTy::Tuple(h) => self.types.tuple_def(h).fields.iter().any(|&(ft, _)| has(ft)),
            SliceTy::Named(ti) => match self.types.def(ti) {
                crate::types_table::NamedDef::Record(def) => def.fields.iter().any(|fi| has(fi.ty)),
                crate::types_table::NamedDef::Variant(def) => {
                    def.cases.iter().any(|c| c.fields.iter().any(|fi| has(fi.ty)))
                }
                crate::types_table::NamedDef::Excluded => false,
            },
            _ => false,
        }
    }

    /// Register a per-shape glue (`DropShape` / `IncShape`) and build its
    /// body once. A RECURSIVE shape (a tree variant whose case holds the
    /// tree) reaches its own registration while its slots are computed:
    /// the placeholder inserted first makes that inner call return the
    /// promised index instead of recursing.
    fn shape_helper(&self, h: crate::work::Helper, ty: SliceTy) -> u32 {
        let idx = self.work.helper(h.clone());
        if self.work.drop_bodies.borrow().iter().any(|(k, _)| *k == h) {
            return idx;
        }
        self.work.drop_bodies.borrow_mut().push((h.clone(), None));
        let (slots, tagged) = self.shape_slots(ty);
        let body = match h {
            crate::work::Helper::IncShape { .. } => crate::runtime_alloc::emit_inc_shape(&slots, tagged),
            _ => crate::runtime_alloc::emit_drop_shape(&slots, tagged),
        };
        let mut bodies = self.work.drop_bodies.borrow_mut();
        if let Some(entry) = bodies.iter_mut().find(|(k, _)| *k == h) {
            entry.1 = Some(body);
        }
        idx
    }

    pub(crate) fn dec_fn_of(&self, t: SliceTy) -> u32 {
        match t {
            SliceTy::List(h) => {
                let elem = self.types.el(h);
                if self.elem_is_handle(elem) {
                    let elem_dec = self.dec_fn_of(elem);
                    self.work.helper(crate::work::Helper::DropList { elem_dec })
                } else {
                    F_DEC_FLAT
                }
            }
            SliceTy::Option(_) | SliceTy::Result(..) | SliceTy::Tuple(_) | SliceTy::Named(_) => {
                if self.shape_has_handles(t) {
                    self.shape_helper(crate::work::Helper::DropShape { ty: t }, t)
                } else {
                    F_DEC_FLAT
                }
            }
            SliceTy::Map(..) | SliceTy::Set(_) => {
                let raw = self.work.helper(crate::work::Helper::MapIdxSideRaw);
                let side_clear = self.work.helper(crate::work::Helper::MapIdxSideSet { raw });
                let (stride, decs) = self.entry_slots(t);
                let slots: [Option<(u32, u32)>; 2] =
                    [decs[0].map(|(off, ft)| (off, self.dec_fn_of(ft))), decs[1].map(|(off, ft)| (off, self.dec_fn_of(ft)))];
                if slots.iter().all(Option::is_none) {
                    self.work.helper(crate::work::Helper::DropMapSpine { side_clear })
                } else {
                    self.work.helper(crate::work::Helper::DropEntries { stride, slots, side_clear })
                }
            }
            _ => F_DEC_FLAT,
        }
    }

    /// The entry layout of a Map / Set: `(stride, [key slot, value slot])`
    /// where a slot is `Some((offset, type))` only when the type is a heap
    /// handle the entries array holds a credit of (a Set has one slot at
    /// offset 0; a flat key or value is `None`).
    fn entry_slots(&self, t: SliceTy) -> (u32, [Option<(u32, SliceTy)>; 2]) {
        let handle = |ft: SliceTy, off: u32| self.elem_is_handle(ft).then_some((off, ft));
        match t {
            SliceTy::Map(kh, vh) => {
                let (k, v) = (self.types.el(kh), self.types.el(vh));
                let (koff, voff, stride) = crate::collections::entry_layout(k, v);
                (stride, [handle(k, koff), handle(v, voff)])
            }
            SliceTy::Set(h) => {
                let e = self.types.el(h);
                (e.slot_size(), [handle(e, 0), None])
            }
            _ => (0, [None, None]),
        }
    }

    /// `Some($inc_entries)` when the entries of a Map / Set of type `t`
    /// hold handle slots a copied entries array must take credits on.
    pub(crate) fn inc_entries_fn(&self, t: SliceTy) -> Option<u32> {
        let (stride, decs) = self.entry_slots(t);
        let slots = [decs[0].map(|(off, _)| off), decs[1].map(|(off, _)| off)];
        (!slots.iter().all(Option::is_none)).then(|| self.work.helper(crate::work::Helper::IncEntries { stride, slots }))
    }

    /// +1 on every handle slot of the entries in local `h` (the whole
    /// LEN, or the first `nbytes` from local `nbytes_local`) — after a
    /// native arm copied entries out of another Map / Set.
    pub(crate) fn emit_inc_entries(&mut self, h: u32, t: SliceTy, nbytes_local: Option<u32>) {
        if let Some(f) = self.inc_entries_fn(t) {
            let mut i = self.f.instructions();
            i.local_get(h);
            match nbytes_local {
                Some(n) => i.local_get(n),
                None => i.local_get(h).i32_load(len_memarg()),
            };
            i.call(f);
        }
    }

    /// The release fn of an owned LOCAL, by the type `rc_own` recorded
    /// for it (a param is recorded at frame entry).
    pub(crate) fn dec_fn_of_local(&self, idx: u32) -> u32 {
        self.owned_ty.get(&idx).map_or(F_DEC_FLAT, |&t| self.dec_fn_of(t))
    }

    /// `Some($inc_elems)` when a spine of `elem` slots copied from another
    /// spine must take its own element credits.
    pub(crate) fn inc_elems_fn(&self, elem: SliceTy) -> Option<u32> {
        self.elem_is_handle(elem).then(|| self.work.helper(crate::work::Helper::IncElems))
    }

    /// +1 on every element of the spine in local `h` (nothing for scalar
    /// elements) — after a native arm's `memory_copy` of its slots.
    pub(crate) fn emit_inc_elems(&mut self, h: u32, elem: SliceTy) {
        if let Some(f) = self.inc_elems_fn(elem) {
            self.f.instructions().local_get(h).call(f);
        }
    }

    /// The whole-block copy of a value of type `t`: `$block_copy`, or for
    /// a List of handles the variant whose copy takes its element credits.
    pub(crate) fn copy_fn_of(&self, t: SliceTy) -> u32 {
        match t {
            SliceTy::List(h) => match self.inc_elems_fn(self.types.el(h)) {
                Some(inc_elems) => self.work.helper(crate::work::Helper::CopyElems { inc_elems }),
                None => F_BLOCK_COPY,
            },
            SliceTy::Option(_) | SliceTy::Result(..) | SliceTy::Tuple(_) | SliceTy::Named(_)
                if self.shape_has_handles(t) =>
            {
                let inc_elems = self.shape_helper(crate::work::Helper::IncShape { ty: t }, t);
                self.work.helper(crate::work::Helper::CopyElems { inc_elems })
            }
            // Map stage b: a copied entries array takes its entry credits.
            SliceTy::Map(..) | SliceTy::Set(_) => match self.inc_entries_fn(t) {
                Some(inc_entries) => self.work.helper(crate::work::Helper::CopyEntries { inc_entries }),
                None => F_BLOCK_COPY,
            },
            _ => F_BLOCK_COPY,
        }
    }

    /// The copy-on-write judge for a value of type `t`: `$cow`, or for a
    /// List of handles the variant whose copy takes its element credits.
    pub(crate) fn cow_fn_of(&self, t: SliceTy) -> u32 {
        match t {
            SliceTy::List(h) => match self.inc_elems_fn(self.types.el(h)) {
                Some(inc_elems) => self.work.helper(crate::work::Helper::CowElems { inc_elems }),
                None => F_COW,
            },
            _ => F_COW,
        }
    }

    pub(crate) fn rc_droppable(&self, t: SliceTy) -> bool {
        rc_droppable_ty(self.types, t)
    }
}

/// `rc_droppable` as a free function: the param borrow inference
/// (param_borrow.rs) runs before any Emitter exists.
pub(crate) fn rc_droppable_ty(types: &crate::types_table::TypeTable, t: SliceTy) -> bool {
    {
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
            // Stage 2c: an Option / Result / tuple block of ANY payload —
            // a handle slot is released by the typed drop (`dec_fn_of`),
            // a flat slot needs nothing, and a slot of a shape with no
            // typed drop yet (a record, a Map) keeps its credit (leak,
            // never a dangle).
            SliceTy::Option(_) | SliceTy::Result(..) | SliceTy::Tuple(_) => true,
            // Records and variants wait for stage 1b: the `mut` param
            // move-mode rewrite (C-132) carries a record through a
            // (result, buffer) tuple and writes it back through the Assign
            // route, whose ownership is not yet audited for a droppable
            // record (mut_param_effect_never_err double-freed the Tally).
            SliceTy::Named(ti) => matches!(
                types.def(ti),
                crate::types_table::NamedDef::Record(_) | crate::types_table::NamedDef::Variant(_)
            ),
            // Map stage b (#2010): the entries array is released with its
            // index side-table entry, and every handle key / value / member
            // through the typed entry walk (`DropEntries`).
            SliceTy::Map(..) | SliceTy::Set(_) => true,
            _ => false,
        }
    }
}

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
            almide_ir::IrExprKind::Var { id } => {
                if self.cells.contains(id) {
                    return;
                }
                let Some(&(_, vt)) = self.locals.get(id) else { return };
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
            // lower_arm_body normalizes every value arm to one credit.
            return !arms.is_empty();
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
