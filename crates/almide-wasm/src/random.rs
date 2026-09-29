//! `random.float` / `random.choice` / `random.shuffle` and `list.shuffle`
//! (#2749): native arms over the entropy host op (32, the one `random.int`'s
//! linked body already reaches through `prim.random_get`).
//!
//! The self-host bodies (stdlib/random_*.almd) are typed twins — `choice`
//! over `List[Int]`, `_str`, `_pair` — routed by the INCUMBENT's element
//! classifier, and `random_choice_pair` reads the incumbent's list header
//! raw. These arms take any element type through this emitter's own layout.
//!
//! The draws follow the native runtime's constructions
//! (runtime/rs/src/random.rs): a float is the TOP 53 bits of a 64-bit draw
//! scaled by 2^-53 (every value in [0, 1)); `choice` indexes `draw % len`;
//! `shuffle` is Fisher–Yates from the back, `j = draw % (i + 1)`, over a copy
//! (the argument is untouched). The values are nondeterministic on every
//! target (C-112 pins the range, not the bytes); only the shape — range,
//! membership, permutation — is observable across legs.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// One 64-bit draw from the host's entropy, left on the stack as i64.
    fn emit_entropy_u64(&mut self) -> Result<(), EmitError> {
        let dec = self.dec_fn_of(SliceTy::Scalar(Scalar::Bytes));
        let hb = self.hold_i32()?;
        self.note_host_op(crate::fs_meta::OP_RANDOM_GET);
        let mut i = self.f.instructions();
        i.i32_const(8).call(F_ALLOC).local_set(hb);
        i.i32_const(crate::fs_meta::OP_RANDOM_GET);
        i.i32_const(0).i32_const(0).i32_const(0).i32_const(8);
        i.call(F_FS_CALL).drop();
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add().call(F_HOST_READ);
        i.local_get(hb).i64_load(slot_memarg(0));
        i.local_get(hb).call(dec);
        self.release_i32();
        Ok(())
    }

    /// Ok(None) = not a cell of this family.
    pub(crate) fn lower_random_call(
        &mut self,
        module: &str,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<Option<Lowered>>, EmitError> {
        match (module, func, args) {
            ("random", "float", []) => {
                self.emit_entropy_u64()?;
                let mut i = self.f.instructions();
                i.i64_const(11).i64_shr_u().f64_convert_i64_u();
                i.f64_const((1.0 / 9007199254740992.0_f64).into()).f64_mul();
                Ok(Some(Some(Lowered::scalar(FLOAT))))
            }
            ("random", "choice", [xs]) => self.lower_random_choice(xs).map(Some),
            ("random" | "list", "shuffle", [xs]) => self.lower_random_shuffle(xs).map(Some),
            _ => Ok(None),
        }
    }

    fn lower_random_choice(&mut self, xs: &IrExpr) -> ArmResult {
        let el = match self.lower_arg(xs, None, ArgMode::Borrow)? {
            SliceTy::List(h) => self.types.el(h),
            other => return unsup(&format!("random-choice-of:{other:?}")),
        };
        let stride = el.slot_size() as i32;
        let hl = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let hc = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_set(hl);
            i.local_get(hl).i32_load(len_memarg()).i32_const(stride).i32_div_u().local_set(hn);
            i.local_get(hn).i32_eqz().if_(BlockType::Result(ValType::I32));
            i.i32_const(almide_layout::NULL_ADDR as i32);
            i.else_();
            i.i32_const(stride).call(F_ALLOC).local_set(hc);
            i.local_get(hc);
            i.local_get(hl);
        }
        self.emit_entropy_u64()?;
        {
            let mut i = self.f.instructions();
            i.local_get(hn).i64_extend_i32_u().i64_rem_u().i32_wrap_i64();
            i.i32_const(stride).i32_mul().i32_add();
        }
        self.load_ty_slot(el, 0);
        // The picked element is shared into the Option, not moved out.
        if self.elem_is_handle(el) {
            self.rc_inc_top();
        }
        self.store_ty_slot(el, almide_layout::OPTION_FIELD);
        self.f.instructions().local_get(hc).end();
        self.release_i32();
        self.release_i32();
        self.release_i32();
        let eh = self.types.intern(el);
        Ok(Some(Lowered::owned(SliceTy::Option(eh))))
    }

    fn lower_random_shuffle(&mut self, xs: &IrExpr) -> ArmResult {
        let got = self.lower_arg(xs, None, ArgMode::Borrow)?;
        let SliceTy::List(h) = got else {
            return unsup(&format!("random-shuffle-of:{got:?}"));
        };
        let el = self.types.el(h);
        let stride = el.slot_size() as i32;
        // A fresh copy that takes its own element credits; the swaps below
        // only exchange slots within it, which moves no ownership.
        let copy = self.copy_fn_of(got);
        self.f.instructions().call(copy);
        let hl = self.hold_i32()?;
        let hi = self.hold_i32()?;
        let hj = self.hold_i32()?;
        let ht = self.hold_for(el)?;
        {
            let mut i = self.f.instructions();
            i.local_set(hl);
            i.local_get(hl).i32_load(len_memarg()).i32_const(stride).i32_div_u().i32_const(1).i32_sub().local_set(hi);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hi).i32_const(1).i32_lt_s().br_if(1);
        }
        self.emit_entropy_u64()?;
        {
            let mut i = self.f.instructions();
            i.local_get(hi).i32_const(1).i32_add().i64_extend_i32_u().i64_rem_u().i32_wrap_i64().local_set(hj);
            // t = xs[i]; xs[i] = xs[j]; xs[j] = t
            i.local_get(hl).local_get(hi).i32_const(stride).i32_mul().i32_add();
        }
        self.load_ty_slot(el, 0);
        self.f.instructions().local_set(ht);
        {
            let mut i = self.f.instructions();
            i.local_get(hl).local_get(hi).i32_const(stride).i32_mul().i32_add();
            i.local_get(hl).local_get(hj).i32_const(stride).i32_mul().i32_add();
        }
        self.load_ty_slot(el, 0);
        self.store_ty_slot(el, 0);
        {
            let mut i = self.f.instructions();
            i.local_get(hl).local_get(hj).i32_const(stride).i32_mul().i32_add();
            i.local_get(ht);
        }
        self.store_ty_slot(el, 0);
        {
            let mut i = self.f.instructions();
            i.local_get(hi).i32_const(1).i32_sub().local_set(hi);
            i.br(0).end().end();
            i.local_get(hl);
        }
        self.release_for(el);
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(Some(Lowered::owned(got)))
    }
}
