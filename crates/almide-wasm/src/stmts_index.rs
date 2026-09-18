//! `xs[i] = v` — the index-assign route, split from stmts.rs for the
//! 800-line file discipline (the route is unchanged).

use std::collections::HashMap;
use almide_ir::{IrExpr, VarId};
use wasm_encoder::BlockType;
use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// Register a local as an epilogue-released owner. A droppable PARAM
    /// can land here too (a mut-param writeback's Assign) — the epilogue's
    /// param pass skips locals in this set, so each local is released
    /// exactly once (#1770: both passes firing on one local double-freed
    /// the returned buffer, and its freelist link zeroed the first
    /// payload word).
    /// The byte address of element `hi` (i64) of the list in `hb`:
    /// `block + PAYLOAD + hi * stride`.
    pub(crate) fn emit_index_slot_addr(&mut self, hb: u32, hi: u32, stride: i64) {
        self.f
            .instructions()
            .local_get(hb)
            .i64_extend_i32_u()
            .local_get(hi)
            .i64_const(stride)
            .i64_mul()
            .i64_add()
            .i32_wrap_i64()
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add();
    }

    /// `xs[i] = v` — copy-on-write (split from lower_stmt for the complexity budget).
    pub(crate) fn lower_index_assign(
        &mut self,
        target: &VarId,
        index: &IrExpr,
        value: &IrExpr,
    ) -> Result<(), EmitError> {

                let (is_local, declared) = match self.locals.get(target) {
                    Some(&(_, d)) => (true, d),
                    None => match self.globals.get(&(self.var_space, *target)) {
                        Some(&(_, d)) => (false, d),
                        None => return unsup("index-assign:unmapped"),
                    },
                };
                let SliceTy::List(h) = declared else {
                    return unsup(&format!("index-assign-ty:{declared:?}"));
                };
                let el = self.types.el(h);
                let stride = el.slot_size() as i64;
                // Interp order: index, then value, then the bounds check.
                self.lower(index, Some(INT))?;
                let hi = self.hold_i64()?;
                self.f.instructions().local_set(hi);
                self.lower(value, Some(el))?;
                // A handle element stored into the spine is a holder: a
                // borrowed rhs takes its +1 here (#2010 stage 2b).
                self.rc_share_guard(value, el);
                let hv = self.hold_val(el)?;
                let hb = self.hold_i32()?;
                self.f.instructions().local_set(hv);
                let var_space = self.var_space;
                let get_target = |f: &mut wasm_encoder::Function, locals: &HashMap<VarId, (u32, SliceTy)>, globals: &HashMap<GVar, (u32, SliceTy)>| {
                    if is_local {
                        f.instructions().local_get(locals[target].0);
                    } else {
                        f.instructions().global_get(globals[&(var_space, *target)].0);
                    }
                };
                // OOB → the exact native frame + exit 1.
                let msg = self.pool.intern("index out of bounds");
                get_target(self.f, self.locals, self.globals);
                {
                    let mut i = self.f.instructions();
                    i.i32_load(len_memarg())
                        .i64_extend_i32_u()
                        .i64_const(stride)
                        .i64_div_s();
                    i.local_get(hi).i64_le_s();
                    i.local_get(hi).i64_const(0).i64_lt_s();
                    i.i32_or().if_(BlockType::Empty);
                    i.i32_const(msg as i32);
                }
                self.emit_error_frame_abort();
                self.f.instructions().end();
                // RC-5: the COW judge, not an unconditional copy — a
                // uniquely-held list takes the store IN PLACE, a shared one
                // copies and releases one source ref. The old
                // $block_copy-per-write materialized a fresh generation on
                // EVERY index write and never freed the outgrown one, so n
                // writes into a preallocated list retained O(n²) bytes
                // (#1729: the prealloc/fft rows OOM'd at 2^16 writes where
                // the live payload is 512 KiB).
                get_target(self.f, self.locals, self.globals);
                let cow = self.cow_fn_of(declared);
                self.f.instructions().call(cow).local_set(hb);
                if is_local {
                    let idx = self.locals[target].0;
                    self.f.instructions().local_get(hb).local_set(idx);
                } else {
                    let g = self.globals[&(self.var_space, *target)].0;
                    self.f.instructions().local_get(hb).global_set(g);
                }
                // The replaced element's credit goes with it.
                if let Some(dec) = self.elem_is_handle(el).then(|| self.dec_fn_of(el)) {
                    self.emit_index_slot_addr(hb, hi, stride);
                    self.f.instructions().i32_load(wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 }).call(dec);
                }
                self.emit_index_slot_addr(hb, hi, stride);
                self.f.instructions().local_get(hv);
                self.store_ty_slot_raw(el);
                self.release_i32();
                self.release_val(el);
                self.release_i64();
                Ok(())
    }
}
