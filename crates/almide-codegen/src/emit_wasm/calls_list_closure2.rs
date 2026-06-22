//! List stdlib closure-based call dispatch for WASM codegen (part 2).
//!
//! Functions: take_while, drop_while, count, partition, update, scan, zip_with,
//! unique_by, group_by, shuffle, filter, fold, map, and the emit_list_map helper.

use super::FuncCompiler;
use super::values;
use almide_ir::{BinOp, IrExpr, IrExprKind};
use almide_lang::types::Ty;
use wasm_encoder::ValType;

/// A pipeline stage for stream fusion.
enum PipelineStage<'a> {
    Map(&'a IrExpr),    // lambda expr
    Filter(&'a IrExpr), // lambda expr
}

/// SIMD-eligible map operation for Int→Int.
#[derive(Clone, Copy)]
enum SimdMapOp {
    Mul,
    Add,
    Sub,
}

impl FuncCompiler<'_> {
    /// Dispatch list closure calls (second half). Returns true if handled.
    ///
    /// Split into disjoint sub-match groups (`*_g2`..`*_g4`) to keep each file
    /// under the line cap. Every arm pattern (method string) matches exactly
    /// one group, so the chain order below is irrelevant to behavior. Group 1
    /// (take_while/drop_while/count/partition) stays inline; the default `_`
    /// catch-all lives ONLY here.
    pub(super) fn emit_list_closure_call2(&mut self, method: &str, args: &[IrExpr]) -> bool {
        if self.emit_list_closure_call2_g2(method, args) { return true; }
        if self.emit_list_closure_call2_g3(method, args) { return true; }
        if self.emit_list_closure_call2_g4(method, args) { return true; }
        use super::engine::layout::{LIST, SWISS_MAP, list as ll, map as lm};
        let list_data_off = self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32;
        let list_hdr = self.emitter.layout_reg.header_size(LIST) as i32;
        let map_tags_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::TAGS) as i32;
        let map_hdr = self.emitter.layout_reg.header_size(SWISS_MAP) as i32;
        let map_cap_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::CAP);
        match method {
            "take_while" => {
                // take_while(xs, pred) → List[A]: take while pred returns true
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let count = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let copy_i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); local_set(len);
                    i32_const(0); local_set(count);
                    block_empty; loop_empty;
                      local_get(count); local_get(len); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(count); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&elem_ty, &Ty::Bool);
                wasm!(self.func, {
                      i32_eqz; br_if(1);
                      local_get(count); i32_const(1); i32_add; local_set(count);
                      br(0);
                    end; end;
                    // Alloc result
                    i32_const(list_hdr); local_get(count); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(count); i32_store(0);
                    // Copy loop
                    i32_const(0); local_set(copy_i);
                    block_empty; loop_empty;
                      local_get(copy_i); local_get(count); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(copy_i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(copy_i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                      local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(copy_i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(count);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "drop_while" => {
                // drop_while(xs, pred) → List[A]: drop while pred returns true
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let start = self.scratch.alloc_i32();
                let new_len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let copy_i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); local_set(len);
                    i32_const(0); local_set(start);
                    block_empty; loop_empty;
                      local_get(start); local_get(len); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(start); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&elem_ty, &Ty::Bool);
                wasm!(self.func, {
                      i32_eqz; br_if(1);
                      local_get(start); i32_const(1); i32_add; local_set(start);
                      br(0);
                    end; end;
                    // new_len = len - start
                    local_get(len); local_get(start); i32_sub; local_set(new_len);
                    // Alloc result
                    i32_const(list_hdr); local_get(new_len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(new_len); i32_store(0);
                    // Copy loop
                    i32_const(0); local_set(copy_i);
                    block_empty; loop_empty;
                      local_get(copy_i); local_get(new_len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(copy_i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(start); local_get(copy_i); i32_add;
                      i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                      local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(copy_i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(new_len);
                self.scratch.free_i32(start);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "count" => {
                // count(xs, pred) → Int
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let cnt = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    i32_const(0); local_set(i);
                    i32_const(0); local_set(cnt);
                    block_empty; loop_empty;
                      local_get(i); local_get(xs); i32_load(0); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&elem_ty, &Ty::Bool);
                wasm!(self.func, {
                      if_empty;
                        local_get(cnt); i32_const(1); i32_add; local_set(cnt);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(cnt); i64_extend_i32_u;
                });
                self.scratch.free_i32(cnt);
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "partition" => {
                // partition(xs, pred) → (List[A], List[A])
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let true_list = self.scratch.alloc_i32();
                let false_list = self.scratch.alloc_i32();
                let true_cnt = self.scratch.alloc_i32();
                let false_cnt = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let tuple_ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); local_set(len);
                    i32_const(list_hdr); local_get(len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(true_list);
                    i32_const(list_hdr); local_get(len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(false_list);
                    i32_const(0); local_set(true_cnt);
                    i32_const(0); local_set(false_cnt);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&elem_ty, &Ty::Bool);
                wasm!(self.func, {
                      if_i32;
                        local_get(true_list); i32_const(list_data_off); i32_add;
                        local_get(true_cnt); i32_const(es); i32_mul; i32_add;
                        local_get(xs); i32_const(list_data_off); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                        local_get(true_cnt); i32_const(1); i32_add; local_set(true_cnt);
                        i32_const(0);
                      else_;
                        local_get(false_list); i32_const(list_data_off); i32_add;
                        local_get(false_cnt); i32_const(es); i32_mul; i32_add;
                        local_get(xs); i32_const(list_data_off); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                        local_get(false_cnt); i32_const(1); i32_add; local_set(false_cnt);
                        i32_const(0);
                      end;
                      drop;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(true_list); local_get(true_cnt); i32_store(0);
                    local_get(false_list); local_get(false_cnt); i32_store(0);
                    i32_const(list_hdr); call(self.emitter.rt.alloc); local_set(tuple_ptr);
                    local_get(tuple_ptr); local_get(true_list); i32_store(0);
                    local_get(tuple_ptr); local_get(false_list); i32_store(4);
                    local_get(tuple_ptr);
                });
                self.scratch.free_i32(tuple_ptr);
                self.scratch.free_i32(i);
                self.scratch.free_i32(false_cnt);
                self.scratch.free_i32(true_cnt);
                self.scratch.free_i32(false_list);
                self.scratch.free_i32(true_list);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            _ => return false,
        }
        true
    }
}

include!("calls_list_closure2_p2.rs");
include!("calls_list_closure2_p3.rs");
include!("calls_list_closure2_p4.rs");
include!("calls_list_closure2_p5.rs");
