//! random module — WASM codegen dispatch.

use super::FuncCompiler;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use super::values;
use wasm_encoder::Instruction;

impl FuncCompiler<'_> {
    pub(super) fn emit_random_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "int" => {
                // random.int(min, max) → Int in [min, max]
                let min = self.scratch.alloc_i64();
                let max = self.scratch.alloc_i64();
                let state = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(min); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(max);
                    // Load PRNG state from mem[0..8]
                    i32_const(0); i64_load(0); local_set(state);
                    // If state == 0, initialize with seed
                    local_get(state); i64_eqz;
                    if_empty;
                      i32_const(0); i32_const(8); call(self.emitter.rt.random_get); drop;
                      i32_const(0); i64_load(0); local_set(state);
                      local_get(state); i64_eqz;
                      if_empty; i64_const(1); local_set(state); end;
                    end;
                    // xorshift64
                    local_get(state); local_get(state); i64_const(13); i64_shl; i64_xor; local_set(state);
                    local_get(state); local_get(state); i64_const(7); i64_shr_u; i64_xor; local_set(state);
                    local_get(state); local_get(state); i64_const(17); i64_shl; i64_xor; local_set(state);
                    // Store back
                    i32_const(0); local_get(state); i64_store(0);
                    // result = min + abs(state) % (max - min + 1)
                    local_get(min);
                    // abs(state)
                    local_get(state); i64_const(0); i64_lt_s;
                    if_i64; i64_const(0); local_get(state); i64_sub; else_; local_get(state); end;
                    local_get(max); local_get(min); i64_sub; i64_const(1); i64_add;
                    i64_rem_u;
                    i64_add;
                });
                self.scratch.free_i64(state);
                self.scratch.free_i64(max);
                self.scratch.free_i64(min);
            }
            "float" => {
                // random.float() → Float in [0.0, 1.0)
                let state = self.scratch.alloc_i64();
                wasm!(self.func, {
                    i32_const(0); i64_load(0); local_set(state);
                    local_get(state); i64_eqz;
                    if_empty;
                      i32_const(0); i32_const(8); call(self.emitter.rt.random_get); drop;
                      i32_const(0); i64_load(0); local_set(state);
                      local_get(state); i64_eqz;
                      if_empty; i64_const(1); local_set(state); end;
                    end;
                    local_get(state); local_get(state); i64_const(13); i64_shl; i64_xor; local_set(state);
                    local_get(state); local_get(state); i64_const(7); i64_shr_u; i64_xor; local_set(state);
                    local_get(state); local_get(state); i64_const(17); i64_shl; i64_xor; local_set(state);
                    i32_const(0); local_get(state); i64_store(0);
                });
                // Convert to float in [0, 1): abs(state) >>> 11 * (1/2^53)
                wasm!(self.func, {
                    local_get(state); i64_const(0); i64_lt_s;
                    if_i64; i64_const(0); local_get(state); i64_sub; else_; local_get(state); end;
                    i64_const(11); i64_shr_u;
                    f64_convert_i64_u;
                    f64_const(1.1102230246251565e-16); // 1.0 / 2^53
                    f64_mul;
                });
                self.scratch.free_i64(state);
            }
            "choice" => {
                // random.choice(xs: List[T]) → Option[T]: none if empty, some(random elem) if non-empty
                let xs = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let option_box = self.scratch.alloc_i32();
                let state = self.scratch.alloc_i64();
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(xs);
                    local_get(xs); i32_load(0); i32_eqz;
                    if_i32;
                      i32_const(0); // none (empty list)
                    else_;
                      // PRNG to get random index
                      i32_const(0); i64_load(0); local_set(state);
                      local_get(state); i64_eqz;
                      if_empty;
                      i32_const(0); i32_const(8); call(self.emitter.rt.random_get); drop;
                      i32_const(0); i64_load(0); local_set(state);
                      local_get(state); i64_eqz;
                      if_empty; i64_const(1); local_set(state); end;
                    end;
                      local_get(state); local_get(state); i64_const(13); i64_shl; i64_xor; local_set(state);
                      local_get(state); local_get(state); i64_const(7); i64_shr_u; i64_xor; local_set(state);
                      local_get(state); local_get(state); i64_const(17); i64_shl; i64_xor; local_set(state);
                      i32_const(0); local_get(state); i64_store(0);
                      // idx = abs(state) % len
                      local_get(state); i64_const(0); i64_lt_s;
                      if_i64; i64_const(0); local_get(state); i64_sub; else_; local_get(state); end;
                      local_get(xs); i32_load(0); i64_extend_i32_u;
                      i64_rem_u; i32_wrap_i64; local_set(idx);
                      // Alloc option box and load elem into it
                      i32_const(es); call(self.emitter.rt.alloc); local_set(option_box);
                      local_get(option_box);
                      local_get(xs); i32_const(4); i32_add;
                      local_get(idx); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(option_box);
                    end;
                });
                self.scratch.free_i64(state);
                self.scratch.free_i32(option_box);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(xs);
            }
            "shuffle" => {
                // random.shuffle(xs) → List[T]: Fisher-Yates shuffle on a copy
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let src = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let state = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(src);
                    local_get(src); i32_load(0); local_set(len);
                    // Alloc copy
                    i32_const(4); local_get(len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    // Copy all elements
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(src); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    // Fisher-Yates shuffle (backwards)
                    local_get(len); i32_const(1); i32_sub; local_set(i);
                    i32_const(0); i64_load(0); local_set(state);
                    local_get(state); i64_eqz;
                    if_empty;
                      i32_const(0); i32_const(8); call(self.emitter.rt.random_get); drop;
                      i32_const(0); i64_load(0); local_set(state);
                      local_get(state); i64_eqz;
                      if_empty; i64_const(1); local_set(state); end;
                    end;
                    block_empty; loop_empty;
                      local_get(i); i32_const(0); i32_le_s; br_if(1);
                      // xorshift
                      local_get(state); local_get(state); i64_const(13); i64_shl; i64_xor; local_set(state);
                      local_get(state); local_get(state); i64_const(7); i64_shr_u; i64_xor; local_set(state);
                      local_get(state); local_get(state); i64_const(17); i64_shl; i64_xor; local_set(state);
                      // j = abs(state) % (i + 1)
                      local_get(state);
                      local_get(state); i64_const(0); i64_lt_s;
                      if_i64; i64_const(0); local_get(state); i64_sub; else_; local_get(state); end;
                      local_get(i); i32_const(1); i32_add; i64_extend_i32_u;
                      i64_rem_u; i32_wrap_i64; local_set(j);
                      // swap dst[i] and dst[j] using mem[0..es] as temp
                      // Copy dst[i] to temp
                      i32_const(0);
                      local_get(dst); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      i32_load(0); i32_store(0);
                      // Copy dst[j] to dst[i]
                      local_get(dst); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(dst); i32_const(4); i32_add;
                      local_get(j); i32_const(es); i32_mul; i32_add;
                      i32_load(0); i32_store(0);
                      // Copy temp to dst[j]
                      local_get(dst); i32_const(4); i32_add;
                      local_get(j); i32_const(es); i32_mul; i32_add;
                      i32_const(0); i32_load(0); i32_store(0);
                      local_get(i); i32_const(1); i32_sub; local_set(i);
                      br(0);
                    end; end;
                    i32_const(0); local_get(state); i64_store(0); // save state
                    local_get(dst);
                });
                self.scratch.free_i64(state);
                self.scratch.free_i32(j);
                self.scratch.free_i32(i);
                self.scratch.free_i32(len);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(src);
            }
            _ => panic!(
                "[ICE] emit_wasm: no WASM dispatch for `random.{}` — \
                 add an arm in emit_random_call or resolve upstream",
                func
            ),
        }
    }

}
