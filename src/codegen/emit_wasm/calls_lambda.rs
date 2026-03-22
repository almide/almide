//! Lambda and closure emission for WASM codegen.

use crate::ir::VarId;
use crate::types::Ty;
use wasm_encoder::ValType;

use super::FuncCompiler;
use super::values;

impl FuncCompiler<'_> {
    /// Emit a FnRef as a closure: allocate [wrapper_table_idx, 0] on heap.
    pub(super) fn emit_fn_ref_closure(&mut self, name: &str) {
        if let Some(&wrapper_table_idx) = self.emitter.fn_ref_wrappers.get(name) {
            // Allocate closure: [table_idx: i32][env_ptr: i32] = 8 bytes
            let scratch = self.scratch.alloc_i32();
            wasm!(self.func, {
                i32_const(8);
                call(self.emitter.rt.alloc);
                local_set(scratch);
                // Store table_idx
                local_get(scratch);
                i32_const(wrapper_table_idx as i32);
                i32_store(0);
                // Store env_ptr = 0
                local_get(scratch);
                i32_const(0);
                i32_store(4);
                // Return closure ptr
                local_get(scratch);
            });
            self.scratch.free_i32(scratch);
        } else {
            eprintln!("WARNING: FnRef wrapper not found for '{}', using direct table entry", name);
            wasm!(self.func, { unreachable; });
        }
    }

    /// Emit a lambda as a closure: allocate env + closure on heap.
    pub(super) fn emit_lambda_closure(&mut self, _params: &[(VarId, Ty)], _body: &crate::ir::IrExpr) {
        // Match lambda by param VarIds (deterministic, order-independent of scan)
        let param_key: Vec<u32> = _params.iter().map(|(vid, _)| vid.0).collect();
        let lambda_idx = self.emitter.lambdas.iter().enumerate()
            .find(|(_, info)| info.param_ids == param_key)
            .map(|(i, _)| i);

        let lambda_idx = match lambda_idx {
            Some(i) => i,
            None => {
                // Fallback to counter (for compatibility)
                let idx = self.emitter.lambda_counter.get();
                self.emitter.lambda_counter.set(idx + 1);
                if idx >= self.emitter.lambdas.len() {
                    wasm!(self.func, { unreachable; });
                    return;
                }
                idx
            }
        };

        let table_idx = self.emitter.lambdas[lambda_idx].table_idx;
        let captures = self.emitter.lambdas[lambda_idx].captures.clone();

        let scratch = self.scratch.alloc_i32();

        if captures.is_empty() {
            // No captures: allocate closure [table_idx, 0]
            wasm!(self.func, {
                i32_const(8);
                call(self.emitter.rt.alloc);
                local_set(scratch);
                local_get(scratch);
                i32_const(table_idx as i32);
                i32_store(0);
                local_get(scratch);
                i32_const(0);
                i32_store(4);
                local_get(scratch);
            });
            self.scratch.free_i32(scratch);
        } else {
            // Allocate env: each capture gets 8 bytes (padded for alignment)
            let env_size = (captures.len() as u32) * 8;
            let env_scratch = scratch;
            wasm!(self.func, {
                i32_const(env_size as i32);
                call(self.emitter.rt.alloc);
                local_set(env_scratch);
            });

            // Store each captured variable into env
            for (ci, (vid, ty)) in captures.iter().enumerate() {
                let offset = (ci as u32) * 8;
                let is_cell = self.emitter.mutable_captures.contains(&vid.0);
                wasm!(self.func, { local_get(env_scratch); });
                if let Some(&local_idx) = self.var_map.get(&vid.0) {
                    wasm!(self.func, { local_get(local_idx); });
                    if is_cell {
                        // Mutable capture: local is cell ptr (i32), store as i32
                        wasm!(self.func, { i32_store(offset); });
                    } else {
                        self.emit_store_at(ty, offset);
                    }
                } else {
                    match values::ty_to_valtype(ty) {
                        Some(ValType::I64) => { wasm!(self.func, { i64_const(0); }); }
                        Some(ValType::F64) => { wasm!(self.func, { f64_const(0.0); }); }
                        _ => { wasm!(self.func, { i32_const(0); }); }
                    }
                    self.emit_store_at(ty, offset);
                }
            }

            // Allocate closure: [table_idx, env_ptr]
            let closure_scratch = self.scratch.alloc_i32();
            wasm!(self.func, {
                i32_const(8);
                call(self.emitter.rt.alloc);
                local_set(closure_scratch);
                local_get(closure_scratch);
                i32_const(table_idx as i32);
                i32_store(0);
                local_get(closure_scratch);
                local_get(env_scratch);
                i32_store(4);
                local_get(closure_scratch);
            });
            self.scratch.free_i32(closure_scratch);
            self.scratch.free_i32(env_scratch);
        }
    }
}
