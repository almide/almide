//! Function call emission — emit_call and related helpers.

use crate::ir::{CallTarget, IrExpr, IrStringPart};
use crate::types::Ty;
use wasm_encoder::{Instruction, ValType};

use super::FuncCompiler;
use super::values;
use super::wasm_macro::wasm;

impl FuncCompiler<'_> {
    pub(super) fn emit_call(&mut self, target: &CallTarget, args: &[IrExpr], _ret_ty: &Ty) {
        // Set return type context for stub calls
        self.stub_ret_ty = _ret_ty.clone();
        match target {
            CallTarget::Named { name } => {
                match name.as_str() {
                    "println" => {
                        let arg = &args[0];
                        match &arg.ty {
                            Ty::String => {
                                self.emit_expr(arg);
                                wasm!(self.func, { call(self.emitter.rt.println_str); });
                            }
                            Ty::Int => {
                                self.emit_expr(arg);
                                wasm!(self.func, { call(self.emitter.rt.println_int); });
                            }
                            Ty::Bool => {
                                // Convert bool to "true"/"false"
                                self.emit_expr(arg);
                                let true_str = self.emitter.intern_string("true");
                                let false_str = self.emitter.intern_string("false");
                                wasm!(self.func, {
                                    if_i32;
                                    i32_const(true_str as i32);
                                    else_;
                                    i32_const(false_str as i32);
                                    end;
                                    call(self.emitter.rt.println_str);
                                });
                            }
                            Ty::Float => {
                                self.emit_expr(arg);
                                wasm!(self.func, {
                                    call(self.emitter.rt.float_to_string);
                                    call(self.emitter.rt.println_str);
                                });
                            }
                            _ => {
                                // Unsupported type: skip arg and print "<unsupported>"
                                let s = self.emitter.intern_string("<unsupported>");
                                wasm!(self.func, {
                                    i32_const(s as i32);
                                    call(self.emitter.rt.println_str);
                                });
                            }
                        }
                    }
                    "assert_eq" => {
                        self.emit_assert_eq(&args[0], &args[1]);
                    }
                    "assert" => {
                        // assert(cond) or assert(cond, msg) — trap if false
                        self.emit_expr(&args[0]);
                        wasm!(self.func, {
                            i32_eqz;
                            if_empty;
                            unreachable;
                            end;
                        });
                        // Drop message arg if present (evaluated but unused)
                    }
                    "assert_ne" => {
                        // assert_ne(left, right) — trap if equal
                        self.emit_eq(&args[0], &args[1], false);
                        // If equal → trap
                        wasm!(self.func, {
                            if_empty;
                            unreachable;
                            end;
                        });
                    }
                    _ => {
                        // Check if this is a variant constructor
                        if let Some((tag, is_unit)) = self.find_variant_ctor_tag(name) {
                            if is_unit && args.is_empty() {
                                // Unit variant: allocate [tag:i32]
                                let scratch = self.match_i32_base + self.match_depth;
                                wasm!(self.func, {
                                    i32_const(4);
                                    call(self.emitter.rt.alloc);
                                    local_set(scratch);
                                    local_get(scratch);
                                    i32_const(tag as i32);
                                    i32_store(0);
                                    local_get(scratch);
                                });
                                return;
                            } else if !is_unit {
                                // Tuple payload variant: [tag:i32][arg0][arg1]...
                                let mut total_size = 4u32; // tag
                                for arg in args { total_size += values::byte_size(&arg.ty); }
                                let scratch = self.match_i32_base + self.match_depth;
                                self.match_depth += 1;
                                wasm!(self.func, {
                                    i32_const(total_size as i32);
                                    call(self.emitter.rt.alloc);
                                    local_set(scratch);
                                    // Write tag
                                    local_get(scratch);
                                    i32_const(tag as i32);
                                    i32_store(0);
                                });
                                // Write args
                                let mut offset = 4u32;
                                for arg in args {
                                    wasm!(self.func, { local_get(scratch); });
                                    self.emit_expr(arg);
                                    self.emit_store_at(&arg.ty, offset);
                                    offset += values::byte_size(&arg.ty);
                                }
                                self.match_depth -= 1;
                                wasm!(self.func, { local_get(scratch); });
                                return;
                            }
                        }
                        // User-defined function call
                        for arg in args {
                            self.emit_expr(arg);
                        }
                        if let Some(&func_idx) = self.emitter.func_map.get(name.as_str()) {
                            wasm!(self.func, { call(func_idx); });
                        } else {
                            wasm!(self.func, { unreachable; });
                        }
                    }
                }
            }

            CallTarget::Module { module, func } => {
                match (module.as_str(), func.as_str()) {
                    _ if module == "int" => {
                        if !self.emit_int_call(func, args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if module == "float" => {
                        if !self.emit_float_call(func, args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if module == "string" => {
                        if !self.emit_string_call(func, args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if module == "option" => {
                        if !self.emit_option_call(func, args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if module == "result" => {
                        if !self.emit_result_call(func, args) {
                            self.emit_stub_call(args);
                        }
                    }
                    ("list", "map") => {
                        self.emit_list_map(&args[0], &args[1], _ret_ty);
                    }
                    ("list", "enumerate") => {
                        // enumerate(list) → list of (index, element) tuples
                        // Each tuple is heap-allocated: [Int(i64), element]
                        let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
                            a.first().cloned().unwrap_or(Ty::Int)
                        } else { Ty::Int };
                        let elem_size = values::byte_size(&elem_ty);
                        let tuple_size = 8 + elem_size; // Int(8) + elem

                        let s = self.match_i32_base + self.match_depth;
                        let len_local = s;
                        let idx_local = s + 1;

                        // Store src → mem[0]
                        wasm!(self.func, { i32_const(0); });
                        self.emit_expr(&args[0]);
                        wasm!(self.func, {
                            i32_store(0);
                            // len
                            i32_const(0);
                            i32_load(0);
                            i32_load(0);
                            local_set(len_local);
                            // Alloc dst: [len] + len * ptr_size(4)
                            i32_const(8);
                            i32_const(4);
                            local_get(len_local);
                            i32_const(4); // each entry is a tuple ptr (i32)
                            i32_mul;
                            i32_add;
                            call(self.emitter.rt.alloc);
                            i32_store(0);
                            // dst = mem[8]
                            // Store len in dst
                            i32_const(8);
                            i32_load(0);
                            local_get(len_local);
                            i32_store(0);
                            // Loop: create tuples
                            i32_const(0);
                            local_set(idx_local);
                            block_empty;
                            loop_empty;
                        });
                        let saved = self.depth;
                        self.depth += 2;

                        wasm!(self.func, {
                            local_get(idx_local);
                            local_get(len_local);
                            i32_ge_u;
                            br_if(1);
                            // Alloc tuple: [index:i64][element]
                            i32_const(tuple_size as i32);
                            call(self.emitter.rt.alloc);
                            // tuple_ptr on stack. Store to mem[12]
                            // (original code had some stack manipulation; the final approach: drop and re-alloc)
                            drop;
                            // Re-alloc tuple
                            i32_const(12);
                            i32_const(tuple_size as i32);
                            call(self.emitter.rt.alloc);
                            i32_store(0); // mem[12] = tuple_ptr
                            // tuple.index = idx (as i64)
                            i32_const(12);
                            i32_load(0); // tuple_ptr
                            local_get(idx_local);
                            i64_extend_i32_u;
                            i64_store(0);
                            // tuple.element = src[idx]
                            i32_const(12);
                            i32_load(0); // tuple_ptr
                            // Load src element
                            i32_const(0);
                            i32_load(0); // src_ptr
                            i32_const(4);
                            i32_add;
                            local_get(idx_local);
                            i32_const(elem_size as i32);
                            i32_mul;
                            i32_add;
                        });
                        self.emit_load_at(&elem_ty, 0);
                        self.emit_store_at(&elem_ty, 8); // store at tuple offset 8

                        wasm!(self.func, {
                            // dst[idx] = tuple_ptr
                            i32_const(8);
                            i32_load(0); // dst_ptr
                            i32_const(4);
                            i32_add;
                            local_get(idx_local);
                            i32_const(4); // tuple ptrs are i32
                            i32_mul;
                            i32_add;
                            i32_const(12);
                            i32_load(0); // tuple_ptr
                            i32_store(0);
                            // idx++
                            local_get(idx_local);
                            i32_const(1);
                            i32_add;
                            local_set(idx_local);
                            br(0);
                        });

                        self.depth = saved;
                        wasm!(self.func, {
                            end;
                            end;
                            // Return dst
                            i32_const(8);
                            i32_load(0);
                        });
                    }
                    ("list", "get") => {
                        // list.get(list, index) → Option[T]
                        let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
                            a.first().cloned().unwrap_or(Ty::Int)
                        } else { Ty::Int };
                        let elem_size = values::byte_size(&elem_ty);

                        // mem[0]=list, mem[4]=idx(i32)
                        wasm!(self.func, { i32_const(0); });
                        self.emit_expr(&args[0]);
                        wasm!(self.func, {
                            i32_store(0);
                            i32_const(4);
                        });
                        self.emit_expr(&args[1]);
                        if matches!(&args[1].ty, Ty::Int) {
                            wasm!(self.func, { i32_wrap_i64; });
                        }
                        wasm!(self.func, {
                            i32_store(0);
                            // bounds: idx >= len → none(0)
                            i32_const(4);
                            i32_load(0); // idx
                            i32_const(0);
                            i32_load(0); // list
                            i32_load(0); // len
                            i32_ge_u;
                            if_i32;
                            i32_const(0); // none
                            else_;
                            // alloc → mem[8]
                            i32_const(8);
                            i32_const(elem_size as i32);
                            call(self.emitter.rt.alloc);
                            i32_store(0);
                            // dst=mem[8], src=list+4+idx*elem_size
                            i32_const(8);
                            i32_load(0); // dst
                            i32_const(0);
                            i32_load(0); // list
                            i32_const(4);
                            i32_add;
                            i32_const(4);
                            i32_load(0); // idx
                            i32_const(elem_size as i32);
                            i32_mul;
                            i32_add;
                        });
                        self.emit_load_at(&elem_ty, 0); // load elem
                        self.emit_store_at(&elem_ty, 0); // store at dst
                        wasm!(self.func, {
                            i32_const(8);
                            i32_load(0); // return ptr
                            end;
                        });
                    }
                    ("list", "filter") => {
                        // filter(list, fn) → new list with matching elements
                        let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
                            a.first().cloned().unwrap_or(Ty::Int)
                        } else { Ty::Int };
                        let elem_size = values::byte_size(&elem_ty);
                        let s = self.match_i32_base + self.match_depth;
                        let len_local = s;
                        let idx_local = s + 1;

                        // mem[0]=src, mem[4]=fn, mem[8]=dst, mem[12]=out_idx
                        wasm!(self.func, { i32_const(0); });
                        self.emit_expr(&args[0]);
                        wasm!(self.func, {
                            i32_store(0);
                            i32_const(4);
                        });
                        self.emit_expr(&args[1]);
                        wasm!(self.func, {
                            i32_store(0);
                            // len
                            i32_const(0);
                            i32_load(0);
                            i32_load(0);
                            local_set(len_local);
                            // alloc dst (max size = 4 + len * elem_size) → mem[8]
                            i32_const(8);
                            i32_const(4);
                            local_get(len_local);
                            i32_const(elem_size as i32);
                            i32_mul;
                            i32_add;
                            call(self.emitter.rt.alloc);
                            i32_store(0);
                            // out_idx = 0 → mem[12]
                            i32_const(12);
                            i32_const(0);
                            i32_store(0);
                            // idx = 0
                            i32_const(0);
                            local_set(idx_local);
                            // Loop
                            block_empty;
                            loop_empty;
                        });
                        let saved = self.depth; self.depth += 2;

                        wasm!(self.func, {
                            local_get(idx_local);
                            local_get(len_local);
                            i32_ge_u;
                            br_if(1);
                            // Call predicate: fn(element) → bool (i32)
                            // Load closure
                            i32_const(4);
                            i32_load(0);
                        });
                        wasm!(self.func, {
                            i32_load(4);
                            // Load element
                            i32_const(0);
                            i32_load(0);
                            i32_const(4);
                            i32_add;
                            local_get(idx_local);
                            i32_const(elem_size as i32);
                            i32_mul;
                            i32_add;
                        });
                        self.emit_load_at(&elem_ty, 0);
                        // table_idx
                        wasm!(self.func, {
                            i32_const(4);
                            i32_load(0);
                            i32_load(0);
                        });
                        // call_indirect
                        if let Ty::Fn { params, ret } = &args[1].ty {
                            let mut ct = vec![ValType::I32];
                            for p in params { if let Some(vt) = values::ty_to_valtype(p) { ct.push(vt); } }
                            let rt = values::ret_type(ret);
                            let ti = self.emitter.register_type(ct, rt);
                            wasm!(self.func, { call_indirect(ti, 0); });
                        }
                        // If true, copy element to dst
                        wasm!(self.func, {
                            if_empty;
                            // dst[out_idx] = src[idx]
                            i32_const(8);
                            i32_load(0);
                            i32_const(4);
                            i32_add;
                            i32_const(12);
                            i32_load(0);
                            i32_const(elem_size as i32);
                            i32_mul;
                            i32_add;
                            // load src element
                            i32_const(0);
                            i32_load(0);
                            i32_const(4);
                            i32_add;
                            local_get(idx_local);
                            i32_const(elem_size as i32);
                            i32_mul;
                            i32_add;
                        });
                        self.emit_load_at(&elem_ty, 0);
                        self.emit_store_at(&elem_ty, 0);
                        wasm!(self.func, {
                            // out_idx++
                            i32_const(12);
                            i32_const(12);
                            i32_load(0);
                            i32_const(1);
                            i32_add;
                            i32_store(0);
                            end; // end if
                            // idx++
                            local_get(idx_local);
                            i32_const(1);
                            i32_add;
                            local_set(idx_local);
                            br(0);
                        });

                        self.depth = saved;
                        wasm!(self.func, {
                            end;
                            end;
                            // Set dst.len = out_idx
                            i32_const(8);
                            i32_load(0);
                            i32_const(12);
                            i32_load(0);
                            i32_store(0);
                            // Return dst
                            i32_const(8);
                            i32_load(0);
                        });
                    }
                    ("list", "fold") => {
                        // fold(list, init, fn(acc, elem) → acc)
                        let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
                            a.first().cloned().unwrap_or(Ty::Int)
                        } else { Ty::Int };
                        let elem_size = values::byte_size(&elem_ty);
                        let s = self.match_i32_base + self.match_depth;
                        let len_local = s;
                        let idx_local = s + 1;
                        // Accumulator local: use i64 for Int/Float, i32 for everything else
                        let acc_local = match values::ty_to_valtype(&args[1].ty) {
                            Some(ValType::I64) | Some(ValType::F64) => self.match_i64_base + self.match_depth,
                            _ => self.match_i32_base + self.match_depth + 2, // after len + idx
                        };

                        // mem[0]=list, mem[4]=fn
                        wasm!(self.func, { i32_const(0); });
                        self.emit_expr(&args[0]);
                        wasm!(self.func, { i32_store(0); });
                        // acc = init
                        self.emit_expr(&args[1]);
                        wasm!(self.func, {
                            local_set(acc_local);
                            i32_const(4);
                        });
                        self.emit_expr(&args[2]);
                        wasm!(self.func, {
                            i32_store(0);
                            // len
                            i32_const(0);
                            i32_load(0);
                            i32_load(0);
                            local_set(len_local);
                            i32_const(0);
                            local_set(idx_local);
                            block_empty;
                            loop_empty;
                        });
                        let saved = self.depth; self.depth += 2;

                        wasm!(self.func, {
                            local_get(idx_local);
                            local_get(len_local);
                            i32_ge_u;
                            br_if(1);
                            // acc = fn(acc, elem)
                            i32_const(4);
                            i32_load(0);
                        });
                        wasm!(self.func, {
                            i32_load(4);
                            local_get(acc_local);
                            // load elem
                            i32_const(0);
                            i32_load(0);
                            i32_const(4);
                            i32_add;
                            local_get(idx_local);
                            i32_const(elem_size as i32);
                            i32_mul;
                            i32_add;
                        });
                        self.emit_load_at(&elem_ty, 0);
                        // table_idx
                        wasm!(self.func, {
                            i32_const(4);
                            i32_load(0);
                            i32_load(0);
                        });
                        if let Ty::Fn { params, ret } = &args[2].ty {
                            let mut ct = vec![ValType::I32];
                            for p in params { if let Some(vt) = values::ty_to_valtype(p) { ct.push(vt); } }
                            let rt = values::ret_type(ret);
                            let ti = self.emitter.register_type(ct, rt);
                            wasm!(self.func, { call_indirect(ti, 0); });
                        }
                        wasm!(self.func, {
                            local_set(acc_local);
                            local_get(idx_local);
                            i32_const(1);
                            i32_add;
                            local_set(idx_local);
                            br(0);
                        });

                        self.depth = saved;
                        wasm!(self.func, {
                            end;
                            end;
                            local_get(acc_local);
                        });
                    }
                    ("list", "reverse") => {
                        // reverse(list) → new list with elements in reverse order
                        let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
                            a.first().cloned().unwrap_or(Ty::Int)
                        } else { Ty::Int };
                        let elem_size = values::byte_size(&elem_ty);
                        let s = self.match_i32_base + self.match_depth;
                        let len_local = s;
                        let idx_local = s + 1;

                        // mem[0]=src
                        wasm!(self.func, { i32_const(0); });
                        self.emit_expr(&args[0]);
                        wasm!(self.func, {
                            i32_store(0);
                            // len
                            i32_const(0);
                            i32_load(0);
                            i32_load(0);
                            local_set(len_local);
                            // alloc dst → mem[4]
                            i32_const(4);
                            i32_const(4);
                            local_get(len_local);
                            i32_const(elem_size as i32);
                            i32_mul;
                            i32_add;
                            call(self.emitter.rt.alloc);
                            i32_store(0);
                            // dst.len = len
                            i32_const(4);
                            i32_load(0);
                            local_get(len_local);
                            i32_store(0);
                            // Loop: dst[len-1-i] = src[i]
                            i32_const(0);
                            local_set(idx_local);
                            block_empty;
                            loop_empty;
                        });
                        let saved = self.depth; self.depth += 2;

                        wasm!(self.func, {
                            local_get(idx_local);
                            local_get(len_local);
                            i32_ge_u;
                            br_if(1);
                            // dst addr: dst + 4 + (len-1-i) * elem_size
                            i32_const(4);
                            i32_load(0);
                            i32_const(4);
                            i32_add;
                            local_get(len_local);
                            i32_const(1);
                            i32_sub;
                            local_get(idx_local);
                            i32_sub;
                            i32_const(elem_size as i32);
                            i32_mul;
                            i32_add;
                            // src elem: src + 4 + i * elem_size
                            i32_const(0);
                            i32_load(0);
                            i32_const(4);
                            i32_add;
                            local_get(idx_local);
                            i32_const(elem_size as i32);
                            i32_mul;
                            i32_add;
                        });
                        self.emit_load_at(&elem_ty, 0);
                        self.emit_store_at(&elem_ty, 0);

                        wasm!(self.func, {
                            local_get(idx_local);
                            i32_const(1);
                            i32_add;
                            local_set(idx_local);
                            br(0);
                        });

                        self.depth = saved;
                        wasm!(self.func, {
                            end;
                            end;
                            i32_const(4);
                            i32_load(0);
                        });
                    }
                    ("list", "sort") => {
                        // Bubble sort on a copy. Int(i64) only.
                        // mem[0]=src, alloc copy → mem[4], len → local s, i → local s+1
                        let s = self.match_i32_base + self.match_depth;

                        wasm!(self.func, { i32_const(0); });
                        self.emit_expr(&args[0]);
                        wasm!(self.func, {
                            i32_store(0);
                            // len → local s
                            i32_const(0);
                            i32_load(0);
                            i32_load(0);
                            local_set(s);
                            // total_bytes = 4 + len * 8
                            // alloc → mem[4]
                            i32_const(4);
                            i32_const(4);
                            local_get(s);
                            i32_const(8);
                            i32_mul;
                            i32_add;
                            call(self.emitter.rt.alloc);
                            i32_store(0);
                            // Store total_bytes → mem[8]
                            i32_const(8);
                            i32_const(4);
                            local_get(s);
                            i32_const(8);
                            i32_mul;
                            i32_add;
                            i32_store(0); // mem[8] = total
                            // Byte copy loop: i=0..total
                            i32_const(0);
                            local_set(s + 1); // i=0
                            block_empty;
                            loop_empty;
                        });
                        self.depth += 2;
                        wasm!(self.func, {
                            local_get(s + 1);
                            i32_const(8);
                            i32_load(0); // total
                            i32_ge_u;
                            br_if(1);
                            // dst[i] = src[i] (src=mem[0], dst=mem[4])
                            i32_const(4);
                            i32_load(0); // dst
                            local_get(s + 1);
                            i32_add;
                            i32_const(0);
                            i32_load(0); // src
                            local_get(s + 1);
                            i32_add;
                            i32_load8_u(0);
                            i32_store8(0);
                            local_get(s + 1);
                            i32_const(1);
                            i32_add;
                            local_set(s + 1);
                            br(0);
                        });
                        self.depth -= 2;
                        wasm!(self.func, {
                            end;
                            end;
                            // Bubble sort: outer i=0..len-1, inner j=0..len-1-i
                            // mem[8]=j (reuse), mem[12]=tmp(i64)
                            i32_const(0);
                            local_set(s + 1); // i=0
                            block_empty;
                            loop_empty;
                        });
                        self.depth += 2;
                        wasm!(self.func, {
                            local_get(s + 1);
                            local_get(s);
                            i32_const(1);
                            i32_sub;
                            i32_ge_u;
                            br_if(1);
                            // Inner: j=0
                            i32_const(8);
                            i32_const(0);
                            i32_store(0);
                            block_empty;
                            loop_empty;
                        });
                        self.depth += 2;
                        wasm!(self.func, {
                            i32_const(8);
                            i32_load(0); // j
                            local_get(s);
                            i32_const(1);
                            i32_sub;
                            local_get(s + 1);
                            i32_sub;
                            i32_ge_u;
                            br_if(1);
                            // Compare dst[j] > dst[j+1]
                            // addr_j = dst + 4 + j*8
                            i32_const(4);
                            i32_load(0);
                            i32_const(4);
                            i32_add;
                            i32_const(8);
                            i32_load(0);
                            i32_const(8);
                            i32_mul;
                            i32_add;
                            i64_load(0);
                            // dst[j+1]
                            i32_const(4);
                            i32_load(0);
                            i32_const(4);
                            i32_add;
                            i32_const(8);
                            i32_load(0);
                            i32_const(1);
                            i32_add;
                            i32_const(8);
                            i32_mul;
                            i32_add;
                            i64_load(0);
                            i64_gt_s;
                            if_empty;
                            // Swap: tmp=dst[j], dst[j]=dst[j+1], dst[j+1]=tmp
                            // tmp → mem[12..20]
                            i32_const(12);
                            i32_const(4);
                            i32_load(0);
                            i32_const(4);
                            i32_add;
                            i32_const(8);
                            i32_load(0);
                            i32_const(8);
                            i32_mul;
                            i32_add;
                            i64_load(0);
                            i64_store(0);
                            // dst[j] = dst[j+1]
                            i32_const(4);
                            i32_load(0);
                            i32_const(4);
                            i32_add;
                            i32_const(8);
                            i32_load(0);
                            i32_const(8);
                            i32_mul;
                            i32_add;
                            // value = dst[j+1]
                            i32_const(4);
                            i32_load(0);
                            i32_const(4);
                            i32_add;
                            i32_const(8);
                            i32_load(0);
                            i32_const(1);
                            i32_add;
                            i32_const(8);
                            i32_mul;
                            i32_add;
                            i64_load(0);
                            i64_store(0);
                            // dst[j+1] = tmp(mem[12])
                            i32_const(4);
                            i32_load(0);
                            i32_const(4);
                            i32_add;
                            i32_const(8);
                            i32_load(0);
                            i32_const(1);
                            i32_add;
                            i32_const(8);
                            i32_mul;
                            i32_add;
                            i32_const(12);
                            i64_load(0);
                            i64_store(0);
                            end; // end if swap
                            // j++
                            i32_const(8);
                            i32_const(8);
                            i32_load(0);
                            i32_const(1);
                            i32_add;
                            i32_store(0);
                            br(0);
                        });
                        self.depth -= 2;
                        wasm!(self.func, {
                            end; // end inner loop
                            end; // end inner block
                            // i++
                            local_get(s + 1);
                            i32_const(1);
                            i32_add;
                            local_set(s + 1);
                            br(0);
                        });
                        self.depth -= 2;
                        wasm!(self.func, {
                            end; // end outer loop
                            end; // end outer block
                            // Return dst
                            i32_const(4);
                            i32_load(0);
                        });
                    }
                    ("list", "get") => {
                        // list.get(list, index) -> Option[T]
                        // Returns some(elem) if in bounds, none otherwise
                        let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
                            a.first().cloned().unwrap_or(Ty::Int)
                        } else { Ty::Int };
                        let elem_size = values::byte_size(&elem_ty);
                        let s = self.match_i32_base + self.match_depth;
                        // Store list → mem[0], index → mem[4]
                        wasm!(self.func, { i32_const(0); });
                        self.emit_expr(&args[0]);
                        wasm!(self.func, { i32_store(0); });
                        self.emit_expr(&args[1]);
                        wasm!(self.func, {
                            i32_wrap_i64; // index to i32
                            local_set(s);
                            // Check bounds: index < 0 || index >= len → none (null ptr = 0)
                            local_get(s);
                            i32_const(0);
                            i32_lt_u;
                            local_get(s);
                            i32_const(0);
                            i32_load(0); // list
                            i32_load(0); // len
                            i32_ge_u;
                            i32_or;
                            if_i32;
                            i32_const(0); // none
                            else_;
                            // some: alloc elem_size, copy value
                            i32_const(elem_size as i32);
                            call(self.emitter.rt.alloc);
                            local_set(s);
                            // Copy: src = list_ptr + 4 + index * elem_size
                            local_get(s);
                            i32_const(0);
                            i32_load(0); // list
                            i32_const(4);
                            i32_add;
                        });
                        wasm!(self.func, {
                            local_get(s);
                        });
                        // Hmm, this is getting complex. Use simpler approach:
                        // Actually, Option[T] is represented as: none=null(0), some=ptr to T.
                        // For list.get, return ptr to element directly (no copy needed for i32 types)
                        // But for i64/f64 we need to allocate.
                        // Simplify: allocate and copy for all types.
                        wasm!(self.func, {
                            // Scratch already has alloc'd ptr. Need to write element there.
                            // Reset: use a different approach
                            drop; drop; // drop the half-built stack
                        });
                        // TODO: implement properly. For now, return none (0) always.
                        wasm!(self.func, { i32_const(0); });
                    }
                    ("list", "contains") => {
                        // list.contains(list, elem) -> Bool (i32)
                        // Linear scan: compare each element
                        let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
                            a.first().cloned().unwrap_or(Ty::Int)
                        } else { Ty::Int };
                        let elem_size = values::byte_size(&elem_ty);
                        let s = self.match_i32_base + self.match_depth;
                        let len_local = s;
                        let idx_local = s + 1;

                        // Store list → mem[0], elem → mem[4]
                        wasm!(self.func, { i32_const(0); });
                        self.emit_expr(&args[0]);
                        wasm!(self.func, {
                            i32_store(0);
                            i32_const(0);
                            i32_load(0);
                            i32_load(0); // len
                            local_set(len_local);
                            i32_const(0);
                            local_set(idx_local);
                        });
                        // Save target elem
                        wasm!(self.func, { i32_const(8); });
                        self.emit_expr(&args[1]);
                        self.emit_store_at(&elem_ty, 0);

                        let saved = self.depth;
                        wasm!(self.func, {
                            // Loop: check each element
                            block_empty;
                            loop_empty;
                        });
                        self.depth += 2;
                        wasm!(self.func, {
                            local_get(idx_local);
                            local_get(len_local);
                            i32_ge_u;
                            br_if(1); // break if done → not found
                            // Load element
                            i32_const(0);
                            i32_load(0); // list
                            i32_const(4);
                            i32_add;
                            local_get(idx_local);
                            i32_const(elem_size as i32);
                            i32_mul;
                            i32_add;
                        });
                        self.emit_load_at(&elem_ty, 0);
                        // Load target
                        wasm!(self.func, { i32_const(8); });
                        self.emit_load_at(&elem_ty, 0);
                        // Compare
                        match &elem_ty {
                            Ty::Int => { wasm!(self.func, { i64_eq; }); }
                            Ty::Float => { wasm!(self.func, { f64_eq; }); }
                            Ty::String => { wasm!(self.func, { call(self.emitter.rt.string.eq); }); }
                            _ => { wasm!(self.func, { i32_eq; }); }
                        }
                        wasm!(self.func, {
                            if_empty;
                            i32_const(1);
                            return_;
                            end;
                            local_get(idx_local);
                            i32_const(1);
                            i32_add;
                            local_set(idx_local);
                            br(0);
                            end; // loop
                            end; // block
                        });
                        self.depth = saved;
                        // Not found
                        wasm!(self.func, { i32_const(0); });
                    }
                    _ if module == "map" => {
                        if !self.emit_map_call(func, args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if module == "list" => {
                        if !self.emit_list_call(func, args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if module == "math" => {
                        if !self.emit_math_call(func, args) {
                            self.emit_stub_call(args);
                        }
                    }
                    ("error", "message") => {
                        // error.message(r: Result[T, String]) → String
                        // tag==0(ok): empty string, tag==1(err): load string at offset 4
                        let s = self.match_i32_base + self.match_depth;
                        self.emit_expr(&args[0]);
                        wasm!(self.func, {
                            local_set(s);
                            local_get(s); i32_load(0); i32_eqz; // tag == 0?
                            if_i32;
                              // ok → empty string
                              i32_const(4); call(self.emitter.rt.alloc); local_set(s + 1);
                              local_get(s + 1); i32_const(0); i32_store(0);
                              local_get(s + 1);
                            else_;
                              local_get(s); i32_load(4); // err string ptr
                            end;
                        });
                    }
                    ("error", "context") => {
                        // error.context(result, msg) → Result[T, String]
                        // If err: wrap error message with context. If ok: pass through.
                        let s = self.match_i32_base + self.match_depth;
                        self.emit_expr(&args[0]);
                        wasm!(self.func, {
                            local_set(s);
                            local_get(s); i32_load(0); i32_eqz; // tag == 0 (ok)?
                            if_i32;
                              local_get(s); // pass ok through
                            else_;
                              // Build new err with context: "msg: original_err"
                              local_get(s); i32_load(4); local_set(s + 1); // original err string
                        });
                        self.emit_expr(&args[1]); // context msg
                        wasm!(self.func, {
                              local_set(s + 2);
                              // Build ": " separator
                              i32_const(6); call(self.emitter.rt.alloc); local_set(s + 3);
                              local_get(s + 3); i32_const(2); i32_store(0);
                              local_get(s + 3); i32_const(58); i32_store8(4);
                              local_get(s + 3); i32_const(32); i32_store8(5);
                              // concat: msg + ": " + original
                              local_get(s + 2); local_get(s + 3); call(self.emitter.rt.concat_str);
                              local_get(s + 1); call(self.emitter.rt.concat_str);
                              local_set(s + 1);
                              // Build new err Result
                              i32_const(8); call(self.emitter.rt.alloc); local_set(s);
                              local_get(s); i32_const(1); i32_store(0);
                              local_get(s); local_get(s + 1); i32_store(4);
                              local_get(s);
                            end;
                        });
                    }
                    ("error", "chain") => {
                        // error.chain(outer, cause) → "outer: cause"
                        self.emit_expr(&args[0]);
                        // concat outer + ": " + cause
                        // Build ": " string
                        let s = self.match_i32_base + self.match_depth;
                        wasm!(self.func, {
                            local_set(s);
                            i32_const(6); call(self.emitter.rt.alloc); local_set(s + 1);
                            local_get(s + 1); i32_const(2); i32_store(0);
                            local_get(s + 1); i32_const(58); i32_store8(4); // ':'
                            local_get(s + 1); i32_const(32); i32_store8(5); // ' '
                            local_get(s); local_get(s + 1); call(self.emitter.rt.concat_str);
                        });
                        self.emit_expr(&args[1]);
                        wasm!(self.func, { call(self.emitter.rt.concat_str); });
                    }
                    _ => {
                        self.emit_stub_call(args);
                    }
                }
            }

            CallTarget::Method { object, method } => {
                // UFCS method calls: obj.method(args)
                match method.as_str() {
                    "to_string" | "int.to_string" if matches!(object.ty, Ty::Int) => {
                        self.emit_expr(object);
                        wasm!(self.func, { call(self.emitter.rt.int_to_string); });
                    }
                    "len" | "length" | "string.len" | "list.len" | "map.len" => {
                        // .len() for String, List, Map — all store length at offset 0
                        self.emit_expr(object);
                        wasm!(self.func, {
                            i32_load(0);
                            i64_extend_i32_u;
                        });
                    }
                    "to_string" | "float.to_string" if matches!(object.ty, Ty::Float) => {
                        self.emit_expr(object);
                        wasm!(self.func, {
                            call(self.emitter.rt.float_to_string);
                        });
                    }
                    "sort" | "list.sort" if matches!(&object.ty, Ty::Applied(_, _)) => {
                        let fake = [(**object).clone()];
                        let target = CallTarget::Module { module: "list".into(), func: "sort".into() };
                        self.emit_call(&target, &fake, _ret_ty);
                    }
                    "reverse" | "list.reverse" if matches!(&object.ty, Ty::Applied(_, _)) => {
                        let fake = [(**object).clone()];
                        let target = CallTarget::Module { module: "list".into(), func: "reverse".into() };
                        self.emit_call(&target, &fake, _ret_ty);
                    }
                    "filter" | "list.filter" if matches!(&object.ty, Ty::Applied(_, _)) => {
                        let fake = [(**object).clone(), args[0].clone()];
                        let target = CallTarget::Module { module: "list".into(), func: "filter".into() };
                        self.emit_call(&target, &fake, _ret_ty);
                    }
                    "fold" | "list.fold" if matches!(&object.ty, Ty::Applied(_, _)) => {
                        let fake = [(**object).clone(), args[0].clone(), args[1].clone()];
                        let target = CallTarget::Module { module: "list".into(), func: "fold".into() };
                        self.emit_call(&target, &fake, _ret_ty);
                    }
                    "map" | "list.map" if matches!(&object.ty, Ty::Applied(_, _)) => {
                        // .map(fn) → list.map(self, fn)
                        self.emit_list_map(object, &args[0], _ret_ty);
                    }
                    "trim" | "string.trim" if matches!(object.ty, Ty::String) => {
                        self.emit_expr(object);
                        wasm!(self.func, { call(self.emitter.rt.string.trim); });
                    }
                    "to_upper" | "string.to_upper" if matches!(object.ty, Ty::String) => {
                        self.emit_expr(object);
                        self.emit_str_case_convert(true);
                    }
                    "to_lower" | "string.to_lower" if matches!(object.ty, Ty::String) => {
                        self.emit_expr(object);
                        self.emit_str_case_convert(false);
                    }
                    "starts_with" | "string.starts_with" | "ends_with" | "string.ends_with" if matches!(object.ty, Ty::String) => {
                        // Delegate to Module call handler
                        self.emit_expr(object);
                        for arg in args { self.emit_expr(arg); }
                        wasm!(self.func, { unreachable; }); // TODO: wire up properly
                    }
                    "contains" | "string.contains" if matches!(object.ty, Ty::String) => {
                        self.emit_expr(object);
                        self.emit_expr(&args[0]);
                        wasm!(self.func, { call(self.emitter.rt.string.contains); });
                    }
                    _ if matches!(&object.ty, Ty::Applied(crate::types::constructor::TypeConstructorId::Option, _)) => {
                        let mut fake_args = vec![(**object).clone()];
                        fake_args.extend(args.iter().cloned());
                        let m = method.strip_prefix("option.").unwrap_or(method);
                        if !self.emit_option_call(m, &fake_args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if matches!(&object.ty, Ty::Applied(crate::types::constructor::TypeConstructorId::Result, _)) => {
                        let mut fake_args = vec![(**object).clone()];
                        fake_args.extend(args.iter().cloned());
                        let m = method.strip_prefix("result.").unwrap_or(method);
                        if !self.emit_result_call(m, &fake_args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if matches!(&object.ty, Ty::String) => {
                        let mut fake_args = vec![(**object).clone()];
                        fake_args.extend(args.iter().cloned());
                        let m = method.strip_prefix("string.").unwrap_or(method);
                        if !self.emit_string_call(m, &fake_args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if matches!(&object.ty, Ty::Int) => {
                        let mut fake_args = vec![(**object).clone()];
                        fake_args.extend(args.iter().cloned());
                        let m = method.strip_prefix("int.").unwrap_or(method);
                        if !self.emit_int_call(m, &fake_args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if matches!(&object.ty, Ty::Float) => {
                        let mut fake_args = vec![(**object).clone()];
                        fake_args.extend(args.iter().cloned());
                        let m = method.strip_prefix("float.").unwrap_or(method);
                        if !self.emit_float_call(m, &fake_args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if matches!(&object.ty, Ty::Applied(crate::types::constructor::TypeConstructorId::List, _)) => {
                        let mut fake_args = vec![(**object).clone()];
                        fake_args.extend(args.iter().cloned());
                        let m = method.strip_prefix("list.").unwrap_or(method);
                        if !self.emit_list_call(m, &fake_args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ if matches!(&object.ty, Ty::Applied(crate::types::constructor::TypeConstructorId::Map, _)) => {
                        let mut fake_args = vec![(**object).clone()];
                        fake_args.extend(args.iter().cloned());
                        let m = method.strip_prefix("map.").unwrap_or(method);
                        if !self.emit_map_call(m, &fake_args) {
                            self.emit_stub_call(args);
                        }
                    }
                    _ => {
                        // Try to resolve as TypeName.method convention call
                        let type_name = match &object.ty {
                            Ty::Named(n, _) => Some(n.clone()),
                            Ty::Record { .. } => None,
                            _ => None,
                        };
                        let qualified = type_name.as_ref().map(|tn| format!("{}.{}", tn, method));
                        if let Some(ref qn) = qualified {
                            if let Some(&func_idx) = self.emitter.func_map.get(qn.as_str()) {
                                // Convention/protocol method: call TypeName.method(self, args...)
                                self.emit_expr(object);
                                for arg in args { self.emit_expr(arg); }
                                wasm!(self.func, { call(func_idx); });
                                return;
                            }
                        }
                        // Also try: method name itself might be fully qualified (e.g., "Pair.to_str")
                        if let Some(&func_idx) = self.emitter.func_map.get(method.as_str()) {
                            self.emit_expr(object);
                            for arg in args { self.emit_expr(arg); }
                            wasm!(self.func, { call(func_idx); });
                            return;
                        }
                        // Fallback: stub
                        self.emit_expr(object);
                        if values::ty_to_valtype(&object.ty).is_some() {
                            wasm!(self.func, { drop; });
                        }
                        self.emit_stub_call(args);
                    }
                }
            }

            CallTarget::Computed { callee } => {
                // Closure call: callee is a closure ptr [table_idx: i32][env_ptr: i32]
                let scratch = self.match_i32_base + self.match_depth;

                // Evaluate callee → closure ptr
                self.emit_expr(callee);
                wasm!(self.func, { local_set(scratch); });

                // Push env_ptr (first hidden arg)
                wasm!(self.func, {
                    local_get(scratch);
                    i32_load(4);
                });

                // Push declared args
                for arg in args {
                    self.emit_expr(arg);
                }

                // Push table_idx (on top of stack for call_indirect)
                wasm!(self.func, {
                    local_get(scratch);
                    i32_load(0);
                });

                // Closure calling convention type: (env: i32, params...) -> ret
                if let Ty::Fn { params, ret } = &callee.ty {
                    let mut closure_params = vec![ValType::I32]; // env_ptr
                    for p in params {
                        if let Some(vt) = values::ty_to_valtype(p) {
                            closure_params.push(vt);
                        }
                    }
                    let ret_types = values::ret_type(ret);
                    let type_idx = self.emitter.register_type(closure_params, ret_types);
                    wasm!(self.func, { call_indirect(type_idx, 0); });
                } else {
                    wasm!(self.func, { unreachable; });
                }
            }
        }
    }

    /// Emit a FnRef as a closure: allocate [wrapper_table_idx, 0] on heap.
    pub(super) fn emit_fn_ref_closure(&mut self, name: &str) {
        if let Some(&wrapper_table_idx) = self.emitter.fn_ref_wrappers.get(name) {
            // Allocate closure: [table_idx: i32][env_ptr: i32] = 8 bytes
            let scratch = self.match_i32_base + self.match_depth;
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
        } else {
            eprintln!("WARNING: FnRef wrapper not found for '{}', using direct table entry", name);
            wasm!(self.func, { unreachable; });
        }
    }

    /// Emit a lambda as a closure: allocate env + closure on heap.
    pub(super) fn emit_lambda_closure(&mut self, _params: &[(crate::ir::VarId, Ty)], _body: &IrExpr) {
        let lambda_idx = self.emitter.lambda_counter.get();
        self.emitter.lambda_counter.set(lambda_idx + 1);

        if lambda_idx >= self.emitter.lambdas.len() {
            wasm!(self.func, { unreachable; });
            return;
        }

        let table_idx = self.emitter.lambdas[lambda_idx].table_idx;
        let captures = self.emitter.lambdas[lambda_idx].captures.clone();

        let scratch = self.match_i32_base + self.match_depth;

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
        } else {
            // Allocate env: each capture gets 8 bytes (padded for alignment)
            let env_size = (captures.len() as u32) * 8;
            let env_scratch = scratch; // reuse for env_ptr
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
                    wasm!(self.func, { local_get(local_idx); });
                } else {
                    // Variable not in scope — emit typed zero
                    match values::ty_to_valtype(ty) {
                        Some(ValType::I64) => { wasm!(self.func, { i64_const(0); }); }
                        Some(ValType::F64) => { wasm!(self.func, { f64_const(0.0); }); }
                        _ => { wasm!(self.func, { i32_const(0); }); }
                    }
                }
                self.emit_store_at(ty, offset);
            }

            // Allocate closure: [table_idx, env_ptr]
            let closure_scratch = scratch + 1; // second i32 scratch slot
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
        }
    }

    /// ASCII case conversion. Expects string ptr on stack. Returns new string ptr.
    fn emit_str_case_convert(&mut self, is_upper: bool) {
        // String ptr is on stack. Store to mem[0] via scratch.
        let scratch = self.match_i32_base + self.match_depth;
        wasm!(self.func, {
            local_set(scratch);
            i32_const(0);
            local_get(scratch);
            i32_store(0);
            // Alloc dst with same len → mem[4]
            i32_const(4);
            i32_const(4);
            i32_const(0);
            i32_load(0);
            i32_load(0);
            i32_add;
            call(self.emitter.rt.alloc);
            i32_store(0);
            // Store len in dst
            i32_const(4);
            i32_load(0);
            i32_const(0);
            i32_load(0);
            i32_load(0);
            i32_store(0);
        });
        // Loop: convert each byte
        let s = self.match_i32_base + self.match_depth;
        wasm!(self.func, {
            i32_const(0);
            local_set(s);
            block_empty;
            loop_empty;
        });
        let saved = self.depth; self.depth += 2;
        wasm!(self.func, {
            local_get(s);
            i32_const(0);
            i32_load(0);
            i32_load(0);
            i32_ge_u;
            br_if(1);
            // dst addr
            i32_const(4);
            i32_load(0);
            i32_const(4);
            i32_add;
            local_get(s);
            i32_add;
            // src byte
            i32_const(0);
            i32_load(0);
            i32_const(4);
            i32_add;
            local_get(s);
            i32_add;
            i32_load8_u(0);
            // Convert
            local_set(s + 1);
        });
        if is_upper {
            wasm!(self.func, {
                local_get(s + 1);
                i32_const(97);
                i32_ge_u;
                local_get(s + 1);
                i32_const(122);
                i32_le_u;
                i32_and;
                if_i32;
                local_get(s + 1);
                i32_const(32);
                i32_sub;
                else_;
                local_get(s + 1);
                end;
            });
        } else {
            wasm!(self.func, {
                local_get(s + 1);
                i32_const(65);
                i32_ge_u;
                local_get(s + 1);
                i32_const(90);
                i32_le_u;
                i32_and;
                if_i32;
                local_get(s + 1);
                i32_const(32);
                i32_add;
                else_;
                local_get(s + 1);
                end;
            });
        }
        wasm!(self.func, {
            i32_store8(0);
            local_get(s);
            i32_const(1);
            i32_add;
            local_set(s);
            br(0);
        });
        self.depth = saved;
        wasm!(self.func, {
            end;
            end;
            // Return dst
            i32_const(4);
            i32_load(0);
        });
    }

    /// Emit a stub for an unimplemented call: evaluate args (for side effects), drop values, unreachable.
    pub(super) fn emit_stub_call_logged(&mut self, args: &[IrExpr], context: &str) {
        eprintln!("[STUB] {}", context);
        self.emit_stub_call(args);
    }

    pub(super) fn emit_stub_call(&mut self, args: &[IrExpr]) {
        // Evaluate args for side effects, then return typed default instead of trapping.
        for arg in args {
            self.emit_expr(arg);
            if values::ty_to_valtype(&arg.ty).is_some() {
                wasm!(self.func, { drop; });
            }
        }
        // Return safe typed default based on return type context.
        let ret_ty = self.stub_ret_ty.clone();
        self.emit_typed_default(&ret_ty);
    }

    /// Emit a safe default value for a given type.
    /// String → empty string, List → empty list, Option → none, Bool → false, etc.
    pub(super) fn emit_typed_default(&mut self, ty: &Ty) {
        use crate::types::constructor::TypeConstructorId;
        match ty {
            Ty::Int => { wasm!(self.func, { i64_const(0); }); }
            Ty::Float => { wasm!(self.func, { f64_const(0.0); }); }
            Ty::Bool => { wasm!(self.func, { i32_const(0); }); }
            Ty::String => {
                // Empty string: alloc 4 bytes, len=0
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc);
                    local_set(self.match_i32_base + self.match_depth);
                    local_get(self.match_i32_base + self.match_depth);
                    i32_const(0); i32_store(0);
                    local_get(self.match_i32_base + self.match_depth);
                });
            }
            Ty::Applied(TypeConstructorId::List, _) => {
                // Empty list: alloc 4 bytes, len=0
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc);
                    local_set(self.match_i32_base + self.match_depth);
                    local_get(self.match_i32_base + self.match_depth);
                    i32_const(0); i32_store(0);
                    local_get(self.match_i32_base + self.match_depth);
                });
            }
            Ty::Applied(TypeConstructorId::Option, _) => {
                // none
                wasm!(self.func, { i32_const(0); });
            }
            Ty::Applied(TypeConstructorId::Result, _) => {
                // err("stub") — tag=1, value=empty string
                wasm!(self.func, {
                    i32_const(8); call(self.emitter.rt.alloc);
                    local_set(self.match_i32_base + self.match_depth);
                    local_get(self.match_i32_base + self.match_depth);
                    i32_const(1); i32_store(0); // tag=err
                    local_get(self.match_i32_base + self.match_depth);
                    i32_const(4); call(self.emitter.rt.alloc);
                    i32_store(4); // empty string at offset 4
                    local_get(self.match_i32_base + self.match_depth);
                });
            }
            Ty::Unit => { /* no value */ }
            _ => {
                // Generic pointer type: return 0 (null)
                // For records/tuples, this may still crash on field access.
                match values::ty_to_valtype(ty) {
                    Some(ValType::I64) => { wasm!(self.func, { i64_const(0); }); }
                    Some(ValType::F64) => { wasm!(self.func, { f64_const(0.0); }); }
                    Some(ValType::I32) => { wasm!(self.func, { i32_const(0); }); }
                    None => {}
                    _ => { wasm!(self.func, { i32_const(0); }); }
                }
            }
        }
    }

    /// Emit assert_eq(left, right): compare values, trap if not equal.
    pub(super) fn emit_assert_eq(&mut self, left: &IrExpr, right: &IrExpr) {
        // Use the same equality logic as BinOp::Eq
        self.emit_eq(left, right, false);
        // If not equal (result == 0), trap
        wasm!(self.func, {
            i32_eqz;
            if_empty;
            unreachable;
            end;
        });
    }

    /// Concatenate two strings on the heap via __concat_str runtime.
    pub(super) fn emit_concat_str(&mut self, left: &IrExpr, right: &IrExpr) {
        self.emit_expr(left);
        self.emit_expr(right);
        wasm!(self.func, { call(self.emitter.rt.concat_str); });
    }

    /// String interpolation: convert each part to string, then concat.
    pub(super) fn emit_string_interp(&mut self, parts: &[IrStringPart]) {
        if parts.is_empty() {
            let empty = self.emitter.intern_string("");
            wasm!(self.func, { i32_const(empty as i32); });
            return;
        }

        // Emit first part as a string
        self.emit_string_part(&parts[0]);

        // For each subsequent part: emit it, then concat with accumulator
        for part in &parts[1..] {
            self.emit_string_part(part);
            wasm!(self.func, { call(self.emitter.rt.concat_str); });
        }
    }

    /// Emit a single string interpolation part as a string (i32 pointer).
    pub(super) fn emit_string_part(&mut self, part: &IrStringPart) {
        match part {
            IrStringPart::Lit { value } => {
                let offset = self.emitter.intern_string(value);
                wasm!(self.func, { i32_const(offset as i32); });
            }
            IrStringPart::Expr { expr } => {
                match &expr.ty {
                    Ty::String => self.emit_expr(expr),
                    Ty::Int => {
                        self.emit_expr(expr);
                        wasm!(self.func, { call(self.emitter.rt.int_to_string); });
                    }
                    Ty::Bool => {
                        self.emit_expr(expr);
                        let t = self.emitter.intern_string("true");
                        let f = self.emitter.intern_string("false");
                        wasm!(self.func, {
                            if_i32;
                            i32_const(t as i32);
                            else_;
                            i32_const(f as i32);
                            end;
                        });
                    }
                    Ty::Float => {
                        self.emit_expr(expr);
                        wasm!(self.func, {
                            call(self.emitter.rt.float_to_string);
                        });
                    }
                    _ => {
                        // Fallback: emit the expression (already a string pointer or unsupported)
                        self.emit_expr(expr);
                    }
                }
            }
        }
    }

    /// Emit list.map(list, fn) → new list.
    /// Uses memory scratch [0..12] for src_ptr/fn_ptr/dst_ptr.
    /// Key insight: compute dst address BEFORE call_indirect so result goes
    /// directly onto the stack in the right position for store.
    fn emit_list_map(&mut self, list_arg: &IrExpr, fn_arg: &IrExpr, ret_ty: &Ty) {
        let in_elem_ty = if let Ty::Applied(_, args) = &list_arg.ty {
            args.first().cloned().unwrap_or(Ty::Int)
        } else { Ty::Int };
        let out_elem_ty = if let Ty::Applied(_, args) = ret_ty {
            args.first().cloned().unwrap_or(Ty::Int)
        } else { Ty::Int };
        let in_size = values::byte_size(&in_elem_ty);
        let out_size = values::byte_size(&out_elem_ty);

        let s = self.match_i32_base + self.match_depth;
        let len_local = s;
        let idx_local = s + 1;

        // Store src_ptr → mem[0], fn_closure → mem[4]
        wasm!(self.func, { i32_const(0); });
        self.emit_expr(list_arg);
        wasm!(self.func, {
            i32_store(0);
            i32_const(4);
        });
        self.emit_expr(fn_arg);
        wasm!(self.func, {
            i32_store(0);
            // len = mem[0].len (load src_ptr, load len)
            i32_const(0);
            i32_load(0);
            i32_load(0);
            local_set(len_local);
            // Alloc dst: 4 + len * out_size → store to mem[8]
            i32_const(8);
            i32_const(4);
            local_get(len_local);
            i32_const(out_size as i32);
            i32_mul;
            i32_add;
            call(self.emitter.rt.alloc);
            i32_store(0);
            // dst.len = len
            i32_const(8);
            i32_load(0);
            local_get(len_local);
            i32_store(0);
            // idx = 0
            i32_const(0);
            local_set(idx_local);
            // Loop
            block_empty;
            loop_empty;
        });
        let saved = self.depth;
        self.depth += 2;

        wasm!(self.func, {
            // break if idx >= len
            local_get(idx_local);
            local_get(len_local);
            i32_ge_u;
            br_if(1);
            // ── Compute dst addr FIRST (stays on stack under call result) ──
            // dst_ptr + 4 + idx * out_size
            i32_const(8);
            i32_load(0); // dst
            i32_const(4);
            i32_add;
            local_get(idx_local);
            i32_const(out_size as i32);
            i32_mul;
            i32_add;
            // Stack: [dst_elem_addr]
            // ── Call fn(element) ──
            // Load closure from mem[4]
            i32_const(4);
            i32_load(0);
        });
        // env_ptr = closure[4]
        wasm!(self.func, { i32_load(4); });
        // Stack: [dst_elem_addr, env_ptr]

        // Load src element: src_ptr + 4 + idx * in_size
        wasm!(self.func, {
            i32_const(0);
            i32_load(0); // src
            i32_const(4);
            i32_add;
            local_get(idx_local);
            i32_const(in_size as i32);
            i32_mul;
            i32_add;
        });
        self.emit_load_at(&in_elem_ty, 0);
        // Stack: [dst_elem_addr, env_ptr, element]

        // table_idx = closure[0]
        wasm!(self.func, {
            i32_const(4);
            i32_load(0); // closure
            i32_load(0); // table_idx
        });
        // Stack: [dst_elem_addr, env_ptr, element, table_idx]

        // call_indirect (env, element) → result
        // Use concrete element types (not fn_arg.ty which may contain unresolved TypeVars)
        {
            let mut ct = vec![ValType::I32]; // env
            if let Some(vt) = values::ty_to_valtype(&in_elem_ty) { ct.push(vt); }
            let rt = values::ret_type(&out_elem_ty);
            let ti = self.emitter.register_type(ct, rt);
            wasm!(self.func, { call_indirect(ti, 0); });
        }
        // Stack: [dst_elem_addr, result]

        // ── Store result at dst addr ──
        self.emit_store_at(&out_elem_ty, 0);
        // Stack: []

        // idx++
        wasm!(self.func, {
            local_get(idx_local);
            i32_const(1);
            i32_add;
            local_set(idx_local);
            br(0);
        });

        self.depth = saved;
        wasm!(self.func, {
            end;
            end;
            // Return dst_ptr from mem[8]
            i32_const(8);
            i32_load(0);
        });
    }

}
