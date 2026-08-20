//! The dynamic `Value` model (Codec/json's data carrier), NATIVE to this
//! backend's ratified layout (2026-08-20 ○: rebuild, do not adopt the
//! incumbent's len-as-tag convention). A Value is a 16-byte block:
//! `[rc][len][cap][tag:i32 @SUM_TAG][pad][payload:8B @SUM_FIELD]` — the
//! SAME offsets Result blocks use, so the machinery reads familiarly.
//! Tags: 0=Null, 1=Bool, 2=Int, 3=Float, 4=Str, 5=Array, 6=Object.
//! Str/Array payloads hold BLOCK ADDRESSES (our 4-byte-slot lists);
//! sharing is unobservable because the Value API never mutates in place.

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

pub(crate) const VT_NULL: i32 = 0;
pub(crate) const VT_BOOL: i32 = 1;
pub(crate) const VT_INT: i32 = 2;
pub(crate) const VT_FLOAT: i32 = 3;
pub(crate) const VT_STR: i32 = 4;
pub(crate) const VT_ARRAY: i32 = 5;

impl Emitter<'_> {
    /// `value.*` module calls. Returns Ok(None) for unhandled names so the
    /// caller can fall through to the qualified table / whitelist.
    pub(crate) fn lower_value_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<Option<SliceTy>>, EmitError> {
        let out = match (func, args) {
            ("null", []) => {
                self.emit_value_box(VT_NULL, None)?;
                Some(SliceTy::Value)
            }
            ("int", [n]) => {
                self.lower(n, Some(INT))?;
                self.emit_value_box(VT_INT, Some(INT))?;
                Some(SliceTy::Value)
            }
            ("bool", [b]) => {
                self.lower(b, Some(BOOL))?;
                self.f.instructions().i64_extend_i32_u();
                self.emit_value_box(VT_BOOL, Some(INT))?;
                Some(SliceTy::Value)
            }
            ("float", [x]) => {
                self.lower(x, Some(FLOAT))?;
                self.emit_value_box(VT_FLOAT, Some(FLOAT))?;
                Some(SliceTy::Value)
            }
            ("str", [s]) => {
                self.lower(s, Some(STR))?;
                self.emit_value_box(VT_STR, Some(STR))?;
                Some(SliceTy::Value)
            }
            ("array", [xs]) => {
                let got = self.lower(xs, None)?;
                let SliceTy::List(h) = got else {
                    return Err(EmitError::Unsupported(format!("value.array-of:{got:?}")));
                };
                if self.types.el(h) != SliceTy::Value {
                    return Err(EmitError::Unsupported("value.array-el".into()));
                }
                self.emit_value_box(VT_ARRAY, Some(STR))?; // addr slot (i32 class)
                Some(SliceTy::Value)
            }
            ("as_int", [v]) => {
                self.lower(v, Some(SliceTy::Value))?;
                self.emit_value_unbox(VT_INT, INT, "expected Int")?;
                Some(SliceTy::Result(self.types.intern(INT), self.types.intern(STR)))
            }
            ("as_bool", [v]) => {
                self.lower(v, Some(SliceTy::Value))?;
                self.emit_value_unbox(VT_BOOL, BOOL, "expected Bool")?;
                Some(SliceTy::Result(self.types.intern(BOOL), self.types.intern(STR)))
            }
            ("as_string", [v]) => {
                self.lower(v, Some(SliceTy::Value))?;
                self.emit_value_unbox(VT_STR, STR, "expected String")?;
                Some(SliceTy::Result(self.types.intern(STR), self.types.intern(STR)))
            }
            ("as_array", [v]) => {
                self.lower(v, Some(SliceTy::Value))?;
                let lv = SliceTy::List(self.types.intern(SliceTy::Value));
                self.emit_value_unbox(VT_ARRAY, lv, "expected Array")?;
                Some(SliceTy::Result(self.types.intern(lv), self.types.intern(STR)))
            }
            // #658: a JSON number has no int/float split — an Int Value
            // widens to a valid Float.
            ("as_float", [v]) => {
                self.lower(v, Some(SliceTy::Value))?;
                self.emit_value_as_float()?;
                Some(SliceTy::Result(self.types.intern(FLOAT), self.types.intern(STR)))
            }
            _ => return Ok(None),
        };
        Ok(Some(out))
    }

    /// `[payload?]` -> `[value block]`: alloc, tag, store the 8-byte slot.
    /// `payload_kind` picks the store width (None = tag-only Null).
    fn emit_value_box(
        &mut self,
        tag: i32,
        payload_kind: Option<SliceTy>,
    ) -> Result<(), EmitError> {
        let hv = payload_kind.map(|k| self.hold_val(k)).transpose()?;
        let hb = self.hold_i32()?;
        if let (Some(h), Some(_)) = (hv, payload_kind) {
            self.f.instructions().local_set(h);
        }
        self.f
            .instructions()
            .i32_const(16)
            .call(F_ALLOC)
            .local_tee(hb)
            .i32_const(tag)
            .i32_store(slot_memarg(almide_layout::SUM_TAG));
        if let (Some(h), Some(k)) = (hv, payload_kind) {
            self.f.instructions().local_get(hb).local_get(h);
            self.store_ty_slot(k, almide_layout::SUM_FIELD);
        }
        self.f.instructions().local_get(hb);
        self.release_i32();
        if let Some(k) = payload_kind {
            self.release_val(k);
        }
        Ok(())
    }

    /// `[value block]` -> `[Result block]`: tag match yields ok(payload),
    /// anything else the exact incumbent err line.
    fn emit_value_unbox(
        &mut self,
        want_tag: i32,
        payload: SliceTy,
        err_msg: &str,
    ) -> Result<(), EmitError> {
        let msg = self.pool.intern(err_msg);
        let hv = self.hold_i32()?;
        let hr = self.hold_i32()?;
        self.f.instructions().local_set(hv);
        self.f
            .instructions()
            .i32_const(16)
            .call(F_ALLOC)
            .local_set(hr);
        let mut i = self.f.instructions();
        i.local_get(hv)
            .i32_load(slot_memarg(almide_layout::SUM_TAG))
            .i32_const(want_tag)
            .i32_eq()
            .if_(BlockType::Empty);
        i.local_get(hr).i32_const(0).i32_store(slot_memarg(almide_layout::SUM_TAG));
        i.local_get(hr).local_get(hv);
        let _ = i;
        self.load_ty_slot(payload, almide_layout::SUM_FIELD);
        self.store_ty_slot(payload, almide_layout::SUM_FIELD);
        let mut i = self.f.instructions();
        i.else_();
        i.local_get(hr).i32_const(1).i32_store(slot_memarg(almide_layout::SUM_TAG));
        i.local_get(hr)
            .i32_const(msg as i32)
            .i32_store(slot_memarg(almide_layout::SUM_FIELD));
        i.end();
        i.local_get(hr);
        self.release_i32();
        self.release_i32();
        Ok(())
    }

    /// `as_float` with the #658 widening: Float passes through, Int
    /// converts, anything else errs "expected Float".
    fn emit_value_as_float(&mut self) -> Result<(), EmitError> {
        let msg = self.pool.intern("expected Float");
        let hv = self.hold_i32()?;
        let hr = self.hold_i32()?;
        self.f.instructions().local_set(hv);
        self.f.instructions().i32_const(16).call(F_ALLOC).local_set(hr);
        let m_tag = slot_memarg(almide_layout::SUM_TAG);
        let m_pay = slot_memarg(almide_layout::SUM_FIELD);
        let mut i = self.f.instructions();
        i.local_get(hv).i32_load(m_tag).i32_const(VT_FLOAT).i32_eq();
        i.if_(BlockType::Empty);
        i.local_get(hr).i32_const(0).i32_store(m_tag);
        i.local_get(hr).local_get(hv).f64_load(m_pay).f64_store(m_pay);
        i.else_();
        i.local_get(hv).i32_load(m_tag).i32_const(VT_INT).i32_eq();
        i.if_(BlockType::Empty);
        i.local_get(hr).i32_const(0).i32_store(m_tag);
        i.local_get(hr).local_get(hv).i64_load(m_pay).f64_convert_i64_s().f64_store(m_pay);
        i.else_();
        i.local_get(hr).i32_const(1).i32_store(m_tag);
        i.local_get(hr).i32_const(msg as i32).i32_store(m_pay);
        i.end();
        i.end();
        i.local_get(hr);
        self.release_i32();
        self.release_i32();
        Ok(())
    }
}
