//! Sum- and record-shaped VALUE lowering (constructors, unwrap markers,
//! record literals, spreads) — split from emitter.rs for the complexity
//! budget; `lower`'s shared want-check tail still judges every result.

use almide_ir::{IrExpr, IrExprKind};
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::types_table::NamedDef;
use crate::*;

/// The String-channel conversions at a `!` (split for the file budget).
#[path = "err_channel.rs"]
pub(crate) mod err_channel;

/// The `!` / `?` extraction (split for the file budget).
#[path = "data_unwrap.rs"]
mod data_unwrap;

impl Emitter<'_> {
    /// Sum-shaped values: constructors and unwraps — split from
    /// `lower_data` for complexity budget. The `want` check happens in
    /// `lower`'s shared tail.
    pub(crate) fn lower_sum(
        &mut self,
        e: &IrExpr,
        want: Option<SliceTy>,
    ) -> Result<SliceTy, EmitError> {
        let got = match &e.kind {
            // Sum constructors — `none`/`ok`/`err` REQUIRE the hint.
            IrExprKind::OptionNone => match want.map_or_else(|| self.infer(e), Ok)? {
                SliceTy::Option(s) => {
                    self.f.instructions().i32_const(almide_layout::NULL_ADDR as i32);
                    SliceTy::Option(s)
                }
                other => return unsup(&format!("ty-mismatch:none-vs-{other:?}")),
            },
            IrExprKind::OptionSome { expr } => self.lower_option_some(e, expr, want)?,
            IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr } => {
                let is_ok = matches!(&e.kind, IrExprKind::ResultOk { .. });
                let (hty, o, er) = match want.map_or_else(|| self.infer(e), Ok)? {
                    SliceTy::Result(o, er) => (SliceTy::Result(o, er), o, er),
                    other => return self.lower_err_raise(e, is_ok, other),
                };
                let side = self.types.el(if is_ok { o } else { er });
                if crate::fs_meta::expr_propagates(expr) {
                    let tag = i32::from(!is_ok);
                    self.lower_payload_then_box(expr, side, 16, Some(tag), almide_layout::SUM_FIELD)?;
                    return Ok(hty);
                }
                // Hold-local, not shared tmp — same seed-79 lesson as
                // OptionSome above.
                let hold = self.hold_i32()?;
                self.f
                    .instructions()
                    .i32_const(16)
                    .call(F_ALLOC)
                    .local_tee(hold)
                    .i32_const(i32::from(!is_ok))
                    .i32_store(slot_memarg(almide_layout::SUM_TAG));
                self.f.instructions().local_get(hold);
                self.lower(expr, Some(side))?;
                self.rc_share_guard(expr, side);
                self.witness_store(expr, side);
                self.store_ty_slot(side, almide_layout::SUM_FIELD);
                self.f.instructions().local_get(hold);
                self.release_i32();
                hty
            }
            IrExprKind::Try { expr } | IrExprKind::Unwrap { expr } => {
                self.lower_try_unwrap(e, expr)?
            }
            // `??` — fallback on none/Err. The fallback branch may clobber
            // the scratch, but the branch that reads the scratch is the
            // exclusive other path.
            IrExprKind::UnwrapOr { expr, fallback } => match self.lower(expr, None)? {
                SliceTy::Option(h) => {
                    let et = self.types.el(h);
                    self.f
                        .instructions()
                        .local_tee(self.scr_i32_local)
                        .i32_eqz()
                        .if_(BlockType::Result(et.val_type()));
                    self.witness_branch_open();
                    self.witness_branch_arm();
                    self.lower(fallback, Some(et))?;
                    self.witness_unwrap_or_arm(e, Some(fallback), et);
                    self.f.instructions().else_().local_get(self.scr_i32_local);
                    self.witness_branch_arm();
                    self.load_ty_slot(et, almide_layout::OPTION_FIELD);
                    self.own_unwrap_or_join(e, fallback, et);
                    self.witness_unwrap_or_arm(e, None, et);
                    self.witness_branch_close();
                    self.f.instructions().end();
                    et
                }
                SliceTy::Result(o, _) => {
                    let et = self.types.el(o);
                    self.f
                        .instructions()
                        .local_tee(self.scr_i32_local)
                        .i32_load(slot_memarg(almide_layout::SUM_TAG))
                        .i32_const(0)
                        .i32_ne()
                        .if_(BlockType::Result(et.val_type()));
                    self.witness_branch_open();
                    self.witness_branch_arm();
                    self.lower(fallback, Some(et))?;
                    self.witness_unwrap_or_arm(e, Some(fallback), et);
                    self.f.instructions().else_().local_get(self.scr_i32_local);
                    self.witness_branch_arm();
                    self.load_ty_slot(et, almide_layout::SUM_FIELD);
                    self.own_unwrap_or_join(e, fallback, et);
                    self.witness_unwrap_or_arm(e, None, et);
                    self.witness_branch_close();
                    self.f.instructions().end();
                    et
                }
                other => return unsup(&format!("unwrap-or-of:{other:?}")),
            },
            // `?` — Result→Option (identity on Option, the interp's
            // eval order): Ok(x) → a fresh some cell, Err → none (null).
            IrExprKind::ToOption { expr } => match self.lower(expr, None)? {
                SliceTy::Result(o, er) => {
                    let et = self.types.el(o);
                    // #2509, the `?` sibling: the carrier this reads is
                    // discarded here on BOTH paths, so an OWNED one has to
                    // be released — the some-cell keeps the ok payload's
                    // credit ($dec_flat: the spine only, the payload moved
                    // into the cell), and the none path releases the
                    // carrier WITH its err payload (the typed drop, whose
                    // tag-1 arm is the one that runs).
                    let owned_carrier = self.rc_owned_result(expr);
                    self.witness_to_option(owned_carrier, et);
                    let err_dec = owned_carrier.then(|| self.dec_fn_of(SliceTy::Result(o, er)));
                    let hr = self.hold_i32()?;
                    let hc = self.hold_i32()?;
                    self.f
                        .instructions()
                        .local_set(hr)
                        .local_get(hr)
                        .i32_load(slot_memarg(almide_layout::SUM_TAG))
                        .i32_const(0)
                        .i32_ne()
                        .if_(BlockType::Result(ValType::I32));
                    if let Some(dec) = err_dec {
                        self.f.instructions().local_get(hr).call(dec);
                    }
                    self.f
                        .instructions()
                        .i32_const(0)
                        .else_()
                        .i32_const(et.slot_size() as i32)
                        .call(F_ALLOC)
                        .local_tee(hc)
                        .local_get(hr);
                    self.load_ty_slot(et, almide_layout::SUM_FIELD);
                    if !owned_carrier {
                        // #2969: over a BORROWED carrier the cell takes its
                        // own credit on the payload, so it is owned too.
                        self.share_handle_top(et);
                    }
                    self.store_ty_slot(et, almide_layout::OPTION_FIELD);
                    if owned_carrier {
                        self.f.instructions().local_get(hr).call(F_DEC_FLAT);
                    }
                    self.f.instructions().local_get(hc).end();
                    self.release_i32();
                    self.release_i32();
                    // #2516: over an OWNED carrier the some-cell is fresh at
                    // rc 1 AND holds the payload's one credit (moved out of
                    // the carrier just released above), so it is an owned
                    // value — mark the node, or the bind takes the
                    // borrowed-source `+1` and the cell stays at rc 1
                    // forever. Over a BORROWED carrier the cell shared the
                    // payload above (#2969): it is fresh and owns its slot
                    // on that path as well. Left unmarked, the bind's `+1`
                    // landed on the fresh cell and it was never freed.
                    self.owned_call_marks.mark(e);
                    SliceTy::Option(o)
                }
                got @ SliceTy::Option(_) => got,
                other => return unsup(&format!("to-option-of:{other:?}")),
            },
            other => return unsup(&format!("expr:{}", expr_kind_name(other))),
        };
        Ok(got)
    }

    /// The RAISE leaf (ALS-ST5/ST6, #1340's wasm half): a bare `err(e)`
    /// where the RAW type is expected inside an effect fn early-returns
    /// the Err — guard-let's desugared else-arm. The code after `return`
    /// is unreachable, so claiming the raw type keeps the caller's
    /// stack bookkeeping true.
    fn lower_err_raise(
        &mut self,
        e: &IrExpr,
        is_ok: bool,
        raw: SliceTy,
    ) -> Result<SliceTy, EmitError> {
        // `ok(x)` where the RAW type is expected: the effect ABI's
        // transparent spot — the payload IS the value (the Result layer
        // exists only at the fn boundary, where wrap_ok adds it).
        if is_ok
            && let IrExprKind::ResultOk { expr } = &e.kind
            && matches!(self.fn_ret, Some(SliceTy::Result(..)))
        {
            self.lower(expr, Some(raw))?;
            // #2758: the consumer reads `ok(x)` as fresh; under this ABI the
            // value is x's own, so only an owned x keeps that true.
            if self.rc_droppable(raw) && !self.rc_owned_result(expr) {
                self.witness_decline("effect:carrier:ok-borrowed");
            }
            return Ok(raw);
        }
        if !is_ok
            && !self.in_main
            && self.region_repair.is_none()
            && let Some(ret @ SliceTy::Result(..)) = self.fn_ret
        {
            self.lower(e, Some(ret))?;
            // An exit like any other: the frame's owners are released
            // before the jump (the err block already shares its payload).
            let plan = self.exit_plan(crate::exit_plan::Continuation::ReturnError);
            self.witness_err_raise_arm();
            self.emit_exit(&plan);
            self.witness_err_raise_leave();
            self.f.instructions().return_();
            return Ok(raw);
        }
        unsup(&format!("ty-mismatch:result-vs-{raw:?}"))
    }

    /// Record-shaped values: literals, spreads, member reads — split from
    /// `lower_data` for complexity budget.
    pub(crate) fn lower_record(
        &mut self,
        e: &IrExpr,
        want: Option<SliceTy>,
    ) -> Result<SliceTy, EmitError> {
        let got = match &e.kind {
            // Tuple literal: positional record.
            IrExprKind::Tuple { elements } => {
                let ty = want.map_or_else(|| self.infer(e), Ok)?;
                let SliceTy::Tuple(ti) = ty else {
                    return unsup(&format!("ty-mismatch:tuple-vs-{ty:?}"));
                };
                let def = self.types.tuple_def(ti);
                if def.fields.len() != elements.len() {
                    return unsup("tuple-arity");
                }
                let hold = self.hold_i32()?;
                self.f.instructions().i32_const(def.size as i32).call(F_ALLOC).local_set(hold);
                for (el, (fty, off)) in elements.iter().zip(def.fields) {
                    self.f.instructions().local_get(hold);
                    self.lower(el, Some(fty))?;
                    self.rc_share_guard(el, fty);
                    self.witness_store(el, fty);
                    self.store_ty_slot(fty, off);
                }
                self.f.instructions().local_get(hold);
                self.release_i32();
                ty
            }
            // t.0 / t.1 — positional field read.
            IrExprKind::TupleIndex { object, index } => {
                let ty = self.lower(object, None)?;
                let SliceTy::Tuple(ti) = ty else {
                    return unsup(&format!("tuple-index-of:{ty:?}"));
                };
                let def = self.types.tuple_def(ti);
                let Some(&(fty, off)) = def.fields.get(*index) else {
                    return unsup("tuple-index-oob");
                };
                self.load_ty_slot(fty, off);
                fty
            }
            // Record literal: alloc + store each field at its packed offset.
            // Anonymous record literal: the shape interns as a synthetic
            // Named record — construction below is shared.
            IrExprKind::Record { name: None, fields } => {
                let ty = want.map_or_else(|| self.infer(e), Ok)?;
                let SliceTy::Named(ti) = ty else {
                    return unsup(&format!("ty-mismatch:anon-record-vs-{ty:?}"));
                };
                let NamedDef::Record(def) = &self.types.def(ti) else {
                    return unsup("anon-record-non-record-ty");
                };
                if def.fields.len() != fields.len() {
                    return unsup("anon-record-defaults");
                }
                let mut slots = Vec::new();
                for (fname, _) in fields {
                    match def.fields.iter().find(|fi| fi.name == fname.as_str()) {
                        Some(fi) => slots.push((fi.ty, fi.offset)),
                        None => return unsup("anon-record-unknown-field"),
                    }
                }
                let size = def.size;
                let hold = self.hold_i32()?;
                self.f.instructions().i32_const(size as i32).call(F_ALLOC).local_set(hold);
                for ((_, fexpr), (fty, off)) in fields.iter().zip(slots) {
                    self.f.instructions().local_get(hold);
                    self.lower(fexpr, Some(fty))?;
                    self.rc_share_guard(fexpr, fty);
                    self.witness_store(fexpr, fty);
                    self.store_ty_slot(fty, off);
                }
                self.f.instructions().local_get(hold);
                self.release_i32();
                ty
            }
            IrExprKind::Record { name, fields } if name.is_some() => {
                self.lower_named_record(e, name, fields, want)?
            }
            // {...base, f: v}: copy then overwrite — functional update.
            IrExprKind::SpreadRecord { base, fields } => {
                self.lower_spread_record(e, base, fields, want)?
            }
            // r.field: offset load from the record block.
            IrExprKind::Member { object, field } => {
                let ty = self.lower(object, None)?;
                let SliceTy::Named(ti) = ty else {
                    return unsup(&format!("member-of:{ty:?}"));
                };
                let NamedDef::Record(def) = &self.types.def(ti) else {
                    return unsup("member-of-variant");
                };
                let Some(fi) = def.fields.iter().find(|fi| fi.name == field.as_str()) else {
                    return unsup("record-unknown-field");
                };
                let (fty, off) = (fi.ty, fi.offset);
                // #3406: a dying spread base's field read may move its slot.
                if !self.try_take_slot(e, fty)? {
                    self.load_ty_slot(fty, off);
                }
                fty
            }
            other => return unsup(&format!("expr:{}", expr_kind_name(other))),
        };
        Ok(got)
    }
}


impl Emitter<'_> {
    /// `{ ...base, f: v }` — spread-record build (split from
    /// lower_record for the complexity budget).
    pub(crate) fn lower_spread_record(
        &mut self,
        _e: &IrExpr,
        base: &IrExpr,
        fields: &[(almide_base::intern::Sym, IrExpr)],
        _want: Option<SliceTy>,
    ) -> Result<SliceTy, EmitError> {
        // #3406: a base that dies here is rebuilt in place (dying_move.rs).
        if let Some(ty) = self.try_spread_dying(base, fields)? {
            return Ok(ty);
        }
        Ok({

                let ty = self.lower(base, None)?;
                let SliceTy::Named(ti) = ty else {
                    return unsup(&format!("spread-of:{ty:?}"));
                };
                let NamedDef::Record(def) = &self.types.def(ti) else {
                    return unsup("spread-of-variant");
                };
                let mut slots = Vec::new();
                for (fname, _) in fields {
                    match def.fields.iter().find(|fi| fi.name == fname.as_str()) {
                        Some(fi) => slots.push((fi.ty, fi.offset)),
                        None => return unsup("record-unknown-field"),
                    }
                }
                let hold = self.hold_i32()?;
                let copy = self.copy_fn_of(ty);
                // #3373: a PRODUCED base (a call result, any owned value)
                // is a temporary the copy only reads — its credit, and
                // through its slots the credits the copy took its own of,
                // are released right after the copy. A bound base is
                // borrowed: its owner releases it.
                if self.rc_owned_result(base) {
                    let src = self.hold_i32()?;
                    let dec = self.dec_fn_of(ty);
                    self.f.instructions().local_tee(src).call(copy).local_set(hold);
                    self.f.instructions().local_get(src).call(dec);
                    self.release_i32();
                    self.witness_owned_released(base, ty);
                } else {
                    if !crate::witness_unwrap::slot_read_of_var(base) {
                        self.witness_decline("SpreadRecord-base:borrowed");
                    }
                    self.f.instructions().call(copy).local_set(hold);
                }
                for ((_, fexpr), (fty, off)) in fields.iter().zip(slots) {
                    // The overwritten field's credit goes with it.
                    if let Some(dec) = self.elem_is_handle(fty).then(|| self.dec_fn_of(fty)) {
                        self.f.instructions().local_get(hold).i32_load(slot_memarg(off)).call(dec);
                    }
                    self.f.instructions().local_get(hold);
                    self.lower(fexpr, Some(fty))?;
                    self.rc_share_guard(fexpr, fty);
                    self.witness_store(fexpr, fty);
                    self.store_ty_slot(fty, off);
                }
                self.f.instructions().local_get(hold);
                self.release_i32();
                ty
        })
    }
}

impl Emitter<'_> {
    /// NAMED record literal (split from lower_record for the
    /// complexity budget).
    fn lower_named_record(
        &mut self,
        e: &IrExpr,
        name: &Option<almide_base::intern::Sym>,
        fields: &[(almide_base::intern::Sym, IrExpr)],
        want: Option<SliceTy>,
    ) -> Result<SliceTy, EmitError> {
        Ok({

                let ty = want.map_or_else(|| self.infer(e), Ok)?;
                let SliceTy::Named(ti) = ty else {
                    return unsup(&format!("ty-mismatch:record-vs-{ty:?}"));
                };
                // A record LITERAL with a variant type is a record-shaped
                // CASE construction (`Scroll { dy: 3 }`).
                if let NamedDef::Variant(v) = &self.types.def(ti) {
                    let Some(cname) = name else {
                        return unsup("record-case-unnamed");
                    };
                    let Some(c) = v.cases.iter().find(|c| c.name == cname.as_str()) else {
                        return unsup("record-case-unknown");
                    };
                    let mut slots = Vec::new();
                    for (fname, _) in fields {
                        match c.fields.iter().find(|fi| fi.name == fname.as_str()) {
                            Some(fi) => slots.push((fi.ty, fi.offset)),
                            None => return unsup("record-case-unknown-field"),
                        }
                    }
                    // Omitted case fields lower their DECL DEFAULTS —
                    // the record path's exact discipline, variant-shaped.
                    let mut defaults = Vec::new();
                    for fi in c.fields.iter() {
                        if fields.iter().any(|(fname, _)| fi.name == fname.as_str()) {
                            continue;
                        }
                        let Some(d) = &fi.default else {
                            return unsup("record-case-missing-field");
                        };
                        defaults.push((fi.ty, fi.offset, d.clone()));
                    }
                    let (size, tag) = (c.size, c.tag);
                    let hold = self.hold_i32()?;
                    self.f
                        .instructions()
                        .i32_const(size as i32)
                        .call(F_ALLOC)
                        .local_tee(hold)
                        .i32_const(tag as i32)
                        .i32_store(slot_memarg(almide_layout::SUM_TAG));
                    // Every handle store into a block takes the RC-3 share
                    // guard, exactly as the record branch below does. Without
                    // it a `let`-bound list moved into a case payload was
                    // stored WITHOUT the co-owning +1 and then freed by the
                    // frame epilogue that still owned the binding, so the
                    // case held a dangling block — `Supported { matched:
                    // checked }` printed freed memory where native printed
                    // the elements (#2133).
                    for (fty, off, d) in defaults {
                        self.f.instructions().local_get(hold);
                        self.witness_record_default(&d);
                        self.lower(&d, Some(fty))?;
                        self.rc_share_guard(&d, fty);
                        self.witness_store(&d, fty);
                        self.store_ty_slot(fty, off);
                    }
                    for ((_, fexpr), (fty, off)) in fields.iter().zip(slots) {
                        self.f.instructions().local_get(hold);
                        self.lower(fexpr, Some(fty))?;
                        self.rc_share_guard(fexpr, fty);
                        self.witness_store(fexpr, fty);
                        self.store_ty_slot(fty, off);
                    }
                    self.f.instructions().local_get(hold);
                    self.release_i32();
                    return Ok(ty);
                }
                let NamedDef::Record(def) = &self.types.def(ti) else {
                    return unsup("record-of-variant-ty");
                };
                let size = def.size;
                // (name → (offset, ty)) resolved up front to end the borrow.
                let mut slots = Vec::new();
                for (fname, _) in fields {
                    match def.fields.iter().find(|fi| fi.name == fname.as_str()) {
                        Some(fi) => slots.push((fi.ty, fi.offset)),
                        None => return unsup("record-unknown-field"),
                    }
                }
                // Omitted fields lower their DECL DEFAULTS (after the
                // literal's own fields, preserving the literal's effect
                // order); omitted with no default is a checker miss.
                let mut defaults = Vec::new();
                for fi in def.fields.iter() {
                    if fields.iter().any(|(fname, _)| fi.name == fname.as_str()) {
                        continue;
                    }
                    let Some(d) = &fi.default else {
                        return unsup("record-missing-field");
                    };
                    defaults.push((fi.ty, fi.offset, d.clone()));
                }
                let hold = self.hold_i32()?;
                self.f.instructions().i32_const(size as i32).call(F_ALLOC).local_set(hold);
                for ((_, fexpr), (fty, off)) in fields.iter().zip(slots) {
                    self.f.instructions().local_get(hold);
                    self.lower(fexpr, Some(fty))?;
                    self.rc_share_guard(fexpr, fty);
                    self.witness_store(fexpr, fty);
                    self.store_ty_slot(fty, off);
                }
                for (fty, off, d) in defaults {
                    self.f.instructions().local_get(hold);
                    self.witness_record_default(&d);
                    self.lower(&d, Some(fty))?;
                    self.rc_share_guard(&d, fty);
                    self.witness_store(&d, fty);
                    self.store_ty_slot(fty, off);
                }
                self.f.instructions().local_get(hold);
                self.release_i32();
                ty
        })
    }
}

impl Emitter<'_> {
    /// `some(v)` — split from `lower_sum` for the complexity budget. The
    /// base lives in a HOLD local (stack-disciplined), never the shared
    /// tmp: the inner expression can contain its own `some(...)`/`ok(...)`
    /// as a SUBEXPRESSION (differential-fuzz seed 79).
    fn lower_option_some(
        &mut self,
        e: &IrExpr,
        expr: &IrExpr,
        want: Option<SliceTy>,
    ) -> Result<SliceTy, EmitError> {
                let (hty, s) = match want.map_or_else(|| self.infer(e), Ok)? {
                    SliceTy::Option(h) => (SliceTy::Option(h), self.types.el(h)),
                    other => return unsup(&format!("ty-mismatch:some-vs-{other:?}")),
                };
                // The base lives in a HOLD local (stack-disciplined),
                // never the shared tmp: the inner expression can contain
                // its own `some(...)`/`ok(...)` as a SUBEXPRESSION even
                // when the types forbid nested sums — the differential
                // fuzzer falsified the old shared-tmp argument on day one
                // (seed 79: the outer `some` returned the inner block).
                if crate::fs_meta::expr_propagates(expr) {
                    self.lower_payload_then_box(expr, s, s.slot_size() as i32, None, almide_layout::OPTION_FIELD)?;
                    return Ok(hty);
                }
                let hold = self.hold_i32()?;
                self.f
                    .instructions()
                    .i32_const(s.slot_size() as i32)
                    .call(F_ALLOC)
                    .local_tee(hold);
                self.lower(expr, Some(s))?;
                self.rc_share_guard(expr, s);
                self.witness_store(expr, s);
                self.store_ty_slot(s, almide_layout::OPTION_FIELD);
                self.f.instructions().local_get(hold);
                self.release_i32();
        Ok(hty)
    }
}

impl Emitter<'_> {
    /// #2970 — a one-slot box (`ok(p)` / `err(p)` / `some(p)`) whose payload
    /// can PROPAGATE (`ok(1 + f(x)!)`, the fallible-callback carrier the
    /// frontend wraps every non-canonical `!` body in). The ordinary build
    /// allocates the box first and lowers the payload into it; a `!` in the
    /// payload then returns the err block from the middle of the build and
    /// the half-built box, held only by the operand stack, is never
    /// released — one 16 B block per err, and per element on the fallible
    /// HOF carriers' err path. Lowering the payload FIRST (parked in a typed
    /// hold) leaves nothing allocated when the `!` exits; the box is built
    /// only once the payload exists. Every other payload keeps the
    /// allocate-first order and its exact bytes.
    fn lower_payload_then_box(
        &mut self,
        expr: &IrExpr,
        side: SliceTy,
        size: i32,
        tag: Option<i32>,
        field: u32,
    ) -> Result<(), EmitError> {
        self.lower(expr, Some(side))?;
        self.rc_share_guard(expr, side);
        let val = self.hold_val(side)?;
        self.f.instructions().local_set(val);
        let hold = self.hold_i32()?;
        self.f.instructions().i32_const(size).call(F_ALLOC).local_set(hold);
        if let Some(tag) = tag {
            self.f
                .instructions()
                .local_get(hold)
                .i32_const(tag)
                .i32_store(slot_memarg(almide_layout::SUM_TAG));
        }
        self.f.instructions().local_get(hold).local_get(val);
        self.store_ty_slot(side, field);
        self.f.instructions().local_get(hold);
        self.release_i32();
        self.release_val(side);
        Ok(())
    }
}

impl Emitter<'_> {
    /// #2970 — `r ?? fallback` over a heap payload joins two values of
    /// different ownership: the payload is a VIEW of the carrier (whose own
    /// holder releases it), the fallback is often FRESH (`?? [0]`, a call).
    /// Classed borrowed as a whole, every consumer took the
    /// `+1` a view needs, and on the none / err path that second credit
    /// landed on the fresh fallback — one block per fallback taken, never
    /// released (`list.map(xs, (x) => f(x)!) ?? [0]`, the fallible-HOF
    /// consumer idiom). When the fallback is owned, the payload arm takes
    /// its `+1` here instead, so both arms hand the join one credit, and the
    /// node is marked owned (`rc_owned_result`) so no consumer adds another
    /// — the same normalization `lower_if_arms` applies to an `if`. A
    /// borrowed or pool-static fallback (a var, a string literal) leaves both
    /// arms views, as before, and so does a payload type arg_temps does not
    /// name in a reader position (`arg_temps::bindable_ty`): an owned join
    /// there would be read and never released.
    fn own_unwrap_or_join(&mut self, e: &IrExpr, fallback: &IrExpr, et: SliceTy) {
        if !self.unwrap_or_owns_join(e, fallback, et) {
            return;
        }
        self.rc_inc_top();
        self.owned_call_marks.mark(e);
    }

    /// Does `own_unwrap_or_join` normalize this `??` to an owned join? (The
    /// witness reads the same predicate, witness_hooks.rs.)
    pub(crate) fn unwrap_or_owns_join(&self, e: &IrExpr, fallback: &IrExpr, et: SliceTy) -> bool {
        self.rc_droppable(et)
            && crate::arg_temps::bindable_ty(&e.ty)
            && crate::arg_temps::unwrap_or_joins_owned(e)
            && self.rc_owned_result(fallback)
    }
}
