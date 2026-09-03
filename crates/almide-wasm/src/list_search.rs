//! Search-flavored list ops: binary_search (std's branchless schedule),
//! window (the singular guarded form), unique_by (key-scan dedup).

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// `xs.binary_search(&t)` on List[Int] — std's BRANCHLESS loop
    /// (base/size halving with a select), which fixes WHICH duplicate
    /// index comes back (C-054 family: [2,2,2,2,2] → 4, not the classic
    /// midpoint 2). The final compare happens at `base`.
    pub(crate) fn lower_list_binary_search(
        &mut self,
        xs: &IrExpr,
        t: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        match self.lower(xs, None)? {
            SliceTy::List(h) if self.types.el(h) == INT => {}
            other => return unsup(&format!("list-binary-search-of:{other:?}")),
        }
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower(t, Some(INT))?;
        let ht = self.hold_i64()?;
        let hsize = self.hold_i32()?;
        let hbase = self.hold_i32()?;
        let hhalf = self.hold_i32()?;
        let hv = self.hold_i64()?;
        let mut i = self.f.instructions();
        i.local_set(ht);
        i.local_get(hb).i32_load(len_memarg()).i32_const(8).i32_div_u().local_set(hsize);
        i.local_get(hsize).i32_eqz().if_(BlockType::Result(ValType::I32));
        i.i32_const(almide_layout::NULL_ADDR as i32);
        i.else_();
        i.i32_const(0).local_set(hbase);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hsize).i32_const(1).i32_le_u().br_if(1);
        i.local_get(hsize).i32_const(1).i32_shr_u().local_set(hhalf);
        i.local_get(hb)
            .local_get(hbase)
            .local_get(hhalf)
            .i32_add()
            .i32_const(8)
            .i32_mul()
            .i32_add()
            .i64_load(slot_memarg(0))
            .local_set(hv);
        // base = (v > t) ? base : mid
        i.local_get(hbase);
        i.local_get(hbase).local_get(hhalf).i32_add();
        i.local_get(hv).local_get(ht).i64_gt_s();
        i.select().local_set(hbase);
        i.local_get(hsize).local_get(hhalf).i32_sub().local_set(hsize);
        i.br(0).end().end();
        i.local_get(hb)
            .local_get(hbase)
            .i32_const(8)
            .i32_mul()
            .i32_add()
            .i64_load(slot_memarg(0));
        i.local_get(ht).i64_eq().if_(BlockType::Result(ValType::I32));
        // some(base): an 8-byte option cell holding the i64 index
        i.i32_const(8).call(F_ALLOC).local_tee(hhalf);
        i.local_get(hbase).i64_extend_i32_u();
        i.i64_store(slot_memarg(almide_layout::OPTION_FIELD));
        i.local_get(hhalf);
        i.else_();
        i.i32_const(almide_layout::NULL_ADDR as i32);
        i.end();
        i.end();
        let _ = i;
        self.release_i64();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_i64();
        self.release_i32();
        Ok(Some(SliceTy::Option(self.types.intern(INT))))
    }

    /// `list.window(xs, n)` — n == 0 dies loudly (both targets, C-153
    /// family), n < 0 or n > len is the empty list, else len-n+1
    /// overlapping windows.
    pub(crate) fn lower_list_window(
        &mut self,
        xs: &IrExpr,
        n: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let e = match self.lower(xs, None)? {
            SliceTy::List(h) => self.types.el(h),
            other => return unsup(&format!("list-window-of:{other:?}")),
        };
        let stride = e.slot_size() as i32;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower(n, Some(INT))?;
        let hn64 = self.hold_i64()?;
        let hw = self.hold_i32()?;
        let ho = self.hold_i32()?;
        let hk = self.hold_i32()?;
        let hc = self.hold_i32()?;
        let die = self.pool.intern("Error: window size must be positive");
        let mut i = self.f.instructions();
        i.local_set(hn64);
        i.local_get(hn64).i64_eqz().if_(BlockType::Empty);
        i.i32_const(die as i32).call(F_EPRINTLN_BLOCK);
        i.i32_const(1).call(F_EXIT_IMPORT).unreachable();
        i.end();
        i.local_get(hn64).i64_const(0).i64_lt_s();
        i.local_get(hn64);
        i.local_get(hb).i32_load(len_memarg()).i32_const(stride).i32_div_u().i64_extend_i32_u();
        i.i64_gt_s();
        i.i32_or().if_(BlockType::Result(ValType::I32));
        i.i32_const(0).call(F_ALLOC);
        i.else_();
        i.local_get(hn64).i32_wrap_i64().local_set(hw);
        // wins = count - n + 1
        i.local_get(hb)
            .i32_load(len_memarg())
            .i32_const(stride)
            .i32_div_u()
            .local_get(hw)
            .i32_sub()
            .i32_const(1)
            .i32_add()
            .local_set(hc);
        i.local_get(hc).i32_const(4).i32_mul().call(F_ALLOC).local_set(ho);
        i.i32_const(0).local_set(hk);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hk).local_get(hc).i32_ge_u().br_if(1);
        i.local_get(hw).i32_const(stride).i32_mul().call(F_ALLOC);
        let _ = i;
        // window block: alloc + copy stride*n bytes from offset k*stride
        let hwin = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hwin);
        i.local_get(hwin).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hb)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add()
            .local_get(hk)
            .i32_const(stride)
            .i32_mul()
            .i32_add();
        i.local_get(hw).i32_const(stride).i32_mul();
        i.memory_copy(0, 0);
        i.local_get(ho).local_get(hk).i32_const(4).i32_mul().i32_add();
        i.local_get(hwin).i32_store(slot_memarg(0));
        i.local_get(hk).i32_const(1).i32_add().local_set(hk);
        i.br(0).end().end();
        i.local_get(ho);
        i.end();
        let _ = i;
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_i64();
        self.release_i32();
        let inner = self.types.intern(e);
        Ok(Some(SliceTy::List(self.types.intern(SliceTy::List(inner)))))
    }

    /// `list.map(xs, f)` with a FN-VALUE callback: the closure
    /// convention (env block first, table slot in payload[0]) per
    /// element via call_indirect.
    pub(crate) fn lower_list_map_fnvalue(
        &mut self,
        xs: &IrExpr,
        cb: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let got = self.lower(cb, None)?;
        let SliceTy::Fn(sig) = got else {
            return unsup("list-hof-nonlambda");
        };
        let def = self.types.fn_sig_def(sig);
        if def.params.len() != 1 {
            return unsup("list-hof-arity");
        }
        let Some(u) = def.ret else {
            return unsup("list-map-fn-unit");
        };
        let henv = self.hold_i32()?;
        self.f.instructions().local_set(henv);
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        if def.params[0] != elem {
            return unsup("list-map-fn-param");
        }
        let hx = self.hold_for(elem)?;
        let rh = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(ch).i32_const(u.slot_size() as i32).i32_mul().call(F_ALLOC).local_set(rh);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
        }
        self.hof_elem_into(elem, bh, ch, ih, hx);
        {
            let mut i = self.f.instructions();
            i.local_get(rh).local_get(ih).i32_const(u.slot_size() as i32).i32_mul().i32_add();
            i.local_get(henv);
            i.local_get(hx);
        }
        // Closure convention (calls.rs): the callee OWNS its args and its
        // epilogue decs every droppable param, so the element — a borrowed
        // read the list still holds — takes the RC-3 +1 here, exactly as
        // the fs walkers' per-line call does.
        if self.rc_droppable(elem) {
            self.rc_inc_top();
        }
        self.f.instructions().local_get(henv).i32_load(slot_memarg(0));
        let ti = self.work.itype(
            vec![ValType::I32, elem.val_type()],
            Some(u.val_type()),
        );
        self.f.instructions().call_indirect(0, ti);
        self.store_ty_slot(u, 0);
        self.hof_step(ih);
        self.f.instructions().local_get(rh);
        self.release_i32();
        self.release_for(elem);
        for _ in 0..3 {
            self.release_i32();
        }
        self.release_i32();
        Ok(Some(SliceTy::List(self.types.intern(u))))
    }

    /// `list.unique_by(xs, f)` — first-seen dedup keyed by the callback's
    /// result; elements keep first-occurrence order. A SCALAR key (Int /
    /// Bool / String / Float) scans the seen set through the scan family;
    /// any other equatable key takes the deep lane (#1797).
    pub(crate) fn lower_list_unique_by(
        &mut self,
        xs: &IrExpr,
        cb: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let (params, body) = self.hof_lambda(cb, 1)?;
        let (elem, bh, ch, ih) = self.hof_loop_open(xs)?;
        let kt = self.infer(body)?;
        let SliceTy::Scalar(_) = kt else {
            return self.lower_list_unique_by_deep(kt, elem, (bh, ch, ih), params[0], body);
        };
        let scan = self.scan_helper(kt)?;
        let kstride = kt.slot_size() as i32;
        let stride = elem.slot_size() as i32;
        let push = match kt.slot_size() {
            8 => F_LIST_PUSH_8,
            _ => F_LIST_PUSH_4,
        };
        let hseen = self.hold_i32()?;
        let hout = self.hold_i32()?;
        let hkept = self.hold_i32()?;
        let hkey = self.hold_for(kt)?;
        {
            let mut i = self.f.instructions();
            i.i32_const(0).call(F_ALLOC).local_set(hseen);
            i.local_get(ch).i32_const(stride).i32_mul().call(F_ALLOC).local_set(hout);
            i.i32_const(0).local_set(hkept);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
        }
        self.hof_elem_into(elem, bh, ch, ih, params[0]);
        self.lower(body, Some(kt))?;
        {
            let mut i = self.f.instructions();
            i.local_set(hkey);
            i.local_get(hseen).i32_const(kstride).i32_const(0).local_get(hkey);
            i.call(scan).i32_eqz().if_(BlockType::Empty);
            i.local_get(hseen).local_get(hkey);
            if kt.val_type() == ValType::F64 {
                i.i64_reinterpret_f64();
            }
            i.call(push).local_set(hseen);
            i.local_get(hout).local_get(hkept).i32_const(stride).i32_mul().i32_add();
            i.local_get(params[0]);
        }
        self.store_ty_slot(elem, 0);
        {
            let mut i = self.f.instructions();
            i.local_get(hkept).i32_const(1).i32_add().local_set(hkept);
            i.end();
        }
        self.hof_step(ih);
        {
            let mut i = self.f.instructions();
            i.local_get(hout).local_get(hkept).i32_const(stride).i32_mul().i32_store(len_memarg());
            i.local_get(hout);
        }
        self.release_for(kt);
        for _ in 0..6 {
            self.release_i32();
        }
        Ok(Some(SliceTy::List(self.types.intern(elem))))
    }
}

/// The key types the deep lane admits: every shape `emit_val_eq` compares
/// structurally. `Fn`/`Matrix` have no equality; `Unit` never FLOWS out of
/// a lambda body as a value (the void convention) — each walls by name.
fn unique_by_key_equatable(kt: SliceTy) -> bool {
    matches!(
        kt,
        SliceTy::Option(_)
            | SliceTy::Result(..)
            | SliceTy::List(_)
            | SliceTy::Map(..)
            | SliceTy::Set(_)
            | SliceTy::Tuple(_)
            | SliceTy::Named(_)
            | SliceTy::Value
    )
}

impl Emitter<'_> {
    /// The NON-SCALAR key lane of `list.unique_by` (#1797): the seen set is
    /// a list of key HANDLES (every non-scalar key is an i32 block) scanned
    /// with the type-directed `==` (`emit_val_eq`) instead of the scalar
    /// scan family — the documented `(s) => string.get(s, 0)` (an
    /// `Option[String]` key) walled `list-unique-by-key-nonscalar` here.
    /// `loop_` is `hof_loop_open`'s (base, count, idx) triple; the caller
    /// already lowered `xs` and holds those three.
    fn lower_list_unique_by_deep(
        &mut self,
        kt: SliceTy,
        elem: SliceTy,
        loop_: (u32, u32, u32),
        param: u32,
        body: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        if !unique_by_key_equatable(kt) {
            return unsup(&format!("list-unique-by-key:{kt:?}"));
        }
        let (bh, ch, ih) = loop_;
        let stride = elem.slot_size() as i32;
        let hseen = self.hold_i32()?;
        let hout = self.hold_i32()?;
        let hkept = self.hold_i32()?;
        let hkey = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.i32_const(0).call(F_ALLOC).local_set(hseen);
            i.local_get(ch).i32_const(stride).i32_mul().call(F_ALLOC).local_set(hout);
            i.i32_const(0).local_set(hkept);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
        }
        self.hof_elem_into(elem, bh, ch, ih, param);
        self.lower(body, Some(kt))?;
        self.f.instructions().local_set(hkey);
        self.emit_seen_key_scan(kt, hseen, hkey)?;
        {
            let mut i = self.f.instructions();
            i.i32_eqz().if_(BlockType::Empty);
            i.local_get(hseen).local_get(hkey);
        }
        // The seen list becomes a co-owner of a key read from a droppable
        // local (RC-3 share rule); fresh keys transfer as-is.
        self.rc_share_guard(body, kt);
        {
            let mut i = self.f.instructions();
            i.call(F_LIST_PUSH_4).local_set(hseen);
            i.local_get(hout).local_get(hkept).i32_const(stride).i32_mul().i32_add();
            i.local_get(param);
        }
        self.store_ty_slot(elem, 0);
        {
            let mut i = self.f.instructions();
            i.local_get(hkept).i32_const(1).i32_add().local_set(hkept);
            i.end();
        }
        self.hof_step(ih);
        {
            let mut i = self.f.instructions();
            i.local_get(hout).local_get(hkept).i32_const(stride).i32_mul().i32_store(len_memarg());
            i.local_get(hout);
        }
        for _ in 0..7 {
            self.release_i32();
        }
        Ok(Some(SliceTy::List(self.types.intern(elem))))
    }

    /// `[] -> i32`: 1 when the key in `hkey` structurally equals any handle
    /// in the 4-byte-slot list `hseen`, else 0. The verdict local is set on
    /// the first hit and the scan breaks out of its own block.
    fn emit_seen_key_scan(&mut self, kt: SliceTy, hseen: u32, hkey: u32) -> Result<(), EmitError> {
        let hj = self.hold_i32()?;
        let hf = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.i32_const(0).local_set(hf);
            i.i32_const(0).local_set(hj);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hj).local_get(hseen).i32_load(len_memarg()).i32_ge_u().br_if(1);
            i.local_get(hseen).local_get(hj).i32_add().i32_load(slot_memarg(0));
            i.local_get(hkey);
        }
        self.emit_val_eq(kt)?;
        {
            let mut i = self.f.instructions();
            i.if_(BlockType::Empty);
            i.i32_const(1).local_set(hf);
            i.br(2);
            i.end();
            i.local_get(hj).i32_const(4).i32_add().local_set(hj);
            i.br(0).end().end();
            i.local_get(hf);
        }
        self.release_i32();
        self.release_i32();
        Ok(())
    }
}
