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
    ) -> Result<Option<Option<Lowered>>, EmitError> {
        let out = match (func, args) {
            ("null", []) => {
                self.emit_value_box(VT_NULL, None)?;
                Some(Lowered::owned(SliceTy::Value))
            }
            ("int", [n]) => {
                self.lower_arg(n, Some(INT), ArgMode::Borrow)?;
                self.emit_value_box(VT_INT, Some(INT))?;
                Some(Lowered::owned(SliceTy::Value))
            }
            ("bool", [b]) => {
                self.lower_arg(b, Some(BOOL), ArgMode::Borrow)?;
                self.f.instructions().i64_extend_i32_u();
                self.emit_value_box(VT_BOOL, Some(INT))?;
                Some(Lowered::owned(SliceTy::Value))
            }
            ("float", [x]) => {
                self.lower_arg(x, Some(FLOAT), ArgMode::Borrow)?;
                self.emit_value_box(VT_FLOAT, Some(FLOAT))?;
                Some(Lowered::owned(SliceTy::Value))
            }
            ("string", [s]) => {
                self.lower_arg(s, Some(STR), ArgMode::Retain)?;
                self.emit_value_box(VT_STR, Some(STR))?;
                Some(Lowered::owned(SliceTy::Value))
            }
            ("merge", [va, vb]) => Some(Lowered::owned(self.lower_value_merge(va, vb)?)),
            // pick/omit: keep (drop) the named keys, kept pairs SHARED with
            // the source (native filter+clone of the pair vec — the pair
            // blocks themselves are never copied). Non-object passes through.
            ("pick" | "omit", [v, keys]) => Some(Lowered::owned(self.lower_value_pick_omit(func, v, keys)?)),
            // Object: tag 6, payload = the (String, Value) pairs list —
            // insertion order IS the block, exactly the interp's ordered
            // object model.
            ("object", [pairs]) => {
                // The Value KEEPS the pairs spine (its entries are read
                // through it forever): retained, like `array`'s elements.
                // Declared Borrow, a born-here literal was released under
                // the object the moment list spines became droppable
                // (`{"":null}` across the codec family, #2010 stage 2).
                let got = self.lower_arg(pairs, None, ArgMode::Retain)?;
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
                // No Object holds a key twice (C-401): the pair list goes
                // through the self-host fold (first position, last value)
                // before it is boxed; the fold takes the retained list and
                // answers an owned one.
                let fold = self.value_fold_helper()?;
                self.f.instructions().call(fold);
                self.emit_value_box(VT_OBJECT, Some(STR))?;
                Some(Lowered::owned(SliceTy::Value))
            }
            ("array", [xs]) => {
                let got = self.lower_arg(xs, None, ArgMode::Retain)?;
                let SliceTy::List(h) = got else {
                    return Err(EmitError::Unsupported(format!("value.array-of:{got:?}")));
                };
                if self.types.el(h) != SliceTy::Value {
                    return Err(EmitError::Unsupported("value.array-el".into()));
                }
                self.emit_value_box(VT_ARRAY, Some(STR))?; // addr slot (i32 class)
                Some(Lowered::owned(SliceTy::Value))
            }
            _ => return self.lower_value_call_b(func, args),
        };
        Ok(Some(out))
    }

    /// The pooled JSON/display fragment set.
    /// merge: two Objects merge through the self-host `__vu_merge_pairs`
    /// over their pair lists (#4207: one keyed index per call, O(|a| + |b|),
    /// where the emitted scan per key of b was O(|a|·|b|)); any non-Object
    /// operand yields b itself. Both operands are only READ (#2010 item 5).
    fn lower_value_merge(&mut self, va: &IrExpr, vb: &IrExpr) -> Result<SliceTy, EmitError> {
        let (merge, owned) = self.pairs_helper("__vu_merge_pairs", 2)?;
        self.lower_arg(va, Some(SliceTy::Value), ArgMode::Borrow)?;
        let ha = self.hold_i32()?;
        self.f.instructions().local_set(ha);
        self.lower_arg(vb, Some(SliceTy::Value), ArgMode::Borrow)?;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        let ho = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_get(ha).i32_load(slot_memarg(almide_layout::SUM_TAG)).i32_const(VT_OBJECT).i32_ne();
        i.local_get(hb).i32_load(slot_memarg(almide_layout::SUM_TAG)).i32_const(VT_OBJECT).i32_ne();
        i.i32_or().if_(BlockType::Result(wasm_encoder::ValType::I32));
        i.local_get(hb).call(F_INC);
        i.local_get(hb);
        i.else_();
        // Both operands are Objects on this arm (the tag test above), so the
        // payload slot is each one's pair list: pushed as the helper's
        // argument, with a fresh credit when the callee owns the param.
        for (k, h) in [ha, hb].into_iter().enumerate() {
            i.local_get(h).i32_load(slot_memarg(almide_layout::SUM_FIELD));
            if owned[k] {
                i.local_get(h).i32_load(slot_memarg(almide_layout::SUM_FIELD)).call(F_INC);
            }
        }
        i.call(merge).local_set(ho);
        box_object(&mut i, ho, ha);
        i.end();
        let _ = i;
        for _ in 0..3 {
            self.release_i32();
        }
        Ok(SliceTy::Value)
    }

    /// pick/omit: keep (drop) the named keys through the self-host
    /// `__vu_pick_pairs` / `__vu_omit_pairs` (#4207: the key list hashed once,
    /// O(n + k), where a scan of the key list per pair was O(n·k)).
    /// Non-object: pick answers `{}` (C-401), omit passes it through.
    fn lower_value_pick_omit(&mut self, func: &str, v: &IrExpr, keys: &IrExpr) -> Result<SliceTy, EmitError> {
        let pick = func == "pick";
        let (sel, owned) = self.pairs_helper(if pick { "__vu_pick_pairs" } else { "__vu_omit_pairs" }, 2)?;
        // #2010 item 5: the source is only READ — every block the result
        // shares with it (a passed-through non-object) takes its own credit.
        self.lower_arg(v, Some(SliceTy::Value), ArgMode::Borrow)?;
        let hv = self.hold_i32()?;
        self.f.instructions().local_set(hv);
        match self.lower_arg(keys, None, ArgMode::Borrow)? {
            SliceTy::List(h) if self.types.el(h) == STR => {}
            other => return Err(EmitError::Unsupported(format!("value.{func}-keys:{other:?}"))),
        }
        let hk = self.hold_i32()?;
        self.f.instructions().local_set(hk);
        let ho = self.hold_i32()?;
        let hp = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_get(hv)
            .i32_load(slot_memarg(almide_layout::SUM_TAG))
            .i32_const(VT_OBJECT)
            .i32_ne()
            .if_(BlockType::Result(wasm_encoder::ValType::I32));
        if pick {
            // pick: a non-object has none of the keys, so it picks to the
            // empty object (C-401) — never the value back whole.
            i.i32_const(0).call(F_ALLOC).local_set(ho);
            i.local_get(ho).i32_const(0).i32_store(len_memarg());
            box_object(&mut i, ho, hp);
        } else {
            i.local_get(hv).call(F_INC);
            i.local_get(hv);
        }
        i.else_();
        // An Object on this arm (the tag test above): its pair list.
        i.local_get(hv).i32_load(slot_memarg(almide_layout::SUM_FIELD));
        if owned[0] {
            i.local_get(hv).i32_load(slot_memarg(almide_layout::SUM_FIELD)).call(F_INC);
        }
        i.local_get(hk);
        if owned[1] {
            i.local_get(hk).call(F_INC);
        }
        i.call(sel).local_set(ho);
        box_object(&mut i, ho, hp);
        i.end();
        let _ = i;
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(SliceTy::Value)
    }

    /// A linked self-host pair-list helper of `arity` list params: its wasm
    /// index and which params the callee owns (an owned param is handed a
    /// fresh credit, the callee's epilogue releases it).
    fn pairs_helper(&mut self, name: &str, arity: usize) -> Result<(u32, Vec<bool>), EmitError> {
        let Some(hi) = self.selfhost_helper(name) else {
            return Err(EmitError::Unsupported(format!("value:unlinked:{name}")));
        };
        if let Some(r) = &self.table.infos[hi].refuse {
            return Err(EmitError::Unsupported(format!("call-fn:{name}:{r}")));
        }
        self.calls.insert(hi);
        let info = &self.table.infos[hi];
        let owned = (0..arity).map(|k| info.param_owned.get(k).copied().unwrap_or(true)).collect();
        Ok((info.wasm_index, owned))
    }

    /// `__vu_fold_pairs` (stdlib/value_utils.almd): the duplicate-key fold
    /// every Object construction goes through (C-401).
    fn value_fold_helper(&mut self) -> Result<u32, EmitError> {
        let Some(hi) = self.selfhost_helper("__vu_fold_pairs") else {
            return Err(EmitError::Unsupported("value.object:unlinked:__vu_fold_pairs".into()));
        };
        if let Some(r) = &self.table.infos[hi].refuse {
            return Err(EmitError::Unsupported(format!("call-fn:__vu_fold_pairs:{r}")));
        }
        self.calls.insert(hi);
        Ok(self.table.infos[hi].wasm_index)
    }

    /// A self-host module's PRIVATE helper by unique `.name` suffix —
    /// registry modules carry ordinal names (`__selfhost_N`), so the
    /// qualified key is not spellable up front. Ambiguity walls.
    fn selfhost_helper(&self, helper: &str) -> Option<usize> {
        let suffix = format!(".{helper}");
        let mut hits = self.table.by_name.iter().filter(|(k, _)| k.ends_with(&suffix));
        let first = hits.next()?;
        if hits.next().is_some() {
            return None;
        }
        Some(*first.1)
    }

    pub(crate) fn json_ctrl_frags(&mut self) -> crate::work::CtrlFrags {
        crate::work::CtrlFrags {
            esc_u00: self.pool.intern("\\u00"),
            hex_digits: self.pool.intern("0123456789abcdef"),
        }
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
        let ctrl = self.json_ctrl_frags();
        let _ = self.work.helper(Helper::JsonQuote { frags, ctrl });
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
        let ctrl = self.json_ctrl_frags();
        let _ = self.work.helper(Helper::JsonQuote { frags, ctrl });
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
            .map(|k| self.pool.intern(&format!("{prefix}, received {k}")))
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
        // The Value keeps its interior: the Result's payload is a SHARE of
        // it (+1), released by the Result's typed drop (#2010 stage 2c —
        // `value.as_array` twice on one Value freed its array).
        self.share_handle_top(payload);
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
    ) -> Result<Option<Option<Lowered>>, EmitError> {
        let out = match (func, args) {
            ("as_int", [v]) => {
                self.lower_arg(v, Some(SliceTy::Value), ArgMode::Borrow)?;
                self.emit_value_unbox(VT_INT, INT, "expected Int")?;
                Some(Lowered::owned(SliceTy::Result(self.types.intern(INT), self.types.intern(STR))))
            }
            ("as_bool", [v]) => {
                self.lower_arg(v, Some(SliceTy::Value), ArgMode::Borrow)?;
                self.emit_value_unbox(VT_BOOL, BOOL, "expected Bool")?;
                Some(Lowered::owned(SliceTy::Result(self.types.intern(BOOL), self.types.intern(STR))))
            }
            ("as_string", [v]) => {
                self.lower_arg(v, Some(SliceTy::Value), ArgMode::Borrow)?;
                self.emit_value_unbox(VT_STR, STR, "expected Str")?;
                Some(Lowered::owned(SliceTy::Result(self.types.intern(STR), self.types.intern(STR))))
            }
            ("as_array", [v]) => {
                self.lower_arg(v, Some(SliceTy::Value), ArgMode::Borrow)?;
                let lv = SliceTy::List(self.types.intern(SliceTy::Value));
                self.emit_value_unbox(VT_ARRAY, lv, "expected Array")?;
                Some(Lowered::owned(SliceTy::Result(self.types.intern(lv), self.types.intern(STR))))
            }
            // as_object (#4220): an Object's payload IS its (String, Value)
            // pairs list (the `object` ctor boxes it), so the Ok payload is a
            // share of that list — as_array's unbox at the pair element type.
            ("as_object", [v]) => {
                self.lower_arg(v, Some(SliceTy::Value), ArgMode::Borrow)?;
                let ti = self.types.tuple(vec![STR, SliceTy::Value]);
                let lp = SliceTy::List(self.types.intern(SliceTy::Tuple(ti)));
                self.emit_value_unbox(VT_OBJECT, lp, "expected Object")?;
                Some(Lowered::owned(SliceTy::Result(self.types.intern(lp), self.types.intern(STR))))
            }
            // #658: a JSON number has no int/float split — an Int Value
            // widens to a valid Float.
            ("as_float", [v]) => {
                self.lower_arg(v, Some(SliceTy::Value), ArgMode::Borrow)?;
                self.emit_value_as_float()?;
                Some(Lowered::owned(SliceTy::Result(self.types.intern(FLOAT), self.types.intern(STR))))
            }
            // The Codec-derive field accessor: tag check, first-match
            // scan, the incumbent's exact err lines.
            ("__field", [v, key]) => {
                self.lower_arg(v, Some(SliceTy::Value), ArgMode::Borrow)?;
                let hv = self.hold_i32()?;
                self.f.instructions().local_set(hv);
                self.lower_arg(key, Some(STR), ArgMode::Borrow)?;
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
                // The key is escaped by C-418's rule (#4126), as on native.
                let et = self.work.helper(Helper::ErrText);
                let he = self.hold_i32()?;
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
                // The key is no longer needed. Park the intermediate
                // prefix so the second concatenation can release it.
                i.local_get(hk).call(et).local_set(he);
                i.i32_const(miss_pre as i32).local_get(he).call(F_CONCAT).local_tee(hk);
                i.i32_const(miss_post as i32).call(F_CONCAT);
                i.i32_store(m_pay);
                i.local_get(hk).call(F_DEC_FLAT);
                i.local_get(he).call(F_DEC_FLAT);
                i.else_();
                i.local_get(hr).i32_const(0).i32_store(m_tag);
                // The object keeps the field's Value: the ok payload is a
                // share of it (#2010 item 5 — the Result's typed drop
                // releases it).
                i.local_get(hv).call(F_INC);
                i.local_get(hr).local_get(hv).i32_store(m_pay);
                i.end();
                i.end();
                i.local_get(hr);
                self.release_i32();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                Some(Lowered::owned(SliceTy::Result(
                    self.types.intern(SliceTy::Value),
                    self.types.intern(STR),
                )))
            }
            ("__keys", [v]) => {
                self.lower_arg(v, Some(SliceTy::Value), ArgMode::Borrow)?;
                let vk = self.work.helper(Helper::ValueKeys);
                // The keys are COPIES of the object's key handles: the list
                // takes its own credits, as `map.keys` does (#2010 stage 2b).
                // Its typed drop releases every element, so without the inc
                // each dropped key list spent one of the Value's own credits
                // — `value.__keys(v)` released twice freed the keys under `v`.
                let hl = self.hold_i32()?;
                self.f.instructions().call(vk).local_set(hl);
                self.emit_inc_elems(hl, STR);
                self.f.instructions().local_get(hl);
                self.release_i32();
                Some(Lowered::owned(SliceTy::List(self.types.intern(STR))))
            }
            ("__stringify", [v]) => {
                self.lower_arg(v, Some(SliceTy::Value), ArgMode::Borrow)?;
                self.emit_value_stringify()?;
                Some(Lowered::owned(STR))
            }
            _ => return Ok(None),
        };
        Ok(Some(out))
    }
}

/// Box the owned pair list in `pairs` as a fresh Object Value and leave it on
/// the stack (`scratch` holds the box).
fn box_object(i: &mut wasm_encoder::InstructionSink, pairs: u32, scratch: u32) {
    i.i32_const(16).call(F_ALLOC).local_set(scratch);
    i.local_get(scratch).i32_const(VT_OBJECT).i32_store(slot_memarg(almide_layout::SUM_TAG));
    i.local_get(scratch).local_get(pairs).i32_store(slot_memarg(almide_layout::SUM_FIELD));
    i.local_get(scratch);
}
