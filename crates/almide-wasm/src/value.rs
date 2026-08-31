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


pub(crate) mod value_tags {
    pub(crate) const VT_OBJECT: i32 = 6;
}
pub(crate) const VT_NULL: i32 = 0;
pub(crate) const VT_BOOL: i32 = 1;
pub(crate) const VT_INT: i32 = 2;
pub(crate) const VT_FLOAT: i32 = 3;
pub(crate) const VT_STR: i32 = 4;
pub(crate) const VT_ARRAY: i32 = 5;
pub(crate) const VT_OBJECT: i32 = 6;

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
            ("merge", [va, vb]) => {
                let ti = self.types.tuple(vec![STR, SliceTy::Value]);
                let def = self.types.tuple_def(ti);
                let m = self.work.helper(Helper::ValueMerge {
                    key_off: def.fields[0].1,
                    val_off: def.fields[1].1,
                });
                self.lower(va, Some(SliceTy::Value))?;
                self.lower(vb, Some(SliceTy::Value))?;
                self.f.instructions().call(m);
                Some(SliceTy::Value)
            }
            // pick/omit: keep (drop) the named keys, kept pairs SHARED with
            // the source (native filter+clone of the pair vec — the pair
            // blocks themselves are never copied). Non-object passes through.
            ("pick" | "omit", [v, keys]) => Some(self.lower_value_pick_omit(func, v, keys)?),
            // Object: tag 6, payload = the (String, Value) pairs list —
            // insertion order IS the block, exactly the interp's ordered
            // object model.
            ("object", [pairs]) => {
                let got = self.lower(pairs, None)?;
                let SliceTy::List(h) = got else {
                    return Err(EmitError::Unsupported(format!("value.object-of:{got:?}")));
                };
                let ok = match self.types.el(h) {
                    SliceTy::Tuple(ti) => {
                        let def = self.types.tuple_def(ti);
                        def.fields.len() == 2
                            && def.fields[0].0 == STR
                            && def.fields[1].0 == SliceTy::Value
                    }
                    _ => false,
                };
                if !ok {
                    return Err(EmitError::Unsupported("value.object-el".into()));
                }
                self.emit_value_box(VT_OBJECT, Some(STR))?;
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
            _ => return self.lower_value_call_b(func, args),
        };
        Ok(Some(out))
    }

    /// The pooled JSON/display fragment set.
    /// pick/omit: keep (drop) the named keys, kept pairs SHARED with
    /// the source (native filter+clone of the pair vec — the pair
    /// blocks themselves are never copied). Non-object passes through.
    fn lower_value_pick_omit(&mut self, func: &str, v: &IrExpr, keys: &IrExpr) -> Result<SliceTy, EmitError> {
        let keep_found = i32::from(func == "pick");
        self.lower(v, Some(SliceTy::Value))?;
        let hv = self.hold_i32()?;
        self.f.instructions().local_set(hv);
        match self.lower(keys, None)? {
            SliceTy::List(h) if self.types.el(h) == STR => {}
            other => return Err(EmitError::Unsupported(format!("value.{func}-keys:{other:?}"))),
        }
        let hk = self.hold_i32()?;
        self.f.instructions().local_set(hk);
        let scan = self.scan_helper(STR)?;
        let ti = self.types.tuple(vec![STR, SliceTy::Value]);
        let key_off = self.types.tuple_def(ti).fields[0].1;
        let hp = self.hold_i32()?;
        let ho = self.hold_i32()?;
        let hw = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_get(hv)
            .i32_load(slot_memarg(almide_layout::SUM_TAG))
            .i32_const(VT_OBJECT)
            .i32_ne()
            .if_(BlockType::Result(wasm_encoder::ValType::I32));
        i.local_get(hv);
        i.else_();
        i.local_get(hv).i32_load(slot_memarg(almide_layout::SUM_FIELD)).local_set(hp);
        i.local_get(hp).i32_load(len_memarg()).call(F_ALLOC).local_set(ho);
        i.i32_const(0).local_set(hw);
        i.i32_const(0).local_set(hv); // reuse: read cursor (bytes)
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hv).local_get(hp).i32_load(len_memarg()).i32_ge_u().br_if(1);
        // pair handle → its key → membership scan
        i.local_get(hk).i32_const(4).i32_const(0);
        i.local_get(hp).local_get(hv).i32_add().i32_load(slot_memarg(0));
        i.i32_load(slot_memarg(key_off));
        i.call(scan).i32_const(0).i32_ne();
        i.i32_const(keep_found).i32_eq().if_(BlockType::Empty);
        i.local_get(ho).local_get(hw).i32_add();
        i.local_get(hp).local_get(hv).i32_add().i32_load(slot_memarg(0));
        i.i32_store(slot_memarg(0));
        i.local_get(hw).i32_const(4).i32_add().local_set(hw);
        i.end();
        i.local_get(hv).i32_const(4).i32_add().local_set(hv);
        i.br(0).end().end();
        i.local_get(ho).local_get(hw).i32_store(len_memarg());
        // box a fresh Object value
        i.i32_const(16).call(F_ALLOC).local_set(hp);
        i.local_get(hp).i32_const(VT_OBJECT).i32_store(slot_memarg(almide_layout::SUM_TAG));
        i.local_get(hp).local_get(ho).i32_store(slot_memarg(almide_layout::SUM_FIELD));
        i.local_get(hp);
        i.end();
        let _ = i;
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(SliceTy::Value)
    }

    pub(crate) fn json_frags(&mut self) -> JsonFrags {
        JsonFrags {
            null_: self.pool.intern("null"),
            true_: self.pool.intern("true"),
            false_: self.pool.intern("false"),
            esc_backslash: self.pool.intern("\\\\"),
            esc_quote: self.pool.intern("\\\""),
            esc_n: self.pool.intern("\\n"),
            esc_r: self.pool.intern("\\r"),
            esc_t: self.pool.intern("\\t"),
            quote: self.pool.intern("\""),
            comma: self.pool.intern(","),
            colon: self.pool.intern(":"),
            lbrack: self.pool.intern("["),
            rbrack: self.pool.intern("]"),
            lbrace: self.pool.intern("{"),
            rbrace: self.pool.intern("}"),
        }
    }

    /// `[value]` -> `[String]`: run the JSON serializer helpers over the
    /// line buffer and capture the region as a real block.
    pub(crate) fn emit_value_stringify(&mut self) -> Result<(), EmitError> {
        let Some(fi) = self.resolve_qualified("float.to_string") else {
            return Err(EmitError::Unsupported("stringify:float-unlinked".into()));
        };
        let info = &self.table.infos[fi];
        if info.refuse.is_some() || info.ret != Some(STR) {
            return Err(EmitError::Unsupported("stringify:float-impl".into()));
        }
        let float_idx = info.wasm_index;
        self.calls.insert(fi);
        let frags = self.json_frags();
        let _ = self.work.helper(Helper::JsonQuote { frags });
        let vj = self.work.helper(Helper::JsonValue { float_to_string: float_idx, frags });
        let hv = self.hold_i32()?;
        let start = self.hold_i32()?;
        self.f.instructions().local_set(hv);
        self.f.instructions().global_get(G_LINE_CURSOR).local_set(start);
        self.f
            .instructions()
            .local_get(start)
            .local_get(hv)
            .call(vj)
            .local_set(self.tmp_i32_local);
        self.f
            .instructions()
            .local_get(start)
            .local_get(self.tmp_i32_local)
            .call(F_BUF_TO_BLOCK);
        self.release_i32();
        self.release_i32();
        Ok(())
    }

    /// `[value]` -> `[String]` — the PRETTY twin: `$vjson_pretty` from
    /// depth 0 over the line buffer, captured as a block.
    pub(crate) fn emit_value_stringify_pretty(&mut self) -> Result<(), EmitError> {
        let Some(fi) = self.resolve_qualified("float.to_string") else {
            return Err(EmitError::Unsupported("stringify:float-unlinked".into()));
        };
        let info = &self.table.infos[fi];
        if info.refuse.is_some() || info.ret != Some(STR) {
            return Err(EmitError::Unsupported("stringify:float-impl".into()));
        }
        let float_idx = info.wasm_index;
        self.calls.insert(fi);
        let frags = self.json_frags();
        let pfrags = crate::work::PrettyFrags {
            nl: self.pool.intern("\n"),
            colon_sp: self.pool.intern(": "),
            comma_nl: self.pool.intern(",\n"),
            indent2: self.pool.intern("  "),
            empty_arr: self.pool.intern("[]"),
            empty_obj: self.pool.intern("{}"),
        };
        let _ = self.work.helper(Helper::JsonQuote { frags });
        let vp = self.work.helper(Helper::JsonValuePretty {
            float_to_string: float_idx,
            frags,
            pfrags,
        });
        let hv = self.hold_i32()?;
        let start = self.hold_i32()?;
        self.f.instructions().local_set(hv);
        self.f.instructions().global_get(G_LINE_CURSOR).local_set(start);
        self.f
            .instructions()
            .local_get(start)
            .local_get(hv)
            .i32_const(0)
            .call(vp)
            .local_set(self.tmp_i32_local);
        self.f
            .instructions()
            .local_get(start)
            .local_get(self.tmp_i32_local)
            .call(F_BUF_TO_BLOCK);
        self.release_i32();
        self.release_i32();
        Ok(())
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
    /// The seven `<prefix>, received <Kind>` messages, indexed by the
    /// Value tag — the byte-for-byte twins of value_core's `__vkind`
    /// leaves (#1675).
    fn intern_kind_msgs(&mut self, prefix: &str) -> [u32; 7] {
        ["Null", "Bool", "Int", "Float", "Str", "Array", "Object"]
            .map(|k| self.pool.intern(&format!("{prefix}, received {k}")) as u32)
    }

    /// Leaves the tag-selected message from `msgs` on the stack: a
    /// select chain over `ht` (the value's tag, already in a local).
    fn emit_kind_msg_select(&mut self, msgs: [u32; 7], ht: u32) {
        let mut i = self.f.instructions();
        i.i32_const(msgs[6] as i32);
        for (t, m) in msgs.iter().enumerate().take(6) {
            i.i32_const(*m as i32).local_get(ht).i32_const(t as i32).i32_ne().select();
        }
    }

    fn emit_value_unbox(
        &mut self,
        want_tag: i32,
        payload: SliceTy,
        err_msg: &str,
    ) -> Result<(), EmitError> {
        let msgs = self.intern_kind_msgs(err_msg);
        let hv = self.hold_i32()?;
        let hr = self.hold_i32()?;
        let ht = self.hold_i32()?;
        self.f.instructions().local_set(hv);
        self.f
            .instructions()
            .i32_const(16)
            .call(F_ALLOC)
            .local_set(hr);
        let mut i = self.f.instructions();
        i.local_get(hv)
            .i32_load(slot_memarg(almide_layout::SUM_TAG))
            .local_tee(ht)
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
        i.local_get(hr);
        let _ = i;
        self.emit_kind_msg_select(msgs, ht);
        self.f.instructions().i32_store(slot_memarg(almide_layout::SUM_FIELD));
        let mut i = self.f.instructions();
        i.end();
        i.local_get(hr);
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(())
    }

    /// `as_float` with the #658 widening: Float passes through, Int
    /// converts, anything else errs "expected Float".
    fn emit_value_as_float(&mut self) -> Result<(), EmitError> {
        let msgs = self.intern_kind_msgs("expected Float");
        let hv = self.hold_i32()?;
        let hr = self.hold_i32()?;
        let ht = self.hold_i32()?;
        self.f.instructions().local_set(hv);
        self.f.instructions().i32_const(16).call(F_ALLOC).local_set(hr);
        let m_tag = slot_memarg(almide_layout::SUM_TAG);
        let m_pay = slot_memarg(almide_layout::SUM_FIELD);
        let mut i = self.f.instructions();
        i.local_get(hv).i32_load(m_tag).local_tee(ht).i32_const(VT_FLOAT).i32_eq();
        i.if_(BlockType::Empty);
        i.local_get(hr).i32_const(0).i32_store(m_tag);
        i.local_get(hr).local_get(hv).f64_load(m_pay).f64_store(m_pay);
        i.else_();
        i.local_get(ht).i32_const(VT_INT).i32_eq();
        i.if_(BlockType::Empty);
        i.local_get(hr).i32_const(0).i32_store(m_tag);
        i.local_get(hr).local_get(hv).i64_load(m_pay).f64_convert_i64_s().f64_store(m_pay);
        i.else_();
        i.local_get(hr).i32_const(1).i32_store(m_tag);
        i.local_get(hr);
        let _ = i;
        self.emit_kind_msg_select(msgs, ht);
        let mut i = self.f.instructions();
        i.i32_store(m_pay);
        i.end();
        i.end();
        i.local_get(hr);
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(())
    }
}

impl Emitter<'_> {
    /// The `value.as_*` / remaining half of the value dispatch — split
    /// from `lower_value_call` for the complexity budget.
    fn lower_value_call_b(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<Option<SliceTy>>, EmitError> {
        let out = match (func, args) {
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
                self.emit_value_unbox(VT_STR, STR, "expected Str")?;
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
            // The Codec-derive field accessor: tag check, first-match
            // scan, the incumbent's exact err lines.
            ("field", [v, key]) => {
                self.lower(v, Some(SliceTy::Value))?;
                let hv = self.hold_i32()?;
                self.f.instructions().local_set(hv);
                self.lower(key, Some(STR))?;
                let hk = self.hold_i32()?;
                self.f.instructions().local_set(hk);
                let vf = self.work.helper(Helper::ValueField);
                let hr = self.hold_i32()?;
                self.f.instructions().i32_const(16).call(F_ALLOC).local_set(hr);
                let m_tag = slot_memarg(almide_layout::SUM_TAG);
                let m_pay = slot_memarg(almide_layout::SUM_FIELD);
                let not_obj = self.intern_kind_msgs("expected Object");
                let miss_pre = self.pool.intern("missing field '");
                let miss_post = self.pool.intern("'");
                let ht = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_get(hv).i32_load(m_tag).local_set(ht);
                i.local_get(hv).local_get(hk).call(vf).local_set(hv);
                i.local_get(hv).i32_eqz().if_(BlockType::Empty);
                i.local_get(hr).i32_const(1).i32_store(m_tag);
                i.local_get(hr);
                let _ = i;
                self.emit_kind_msg_select(not_obj, ht);
                let mut i = self.f.instructions();
                i.i32_store(m_pay);
                i.else_();
                i.local_get(hv).i32_const(1).i32_eq().if_(BlockType::Empty);
                i.local_get(hr).i32_const(1).i32_store(m_tag);
                i.local_get(hr);
                i.i32_const(miss_pre as i32).local_get(hk).call(F_CONCAT);
                i.i32_const(miss_post as i32).call(F_CONCAT);
                i.i32_store(m_pay);
                i.else_();
                i.local_get(hr).i32_const(0).i32_store(m_tag);
                i.local_get(hr).local_get(hv).i32_store(m_pay);
                i.end();
                i.end();
                i.local_get(hr);
                self.release_i32();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                Some(SliceTy::Result(
                    self.types.intern(SliceTy::Value),
                    self.types.intern(STR),
                ))
            }
            ("keys", [v]) => {
                self.lower(v, Some(SliceTy::Value))?;
                let vk = self.work.helper(Helper::ValueKeys);
                self.f.instructions().call(vk);
                Some(SliceTy::List(self.types.intern(STR)))
            }
            ("stringify", [v]) => {
                self.lower(v, Some(SliceTy::Value))?;
                self.emit_value_stringify()?;
                Some(STR)
            }
            _ => return Ok(None),
        };
        Ok(Some(out))
    }
}

impl Emitter<'_> {
    /// #1423 bucket A — `json.to_map`: an OBJECT Value's pairs become a
    /// `Map[String, String]` (a Str value's payload verbatim, any other
    /// value stringified — semantics verbatim from
    /// `runtime/rs/src/json.rs::almide_json_to_map`); a non-object is
    /// `none`.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn lower_json_to_map(
        &mut self,
        j: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        self.lower(j, Some(SliceTy::Value))?;
        let ti = self.types.tuple(vec![STR, SliceTy::Value]);
        let def = self.types.tuple_def(ti);
        let (key_off, val_off) = (def.fields[0].1, def.fields[1].1);
        let (koff, voff, esz) = crate::collections::entry_layout(STR, STR);
        let hv = self.hold_i32()?;
        let hp = self.hold_i32()?;
        let hend = self.hold_i32()?;
        let hout = self.hold_i32()?;
        let hw = self.hold_i32()?;
        let hpair = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_set(hv);
            i.local_get(hv)
                .i32_load(slot_memarg(0))
                .i32_const(value_tags::VT_OBJECT)
                .i32_ne()
                .if_(BlockType::Result(wasm_encoder::ValType::I32));
            i.i32_const(almide_layout::NULL_ADDR as i32);
            i.else_();
            // pairs list: n 4-byte slots, each a (key, value) pair block.
            i.local_get(hv).i32_load(slot_memarg(almide_layout::SUM_FIELD)).local_set(hv);
            i.local_get(hv).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hp);
            i.local_get(hp).local_get(hv).i32_load(len_memarg()).i32_add().local_set(hend);
            // out map: len/4 entries of the (String, String) entry layout.
            i.local_get(hv)
                .i32_load(len_memarg())
                .i32_const(2)
                .i32_shr_u()
                .i32_const(esz as i32)
                .i32_mul()
                .call(F_ALLOC)
                .local_set(hout);
            i.local_get(hout).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hw);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hp).local_get(hend).i32_ge_u().br_if(1);
            i.local_get(hp).i32_load(raw_mem()).local_set(hpair);
            // key
            i.local_get(hw).i32_const(koff as i32).i32_add();
            i.local_get(hpair).i32_load(slot_memarg(key_off));
            i.i32_store(raw_mem());
            // value: Str payload verbatim, else the canonical stringify.
            i.local_get(hw).i32_const(voff as i32).i32_add();
            i.local_get(hpair).i32_load(slot_memarg(val_off)).local_set(hpair);
            i.local_get(hpair)
                .i32_load(slot_memarg(0))
                .i32_const(VT_STR)
                .i32_eq()
                .if_(BlockType::Result(wasm_encoder::ValType::I32));
            i.local_get(hpair).i32_load(slot_memarg(almide_layout::SUM_FIELD));
            i.else_();
            i.local_get(hpair);
        }
        self.emit_value_stringify()?;
        {
            let mut i = self.f.instructions();
            i.end();
            i.i32_store(raw_mem());
            i.local_get(hp).i32_const(4).i32_add().local_set(hp);
            i.local_get(hw).i32_const(esz as i32).i32_add().local_set(hw);
            i.br(0).end().end();
            // some(map): the Option block holding the map.
            i.i32_const(4)
                .call(F_ALLOC)
                .local_tee(hv)
                .local_get(hout)
                .i32_store(slot_memarg(almide_layout::OPTION_FIELD));
            i.local_get(hv);
            i.end();
        }
        for _ in 0..6 {
            self.release_i32();
        }
        let sh = self.types.intern(STR);
        let mh = self.types.intern(SliceTy::Map(sh, sh));
        Ok(Some(SliceTy::Option(mh)))
    }
}

/// Raw 4-byte addressing for pointers that already carry the payload
/// offset (the pairs-list walk) — `slot_memarg` would double-shift.
fn raw_mem() -> wasm_encoder::MemArg {
    wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 }
}
