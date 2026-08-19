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
        if arms.iter().any(|a| a.guard.is_some()) {
            return unsup("match-guard");
        }
        let subj_ty = self.lower(subject, None)?;
        let scr = match subj_ty.val_type() {
            ValType::I64 => self.scr_i64_local,
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
        if pattern_irrefutable(&arm.pattern) {
            // Selected unconditionally; later arms are dead (checker-
            // verified reachability aside, the oracle picks the first).
            self.emit_pattern_binds(&arm.pattern, subj_ty, scr)?;
            return self.lower_arm_body(&arm.body, result, tail);
        }
        self.emit_pattern_test(&arm.pattern, subj_ty, scr)?;
        let bt = match result {
            Some(t) => BlockType::Result(t.val_type()),
            None => BlockType::Empty,
        };
        self.f.instructions().if_(bt);
        self.emit_pattern_binds(&arm.pattern, subj_ty, scr)?;
        self.lower_arm_body(&arm.body, result, tail)?;
        self.f.instructions().else_();
        if arms.len() > 1 {
            self.lower_arm_chain(&arms[1..], subj_ty, scr, result, tail)?;
        } else {
            // The checker promises exhaustiveness — if it's ever wrong,
            // trap LOUDLY instead of silently misbehaving.
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
                // some(<literal>): non-null AND slot == literal (scalar slots only).
                let IrPattern::Literal { expr } = inner.as_ref() else {
                    return unsup(&format!("pattern:some-{}", pattern_name(inner)));
                };
                let et = self.types.el(h);
                let SliceTy::Scalar(sc) = et else {
                    return unsup("pattern:some-lit-nonscalar");
                };
                self.f.instructions().local_get(scr).if_(BlockType::Result(ValType::I32));
                self.f.instructions().local_get(scr);
                self.load_ty_slot(et, almide_layout::OPTION_FIELD);
                self.lower(expr, Some(et))?;
                self.emit_scalar_eq(sc);
                self.f.instructions().else_().i32_const(0).end();
                Ok(())
            }
            (IrPattern::Constructor { name, args }, SliceTy::Named(ti)) => {
                self.test_ctor_pattern(name, args, ti, scr)
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
                let IrPattern::Literal { expr } = inner.as_ref() else {
                    return unsup(&format!("pattern:sum-{}", pattern_name(inner)));
                };
                let et = self.types.el(o);
                let SliceTy::Scalar(sc) = et else {
                    return unsup("pattern:sum-lit-nonscalar");
                };
                // tag matches AND field == literal.
                self.f.instructions().if_(BlockType::Result(ValType::I32));
                self.f.instructions().local_get(scr);
                self.load_ty_slot(et, almide_layout::SUM_FIELD);
                self.lower(expr, Some(et))?;
                self.emit_scalar_eq(sc);
                self.f.instructions().else_().i32_const(0).end();
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

                let Some(&(cti, ci)) = self.types.ctors.get(name) else {
                    return unsup(&format!("pattern:ctor-unknown:{name}"));
                };
                if cti != ti {
                    return unsup("pattern:ctor-ty-mismatch");
                }
                let (tag, fields) = {
                    let NamedDef::Variant(v) = &self.types.defs[ti as usize] else {
                        return unsup("pattern:ctor-of-record");
                    };
                    let c = &v.cases[ci as usize];
                    let fs: Vec<(SliceTy, u32)> =
                        c.fields.iter().map(|f| (f.ty, f.offset)).collect();
                    (c.tag, fs)
                };
                if args.len() != fields.len() {
                    return unsup("pattern:ctor-arity");
                }
                // tag test, then AND in each refutable field literal.
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
                    let IrPattern::Literal { expr } = ap else {
                        return unsup(&format!("pattern:ctor-{}", pattern_name(ap)));
                    };
                    let SliceTy::Scalar(fs) = fty else {
                        return unsup("pattern:ctor-lit-nonscalar");
                    };
                    self.f.instructions().if_(BlockType::Result(ValType::I32));
                    self.f.instructions().local_get(scr);
                    self.load_ty_slot(fty, off);
                    self.lower(expr, Some(fty))?;
                    match fs {
                        Scalar::Int => self.f.instructions().i64_eq(),
                        Scalar::Bool => self.f.instructions().i32_eq(),
                        Scalar::Str => self.f.instructions().call(F_STR_EQ),
                    };
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
            IrPattern::Constructor { name, args } => {
                let SliceTy::Named(ti) = subj_ty else {
                    return unsup("pattern:ctor-on-non-named");
                };
                let Some(&(_, ci)) = self.types.ctors.get(name.as_str()) else {
                    return unsup(&format!("pattern:ctor-unknown:{name}"));
                };
                let fields: Vec<(SliceTy, u32)> = {
                    let NamedDef::Variant(v) = &self.types.defs[ti as usize] else {
                        return unsup("pattern:ctor-of-record");
                    };
                    v.cases[ci as usize].fields.iter().map(|f| (f.ty, f.offset)).collect()
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
                        other => {
                            return unsup(&format!("pattern:ctor-{}", pattern_name(other)))
                        }
                    }
                }
                Ok(())
            }
            other => unsup(&format!("pattern:{}", pattern_name(other))),
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
            other => unsup(&format!("pattern:inner-{}", pattern_name(other))),
        }
    }

    /// Scalar equality: i64/i32 compare, byte-equality for strings.
    pub(crate) fn emit_scalar_eq(&mut self, s: Scalar) {
        match s {
            Scalar::Int => self.f.instructions().i64_eq(),
            Scalar::Bool => self.f.instructions().i32_eq(),
            Scalar::Str => self.f.instructions().call(F_STR_EQ),
        };
    }

}
