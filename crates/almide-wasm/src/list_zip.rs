//! `list.zip` / `list.zip_with` — split from list_comb.rs for the file
//! budget (the arm is unchanged).

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// Pairs to the SHORTER length (native zip); zip_with maps the pair
    /// through the callback instead of building tuples.
    pub(crate) fn lower_list_zip(
        &mut self,
        a: &IrExpr,
        b: &IrExpr,
        cb: Option<&IrExpr>,
    ) -> ArmResult {
        let (params, body) = match cb {
            Some(cb) => {
                let (p, bd) = self.hof_lambda(cb, 2)?;
                (p, Some(bd))
            }
            None => (Vec::new(), None),
        };
        let ea = match self.lower_arg(a, None, ArgMode::Borrow)? {
            SliceTy::List(h) => self.types.el(h),
            other => return unsup(&format!("list-zip-of:{other:?}")),
        };
        let ha = self.hold_i32()?;
        self.f.instructions().local_set(ha);
        let eb = match self.lower_arg(b, None, ArgMode::Borrow)? {
            SliceTy::List(h) => self.types.el(h),
            other => return unsup(&format!("list-zip-of:{other:?}")),
        };
        let hb = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let hi = self.hold_i32()?;
        let hacc = self.hold_i32()?;
        let (sa, sb) = (ea.slot_size() as i32, eb.slot_size() as i32);
        let out_ty = match body {
            Some(body) => self.infer(body)?,
            None => SliceTy::Tuple(self.types.tuple(vec![ea, eb])),
        };
        {
            let mut i = self.f.instructions();
            i.local_set(hb);
            // n = min(count_a, count_b)  (select: v1 first)
            i.local_get(ha).i32_load(len_memarg()).i32_const(sa).i32_div_u();
            i.local_get(hb).i32_load(len_memarg()).i32_const(sb).i32_div_u();
            i.local_get(ha).i32_load(len_memarg()).i32_const(sa).i32_div_u();
            i.local_get(hb).i32_load(len_memarg()).i32_const(sb).i32_div_u();
            i.i32_lt_u().select().local_set(hn);
            i.i32_const(0).call(F_ALLOC).local_set(hacc);
            i.i32_const(0).local_set(hi);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hi).local_get(hn).i32_ge_u().br_if(1);
        }
        let val_ty = if let Some(body) = body {
            // load both elems into the callback params, run the body
            self.f.instructions().local_get(ha).local_get(hi).i32_const(sa).i32_mul().i32_add();
            self.load_ty_slot(ea, 0);
            self.f.instructions().local_set(params[0]);
            self.f.instructions().local_get(hb).local_get(hi).i32_const(sb).i32_mul().i32_add();
            self.load_ty_slot(eb, 0);
            self.f.instructions().local_set(params[1]);
            if let Some(cb) = cb {
                self.witness_callback_open(cb, None);
            }
            self.lower(body, Some(out_ty))?;
            // A pass-through body (`(a, b) => a`, a captured String) hands
            // back a VIEW; the result spine is a holder and takes the share
            // here, exactly as `list.map` does. Without it the pushed handle
            // rode on the source's count, and dropping the zip_with result
            // freed the captured string under its owner (fuzz-nightly
            // OutputDivergence, seed 561137265092 index 934).
            self.rc_share_guard(body, out_ty);
            // #2755: the guarded value moves into the result spine.
            self.witness_store(body, out_ty);
            self.witness_loop_close();
            out_ty
        } else {
            // build the (A, B) pair block
            let SliceTy::Tuple(ti) = out_ty else { unreachable!() };
            let def = self.types.tuple_def(ti);
            let hp = self.hold_i32()?;
            self.f.instructions().i32_const(def.size as i32).call(F_ALLOC).local_set(hp);
            self.f.instructions().local_get(hp);
            self.f.instructions().local_get(ha).local_get(hi).i32_const(sa).i32_mul().i32_add();
            self.load_ty_slot(ea, 0);
            // Handles copied into the pair block take +1 (the pair's typed
            // drop releases them, #2010 stage 2c).
            self.share_handle_top(ea);
            self.store_ty_slot(ea, def.fields[0].1);
            self.f.instructions().local_get(hp);
            self.f.instructions().local_get(hb).local_get(hi).i32_const(sb).i32_mul().i32_add();
            self.load_ty_slot(eb, 0);
            self.share_handle_top(eb);
            self.store_ty_slot(eb, def.fields[1].1);
            self.f.instructions().local_get(hp);
            self.release_i32();
            out_ty
        };
        if val_ty.val_type() == ValType::F64 {
            self.f.instructions().i64_reinterpret_f64();
        }
        if val_ty.slot_size() == 8 {
            let hv = self.hold_i64()?;
            self.f.instructions().local_set(hv);
            self.f.instructions().local_get(hacc).local_get(hv).call(F_LIST_PUSH_8);
            self.f.instructions().local_set(hacc);
            self.release_i64();
        } else {
            let hv = self.hold_i32()?;
            self.f.instructions().local_set(hv);
            self.f.instructions().local_get(hacc).local_get(hv).call(F_LIST_PUSH_4);
            self.f.instructions().local_set(hacc);
            self.release_i32();
        }
        {
            let mut i = self.f.instructions();
            i.local_get(hi).i32_const(1).i32_add().local_set(hi);
            i.br(0).end().end();
            i.local_get(hacc);
        }
        for _ in 0..5 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::List(self.types.intern(out_ty)))))
    }
}
