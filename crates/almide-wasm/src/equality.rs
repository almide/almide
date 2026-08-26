//! Structural equality lowering — split from emitter.rs for the
//! complexity budget.

use almide_ir::{BinOp, IrExpr};
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::types_table::NamedDef;
use crate::*;

impl Emitter<'_> {
    /// Comparison operators — Int ordering, and equality over ints,
    /// bools, and byte-equal blocks (strings, List[Int/Bool]).
    /// List[String] holds addresses — identity is NOT equality: refused.
    pub(crate) fn lower_cmp(
        &mut self,
        op: BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Result<SliceTy, EmitError> {
        use BinOp::*;
        if matches!(op, Lt | Gt | Lte | Gte) {
            return self.lower_order_cmp(op, left, right);
        }
        let lt = self.lower(left, None)?;
        self.lower(right, Some(lt))?;
        self.emit_val_eq(lt)?;
        if matches!(op, Neq) {
            self.f.instructions().i32_eqz();
        }
        Ok(BOOL)
    }

    /// Structural equality: `[a, b]` on the stack -> i32 verdict.
    /// Byte-equality only where payload bytes ARE the values (strings,
    /// bytes, Int/Bool lists); address-carrying elements compare
    /// ELEMENT-WISE (byte-compare would be an identity test).
    pub(crate) fn emit_val_eq(&mut self, ty: SliceTy) -> Result<(), EmitError> {
        self.emit_val_eq_at(ty, &mut Vec::new())
    }

    /// The recursive core: `path` carries the Named types currently being
    /// inlined — a CYCLE (recursive type) is cut with a call to the
    /// runtime-recursive `$named_eq_<ti>` helper (the display doctrine).
    pub(crate) fn emit_val_eq_at(
        &mut self,
        ty: SliceTy,
        path: &mut Vec<u32>,
    ) -> Result<(), EmitError> {
        match ty {
            INT => {
                self.f.instructions().i64_eq();
            }
            FLOAT => {
                self.f.instructions().f64_eq();
            }
            BOOL => {
                self.f.instructions().i32_eq();
            }
            STR | SliceTy::Scalar(Scalar::Bytes) => {
                self.f.instructions().call(F_STR_EQ);
            }
            // Deep structural Value equality via the recursive helper
            // (tags, IEEE floats, in-order arrays/objects).
            SliceTy::Value => {
                let ti = self.types.tuple(vec![STR, SliceTy::Value]);
                let def = self.types.tuple_def(ti);
                let eq = self.work.helper(Helper::ValueEq {
                    key_off: def.fields[0].1,
                    val_off: def.fields[1].1,
                });
                self.f.instructions().call(eq);
            }
            SliceTy::Unit => {
                self.f.instructions().drop();
                self.f.instructions().drop();
                self.f.instructions().i32_const(1);
            }
            SliceTy::List(h)
                if matches!(
                    self.types.el(h),
                    SliceTy::Scalar(Scalar::Int) | SliceTy::Scalar(Scalar::Bool)
                ) =>
            {
                self.f.instructions().call(F_STR_EQ);
            }
            SliceTy::List(h) => self.emit_list_eq(h, path)?,
            SliceTy::Option(h) => self.emit_option_eq(h, path)?,
            SliceTy::Result(o, er) => {
                // Tags must agree; the ACTIVE side's payload compares
                // (recursive) — ok vs ok through el(o), err vs err
                // through el(er).
                let (ot, et) = (self.types.el(o), self.types.el(er));
                let hb = self.hold_i32()?;
                let ha = self.hold_i32()?;
                self.f.instructions().local_set(hb).local_set(ha);
                let tag = slot_memarg(almide_layout::SUM_TAG);
                self.f.instructions().local_get(ha).i32_load(tag);
                self.f.instructions().local_get(hb).i32_load(tag);
                self.f.instructions().i32_ne().if_(BlockType::Result(ValType::I32));
                self.f.instructions().i32_const(0);
                self.f.instructions().else_();
                self.f.instructions().local_get(ha).i32_load(tag).i32_eqz();
                self.f.instructions().if_(BlockType::Result(ValType::I32));
                self.f.instructions().local_get(ha);
                self.load_ty_slot(ot, almide_layout::SUM_FIELD);
                self.f.instructions().local_get(hb);
                self.load_ty_slot(ot, almide_layout::SUM_FIELD);
                self.emit_val_eq_at(ot, path)?;
                self.f.instructions().else_();
                self.f.instructions().local_get(ha);
                self.load_ty_slot(et, almide_layout::SUM_FIELD);
                self.f.instructions().local_get(hb);
                self.load_ty_slot(et, almide_layout::SUM_FIELD);
                self.emit_val_eq_at(et, path)?;
                self.f.instructions().end().end();
                self.release_i32();
                self.release_i32();
            }
            SliceTy::Named(ti) => {
                if path.contains(&ti) {
                    if matches!(
                        self.work.eq_bodies.borrow().get(&ti),
                        Some(crate::work::DisplayBuild::Failed)
                    ) {
                        return unsup("eq-helper-failed");
                    }
                    let idx = self.work.helper(Helper::NamedEq { ti });
                    self.f.instructions().call(idx);
                } else {
                    path.push(ti);
                    self.emit_named_eq(ti, path)?;
                    path.pop();
                }
            }
            SliceTy::Tuple(id) => {
                // Field-wise AND, each field recursing through emit_val_eq.
                let fields = self.types.tuple_def(id).fields;
                let hb = self.hold_i32()?;
                let ha = self.hold_i32()?;
                self.f.instructions().local_set(hb).local_set(ha);
                self.f.instructions().i32_const(1);
                for (fty, off) in fields {
                    self.f.instructions().if_(BlockType::Result(ValType::I32));
                    self.f.instructions().local_get(ha);
                    self.load_ty_slot(fty, off);
                    self.f.instructions().local_get(hb);
                    self.load_ty_slot(fty, off);
                    self.emit_val_eq_at(fty, path)?;
                    self.f.instructions().else_().i32_const(0).end();
                }
                self.release_i32();
                self.release_i32();
            }
            // Order-INSENSITIVE map equality (native AlmideMap PartialEq):
            // same entry count, then every a-entry's key scans in b with
            // an equal value (recursive through emit_val_eq).
            SliceTy::Map(kh, vh) => {
                let (kt, vt) = (self.types.el(kh), self.types.el(vh));
                let scan = self.scan_helper(kt)?;
                let (koff, voff, esz) = crate::collections::entry_layout(kt, vt);
                let hb = self.hold_i32()?;
                let ha = self.hold_i32()?;
                let verdict = self.hold_i32()?;
                let cur = self.hold_i32()?;
                let end = self.hold_i32()?;
                let he = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(hb).local_set(ha);
                    i.local_get(ha).i32_load(len_memarg());
                    i.local_get(hb).i32_load(len_memarg());
                    i.i32_ne().if_(BlockType::Result(ValType::I32));
                    i.i32_const(0);
                    i.else_();
                    i.i32_const(1).local_set(verdict);
                    i.local_get(ha)
                        .i32_const(almide_layout::PAYLOAD as i32)
                        .i32_add()
                        .local_set(cur);
                    i.local_get(cur).local_get(ha).i32_load(len_memarg()).i32_add().local_set(end);
                    i.block(BlockType::Empty).loop_(BlockType::Empty);
                    i.local_get(cur).local_get(end).i32_ge_u().br_if(1);
                    i.local_get(hb).i32_const(esz as i32).i32_const(koff as i32);
                    i.local_get(cur).i32_const(koff as i32).i32_add();
                }
                self.load_ty_slot_at(kt);
                {
                    let mut i = self.f.instructions();
                    i.call(scan).local_tee(he).i32_eqz().if_(BlockType::Empty);
                    i.i32_const(0).local_set(verdict);
                    i.br(2);
                    i.end();
                    i.local_get(cur).i32_const(voff as i32).i32_add();
                }
                self.load_ty_slot_at(vt);
                self.f.instructions().local_get(he).i32_const(voff as i32).i32_add();
                self.load_ty_slot_at(vt);
                self.emit_val_eq_at(vt, path)?;
                {
                    let mut i = self.f.instructions();
                    i.i32_eqz().if_(BlockType::Empty);
                    i.i32_const(0).local_set(verdict);
                    i.br(2);
                    i.end();
                    i.local_get(cur).i32_const(esz as i32).i32_add().local_set(cur);
                    i.br(0).end().end();
                    i.local_get(verdict);
                    i.end();
                }
                for _ in 0..6 {
                    self.release_i32();
                }
            }
            // Order-insensitive set equality: same count (dedup makes the
            // count the cardinality), then every a-member scans in b.
            SliceTy::Set(h) => {
                let et = self.types.el(h);
                let scan = self.scan_helper(et)?;
                let stride = et.slot_size() as i32;
                let hb = self.hold_i32()?;
                let ha = self.hold_i32()?;
                let verdict = self.hold_i32()?;
                let cur = self.hold_i32()?;
                let end = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(hb).local_set(ha);
                    i.local_get(ha).i32_load(len_memarg());
                    i.local_get(hb).i32_load(len_memarg());
                    i.i32_ne().if_(BlockType::Result(ValType::I32));
                    i.i32_const(0);
                    i.else_();
                    i.i32_const(1).local_set(verdict);
                    i.local_get(ha)
                        .i32_const(almide_layout::PAYLOAD as i32)
                        .i32_add()
                        .local_set(cur);
                    i.local_get(cur).local_get(ha).i32_load(len_memarg()).i32_add().local_set(end);
                    i.block(BlockType::Empty).loop_(BlockType::Empty);
                    i.local_get(cur).local_get(end).i32_ge_u().br_if(1);
                    i.local_get(hb).i32_const(stride).i32_const(0);
                    i.local_get(cur);
                }
                self.load_ty_slot_at(et);
                {
                    let mut i = self.f.instructions();
                    i.call(scan).i32_eqz().if_(BlockType::Empty);
                    i.i32_const(0).local_set(verdict);
                    i.br(2);
                    i.end();
                    i.local_get(cur).i32_const(stride).i32_add().local_set(cur);
                    i.br(0).end().end();
                    i.local_get(verdict);
                    i.end();
                }
                for _ in 0..5 {
                    self.release_i32();
                }
            }
            other => return unsup(&format!("binop:eq-{other:?}")),
        }
        Ok(())
    }

    /// Element-wise list equality (recursive): same len, then every
    /// element equal under `emit_val_eq`. Five holds per nesting level;
    /// the pool bound turns absurd nesting into an honest refusal.
    fn emit_list_eq(&mut self, h: ETy, path: &mut Vec<u32>) -> Result<(), EmitError> {
        let el = self.types.el(h);
        let stride = el.slot_size() as i32;
        let hb = self.hold_i32()?;
        let ha = self.hold_i32()?;
        let verdict = self.hold_i32()?;
        let end = self.hold_i32()?;
        let cur = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_set(hb).local_set(ha);
            i.local_get(ha).i32_load(len_memarg());
            i.local_get(hb).i32_load(len_memarg());
            i.i32_ne().if_(BlockType::Result(ValType::I32));
            i.i32_const(0);
            i.else_();
            i.i32_const(1).local_set(verdict);
            // cur walks a's payload; b's element is at (cur - a + b).
            i.local_get(ha).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(cur);
            i.local_get(cur).local_get(ha).i32_load(len_memarg()).i32_add().local_set(end);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(cur).local_get(end).i32_ge_u().br_if(1);
            i.local_get(cur);
        }
        self.load_ty_slot_at(el);
        {
            let mut i = self.f.instructions();
            i.local_get(cur).local_get(ha).i32_sub().local_get(hb).i32_add();
        }
        self.load_ty_slot_at(el);
        self.emit_val_eq_at(el, path)?;
        {
            let mut i = self.f.instructions();
            i.i32_eqz().if_(BlockType::Empty);
            i.i32_const(0).local_set(verdict);
            i.br(2);
            i.end();
            i.local_get(cur).i32_const(stride).i32_add().local_set(cur);
            i.br(0);
            i.end();
            i.end();
            i.local_get(verdict);
            i.end();
        }
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(())
    }

    /// Named (record/variant) equality (split from emit_val_eq for the complexity budget).
    pub(crate) fn emit_named_eq(&mut self, ti: u32, path: &mut Vec<u32>) -> Result<(), EmitError> {

                enum Shape {
                    Record(Vec<(SliceTy, u32)>),
                    UnitVariant,
                    Variant(Vec<(u32, Vec<(SliceTy, u32)>)>),
                }
                let shape = match &self.types.def(ti) {
                    NamedDef::Record(r) => {
                        Shape::Record(r.fields.iter().map(|f| (f.ty, f.offset)).collect())
                    }
                    NamedDef::Variant(v) => {
                        if v.cases.iter().all(|c| c.fields.is_empty()) {
                            Shape::UnitVariant
                        } else {
                            Shape::Variant(
                                v.cases
                                    .iter()
                                    .map(|c| {
                                        (c.tag, c.fields.iter().map(|f| (f.ty, f.offset)).collect())
                                    })
                                    .collect(),
                            )
                        }
                    }
                    NamedDef::Excluded => return unsup("binop:eq-excluded"),
                };
                match shape {
                    Shape::UnitVariant => {
                        // Unit-case variants: the tag IS the value.
                        let m = slot_memarg(almide_layout::SUM_TAG);
                        let hb = self.hold_i32()?;
                        self.f.instructions().local_set(hb);
                        self.f.instructions().i32_load(m);
                        self.f.instructions().local_get(hb).i32_load(m).i32_eq();
                        self.release_i32();
                    }
                    Shape::Variant(cases) => {
                        // tags equal AND (dispatch by tag → field-wise).
                        let hb = self.hold_i32()?;
                        let ha = self.hold_i32()?;
                        let m = slot_memarg(almide_layout::SUM_TAG);
                        self.f.instructions().local_set(hb).local_set(ha);
                        self.f.instructions().local_get(ha).i32_load(m);
                        self.f.instructions().local_get(hb).i32_load(m).i32_ne();
                        self.f.instructions().if_(BlockType::Result(ValType::I32));
                        self.f.instructions().i32_const(0);
                        self.f.instructions().else_();
                        let payload: Vec<_> =
                            cases.iter().filter(|(_, fs)| !fs.is_empty()).collect();
                        // if tag==A { fields-A } else if tag==B { fields-B }
                        // … else { 1 } (a unit case: tag equality settled it).
                        for (tag, fields) in &payload {
                            self.f.instructions().local_get(ha).i32_load(m);
                            self.f.instructions().i32_const(*tag as i32).i32_eq();
                            self.f.instructions().if_(BlockType::Result(ValType::I32));
                            self.f.instructions().i32_const(1);
                            for (fty, off) in fields.iter() {
                                self.f.instructions().if_(BlockType::Result(ValType::I32));
                                self.f.instructions().local_get(ha);
                                self.load_ty_slot(*fty, *off);
                                self.f.instructions().local_get(hb);
                                self.load_ty_slot(*fty, *off);
                                self.emit_val_eq_at(*fty, path)?;
                                self.f.instructions().else_().i32_const(0).end();
                            }
                            self.f.instructions().else_();
                        }
                        self.f.instructions().i32_const(1);
                        for _ in &payload {
                            self.f.instructions().end();
                        }
                        self.f.instructions().end(); // the tags-differ if
                        self.release_i32();
                        self.release_i32();
                    }
                    Shape::Record(fields) => {
                        let hb = self.hold_i32()?;
                        let ha = self.hold_i32()?;
                        self.f.instructions().local_set(hb).local_set(ha);
                        self.f.instructions().i32_const(1);
                        for (fty, off) in fields {
                            self.f.instructions().if_(BlockType::Result(ValType::I32));
                            self.f.instructions().local_get(ha);
                            self.load_ty_slot(fty, off);
                            self.f.instructions().local_get(hb);
                            self.load_ty_slot(fty, off);
                            self.emit_val_eq_at(fty, path)?;
                            self.f.instructions().else_().i32_const(0).end();
                        }
                        self.release_i32();
                        self.release_i32();
                    }
                }
        Ok(())
    }
}

impl Emitter<'_> {
    /// Option equality — split from `emit_val_eq_at` for the complexity
    /// budget. Null-ness must agree; both-null is equal; both-some
    /// compares the payload (recursive).
    fn emit_option_eq(&mut self, h: ETy, path: &mut Vec<u32>) -> Result<(), EmitError> {
                // Null-ness must agree; both-null is equal; both-some
                // compares the payload (recursive).
                let et = self.types.el(h);
                let hb = self.hold_i32()?;
                let ha = self.hold_i32()?;
                self.f.instructions().local_set(hb).local_set(ha);
                self.f.instructions().local_get(ha).i32_eqz();
                self.f.instructions().local_get(hb).i32_eqz();
                self.f.instructions().i32_ne().if_(BlockType::Result(ValType::I32));
                self.f.instructions().i32_const(0);
                self.f.instructions().else_();
                self.f.instructions().local_get(ha).i32_eqz();
                self.f.instructions().if_(BlockType::Result(ValType::I32));
                self.f.instructions().i32_const(1);
                self.f.instructions().else_();
                self.f.instructions().local_get(ha);
                self.load_ty_slot(et, almide_layout::OPTION_FIELD);
                self.f.instructions().local_get(hb);
                self.load_ty_slot(et, almide_layout::OPTION_FIELD);
                self.emit_val_eq_at(et, path)?;
                self.f.instructions().end().end();
                self.release_i32();
                self.release_i32();
        Ok(())
    }
}

impl Emitter<'_> {
    /// The four ordering operators — split from `lower_cmp` for the
    /// complexity budget. C-179: a UInt64 operand's slot is a u64 bit
    /// pattern, so ordering reads it UNSIGNED.
    fn lower_order_cmp(&mut self, op: BinOp, left: &IrExpr, right: &IrExpr) -> Result<SliceTy, EmitError> {
        // C-179: a UInt64 operand's slot is a u64 bit pattern —
        // ordering reads it UNSIGNED.
        let unsigned = crate::binop::is_uint64(&left.ty) || crate::binop::is_uint64(&right.ty);
        let lt = self.lower(left, None)?;
        self.lower(right, Some(lt))?;
        match lt {
            INT | BOOL => self.emit_order_int_bool(lt, op, unsigned),
            FLOAT | STR => self.emit_order_float_str(lt, op),
            other => return unsup(&format!("binop:cmp-{other:?}")),
        }
        Ok(BOOL)
    }

    /// Int (signed/unsigned by C-179) and Bool (false < true) ordering.
    fn emit_order_int_bool(&mut self, lt: SliceTy, op: BinOp, unsigned: bool) {
        use BinOp::*;
        let mut i = self.f.instructions();
        match (lt, op, unsigned) {
            (INT, Lt, true) => i.i64_lt_u(),
            (INT, Gt, true) => i.i64_gt_u(),
            (INT, Lte, true) => i.i64_le_u(),
            (INT, Gte, true) => i.i64_ge_u(),
            (INT, Lt, _) => i.i64_lt_s(),
            (INT, Gt, _) => i.i64_gt_s(),
            (INT, Lte, _) => i.i64_le_s(),
            (INT, Gte, _) => i.i64_ge_s(),
            (_, Lt, _) => i.i32_lt_u(),
            (_, Gt, _) => i.i32_gt_u(),
            (_, Lte, _) => i.i32_le_u(),
            (_, _, _) => i.i32_ge_u(),
        };
    }

    /// Float (IEEE compares) and String (byte-lexicographic with length
    /// tiebreak via the shared $str_cmp) ordering.
    fn emit_order_float_str(&mut self, lt: SliceTy, op: BinOp) {
        use BinOp::*;
        let mut i = self.f.instructions();
        if lt == STR {
            i.call(F_STR_CMP).i32_const(0);
            match op {
                Lt => i.i32_lt_s(),
                Gt => i.i32_gt_s(),
                Lte => i.i32_le_s(),
                _ => i.i32_ge_s(),
            };
            return;
        }
        match op {
            Lt => i.f64_lt(),
            Gt => i.f64_gt(),
            Lte => i.f64_le(),
            _ => i.f64_ge(),
        };
    }
}
