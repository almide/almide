//! Map/Set lowering — insertion-ordered entry blocks (the oracle's
//! semantics), entry layout from `almide_layout::pack_fields`, key lookup
//! through the shared `$scan_*` helpers. Keys and set elements are
//! scalars (equality must be defined); map values are any slice type.
//!
//! Mutation doctrine matches List: binds deep-copy, so `map.insert`'s
//! `mut` form is a var write-back of a functionally-built block. (The
//! functional build copies per insert — quadratic for adversarial loops;
//! upgrade to cap-aware in-place growth if a fixture ever trips it.)

use almide_ir::{IrExpr, IrExprKind};
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

/// Entry layout of a Map[K, V]: (key offset, value offset, stride).
pub(crate) fn entry_layout(k: SliceTy, v: SliceTy) -> (u32, u32, u32) {
    let (offs, size) = almide_layout::pack_fields(&[k.slot_size(), v.slot_size()]);
    (offs[0], offs[1], size)
}

impl Emitter<'_> {
    /// Which `$scan_*` helper compares this key class.
    pub(crate) fn scan_helper(&mut self, k: SliceTy) -> Result<u32, EmitError> {
        match k {
            INT => Ok(F_SCAN_W64),
            BOOL => Ok(F_SCAN_W32),
            STR => Ok(F_SCAN_STR),
            other => unsup(&format!("map-key:{other:?}")),
        }
    }

    pub(crate) fn hold_for(&mut self, t: SliceTy) -> Result<u32, EmitError> {
        match t.val_type() {
            wasm_encoder::ValType::I64 => self.hold_i64(),
            wasm_encoder::ValType::F64 => self.hold_f64(),
            _ => self.hold_i32(),
        }
    }

    pub(crate) fn release_for(&mut self, t: SliceTy) {
        match t.val_type() {
            wasm_encoder::ValType::I64 => self.release_i64(),
            wasm_encoder::ValType::F64 => self.release_f64(),
            _ => self.release_i32(),
        }
    }

    /// Evaluate map + key, run the scan; leaves NOTHING on the stack.
    /// Returns ALL holds explicitly — (map, key, entry) — plus key/val
    /// types and the entry layout. Release order at every call site:
    /// entry (i32), key (its own pool), map (i32).
    #[allow(clippy::type_complexity)]
    fn map_scan(
        &mut self,
        m: &IrExpr,
        key: &IrExpr,
    ) -> Result<(u32, u32, u32, SliceTy, SliceTy, (u32, u32, u32)), EmitError> {
        let (k, v) = match self.lower(m, None)? {
            SliceTy::Map(kh, vh) => (self.types.el(kh), self.types.el(vh)),
            other => return unsup(&format!("map-op-of:{other:?}")),
        };
        let lay = entry_layout(k, v);
        let mh = self.hold_i32()?;
        self.f.instructions().local_set(mh);
        let kh = self.hold_for(k)?;
        self.lower(key, Some(k))?;
        self.f.instructions().local_set(kh);
        let scan = self.scan_helper(k)?;
        let eh = self.hold_i32()?;
        self.f
            .instructions()
            .local_get(mh)
            .i32_const(lay.2 as i32)
            .i32_const(lay.0 as i32)
            .local_get(kh);
        self.f.instructions().call(scan).local_set(eh);
        Ok((mh, kh, eh, k, v, lay))
    }

    /// `r = copy of `base` with `extra` bytes of fresh space at the end`;
    /// leaves the result in a fresh hold. `len_hold` receives base's OLD
    /// live length.
    pub(crate) fn emit_copy_grow(&mut self, base_hold: u32, extra: u32) -> Result<(u32, u32), EmitError> {
        let payload = almide_layout::PAYLOAD as i32;
        let len_hold = self.hold_i32()?;
        let rh = self.hold_i32()?;
        self.f
            .instructions()
            .local_get(base_hold)
            .i32_load(len_memarg())
            .local_tee(len_hold)
            .i32_const(extra as i32)
            .i32_add()
            .call(F_ALLOC)
            .local_set(rh);
        self.f.instructions().local_get(rh).i32_const(payload).i32_add();
        self.f.instructions().local_get(base_hold).i32_const(payload).i32_add();
        self.f.instructions().local_get(len_hold);
        self.f.instructions().memory_copy(0, 0);
        Ok((len_hold, rh))
    }

    pub(crate) fn lower_map_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
        ret_hint: Option<SliceTy>,
    ) -> Result<Option<SliceTy>, EmitError> {
        match (func, args) {
            ("new", []) => {
                let Some(ty @ SliceTy::Map(..)) = ret_hint else {
                    return unsup("map-new-needs-context");
                };
                self.f.instructions().i32_const(0).call(F_ALLOC);
                Ok(Some(ty))
            }
            ("len", [m]) => {
                let (k, v) = match self.lower(m, None)? {
                    SliceTy::Map(kh, vh) => (self.types.el(kh), self.types.el(vh)),
                    other => return unsup(&format!("map-op-of:{other:?}")),
                };
                let stride = entry_layout(k, v).2;
                self.f
                    .instructions()
                    .i32_load(len_memarg())
                    .i32_const(stride as i32)
                    .i32_div_u()
                    .i64_extend_i32_u();
                Ok(Some(INT))
            }
            ("contains", [m, key]) => {
                let (_mh, _kh, eh, k, ..) = self.map_scan(m, key)?;
                self.f.instructions().local_get(eh).i32_const(0).i32_ne();
                self.release_i32(); // eh
                self.release_for(k);
                self.release_i32(); // mh
                Ok(Some(BOOL))
            }
            ("get", [m, key]) => {
                let (_mh, _kh, eh, k, v, lay) = self.map_scan(m, key)?;
                // none, or a fresh some-block holding the value slot.
                self.f
                    .instructions()
                    .local_get(eh)
                    .i32_eqz()
                    .if_(BlockType::Result(wasm_encoder::ValType::I32));
                self.f.instructions().i32_const(almide_layout::NULL_ADDR as i32);
                self.f.instructions().else_();
                let vh = self.hold_i32()?;
                self.f
                    .instructions()
                    .i32_const(v.slot_size() as i32)
                    .call(F_ALLOC)
                    .local_tee(vh);
                self.f.instructions().local_get(eh).i32_const(lay.1 as i32).i32_add();
                self.load_ty_slot_at(v); // eh is ABSOLUTE (inside payload)
                self.store_ty_slot(v, almide_layout::OPTION_FIELD);
                self.f.instructions().local_get(vh);
                self.release_i32(); // vh
                self.f.instructions().end();
                self.release_i32(); // eh
                self.release_for(k);
                self.release_i32(); // mh
                Ok(Some(SliceTy::Option(self.types.intern(v))))
            }
            ("get_or", [m, key, default]) => {
                let (_mh, _kh, eh, k, v, lay) = self.map_scan(m, key)?;
                self.f
                    .instructions()
                    .local_get(eh)
                    .i32_eqz()
                    .if_(BlockType::Result(v.val_type()));
                self.lower(default, Some(v))?;
                self.f.instructions().else_();
                self.f.instructions().local_get(eh).i32_const(lay.1 as i32).i32_add();
                self.load_ty_slot_at(v); // eh is ABSOLUTE (inside payload)
                self.f.instructions().end();
                self.release_i32();
                self.release_for(k);
                self.release_i32();
                Ok(Some(v))
            }
            ("set", [m, key, value]) => {
                let (mh, kh_local, eh, k, v, lay) = self.map_scan(m, key)?;
                let vh = self.hold_for(v)?;
                self.lower(value, Some(v))?;
                self.f.instructions().local_set(vh);
                self.f
                    .instructions()
                    .local_get(eh)
                    .i32_const(0)
                    .i32_ne()
                    .if_(BlockType::Result(wasm_encoder::ValType::I32));
                // overwrite in a copy: dest = r + (e - m) + voff
                let (len_h, rh) = self.emit_copy_grow(mh, 0)?;
                self.f
                    .instructions()
                    .local_get(rh)
                    .local_get(eh)
                    .i32_add()
                    .local_get(mh)
                    .i32_sub()
                    .i32_const(lay.1 as i32)
                    .i32_add()
                    .local_get(vh);
                self.store_ty_slot_raw(v);
                self.f.instructions().local_get(rh);
                let _ = len_h;
                self.release_i32();
                self.release_i32();
                self.f.instructions().else_();
                // append a fresh entry at the old end
                let (len_h2, rh2) = self.emit_copy_grow(mh, lay.2)?;
                self.f
                    .instructions()
                    .local_get(rh2)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(len_h2)
                    .i32_add()
                    .i32_const(lay.0 as i32)
                    .i32_add()
                    .local_get(kh_local);
                self.store_ty_slot_raw(k);
                self.f
                    .instructions()
                    .local_get(rh2)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(len_h2)
                    .i32_add()
                    .i32_const(lay.1 as i32)
                    .i32_add()
                    .local_get(vh);
                self.store_ty_slot_raw(v);
                self.f.instructions().local_get(rh2);
                self.release_i32();
                self.release_i32();
                self.f.instructions().end();
                self.release_for(v);
                self.release_i32(); // eh
                self.release_for(k);
                self.release_i32(); // mh
                Ok(Some(SliceTy::Map(self.types.intern(k), self.types.intern(v))))
            }
            ("insert", [m, _key, _value]) => {
                // mut form: var write-back of the functional build.
                let IrExprKind::Var { id } = &m.kind else {
                    return unsup("map-insert-nonvar");
                };
                let Some(&(var_idx, _)) = self.locals.get(id) else {
                    return unsup("var:unmapped");
                };
                let ret = self.lower_map_call("set", args, ret_hint)?;
                self.f.instructions().local_set(var_idx);
                let _ = ret;
                Ok(None)
            }
            // fold over entries in insertion order: (acc, k, v) => acc'.
            // Insertion-ordered (K, V) pairs — memory order IS the
            // map's insertion order, so a straight walk is exact.
            // keys/values: ONE side of every entry, insertion order.
            ("keys" | "values", [m]) => {
                let keys = func == "keys";
                let (k, v) = match self.lower(m, None)? {
                    SliceTy::Map(kh, vh) => (self.types.el(kh), self.types.el(vh)),
                    other => return unsup(&format!("map-{func}-of:{other:?}")),
                };
                let (koff, voff, esz) = entry_layout(k, v);
                let (side, soff) = if keys { (k, koff) } else { (v, voff) };
                let stride = side.slot_size() as i32;
                let hm = self.hold_i32()?;
                let hcur = self.hold_i32()?;
                let hend = self.hold_i32()?;
                let ho = self.hold_i32()?;
                let hw = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(hm);
                i.local_get(hm).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hcur);
                i.local_get(hcur).local_get(hm).i32_load(len_memarg()).i32_add().local_set(hend);
                i.local_get(hm)
                    .i32_load(len_memarg())
                    .i32_const(esz as i32)
                    .i32_div_u()
                    .i32_const(stride)
                    .i32_mul()
                    .call(F_ALLOC)
                    .local_set(ho);
                i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hw);
                i.block(BlockType::Empty).loop_(BlockType::Empty);
                i.local_get(hcur).local_get(hend).i32_ge_u().br_if(1);
                i.local_get(hw);
                i.local_get(hcur).i32_const(soff as i32).i32_add();
                let _ = i;
                self.load_ty_slot_at(side);
                self.store_ty_slot_at(side);
                let mut i = self.f.instructions();
                i.local_get(hw).i32_const(stride).i32_add().local_set(hw);
                i.local_get(hcur).i32_const(esz as i32).i32_add().local_set(hcur);
                i.br(0).end().end();
                i.local_get(ho);
                let _ = i;
                for _ in 0..5 {
                    self.release_i32();
                }
                Ok(Some(SliceTy::List(self.types.intern(side))))
            }
            ("entries", [m]) => {
                let (kh, vh) = match self.lower(m, None)? {
                    SliceTy::Map(kh, vh) => (kh, vh),
                    other => return unsup(&format!("map-entries-of:{other:?}")),
                };
                let (k, v) = (self.types.el(kh), self.types.el(vh));
                let (offs, esize) = almide_layout::pack_fields(&[k.slot_size(), v.slot_size()]);
                let pair_ti = self.types.tuple(vec![k, v]);
                let pdef = self.types.tuple_def(pair_ti);
                let (pk, pv, psize) = (pdef.fields[0].1, pdef.fields[1].1, pdef.size);
                let hm = self.hold_i32()?;
                let hn = self.hold_i32()?;
                let ho = self.hold_i32()?;
                let hi = self.hold_i32()?;
                let hp = self.hold_i32()?;
                self.f.instructions().local_set(hm);
                self.f
                    .instructions()
                    .local_get(hm)
                    .i32_load(len_memarg())
                    .i32_const(esize as i32)
                    .i32_div_u()
                    .local_set(hn);
                self.f
                    .instructions()
                    .local_get(hn)
                    .i32_const(2)
                    .i32_shl()
                    .call(F_ALLOC)
                    .local_set(ho);
                self.f.instructions().i32_const(0).local_set(hi);
                self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
                self.f.instructions().local_get(hi).local_get(hn).i32_ge_u().br_if(1);
                self.f.instructions().i32_const(psize as i32).call(F_ALLOC).local_set(hp);
                for (src_off, dst_off, t) in [(offs[0], pk, k), (offs[1], pv, v)] {
                    self.f.instructions().local_get(hp);
                    self.f
                        .instructions()
                        .local_get(hm)
                        .local_get(hi)
                        .i32_const(esize as i32)
                        .i32_mul()
                        .i32_add();
                    self.load_ty_slot(t, src_off);
                    self.store_ty_slot(t, dst_off);
                }
                self.f
                    .instructions()
                    .local_get(ho)
                    .local_get(hi)
                    .i32_const(2)
                    .i32_shl()
                    .i32_add()
                    .local_get(hp)
                    .i32_store(slot_memarg(0));
                self.f.instructions().local_get(hi).i32_const(1).i32_add().local_set(hi);
                self.f.instructions().br(0).end().end();
                self.f.instructions().local_get(ho);
                for _ in 0..5 {
                    self.release_i32();
                }
                Ok(Some(SliceTy::List(self.types.intern(SliceTy::Tuple(pair_ti)))))
            }
            ("fold", [m, init, cb]) => {
                let (params, body) = self.hof_lambda(cb, 3)?;
                let (acc_p, k_p, v_p) = (params[0], params[1], params[2]);
                let Some(b) = slice_ty_of(&init.ty, self.types) else {
                    return unsup(&format!("map-fold-acc:{}", ty_name(&init.ty)));
                };
                self.lower(init, Some(b))?;
                self.f.instructions().local_set(acc_p);
                let (k, v) = match self.lower(m, None)? {
                    SliceTy::Map(kh, vh) => (self.types.el(kh), self.types.el(vh)),
                    other => return unsup(&format!("map-fold-of:{other:?}")),
                };
                let (koff, voff, esz) = entry_layout(k, v);
                let bh = self.hold_i32()?;
                let cur = self.hold_i32()?;
                let end = self.hold_i32()?;
                {
                    let mut i = self.f.instructions();
                    i.local_set(bh);
                    i.local_get(bh)
                        .i32_const(almide_layout::PAYLOAD as i32)
                        .i32_add()
                        .local_set(cur);
                    i.local_get(cur).local_get(bh).i32_load(len_memarg()).i32_add().local_set(end);
                    i.block(BlockType::Empty).loop_(BlockType::Empty);
                    i.local_get(cur).local_get(end).i32_ge_u().br_if(1);
                    i.local_get(cur).i32_const(koff as i32).i32_add();
                }
                self.load_ty_slot_at(k);
                self.f.instructions().local_set(k_p);
                self.f.instructions().local_get(cur).i32_const(voff as i32).i32_add();
                self.load_ty_slot_at(v);
                self.f.instructions().local_set(v_p);
                self.lower(body, Some(b))?;
                self.f.instructions().local_set(acc_p);
                {
                    let mut i = self.f.instructions();
                    i.local_get(cur).i32_const(esz as i32).i32_add().local_set(cur);
                    i.br(0);
                    i.end();
                    i.end();
                    i.local_get(acc_p);
                }
                self.release_i32();
                self.release_i32();
                self.release_i32();
                Ok(Some(b))
            }
            ("from_list", [pairs]) => {
                // Insertion-ordered upsert over (K, V) pairs. The result
                // is freshly built and uniquely owned, so the overwrite
                // case may store IN PLACE; the append case copy-grows.
                let (k, v, pair_ti) = match self.lower(pairs, None)? {
                    SliceTy::List(h) => match self.types.el(h) {
                        SliceTy::Tuple(ti) => {
                            let def = self.types.tuple_def(ti);
                            if def.fields.len() != 2 {
                                return unsup("map-from-list-arity");
                            }
                            (def.fields[0].0, def.fields[1].0, ti)
                        }
                        other => return unsup(&format!("map-from-of:{other:?}")),
                    },
                    other => return unsup(&format!("map-from-of:{other:?}")),
                };
                let SliceTy::Scalar(_) = k else { return unsup("map-key-nonscalar") };
                let pair_def = self.types.tuple_def(pair_ti);
                let (koff_p, voff_p) = (pair_def.fields[0].1, pair_def.fields[1].1);
                let lay = entry_layout(k, v);
                let scan = self.scan_helper(k)?;
                let bh = self.hold_i32()?;
                let ch = self.hold_i32()?;
                let ih = self.hold_i32()?;
                let rh = self.hold_i32()?;
                let ph = self.hold_i32()?; // current pair base
                let kh = self.hold_for(k)?;
                let eh = self.hold_i32()?;
                self.f.instructions().local_tee(bh);
                self.f
                    .instructions()
                    .i32_load(len_memarg())
                    .i32_const(4)
                    .i32_div_u()
                    .local_set(ch)
                    .i32_const(0)
                    .local_set(ih)
                    .i32_const(0)
                    .call(F_ALLOC)
                    .local_set(rh);
                self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
                self.f.instructions().local_get(ih).local_get(ch).i32_ge_u().br_if(1);
                // pair base (i32 element of the list)
                self.f
                    .instructions()
                    .local_get(bh)
                    .local_get(ih)
                    .i32_const(4)
                    .i32_mul()
                    .i32_add()
                    .i32_load(slot_memarg(0))
                    .local_set(ph);
                // key (field addr = pair base + PAYLOAD + field offset)
                self.f
                    .instructions()
                    .local_get(ph)
                    .i32_const((almide_layout::PAYLOAD + koff_p) as i32)
                    .i32_add();
                self.load_ty_slot_at(k);
                self.f.instructions().local_set(kh);
                // scan result map
                self.f
                    .instructions()
                    .local_get(rh)
                    .i32_const(lay.2 as i32)
                    .i32_const(lay.0 as i32)
                    .local_get(kh)
                    .call(scan)
                    .local_set(eh);
                self.f.instructions().local_get(eh).i32_const(0).i32_ne().if_(BlockType::Empty);
                // overwrite value in place (uniquely owned)
                self.f.instructions().local_get(eh).i32_const(lay.1 as i32).i32_add();
                self.f
                    .instructions()
                    .local_get(ph)
                    .i32_const((almide_layout::PAYLOAD + voff_p) as i32)
                    .i32_add();
                self.load_ty_slot_at(v);
                self.store_ty_slot_raw(v);
                self.f.instructions().else_();
                let (len_h, nh) = self.emit_copy_grow(rh, lay.2)?;
                self.f
                    .instructions()
                    .local_get(nh)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(len_h)
                    .i32_add()
                    .i32_const(lay.0 as i32)
                    .i32_add()
                    .local_get(kh);
                self.store_ty_slot_raw(k);
                self.f
                    .instructions()
                    .local_get(nh)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(len_h)
                    .i32_add()
                    .i32_const(lay.1 as i32)
                    .i32_add();
                self.f
                    .instructions()
                    .local_get(ph)
                    .i32_const((almide_layout::PAYLOAD + voff_p) as i32)
                    .i32_add();
                self.load_ty_slot_at(v);
                self.store_ty_slot_raw(v);
                self.f.instructions().local_get(nh).local_set(rh);
                self.release_i32();
                self.release_i32();
                self.f.instructions().end();
                self.f
                    .instructions()
                    .local_get(ih)
                    .i32_const(1)
                    .i32_add()
                    .local_set(ih)
                    .br(0)
                    .end()
                    .end();
                self.f.instructions().local_get(rh);
                self.release_i32(); // eh
                self.release_for(k); // kh
                self.release_i32(); // ph
                self.release_i32(); // rh
                self.release_i32(); // ih
                self.release_i32(); // ch
                self.release_i32(); // bh
                Ok(Some(SliceTy::Map(self.types.intern(k), self.types.intern(v))))
            }
            _ => unsup(&format!("call:map.{func}")),
        }
    }

    /// Load a slot whose ABSOLUTE address is already on the stack.
    pub(crate) fn load_ty_slot_at(&mut self, t: SliceTy) {
        let m = wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 };
        match t.val_type() {
            wasm_encoder::ValType::I64 => self.f.instructions().i64_load(m),
            wasm_encoder::ValType::F64 => self.f.instructions().f64_load(m),
            _ => self.f.instructions().i32_load(m),
        };
    }

    /// `[addr, value]` -> store at addr (absolute, offset 0).
    pub(crate) fn store_ty_slot_at(&mut self, t: SliceTy) {
        let m = wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 };
        match t.val_type() {
            wasm_encoder::ValType::I64 => self.f.instructions().i64_store(m),
            wasm_encoder::ValType::F64 => self.f.instructions().f64_store(m),
            _ => self.f.instructions().i32_store(m),
        };
    }

    pub(crate) fn store_ty_slot_raw(&mut self, t: SliceTy) {
        // addr and value already on the stack, offset 0 (absolute addr).
        let m = wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 };
        match t.val_type() {
            wasm_encoder::ValType::I64 => self.f.instructions().i64_store(m),
            wasm_encoder::ValType::F64 => self.f.instructions().f64_store(m),
            _ => self.f.instructions().i32_store(m),
        };
    }

}
