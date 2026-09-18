//! Map HOF surfaces (find/filter/map/merge) — split from collections.rs
//! for the file budget. Callbacks run ONCE per entry (observable
//! effects), so filter OVER-allocates and patches the len header down
//! rather than running a counting pass.

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::collections::entry_layout;
use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// Shared prologue: lower the map, return (mh, k, v, layout).
    fn map_hof_open(&mut self, m: &IrExpr) -> Result<(u32, SliceTy, SliceTy), EmitError> {
        let (k, v) = match self.lower_arg(m, None, ArgMode::Borrow)? {
            SliceTy::Map(kh, vh) => (self.types.el(kh), self.types.el(vh)),
            other => return unsup(&format!("map-hof-of:{other:?}")),
        };
        let mh = self.hold_i32()?;
        self.f.instructions().local_set(mh);
        Ok((mh, k, v))
    }

    /// First matching entry as some((K, V)) — insertion order.
    pub(crate) fn lower_map_find(
        &mut self,
        m: &IrExpr,
        cb: &IrExpr,
    ) -> ArmResult {
        let (params, body) = self.hof_lambda(cb, 2)?;
        let (mh, k, v) = self.map_hof_open(m)?;
        let (koff, voff, esz) = entry_layout(k, v);
        let pair_ti = self.types.tuple(vec![k, v]);
        let pdef = self.types.tuple_def(pair_ti);
        let (pk, pv, psize) = (pdef.fields[0].1, pdef.fields[1].1, pdef.size);
        let hcur = self.hold_i32()?;
        let hend = self.hold_i32()?;
        let hr = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.i32_const(0).local_set(hr);
            i.local_get(mh).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hcur);
            i.local_get(hcur).local_get(mh).i32_load(len_memarg()).i32_add().local_set(hend);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hcur).local_get(hend).i32_ge_u().br_if(1);
            i.local_get(hcur).i32_const(koff as i32).i32_add();
        }
        self.load_ty_slot_at(k);
        self.f.instructions().local_set(params[0]);
        self.f.instructions().local_get(hcur).i32_const(voff as i32).i32_add();
        self.load_ty_slot_at(v);
        self.f.instructions().local_set(params[1]);
        self.lower(body, Some(BOOL))?;
        self.f.instructions().if_(BlockType::Empty);
        // some((k, v)): the pair block, then the option cell — the pair
        // co-owns the handles it copied out of the entry
        self.f.instructions().i32_const(psize as i32).call(F_ALLOC).local_set(hr);
        self.f.instructions().local_get(hr).local_get(params[0]);
        self.share_handle_top(k);
        self.store_ty_slot(k, pk);
        self.f.instructions().local_get(hr).local_get(params[1]);
        self.share_handle_top(v);
        self.store_ty_slot(v, pv);
        self.f.instructions().i32_const(4).call(F_ALLOC).local_tee(hend).local_get(hr);
        self.f.instructions().i32_store(slot_memarg(almide_layout::OPTION_FIELD));
        self.f.instructions().local_get(hend).local_set(hr);
        self.f.instructions().br(2);
        self.f.instructions().end();
        {
            let mut i = self.f.instructions();
            i.local_get(hcur).i32_const(esz as i32).i32_add().local_set(hcur);
            i.br(0).end().end();
            i.local_get(hr);
        }
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::Option(self.types.intern(SliceTy::Tuple(pair_ti))))))
    }

    /// Kept entries, order preserved; the out block over-allocates and
    /// its len header patches down to the written bytes.
    pub(crate) fn lower_map_filter(
        &mut self,
        m: &IrExpr,
        cb: &IrExpr,
    ) -> ArmResult {
        let (params, body) = self.hof_lambda(cb, 2)?;
        let (mh, k, v) = self.map_hof_open(m)?;
        let (koff, voff, esz) = entry_layout(k, v);
        let hcur = self.hold_i32()?;
        let hend = self.hold_i32()?;
        let ho = self.hold_i32()?;
        let hw = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(mh).i32_load(len_memarg()).call(F_ALLOC).local_set(ho);
            i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hw);
            i.local_get(mh).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hcur);
            i.local_get(hcur).local_get(mh).i32_load(len_memarg()).i32_add().local_set(hend);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hcur).local_get(hend).i32_ge_u().br_if(1);
            i.local_get(hcur).i32_const(koff as i32).i32_add();
        }
        self.load_ty_slot_at(k);
        self.f.instructions().local_set(params[0]);
        self.f.instructions().local_get(hcur).i32_const(voff as i32).i32_add();
        self.load_ty_slot_at(v);
        self.f.instructions().local_set(params[1]);
        self.lower(body, Some(BOOL))?;
        {
            let mut i = self.f.instructions();
            i.if_(BlockType::Empty);
            i.local_get(hw).local_get(hcur).i32_const(esz as i32);
            i.memory_copy(0, 0);
            i.local_get(hw).i32_const(esz as i32).i32_add().local_set(hw);
            i.end();
            i.local_get(hcur).i32_const(esz as i32).i32_add().local_set(hcur);
            i.br(0).end().end();
            // len = written bytes
            i.local_get(ho);
            i.local_get(hw).local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add().i32_sub();
            i.i32_store(len_memarg());
        }
        // the kept entries are copies: the out map takes their credits
        let mt = SliceTy::Map(self.types.intern(k), self.types.intern(v));
        self.emit_inc_entries(ho, mt, None);
        self.f.instructions().local_get(ho);
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(mt)))
    }

    /// #1423 stage 4 — the map predicate family: all / any (early-exit
    /// Bool) and count (a matching-entry counter), the filter loop's
    /// (k, v) callback protocol without the output map.
    pub(crate) fn lower_map_pred(
        &mut self,
        func: &str,
        m: &IrExpr,
        cb: &IrExpr,
    ) -> ArmResult {
        let (params, body) = self.hof_lambda(cb, 2)?;
        let (mh, k, v) = self.map_hof_open(m)?;
        let (koff, voff, esz) = entry_layout(k, v);
        let hcur = self.hold_i32()?;
        let hend = self.hold_i32()?;
        let hr = self.hold_i32()?;
        let count = func == "count";
        {
            let mut i = self.f.instructions();
            // all starts true; any starts false; count starts 0.
            i.i32_const(if func == "all" { 1 } else { 0 }).local_set(hr);
            i.local_get(mh).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hcur);
            i.local_get(hcur).local_get(mh).i32_load(len_memarg()).i32_add().local_set(hend);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hcur).local_get(hend).i32_ge_u().br_if(1);
            i.local_get(hcur).i32_const(koff as i32).i32_add();
        }
        self.load_ty_slot_at(k);
        self.f.instructions().local_set(params[0]);
        self.f.instructions().local_get(hcur).i32_const(voff as i32).i32_add();
        self.load_ty_slot_at(v);
        self.f.instructions().local_set(params[1]);
        self.lower(body, Some(BOOL))?;
        {
            let mut i = self.f.instructions();
            match func {
                "all" => {
                    // a false verdict decides — flip and break.
                    i.i32_eqz().if_(BlockType::Empty);
                    i.i32_const(0).local_set(hr);
                    i.br(2);
                    i.end();
                }
                "any" => {
                    i.if_(BlockType::Empty);
                    i.i32_const(1).local_set(hr);
                    i.br(2);
                    i.end();
                }
                _ => {
                    i.if_(BlockType::Empty);
                    i.local_get(hr).i32_const(1).i32_add().local_set(hr);
                    i.end();
                }
            }
            i.local_get(hcur).i32_const(esz as i32).i32_add().local_set(hcur);
            i.br(0).end().end();
            i.local_get(hr);
            if count {
                i.i64_extend_i32_u();
            }
        }
        for _ in 0..4 {
            self.release_i32();
        }
        Ok(Some(Lowered::scalar(if count { INT } else { BOOL })))
    }

    /// Value transform (the 1-arg surface): keys copy, the value slot
    /// takes the callback's result — the OUT entry layout re-packs for
    /// the new value type.
    pub(crate) fn lower_map_map(
        &mut self,
        m: &IrExpr,
        cb: &IrExpr,
    ) -> ArmResult {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (mh, k, v) = self.map_hof_open(m)?;
        let (koff, voff, esz) = entry_layout(k, v);
        let hcur = self.hold_i32()?;
        let hend = self.hold_i32()?;
        let ho = self.hold_i32()?;
        let hw = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(mh).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hcur);
            i.local_get(hcur).local_get(mh).i32_load(len_memarg()).i32_add().local_set(hend);
        }
        // First entry decides nothing statically — the callback's type
        // comes from its body, so lower one probe? No: the body's type is
        // known per-iteration; alloc after computing OUT esz needs it
        // up-front. Lower the loop with the value type discovered on the
        // FIRST body lowering is unsound for empty maps — instead the
        // body lowers inside the loop and the out layout derives from
        // the CHECKER type of the callback body.
        let b_ty = {
            let prev = self.f.instructions();
            let _ = prev;
            self.infer(body)?
        };
        let (okoff, ovoff, oesz) = entry_layout(k, b_ty);
        {
            let mut i = self.f.instructions();
            i.local_get(mh)
                .i32_load(len_memarg())
                .i32_const(esz as i32)
                .i32_div_u()
                .i32_const(oesz as i32)
                .i32_mul()
                .call(F_ALLOC)
                .local_set(ho);
            i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hw);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hcur).local_get(hend).i32_ge_u().br_if(1);
            i.local_get(hcur).i32_const(voff as i32).i32_add();
        }
        self.load_ty_slot_at(v);
        self.f.instructions().local_set(params[0]);
        // key copies through (a handle key is co-owned by the out map)
        self.f.instructions().local_get(hw).i32_const(okoff as i32).i32_add();
        self.f.instructions().local_get(hcur).i32_const(koff as i32).i32_add();
        self.load_ty_slot_at(k);
        self.share_handle_top(k);
        self.store_ty_slot_at(k);
        self.f.instructions().local_get(hw).i32_const(ovoff as i32).i32_add();
        let got = self.lower(body, Some(b_ty))?;
        // a borrowed callback result (`(v) => v`) takes its +1; an owned
        // one moves into the slot
        self.rc_share_guard(body, got);
        self.store_ty_slot_at(got);
        {
            let mut i = self.f.instructions();
            i.local_get(hw).i32_const(oesz as i32).i32_add().local_set(hw);
            i.local_get(hcur).i32_const(esz as i32).i32_add().local_set(hcur);
            i.br(0).end().end();
            i.local_get(ho);
        }
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::Map(self.types.intern(k), self.types.intern(b_ty)))))
    }

    /// Upsert merge (native `a.clone()` + inserts): a's entries keep
    /// their positions (B's value wins on a shared key), b's new keys
    /// append in b order.
    pub(crate) fn lower_map_merge(
        &mut self,
        a: &IrExpr,
        b: &IrExpr,
    ) -> ArmResult {
        let (ah, k, v) = self.map_hof_open(a)?;
        let (bk, bv) = match self.lower_arg(b, None, ArgMode::Borrow)? {
            SliceTy::Map(kh, vh) => (self.types.el(kh), self.types.el(vh)),
            other => return unsup(&format!("map-merge-of:{other:?}")),
        };
        if bk != k || bv != v {
            return unsup("map-merge-ty");
        }
        let scan = self.map_scan_fn(k)?;
        let (koff, voff, esz) = entry_layout(k, v);
        let bh = self.hold_i32()?;
        self.f.instructions().local_set(bh);
        let hcur = self.hold_i32()?;
        let hend = self.hold_i32()?;
        let ho = self.hold_i32()?;
        let hw = self.hold_i32()?;
        let he = self.hold_i32()?;
        let hkey = self.hold_for(k)?;
        {
            let mut i = self.f.instructions();
            // pass 0: count b's NEW keys (scan-only, no callbacks) → hw
            i.i32_const(0).local_set(hw);
            i.local_get(bh).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hcur);
            i.local_get(hcur).local_get(bh).i32_load(len_memarg()).i32_add().local_set(hend);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hcur).local_get(hend).i32_ge_u().br_if(1);
            i.local_get(hcur).i32_const(koff as i32).i32_add();
        }
        self.load_ty_slot_at(k);
        {
            let mut i = self.f.instructions();
            i.local_set(hkey);
            i.local_get(ah).i32_const(esz as i32).i32_const(koff as i32).local_get(hkey);
            i.call(scan).i32_eqz().if_(BlockType::Empty);
            i.local_get(hw).i32_const(esz as i32).i32_add().local_set(hw);
            i.end();
            i.local_get(hcur).i32_const(esz as i32).i32_add().local_set(hcur);
            i.br(0).end().end();
            // out = a wholesale + room for the new keys
            i.local_get(ah).i32_load(len_memarg()).local_get(hw).i32_add();
            i.call(F_ALLOC).local_set(ho);
            i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(ah).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(ah).i32_load(len_memarg());
            i.memory_copy(0, 0);
            // append cursor after a's copy
            i.local_get(ho)
                .i32_const(almide_layout::PAYLOAD as i32)
                .i32_add()
                .local_get(ah)
                .i32_load(len_memarg())
                .i32_add()
                .local_set(hw);
            // pass 1: upsert each b entry
            i.local_get(bh).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hcur);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hcur).local_get(hend).i32_ge_u().br_if(1);
            i.local_get(hcur).i32_const(koff as i32).i32_add();
        }
        self.load_ty_slot_at(k);
        {
            let mut i = self.f.instructions();
            i.local_set(hkey);
            i.local_get(ah).i32_const(esz as i32).i32_const(koff as i32).local_get(hkey);
            i.call(scan).local_tee(he).if_(BlockType::Empty);
            // shared key: b's VALUE lands at the SAME offset in out
            i.local_get(he)
                .local_get(ah)
                .i32_const(almide_layout::PAYLOAD as i32)
                .i32_add()
                .i32_sub()
                .local_set(he);
            i.local_get(ho)
                .i32_const(almide_layout::PAYLOAD as i32)
                .i32_add()
                .local_get(he)
                .i32_add()
                .i32_const(voff as i32)
                .i32_add();
            i.local_get(hcur).i32_const(voff as i32).i32_add();
        }
        self.load_ty_slot_at(v);
        self.store_ty_slot_at(v);
        {
            let mut i = self.f.instructions();
            i.else_();
            // new key: the whole entry appends
            i.local_get(hw).local_get(hcur).i32_const(esz as i32);
            i.memory_copy(0, 0);
            i.local_get(hw).i32_const(esz as i32).i32_add().local_set(hw);
            i.end();
            i.local_get(hcur).i32_const(esz as i32).i32_add().local_set(hcur);
            i.br(0).end().end();
        }
        // every out entry is a copy (a's, or b's value / entry): the out
        // map takes their credits once the walk has settled them
        let mt = SliceTy::Map(self.types.intern(k), self.types.intern(v));
        self.emit_inc_entries(ho, mt, None);
        self.f.instructions().local_get(ho);
        self.release_for(k);
        for _ in 0..7 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(mt)))
    }

    /// Copy; a present key's value passes through the callback ONCE
    /// (native `if let Some(v) = get { insert(f(v)) }` — absent = copy).
    pub(crate) fn lower_map_update(
        &mut self,
        m: &IrExpr,
        key: &IrExpr,
        cb: &IrExpr,
    ) -> ArmResult {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (mh, k, v) = self.map_hof_open(m)?;
        let scan = self.map_scan_fn(k)?;
        let (koff, voff, esz) = entry_layout(k, v);
        let _ = koff;
        let hkey = self.hold_for(k)?;
        self.lower_arg(key, Some(k), ArgMode::Borrow)?;
        self.f.instructions().local_set(hkey);
        let ho = self.hold_i32()?;
        let he = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            // out = wholesale copy
            i.local_get(mh).i32_load(len_memarg()).call(F_ALLOC).local_set(ho);
            i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(mh).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(mh).i32_load(len_memarg());
            i.memory_copy(0, 0);
        }
        // the copy holds its own entry credits
        let mt = SliceTy::Map(self.types.intern(k), self.types.intern(v));
        self.emit_inc_entries(ho, mt, None);
        {
            let mut i = self.f.instructions();
            i.local_get(mh)
                .i32_const(esz as i32)
                .i32_const(entry_layout(k, v).0 as i32)
                .local_get(hkey);
            i.call(scan).local_tee(he).if_(BlockType::Empty);
            // he → the value slot's OFFSET, replayed into out
            i.local_get(he)
                .local_get(mh)
                .i32_const(almide_layout::PAYLOAD as i32)
                .i32_add()
                .i32_sub()
                .local_set(he);
            i.local_get(he).i32_const(voff as i32).i32_add();
            i.local_get(mh).i32_const(almide_layout::PAYLOAD as i32).i32_add().i32_add();
        }
        self.load_ty_slot_at(v);
        self.f.instructions().local_set(params[0]);
        self.emit_replace_out_value(ho, he, voff, body, v)?;
        self.f.instructions().end();
        self.f.instructions().local_get(ho);
        self.release_i32();
        self.release_i32();
        self.release_for(k);
        self.release_i32();
        Ok(Some(Lowered::owned(mt)))
    }

    /// The callback's value replaces the entry value at offset `he` of
    /// the out copy `ho`: a borrowed result takes +1, the replaced
    /// value's credit (the copy's own) is released, then the store.
    fn emit_replace_out_value(
        &mut self,
        ho: u32,
        he: u32,
        voff: u32,
        body: &IrExpr,
        v: SliceTy,
    ) -> Result<(), EmitError> {
        let hnew = self.hold_for(v)?;
        self.lower(body, Some(v))?;
        self.rc_share_guard(body, v);
        self.f.instructions().local_set(hnew);
        for _ in 0..2 {
            self.f
                .instructions()
                .local_get(ho)
                .i32_const(almide_layout::PAYLOAD as i32)
                .i32_add()
                .local_get(he)
                .i32_add()
                .i32_const(voff as i32)
                .i32_add();
        }
        self.emit_release_slot_at(v);
        self.f.instructions().local_get(hnew);
        self.store_ty_slot_at(v);
        self.release_for(v);
        Ok(())
    }

    /// Upsert (native): a present key keeps its POSITION and its value
    /// passes through the callback; an absent key APPENDS (k, init).
    /// `init` is evaluated eagerly either way (Rust call-site order).
    pub(crate) fn lower_map_upsert(
        &mut self,
        m: &IrExpr,
        key: &IrExpr,
        init: &IrExpr,
        cb: &IrExpr,
    ) -> ArmResult {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (mh, k, v) = self.map_hof_open(m)?;
        let scan = self.map_scan_fn(k)?;
        let (koff, voff, esz) = entry_layout(k, v);
        let hkey = self.hold_for(k)?;
        self.lower_arg(key, Some(k), ArgMode::Retain)?;
        self.f.instructions().local_set(hkey);
        let hinit = self.hold_for(v)?;
        self.lower_arg(init, Some(v), ArgMode::Retain)?;
        self.f.instructions().local_set(hinit);
        let ho = self.hold_i32()?;
        let he = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            // out = wholesale copy, over-allocated by one entry for the
            // append case; each branch patches len to its own truth.
            i.local_get(mh)
                .i32_load(len_memarg())
                .i32_const(esz as i32)
                .i32_add()
                .call(F_ALLOC)
                .local_set(ho);
            i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(mh).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(mh).i32_load(len_memarg());
            i.memory_copy(0, 0);
        }
        // the copied prefix holds its own entry credits (the over-allocated
        // tail entry is not walked)
        let mt = SliceTy::Map(self.types.intern(k), self.types.intern(v));
        let hn = self.hold_i32()?;
        self.f.instructions().local_get(mh).i32_load(len_memarg()).local_set(hn);
        self.emit_inc_entries(ho, mt, Some(hn));
        self.release_i32();
        {
            let mut i = self.f.instructions();
            i.local_get(mh).i32_const(esz as i32).i32_const(koff as i32).local_get(hkey);
            i.call(scan).local_tee(he).if_(BlockType::Empty);
            i.local_get(ho).local_get(mh).i32_load(len_memarg()).i32_store(len_memarg());
            // he → the entry's OFFSET, replayed into out
            i.local_get(he)
                .local_get(mh)
                .i32_const(almide_layout::PAYLOAD as i32)
                .i32_add()
                .i32_sub()
                .local_set(he);
            i.local_get(he).i32_const(voff as i32).i32_add();
            i.local_get(mh).i32_const(almide_layout::PAYLOAD as i32).i32_add().i32_add();
        }
        self.load_ty_slot_at(v);
        self.f.instructions().local_set(params[0]);
        self.emit_replace_out_value(ho, he, voff, body, v)?;
        // present: the Retain credits of the unstored key and init go back
        self.emit_release_hold(hkey, k);
        self.emit_release_hold(hinit, v);
        {
            let mut i = self.f.instructions();
            i.else_();
            i.local_get(ho)
                .local_get(mh)
                .i32_load(len_memarg())
                .i32_const(esz as i32)
                .i32_add()
                .i32_store(len_memarg());
            i.local_get(ho).local_get(mh).i32_load(len_memarg()).i32_add();
            i.local_get(hkey);
        }
        self.store_ty_slot(k, koff);
        {
            let mut i = self.f.instructions();
            i.local_get(ho).local_get(mh).i32_load(len_memarg()).i32_add();
            i.local_get(hinit);
        }
        self.store_ty_slot(v, voff);
        self.f.instructions().end();
        self.f.instructions().local_get(ho);
        self.release_i32();
        self.release_i32();
        self.release_for(v);
        self.release_for(k);
        self.release_i32();
        Ok(Some(Lowered::owned(mt)))
    }

    /// `list.group_by(xs, f) -> Map[B, List[A]]` — first-seen key order,
    /// grouped lists in traversal order. The map block is grown by a
    /// local realloc-append (we own it; nothing else observes the moves),
    /// and the per-key list slot is patched in place through the fresh
    /// scan address each round.
    pub(crate) fn lower_list_group_by(
        &mut self,
        xs: &IrExpr,
        cb: &IrExpr,
    ) -> ArmResult {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let kt = self.infer(body)?;
        let SliceTy::Scalar(_) = kt else { return unsup("list-group-by-key-nonscalar") };
        // the accumulator copy-grows per new key (a fresh address each
        // time): the plain scan, never the index lane
        let scan = self.scan_helper(kt)?;
        let inner = SliceTy::List(self.types.intern(elem));
        let (koff, voff, esz) = entry_layout(kt, inner);
        let push = match elem.slot_size() {
            8 => F_LIST_PUSH_8,
            _ => F_LIST_PUSH_4,
        };
        let hm = self.hold_i32()?;
        let he = self.hold_i32()?;
        let hkey = self.hold_for(kt)?;
        {
            let mut i = self.f.instructions();
            i.i32_const(0).call(F_ALLOC).local_set(hm);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
        }
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        self.lower(body, Some(kt))?;
        // The key the callback produced: stored on the absent path (a
        // borrowed one takes +1 there), dropped on the present path (an
        // owned one is released there).
        let key_owned = self.rc_owned_result(body);
        {
            let mut i = self.f.instructions();
            i.local_set(hkey);
            i.local_get(hm).i32_const(esz as i32).i32_const(koff as i32).local_get(hkey);
            i.call(scan).local_tee(he).if_(BlockType::Empty);
        }
        if key_owned {
            self.emit_release_hold(hkey, kt);
        }
        {
            let mut i = self.f.instructions();
            // present: push into the entry's list, patch the slot back
            i.local_get(he);
            i.local_get(he).i32_load(MemArg { offset: u64::from(voff), align: 2, memory_index: 0 });
            i.local_get(params[0]);
        }
        // the group list co-owns the element it copied out of xs
        self.share_handle_top(elem);
        if elem.val_type() == wasm_encoder::ValType::F64 {
            self.f.instructions().i64_reinterpret_f64();
        }
        {
            let mut i = self.f.instructions();
            i.call(push);
            i.i32_store(MemArg { offset: u64::from(voff), align: 2, memory_index: 0 });
            i.else_();
            // absent: grow-append the (key, [x]) entry; the outgrown
            // accumulator (uniquely ours) is freed once its entries moved
            i.local_get(hm).i32_load(len_memarg()).i32_const(esz as i32).i32_add();
            i.call(F_ALLOC).local_set(he);
            i.local_get(he).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(hm).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(hm).i32_load(len_memarg());
            i.memory_copy(0, 0);
            i.local_get(he).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(hm).i32_load(len_memarg()).i32_add().local_set(he);
            i.local_get(he).i32_const(koff as i32).i32_add();
            i.local_get(hkey);
        }
        if !key_owned {
            self.share_handle_top(kt);
        }
        self.store_ty_slot_at(kt);
        {
            let mut i = self.f.instructions();
            i.local_get(he);
            i.i32_const(0).call(F_ALLOC);
            i.local_get(params[0]);
        }
        self.share_handle_top(elem);
        if elem.val_type() == wasm_encoder::ValType::F64 {
            self.f.instructions().i64_reinterpret_f64();
        }
        {
            let mut i = self.f.instructions();
            i.call(push);
            i.i32_store(MemArg { offset: u64::from(voff), align: 2, memory_index: 0 });
            // the grown block replaces the old handle, and the outgrown
            // one (uniquely ours, its entries moved) is freed
            i.local_get(hm);
            i.local_get(he);
            i.local_get(hm).i32_load(len_memarg()).i32_sub();
            i.i32_const(almide_layout::PAYLOAD as i32).i32_sub();
            i.local_set(hm);
            i.call(F_FREE);
            i.end();
        }
        self.hof_step(ih);
        self.f.instructions().local_get(hm);
        self.release_for(kt);
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::Map(self.types.intern(kt), self.types.intern(inner)))))
    }

    /// The keyed lookup for a STABLE receiver (merge / update / upsert
    /// probe the input map, which outlives the op): the index lane for
    /// Int/String keys, the scan family otherwise (#1219 stage 2).
    fn map_scan_fn(&mut self, k: SliceTy) -> Result<u32, EmitError> {
        self.keyed_find(k)
    }
}
