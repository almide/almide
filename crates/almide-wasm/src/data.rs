//! Sum- and record-shaped VALUE lowering (constructors, unwrap markers,
//! record literals, spreads) — split from emitter.rs for the complexity
//! budget; `lower`'s shared want-check tail still judges every result.

use almide_ir::{IrExpr, IrExprKind};
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::types_table::NamedDef;
use crate::*;

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
            IrExprKind::OptionSome { expr } => {
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
                let hold = self.hold_i32()?;
                self.f
                    .instructions()
                    .i32_const(s.slot_size() as i32)
                    .call(F_ALLOC)
                    .local_tee(hold);
                self.lower(expr, Some(s))?;
                self.rc_share_guard(expr, s);
                self.store_ty_slot(s, almide_layout::OPTION_FIELD);
                self.f.instructions().local_get(hold);
                self.release_i32();
                hty
            }
            IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr } => {
                let is_ok = matches!(&e.kind, IrExprKind::ResultOk { .. });
                let (hty, o, er) = match want.map_or_else(|| self.infer(e), Ok)? {
                    SliceTy::Result(o, er) => (SliceTy::Result(o, er), o, er),
                    other => return self.lower_err_raise(e, is_ok, other),
                };
                let side = self.types.el(if is_ok { o } else { er });
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
                    self.lower(fallback, Some(et))?;
                    self.f.instructions().else_().local_get(self.scr_i32_local);
                    self.load_ty_slot(et, almide_layout::OPTION_FIELD);
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
                    self.lower(fallback, Some(et))?;
                    self.f.instructions().else_().local_get(self.scr_i32_local);
                    self.load_ty_slot(et, almide_layout::SUM_FIELD);
                    self.f.instructions().end();
                    et
                }
                other => return unsup(&format!("unwrap-or-of:{other:?}")),
            },
            // `?` — Result→Option (identity on Option, the interp's
            // eval order): Ok(x) → a fresh some cell, Err → none (null).
            IrExprKind::ToOption { expr } => match self.lower(expr, None)? {
                SliceTy::Result(o, _) => {
                    let et = self.types.el(o);
                    let hr = self.hold_i32()?;
                    let hc = self.hold_i32()?;
                    self.f
                        .instructions()
                        .local_set(hr)
                        .local_get(hr)
                        .i32_load(slot_memarg(almide_layout::SUM_TAG))
                        .i32_const(0)
                        .i32_ne()
                        .if_(BlockType::Result(ValType::I32))
                        .i32_const(0)
                        .else_()
                        .i32_const(et.slot_size() as i32)
                        .call(F_ALLOC)
                        .local_tee(hc)
                        .local_get(hr);
                    self.load_ty_slot(et, almide_layout::SUM_FIELD);
                    self.store_ty_slot(et, almide_layout::OPTION_FIELD);
                    self.f.instructions().local_get(hc).end();
                    self.release_i32();
                    self.release_i32();
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
            return Ok(raw);
        }
        if !is_ok
            && !self.in_main
            && self.region_repair.is_none()
            && let Some(ret @ SliceTy::Result(..)) = self.fn_ret
        {
            self.lower(e, Some(ret))?;
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
                self.load_ty_slot(fty, off);
                fty
            }
            other => return unsup(&format!("expr:{}", expr_kind_name(other))),
        };
        Ok(got)
    }
}

impl Emitter<'_> {
            // `!` — three enclosing shapes (the interp's eval_try_unwrap):
            //   effect fn  -> PROPAGATE (return the err block as-is; err
            //                 blocks of any Result(_, E) share one layout),
            //   main       -> ABORT with the native frame
            //                 ("Error: {msg}" + exit 1),
            //   pure fn    -> same abort (the checker forbids propagating
            //                 `!` outside effect fns; a pure-Option/Result
            //                 fn's `!` is #1410-propagating — refused).
            // `?` (Try) and `!` (Unwrap) are ONE marker in the oracle:
            // eval.rs dispatches Try | Unwrap to the same eval_try_unwrap.
    pub(crate) fn lower_try_unwrap(
        &mut self,
        e: &IrExpr,
        expr: &IrExpr,
    ) -> Result<SliceTy, EmitError> {
        Ok({

                // C-216: a marker node TYPED Option is the effect-RESULT-
                // layer strip on a declared-Option effect call — identity.
                let node_ty = slice_ty_of(&e.ty, self.types);
                // Propagation returns the operand's err block INTO the
                // enclosing frame — sound only when the err slot types
                // agree (they share one layout then).
                let fn_err = match self.fn_ret {
                    Some(SliceTy::Result(_, fe)) => Some(self.types.el(fe)),
                    _ => None,
                };
                let in_effect = fn_err.is_some();
                // #1067: `!` in a pure Option-returning fn PROPAGATES a
                // none as none (a Result operand there stays refused —
                // no oracle row pins its shape).
                let in_option_fn =
                    !in_effect && matches!(self.fn_ret, Some(SliceTy::Option(_)));
                if in_option_fn {
                    match self.lower(expr, None)? {
                        SliceTy::Option(h) => {
                            let et = self.types.el(h);
                            let mut i = self.f.instructions();
                            i.local_tee(self.scr_i32_local).i32_eqz().if_(BlockType::Empty);
                            i.i32_const(almide_layout::NULL_ADDR as i32).return_();
                            i.end();
                            i.local_get(self.scr_i32_local);
                            let _ = i;
                            self.load_ty_slot(et, almide_layout::OPTION_FIELD);
                            return Ok(et);
                        }
                        _ => return unsup("unwrap-propagating"),
                    }
                }
                match self.lower(expr, None)? {
                    got @ SliceTy::Option(_)
                        if node_ty == Some(got) =>
                    {
                        // Identity: pass the Option through untouched.
                        got
                    }
                    SliceTy::Option(h) => {
                        let et = self.types.el(h);
                        self.f
                            .instructions()
                            .local_tee(self.scr_i32_local)
                            .i32_eqz()
                            .if_(BlockType::Empty);
                        if in_effect {
                            if fn_err != Some(STR) {
                                return unsup("unwrap-none-err-ty");
                            }
                            // err("none") — #556: `!` on none propagates
                            // an Err whose message is "none".
                            let none_msg = self.pool.intern("none");
                            self.f
                                .instructions()
                                .i32_const(16)
                                .call(F_ALLOC)
                                .local_tee(self.tmp_i32_local)
                                .i32_const(1)
                                .i32_store(slot_memarg(almide_layout::SUM_TAG))
                                .local_get(self.tmp_i32_local)
                                .i32_const(none_msg as i32)
                                .i32_store(slot_memarg(almide_layout::SUM_FIELD))
                                .local_get(self.tmp_i32_local)
                                .return_();
                        } else if self.in_main {
                            let none_msg = self.pool.intern("none");
                            self.f.instructions().i32_const(none_msg as i32);
                            self.emit_error_frame_abort();
                        } else {
                            self.f.instructions().unreachable();
                        }
                        self.f.instructions().end().local_get(self.scr_i32_local);
                        self.load_ty_slot(et, almide_layout::OPTION_FIELD);
                        et
                    }
                    SliceTy::Result(o, er) => {
                        let et = self.types.el(o);
                        let ert = self.types.el(er);
                        self.f
                            .instructions()
                            .local_tee(self.scr_i32_local)
                            .i32_load(slot_memarg(almide_layout::SUM_TAG))
                            .i32_const(0)
                            .i32_ne()
                            .if_(BlockType::Empty);
                        if in_effect {
                            if fn_err != Some(ert) {
                                return unsup("unwrap-err-ty-mismatch");
                            }
                            self.f.instructions().local_get(self.scr_i32_local).return_();
                        } else if self.in_main && ert == STR {
                            self.f.instructions().local_get(self.scr_i32_local);
                            self.load_ty_slot(ert, almide_layout::SUM_FIELD);
                            self.emit_error_frame_abort();
                        } else {
                            self.f.instructions().unreachable();
                        }
                        self.f.instructions().end().local_get(self.scr_i32_local);
                        self.load_ty_slot(et, almide_layout::SUM_FIELD);
                        et
                    }
                    other => return unsup(&format!("unwrap-of:{other:?}")),
                }
        })
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
                self.f.instructions().call(F_BLOCK_COPY).local_set(hold);
                for ((_, fexpr), (fty, off)) in fields.iter().zip(slots) {
                    self.f.instructions().local_get(hold);
                    self.lower(fexpr, Some(fty))?;
                    self.rc_share_guard(fexpr, fty);
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
                    if c.fields.len() != fields.len() {
                        return unsup("record-case-defaults");
                    }
                    let mut slots = Vec::new();
                    for (fname, _) in fields {
                        match c.fields.iter().find(|fi| fi.name == fname.as_str()) {
                            Some(fi) => slots.push((fi.ty, fi.offset)),
                            None => return unsup("record-case-unknown-field"),
                        }
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
                    for ((_, fexpr), (fty, off)) in fields.iter().zip(slots) {
                        self.f.instructions().local_get(hold);
                        self.lower(fexpr, Some(fty))?;
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
                    self.store_ty_slot(fty, off);
                }
                for (fty, off, d) in defaults {
                    self.f.instructions().local_get(hold);
                    self.lower(&d, Some(fty))?;
                    self.store_ty_slot(fty, off);
                }
                self.f.instructions().local_get(hold);
                self.release_i32();
                ty
        })
    }
}
