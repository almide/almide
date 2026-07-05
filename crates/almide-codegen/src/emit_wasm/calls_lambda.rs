//! Lambda and closure emission for WASM codegen.

use almide_ir::VarId;
use almide_lang::types::Ty;
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
            panic!("[ICE] emit_wasm: FnRef wrapper not found for `{}` — register it before emission", name);
        }
    }

    /// Emit a lambda as a closure: allocate env + closure on heap.
    pub(super) fn emit_lambda_closure(&mut self, _params: &[(VarId, Ty)], _body: &almide_ir::IrExpr, lambda_id: Option<u32>) {
        // Match lambda by lambda_id first (if available), then fall back to param VarIds
        let counter = self.emitter.lambda_counter.get();
        let lambda_idx = if let Some(lid) = lambda_id {
            // Match by lambda_id (unique, no skip needed)
            self.emitter.lambdas.iter().enumerate()
                .find(|(_, info)| info.lambda_id == Some(lid))
                .map(|(i, _)| i)
        } else {
            None
        }.or_else(|| {
            // Fall back to param VarIds matching (skip counter for ordering)
            let param_key: Vec<u32> = _params.iter().map(|(vid, _)| vid.0).collect();
            self.emitter.lambdas.iter().enumerate()
                .skip(counter)
                .find(|(_, info)| info.param_ids == param_key)
                .map(|(i, _)| i)
        });

        let lambda_idx = match lambda_idx {
            Some(i) => {
                self.emitter.lambda_counter.set(i + 1);
                i
            }
            None => {
                // Fallback: use counter directly
                let idx = counter;
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
                } else if let Some((global_idx, _)) = self.lookup_global(*vid) {
                    // Module-global capture: load through the SHARED lookup
                    // (the #500-fix sibling this arm was missing — it stored
                    // a silent typed ZERO into the env).
                    wasm!(self.func, { global_get(global_idx); });
                    self.emit_store_at(ty, offset);
                } else {
                    panic!(
                        "[ICE] lambda capture `{}` (VarId {}) resolved to neither local nor global (#522 class)",
                        if (vid.0 as usize) < self.var_table.len() { self.var_table.get(*vid).name.as_str() } else { "?" },
                        vid.0
                    );
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

    /// Emit a ClosureCreate node (from closure conversion pass).
    /// Allocates env, stores captures, creates [table_idx, env_ptr] pair.
    pub(super) fn emit_closure_create(&mut self, func_name: &almide_base::intern::Sym, captures: &[(VarId, Ty)]) {
        // Find the lifted function's index in the function table
        let name_str = func_name.to_string();
        let func_idx = self.emitter.func_map.get(&name_str).copied();
        let table_idx = func_idx.and_then(|fi| self.emitter.func_to_table_idx.get(&fi).copied());

        let table_idx = match table_idx {
            Some(ti) => ti as i32,
            None => {
                panic!("[ICE] emit_wasm: ClosureCreate target `{}` not in the function table — closure conversion / table registration skew", name_str);
            }
        };

        let scratch = self.scratch.alloc_i32();

        if captures.is_empty() {
            // No captures: closure = [table_idx, 0]
            wasm!(self.func, {
                i32_const(8);
                call(self.emitter.rt.alloc);
                local_set(scratch);
                local_get(scratch);
                i32_const(table_idx);
                i32_store(0);
                local_get(scratch);
                i32_const(0);
                i32_store(4);
                local_get(scratch);
            });
        } else {
            // Allocate env
            let env_size = (captures.len() as u32) * 8;
            let env_scratch = self.scratch.alloc_i32();
            wasm!(self.func, {
                i32_const(env_size as i32);
                call(self.emitter.rt.alloc);
                local_set(env_scratch);
            });

            // Store each captured variable into env
            for (ci, (vid, ty)) in captures.iter().enumerate() {
                let offset = (ci as u32) * 8;
                wasm!(self.func, { local_get(env_scratch); });
                if let Some(&local_idx) = self.var_map.get(&vid.0) {
                    // Perceus closure capture rc_inc is now handled by PerceusPass (IR-level RcInc)
                    wasm!(self.func, { local_get(local_idx); });
                    self.emit_store_at(ty, offset);
                } else if let Some((global_idx, _)) = self.lookup_global(*vid) {
                    // A module-level `let`/`var` captured by the closure lives
                    // in a WASM global, not the function-local var_map — load
                    // it via the SHARED global lookup (id, then the
                    // module_origin-prefixed key, then the bare name). The old
                    // id-only lookup missed cross-module synthetic VarIds and
                    // fell to the silent-zero arm below (#500).
                    wasm!(self.func, { global_get(global_idx); });
                    self.emit_store_at(ty, offset);
                } else {
                    // Post-#500 every legitimate storage class resolves
                    // (locals via var_map, globals via lookup_global). A miss
                    // here previously shipped a ZERO into the env behind a
                    // stderr warning nobody gates on — the next #500-class
                    // regression must be a build failure instead.
                    panic!(
                        "[ICE] ClosureCreate capture `{}` (VarId {}) resolved to neither local nor global (#522 class)",
                        if (vid.0 as usize) < self.var_table.len() { self.var_table.get(*vid).name.as_str() } else { "?" },
                        vid.0
                    );
                }
            }

            // Allocate closure pair [table_idx, env_ptr]
            let closure_scratch = self.scratch.alloc_i32();
            wasm!(self.func, {
                i32_const(8);
                call(self.emitter.rt.alloc);
                local_set(closure_scratch);
                local_get(closure_scratch);
                i32_const(table_idx);
                i32_store(0);
                local_get(closure_scratch);
                local_get(env_scratch);
                i32_store(4);
                local_get(closure_scratch);
            });
            self.scratch.free_i32(closure_scratch);
            self.scratch.free_i32(env_scratch);
        }

        self.scratch.free_i32(scratch);
    }
}
