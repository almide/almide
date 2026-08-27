//! Match lowering: arm chains, pattern tests, pattern binds.

use almide_ir::{IrExpr, IrMatchArm, IrPattern};
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::types_table::NamedDef;
use crate::*;

impl Emitter<'_> {
    // ── match lowering ──────────────────────────────────────────────────

    /// `result`: Some(ty) = value position, None = statement position.
    pub(crate) fn lower_match(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result: Option<SliceTy>,
    ) -> Result<(), EmitError> {
        self.lower_match_at(subject, arms, result, false)
    }

    pub(crate) fn lower_match_at(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result: Option<SliceTy>,
        tail: bool,
    ) -> Result<(), EmitError> {
        if arms.is_empty() {
            return unsup("match:no-arms");
        }
        let subj_ty = self.lower(subject, None)?;
        let scr = match subj_ty.val_type() {
            ValType::I64 => self.scr_i64_local,
            ValType::F64 => self.scr_f64_local,
            _ => self.scr_i32_local,
        };
        self.f.instructions().local_set(scr);
        self.lower_arm_chain(arms, subj_ty, scr, result, tail)
    }

    pub(crate) fn lower_arm_chain(
        &mut self,
        arms: &[IrMatchArm],
        subj_ty: SliceTy,
        scr: u32,
        result: Option<SliceTy>,
        tail: bool,
    ) -> Result<(), EmitError> {
        let arm = &arms[0];
        let irrefutable = pattern_irrefutable(&arm.pattern);
        if irrefutable && arm.guard.is_none() {
            // Selected unconditionally; later arms are dead (checker-
            // verified reachability aside, the oracle picks the first).
            self.emit_pattern_binds(&arm.pattern, subj_ty, scr)?;
            return self.lower_arm_body(&arm.body, result, tail);
        }
        // The arm's verdict: pattern test AND guard. Binds run BEFORE the
        // guard (it references them); locals are function-scoped, so on a
        // guarded arm the body needs no re-bind, and a failed guard's
        // binds are harmlessly overwritten by whichever arm matches next.
        match &arm.guard {
            None => self.emit_pattern_test(&arm.pattern, subj_ty, scr)?,
            Some(g) if irrefutable => {
                self.emit_pattern_binds(&arm.pattern, subj_ty, scr)?;
                self.lower(g, Some(BOOL))?;
            }
            Some(g) => {
                self.emit_pattern_test(&arm.pattern, subj_ty, scr)?;
                self.f.instructions().if_(BlockType::Result(ValType::I32));
                self.emit_pattern_binds(&arm.pattern, subj_ty, scr)?;
                self.lower(g, Some(BOOL))?;
                self.f.instructions().else_().i32_const(0).end();
            }
        }
        let bt = match result {
            Some(t) => BlockType::Result(t.val_type()),
            None => BlockType::Empty,
        };
        self.f.instructions().if_(bt);
        if arm.guard.is_none() {
            self.emit_pattern_binds(&arm.pattern, subj_ty, scr)?;
        }
        self.lower_arm_body(&arm.body, result, tail)?;
        self.f.instructions().else_();
        if arms.len() > 1 {
            self.lower_arm_chain(&arms[1..], subj_ty, scr, result, tail)?;
        } else {
            // The checker promises exhaustiveness — if it's ever wrong
            // (or every remaining arm was guarded away), trap LOUDLY
            // instead of silently misbehaving.
            self.f.instructions().unreachable();
        }
        self.f.instructions().end();
        Ok(())
    }

    pub(crate) fn lower_arm_body(
        &mut self,
        body: &IrExpr,
        result: Option<SliceTy>,
        tail: bool,
    ) -> Result<(), EmitError> {
        match result {
            Some(ty) => {
                self.in_tail = tail;
                self.lower(body, Some(ty)).map(|_| ())
            }
            None => self.lower_stmt_expr(body),
        }
    }

    /// Push an i32 bool: does the subject (in `scr`) match `p`?
    pub(crate) fn emit_pattern_test(
        &mut self,
        p: &IrPattern,
        subj_ty: SliceTy,
        scr: u32,
    ) -> Result<(), EmitError> {
        match (p, subj_ty) {
            (IrPattern::Literal { expr }, SliceTy::Scalar(s)) => {
                self.f.instructions().local_get(scr);
                self.lower(expr, Some(SliceTy::Scalar(s)))?;
                self.emit_scalar_eq(s);
                Ok(())
            }
            (IrPattern::None, SliceTy::Option(_)) => {
                self.f.instructions().local_get(scr).i32_eqz();
                Ok(())
            }
            (IrPattern::Some { inner }, SliceTy::Option(h)) => {
                if pattern_irrefutable(inner) {
                    self.f.instructions().local_get(scr).i32_const(0).i32_ne();
                    return Ok(());
                }
                // some(<refutable>): non-null AND the payload matches —
                // the inner pattern recurses through a typed hold, so
                // some(ok(3)), some(Nd(x)), … all compose.
                let et = self.types.el(h);
                self.f.instructions().local_get(scr).if_(BlockType::Result(ValType::I32));
                self.f.instructions().local_get(scr);
                self.load_ty_slot(et, almide_layout::OPTION_FIELD);
                self.test_nested(inner, et)?;
                self.f.instructions().else_().i32_const(0).end();
                Ok(())
            }
            (IrPattern::Constructor { name, args }, SliceTy::Named(ti)) => {
                self.test_ctor_pattern(name, args, ti, scr)
            }
            // Tuple pattern with refutable positions: AND together each
            // refutable field's test (nested via typed holds).
            (IrPattern::Tuple { elements }, SliceTy::Tuple(id)) => {
                let fields = self.types.tuple_def(id).fields;
                if elements.len() != fields.len() {
                    return unsup("pattern:tuple-arity");
                }
                self.f.instructions().i32_const(1);
                for (ep, (fty, off)) in elements.iter().zip(fields) {
                    if pattern_irrefutable(ep) {
                        continue;
                    }
                    self.f.instructions().if_(BlockType::Result(ValType::I32));
                    self.f.instructions().local_get(scr);
                    self.load_ty_slot(fty, off);
                    self.test_nested(ep, fty)?;
                    self.f.instructions().else_().i32_const(0).end();
                }
                Ok(())
            }
            (IrPattern::RecordPattern { name, .. }, SliceTy::Named(ti)) => {
                // Record-shaped case: the TEST is the tag; the named field
                // binds happen in emit_pattern_binds. A plain record
                // subject matches structurally (always true).
                let NamedDef::Variant(v) = &self.types.def(ti) else {
                    self.f.instructions().i32_const(1);
                    return Ok(());
                };
                let Some(c) = v.cases.iter().find(|c| c.name == name.as_str()) else {
                    return unsup(&format!("pattern:case-unknown:{name}"));
                };
                self.f
                    .instructions()
                    .local_get(scr)
                    .i32_load(slot_memarg(almide_layout::SUM_TAG))
                    .i32_const(c.tag as i32)
                    .i32_eq();
                Ok(())
            }
            (IrPattern::Ok { inner }, SliceTy::Result(o, _))
            | (IrPattern::Err { inner }, SliceTy::Result(_, o)) => {
                let want_tag = i32::from(matches!(p, IrPattern::Err { .. }));
                self.f
                    .instructions()
                    .local_get(scr)
                    .i32_load(slot_memarg(almide_layout::SUM_TAG))
                    .i32_const(want_tag)
                    .i32_eq();
                if pattern_irrefutable(inner) {
                    return Ok(());
                }
                let et = self.types.el(o);
                // tag matches AND the payload matches (recursive).
                self.f.instructions().if_(BlockType::Result(ValType::I32));
                self.f.instructions().local_get(scr);
                self.load_ty_slot(et, almide_layout::SUM_FIELD);
                self.test_nested(inner, et)?;
                self.f.instructions().else_().i32_const(0).end();
                Ok(())
            }
            (IrPattern::List { elements }, SliceTy::List(h)) => {
                // Fixed-arity list pattern (#1584's last lowering wall):
                // the block's byte LEN equals arity × stride, then each
                // REFUTABLE element tests at its payload slot. `[]` is the
                // pure length test.
                let et = self.types.el(h);
                let stride = et.slot_size();
                self.f.instructions().local_get(scr);
                self.f.instructions().i32_load(len_memarg());
                self.f.instructions().i32_const((elements.len() as u32 * stride) as i32);
                self.f.instructions().i32_eq();
                for (i, ep) in elements.iter().enumerate() {
                    if pattern_irrefutable(ep) {
                        continue;
                    }
                    self.f.instructions().if_(BlockType::Result(ValType::I32));
                    self.f.instructions().local_get(scr);
                    self.load_ty_slot(et, i as u32 * stride);
                    self.test_nested(ep, et)?;
                    self.f.instructions().else_().i32_const(0).end();
                }
                Ok(())
            }
            _ => unsup(&format!("pattern:{}", pattern_name(p))),
        }
    }

    /// Variant-constructor pattern test: tag equality, then AND in each
    /// refutable field literal — extracted for complexity budget.
    pub(crate) fn test_ctor_pattern(
        &mut self,
        name: &str,
        args: &[IrPattern],
        ti: u32,
        scr: u32,
    ) -> Result<(), EmitError> {

                // Resolve the case by NAME within the subject's own
                // definition — exact for concrete types, and the only
                // unambiguous route for generic instances.
                let (tag, fields) = {
                    let NamedDef::Variant(v) = &self.types.def(ti) else {
                        return unsup("pattern:ctor-of-record");
                    };
                    let Some(c) = v.cases.iter().find(|c| c.name == name) else {
                        return unsup(&format!("pattern:ctor-unknown:{name}"));
                    };
                    let fs: Vec<(SliceTy, u32)> =
                        c.fields.iter().map(|f| (f.ty, f.offset)).collect();
                    (c.tag, fs)
                };
                if args.len() != fields.len() {
                    return unsup("pattern:ctor-arity");
                }
                // tag test, then AND in each refutable field pattern
                // (recursive — nested constructors compose).
                self.f
                    .instructions()
                    .local_get(scr)
                    .i32_load(slot_memarg(almide_layout::SUM_TAG))
                    .i32_const(tag as i32)
                    .i32_eq();
                for (ap, (fty, off)) in args.iter().zip(fields) {
                    if pattern_irrefutable(ap) {
                        continue;
                    }
                    self.f.instructions().if_(BlockType::Result(ValType::I32));
                    self.f.instructions().local_get(scr);
                    self.load_ty_slot(fty, off);
                    self.test_nested(ap, fty)?;
                    self.f.instructions().else_().i32_const(0).end();
                }
                Ok(())
                }

    /// Bind pattern variables from the subject (in `scr`).
    pub(crate) fn emit_pattern_binds(
        &mut self,
        p: &IrPattern,
        subj_ty: SliceTy,
        scr: u32,
    ) -> Result<(), EmitError> {
        match p {
            IrPattern::Wildcard | IrPattern::Literal { .. } | IrPattern::None => Ok(()),
            IrPattern::Bind { var, .. } => {
                let Some(&(idx, _)) = self.locals.get(var) else {
                    return unsup("bind:unmapped");
                };
                self.f.instructions().local_get(scr).local_set(idx);
                Ok(())
            }
            IrPattern::Some { inner } => {
                let SliceTy::Option(s) = subj_ty else {
                    return unsup("pattern:some-on-non-option");
                };
                self.bind_inner(inner, s, almide_layout::OPTION_FIELD, scr)
            }
            IrPattern::Ok { inner } => {
                let SliceTy::Result(o, _) = subj_ty else {
                    return unsup("pattern:ok-on-non-result");
                };
                self.bind_inner(inner, o, almide_layout::SUM_FIELD, scr)
            }
            IrPattern::Err { inner } => {
                let SliceTy::Result(_, e) = subj_ty else {
                    return unsup("pattern:err-on-non-result");
                };
                self.bind_inner(inner, e, almide_layout::SUM_FIELD, scr)
            }
            IrPattern::List { elements } => {
                let SliceTy::List(h) = subj_ty else {
                    return unsup("pattern:list-on-nonlist");
                };
                let stride = self.types.el(h).slot_size();
                for (i, ep) in elements.iter().enumerate() {
                    self.bind_inner(ep, h, i as u32 * stride, scr)?;
                }
                Ok(())
            }
            IrPattern::Tuple { elements } => {
                let SliceTy::Tuple(ti) = subj_ty else {
                    return unsup("pattern:tuple-on-nontuple");
                };
                let def = self.types.tuple_def(ti);
                if def.fields.len() != elements.len() {
                    return unsup("pattern:tuple-arity");
                }
                for (ep, (fty, off)) in elements.iter().zip(def.fields) {
                    match ep {
                        IrPattern::Wildcard | IrPattern::Literal { .. } => {}
                        IrPattern::Bind { var, .. } => {
                            let Some(&(idx, _)) = self.locals.get(var) else {
                                return unsup("bind:unmapped");
                            };
                            self.f.instructions().local_get(scr);
                            self.load_ty_slot(fty, off);
                            self.f.instructions().local_set(idx);
                        }
                        // Any other pattern form composes through the
                        // nested-bind machinery (Ok/Err/Some sub-patterns
                        // inside tuple positions — the fan.settle shape).
                        other => {
                            self.f.instructions().local_get(scr);
                            self.load_ty_slot(fty, off);
                            self.bind_nested(other, fty)?;
                        }
                    }
                }
                Ok(())
            }
            IrPattern::RecordPattern { name, fields, .. } => {
                self.emit_record_pattern_binds(name, fields, subj_ty, scr)
            }
            IrPattern::Constructor { name, args } => {
                self.bind_ctor_fields(name, args, subj_ty, scr)
            }
            // exhaustive: every IrPattern form above has a binds arm
        }
    }

    pub(crate) fn bind_inner(
        &mut self,
        inner: &IrPattern,
        h: ETy,
        field: u32,
        scr: u32,
    ) -> Result<(), EmitError> {
        match inner {
            IrPattern::Wildcard | IrPattern::Literal { .. } => Ok(()),
            IrPattern::Bind { var, .. } => {
                let Some(&(idx, _)) = self.locals.get(var) else {
                    return unsup("bind:unmapped");
                };
                let et = self.types.el(h);
                self.f.instructions().local_get(scr);
                self.load_ty_slot(et, field);
                self.f.instructions().local_set(idx);
                Ok(())
            }
            nested => {
                let et = self.types.el(h);
                self.f.instructions().local_get(scr);
                self.load_ty_slot(et, field);
                self.bind_nested(nested, et)
            }
        }
    }

    /// Test a NESTED pattern against the field value on the stack: park
    /// it in a typed hold (the outer subject's scratch must survive for
    /// later fields) and recurse — any pattern form composes.
    pub(crate) fn test_nested(&mut self, ap: &IrPattern, fty: SliceTy) -> Result<(), EmitError> {
        let h = self.hold_val(fty)?;
        self.f.instructions().local_set(h);
        self.emit_pattern_test(ap, fty, h)?;
        self.release_val(fty);
        Ok(())
    }

    /// Bind a NESTED pattern's variables from the field value on the stack.
    pub(crate) fn bind_nested(&mut self, ap: &IrPattern, fty: SliceTy) -> Result<(), EmitError> {
        let h = self.hold_val(fty)?;
        self.f.instructions().local_set(h);
        self.emit_pattern_binds(ap, fty, h)?;
        self.release_val(fty);
        Ok(())
    }

    /// Scalar equality: i64/i32 compare, byte-equality for strings.
    pub(crate) fn emit_scalar_eq(&mut self, s: Scalar) {
        match s {
            Scalar::Int => self.f.instructions().i64_eq(),
            Scalar::Float => self.f.instructions().f64_eq(),
            Scalar::Bool => self.f.instructions().i32_eq(),
            Scalar::Str | Scalar::Bytes => self.f.instructions().call(F_STR_EQ),
        };
    }


    /// Constructor-pattern binds (split from emit_pattern_binds for the complexity budget).
    fn bind_ctor_fields(
        &mut self,
        name: &str,
        args: &[IrPattern],
        subj_ty: SliceTy,
        scr: u32,
    ) -> Result<(), EmitError> {

                let SliceTy::Named(ti) = subj_ty else {
                    return unsup("pattern:ctor-on-non-named");
                };
                let fields: Vec<(SliceTy, u32)> = {
                    let NamedDef::Variant(v) = &self.types.def(ti) else {
                        return unsup("pattern:ctor-of-record");
                    };
                    let Some(c) = v.cases.iter().find(|c| c.name == name) else {
                        return unsup(&format!("pattern:ctor-unknown:{name}"));
                    };
                    c.fields.iter().map(|f| (f.ty, f.offset)).collect()
                };
                for (ap, (fty, off)) in args.iter().zip(fields) {
                    match ap {
                        IrPattern::Wildcard | IrPattern::Literal { .. } => {}
                        IrPattern::Bind { var, .. } => {
                            let Some(&(idx, _)) = self.locals.get(var) else {
                                return unsup("bind:unmapped");
                            };
                            self.f.instructions().local_get(scr);
                            self.load_ty_slot(fty, off);
                            self.f.instructions().local_set(idx);
                        }
                        nested => {
                            self.f.instructions().local_get(scr);
                            self.load_ty_slot(fty, off);
                            self.bind_nested(nested, fty)?;
                        }
                    }
                }
                Ok(())
    }
}

impl Emitter<'_> {
    /// Record/variant-case pattern binds — split from `emit_pattern_binds`
    /// for the complexity budget.
    fn emit_record_pattern_binds(
        &mut self,
        name: &str,
        fields: &[almide_ir::IrFieldPattern],
        subj_ty: SliceTy,
        scr: u32,
    ) -> Result<(), EmitError> {
                let SliceTy::Named(ti) = subj_ty else {
                    return unsup("pattern:record-on-non-named");
                };
                let finfo: Vec<(String, SliceTy, u32)> = match &self.types.def(ti) {
                    NamedDef::Variant(v) => {
                        let Some(c) = v.cases.iter().find(|c| c.name == name) else {
                            return unsup(&format!("pattern:case-unknown:{name}"));
                        };
                        c.fields.iter().map(|f| (f.name.clone(), f.ty, f.offset)).collect()
                    }
                    NamedDef::Record(r) => {
                        r.fields.iter().map(|f| (f.name.clone(), f.ty, f.offset)).collect()
                    }
                    NamedDef::Excluded => return unsup("pattern:record-excluded"),
                };
                for fp in fields {
                    let Some((_, fty, off)) = finfo
                        .iter()
                        .find(|(n, ..)| *n == fp.name)
                        .map(|(_, t, o)| ((), *t, *o))
                    else {
                        return unsup("pattern:record-unknown-field");
                    };
                    match &fp.pattern {
                        None => return unsup("pattern:record-shorthand"),
                        Some(IrPattern::Wildcard) | Some(IrPattern::Literal { .. }) => {}
                        Some(IrPattern::Bind { var, .. }) => {
                            let Some(&(idx, _)) = self.locals.get(var) else {
                                return unsup("bind:unmapped");
                            };
                            self.f.instructions().local_get(scr);
                            self.load_ty_slot(fty, off);
                            self.f.instructions().local_set(idx);
                        }
                        Some(other) => {
                            return unsup(&format!("pattern:record-{}", pattern_name(other)))
                        }
                    }
                }
                Ok(())
    }
}
