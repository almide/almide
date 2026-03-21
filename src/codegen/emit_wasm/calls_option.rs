//! Option and Result stdlib call dispatch for WASM codegen.

use super::FuncCompiler;
use super::values;
use crate::ir::IrExpr;
use crate::types::Ty;
use wasm_encoder::ValType;

impl FuncCompiler<'_> {
    /// Dispatch an option stdlib method call. Returns true if handled.
    pub(super) fn emit_option_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "is_some" => {
                // is_some(opt) → Bool(i32): ptr != 0
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_const(0); i32_ne; });
            }
            "is_none" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_eqz; });
            }
            "unwrap_or" => {
                // unwrap_or(opt, default) → T
                // if opt != 0 then load *opt else default
                let inner_ty = self.option_inner_ty(&args[0].ty);
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                let vt = values::ty_to_valtype(&inner_ty);
                match vt {
                    Some(ValType::I64) => {
                        wasm!(self.func, {
                            local_set(s);
                            local_get(s); i32_eqz;
                            if_i64;
                        });
                        self.emit_expr(&args[1]);
                        wasm!(self.func, {
                            else_;
                              local_get(s);
                              i64_load(0);
                            end;
                        });
                    }
                    Some(ValType::F64) => {
                        wasm!(self.func, {
                            local_set(s);
                            local_get(s); i32_eqz;
                            if_f64;
                        });
                        self.emit_expr(&args[1]);
                        wasm!(self.func, {
                            else_;
                              local_get(s);
                              f64_load(0);
                            end;
                        });
                    }
                    _ => {
                        // i32 (String, Option, etc.)
                        wasm!(self.func, {
                            local_set(s);
                            local_get(s); i32_eqz;
                            if_i32;
                        });
                        self.emit_expr(&args[1]);
                        wasm!(self.func, {
                            else_;
                              local_get(s);
                              i32_load(0);
                            end;
                        });
                    }
                }
            }
            "map" => {
                // map(opt, f) → Option[B]
                // Use mem[4]=opt_ptr, mem[0]=closure to avoid scratch conflicts
                let inner_ty = self.option_inner_ty(&args[0].ty);
                let s = self.match_i32_base + self.match_depth;
                // Store opt → mem[4]
                wasm!(self.func, { i32_const(4); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                // Store closure → mem[0]
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(4); i32_load(0); i32_eqz; // opt == 0?
                    if_i32;
                      i32_const(0); // none
                    else_;
                });
                let out_ty = self.fn_ret_ty(&args[1].ty);
                let out_size = values::byte_size(&out_ty);
                wasm!(self.func, {
                    i32_const(out_size as i32);
                    call(self.emitter.rt.alloc);
                    local_set(s);
                    local_get(s);
                    // env_ptr
                    i32_const(0); i32_load(0); i32_load(4);
                    // Load inner value
                    i32_const(4); i32_load(0);
                });
                self.emit_load_at(&inner_ty, 0);
                // table_idx
                wasm!(self.func, {
                    i32_const(0); i32_load(0); i32_load(0);
                });
                self.emit_closure_call(&inner_ty, &out_ty);
                self.emit_store_at(&out_ty, 0);
                wasm!(self.func, {
                      local_get(s);
                    end;
                });
            }
            "flat_map" => {
                // flat_map(opt, f) → Option[B]
                // if opt == 0 → 0; else → f(*opt) (f returns Option)
                // Use mem[4]=opt_ptr, mem[0]=closure to avoid scratch local conflicts
                let inner_ty = self.option_inner_ty(&args[0].ty);
                // Store opt → mem[4]
                wasm!(self.func, { i32_const(4); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                // Store closure → mem[0]
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(4); i32_load(0); i32_eqz; // opt == 0?
                    if_i32;
                      i32_const(0);
                    else_;
                      i32_const(0); i32_load(0); i32_load(4); // env
                });
                wasm!(self.func, { i32_const(4); i32_load(0); });
                self.emit_load_at(&inner_ty, 0);
                wasm!(self.func, {
                    i32_const(0); i32_load(0); i32_load(0); // table_idx
                });
                // Result is i32 (Option ptr)
                self.emit_closure_call(&inner_ty, &Ty::Unknown);
                wasm!(self.func, { end; });
            }
            "flatten" => {
                // flatten(opt) → Option[T]: if opt != 0 then *opt (which is also an Option ptr) else 0
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(self.match_i32_base + self.match_depth);
                    local_get(self.match_i32_base + self.match_depth); i32_eqz;
                    if_i32;
                      i32_const(0);
                    else_;
                      local_get(self.match_i32_base + self.match_depth);
                      i32_load(0); // inner Option ptr
                    end;
                });
            }
            "filter" => {
                // filter(opt, pred) → Option[T]
                // if opt == 0 → 0; else if pred(*opt) → opt; else → 0
                // Use mem[4]=opt_ptr, mem[0]=closure to avoid scratch local conflicts
                let inner_ty = self.option_inner_ty(&args[0].ty);
                // Store opt → mem[4]
                wasm!(self.func, { i32_const(4); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                // Store closure → mem[0]
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(4); i32_load(0); i32_eqz; // opt == 0?
                    if_i32;
                      i32_const(0);
                    else_;
                      // Call pred(*opt)
                      i32_const(0); i32_load(0); i32_load(4); // env
                });
                wasm!(self.func, { i32_const(4); i32_load(0); });
                self.emit_load_at(&inner_ty, 0);
                wasm!(self.func, {
                    i32_const(0); i32_load(0); i32_load(0); // table_idx
                });
                self.emit_closure_call(&inner_ty, &Ty::Bool);
                // Result is Bool(i32): if true return opt, else 0
                wasm!(self.func, {
                      if_i32;
                        i32_const(4); i32_load(0);
                      else_;
                        i32_const(0);
                      end;
                    end;
                });
            }
            "to_result" => {
                // to_result(opt, err_msg) → Result[T, String]
                // if opt != 0 → ok(*opt) = [tag:0, *opt]
                // else → err(err_msg) = [tag:1, err_msg]
                let inner_ty = self.option_inner_ty(&args[0].ty);
                let inner_size = values::byte_size(&inner_ty);
                let err_size = values::byte_size(&Ty::String);
                let alloc_size = 4 + inner_size.max(err_size);
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_eqz;
                    if_i32;
                      // err(err_msg)
                      i32_const((4 + err_size) as i32);
                      call(self.emitter.rt.alloc);
                      local_set(s + 1);
                      local_get(s + 1); i32_const(1); i32_store(0); // tag=1
                      local_get(s + 1);
                });
                self.emit_expr(&args[1]); // err_msg
                wasm!(self.func, {
                      i32_store(4);
                      local_get(s + 1);
                    else_;
                      // ok(*opt)
                      i32_const(alloc_size as i32);
                      call(self.emitter.rt.alloc);
                      local_set(s + 1);
                      local_get(s + 1); i32_const(0); i32_store(0); // tag=0
                      local_get(s + 1);
                      local_get(s);
                });
                self.emit_load_at(&inner_ty, 0);
                self.emit_store_at(&inner_ty, 4);
                wasm!(self.func, {
                      local_get(s + 1);
                    end;
                });
            }
            "or_else" => {
                // or_else(opt, f) → Option[T]: if opt != 0 then opt else f()
                // Use mem[4]=opt_ptr, mem[0]=closure to avoid scratch local conflicts
                // Store opt → mem[4]
                wasm!(self.func, { i32_const(4); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                // Store closure → mem[0]
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(4); i32_load(0); i32_eqz; // opt == 0?
                    if_i32;
                });
                // Call f() — thunk returning Option
                wasm!(self.func, {
                    i32_const(0); i32_load(0); i32_load(4); // env
                    i32_const(0); i32_load(0); i32_load(0); // table_idx
                });
                // call_indirect () → i32
                let ti = self.emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
                wasm!(self.func, {
                    call_indirect(ti, 0);
                    else_;
                      i32_const(4); i32_load(0);
                    end;
                });
            }
            "unwrap_or_else" => {
                // unwrap_or_else(opt, f) → T: if opt != 0 then *opt else f()
                // Use mem[4]=opt_ptr, mem[0]=closure to avoid scratch local conflicts
                let inner_ty = self.option_inner_ty(&args[0].ty);
                let vt = values::ty_to_valtype(&inner_ty).unwrap_or(ValType::I32);
                // Store opt → mem[4]
                wasm!(self.func, { i32_const(4); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                // Store closure → mem[0]
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_store(0); });
                match vt {
                    ValType::I64 => {
                        wasm!(self.func, { i32_const(4); i32_load(0); i32_eqz; if_i64; });
                        // f()
                        wasm!(self.func, {
                            i32_const(0); i32_load(0); i32_load(4);
                            i32_const(0); i32_load(0); i32_load(0);
                        });
                        let ti = self.emitter.register_type(vec![ValType::I32], vec![ValType::I64]);
                        wasm!(self.func, { call_indirect(ti, 0); else_; i32_const(4); i32_load(0); i64_load(0); end; });
                    }
                    ValType::F64 => {
                        wasm!(self.func, { i32_const(4); i32_load(0); i32_eqz; if_f64; });
                        wasm!(self.func, {
                            i32_const(0); i32_load(0); i32_load(4);
                            i32_const(0); i32_load(0); i32_load(0);
                        });
                        let ti = self.emitter.register_type(vec![ValType::I32], vec![ValType::F64]);
                        wasm!(self.func, { call_indirect(ti, 0); else_; i32_const(4); i32_load(0); f64_load(0); end; });
                    }
                    _ => {
                        wasm!(self.func, { i32_const(4); i32_load(0); i32_eqz; if_i32; });
                        wasm!(self.func, {
                            i32_const(0); i32_load(0); i32_load(4);
                            i32_const(0); i32_load(0); i32_load(0);
                        });
                        let ti = self.emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
                        wasm!(self.func, { call_indirect(ti, 0); else_; i32_const(4); i32_load(0); i32_load(0); end; });
                    }
                }
            }
            "to_list" => {
                // to_list(opt) → List[T]: some(x) → [x], none → []
                let inner_ty = self.option_inner_ty(&args[0].ty);
                let elem_size = values::byte_size(&inner_ty);
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_eqz;
                    if_i32;
                      // empty list: [len=0]
                      i32_const(4); call(self.emitter.rt.alloc); local_set(s + 1);
                      local_get(s + 1); i32_const(0); i32_store(0);
                      local_get(s + 1);
                    else_;
                      // [len=1, elem]
                      i32_const((4 + elem_size) as i32); call(self.emitter.rt.alloc); local_set(s + 1);
                      local_get(s + 1); i32_const(1); i32_store(0);
                      local_get(s + 1);
                      local_get(s);
                });
                self.emit_load_at(&inner_ty, 0);
                self.emit_store_at(&inner_ty, 4);
                wasm!(self.func, {
                      local_get(s + 1);
                    end;
                });
            }
            "zip" => {
                // zip(a, b) → Option[(A,B)]
                // if a == 0 || b == 0 → 0; else → some((*a, *b))
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(s + 1);
                    local_get(s); i32_eqz;
                    local_get(s + 1); i32_eqz;
                    i32_or;
                    if_i32;
                      i32_const(0);
                    else_;
                });
                // Allocate tuple and wrap in some
                let a_ty = self.option_inner_ty(&args[0].ty);
                let b_ty = self.option_inner_ty(&args[1].ty);
                let a_size = values::byte_size(&a_ty);
                let b_size = values::byte_size(&b_ty);
                let tuple_size = a_size + b_size;
                // Tuple ptr
                wasm!(self.func, {
                    i32_const(tuple_size as i32); call(self.emitter.rt.alloc);
                    local_set(s + 2);
                    local_get(s + 2);
                    local_get(s);
                });
                self.emit_load_at(&a_ty, 0);
                self.emit_store_at(&a_ty, 0);
                wasm!(self.func, { local_get(s + 2); local_get(s + 1); });
                self.emit_load_at(&b_ty, 0);
                self.emit_store_at(&b_ty, a_size as u32);
                // Wrap tuple in Some: alloc ptr-size, store tuple ptr
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc);
                    local_set(s + 3);
                    local_get(s + 3);
                    local_get(s + 2);
                    i32_store(0);
                    local_get(s + 3);
                    end;
                });
            }
            _ => return false,
        }
        true
    }

    /// Dispatch a result stdlib method call. Returns true if handled.
    pub(super) fn emit_result_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "is_ok" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_load(0); i32_eqz; }); // tag==0 → true
            }
            "is_err" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_load(0); i32_const(0); i32_ne; });
            }
            "unwrap_or" => {
                // unwrap_or(result, default) → T
                let inner_ty = self.result_ok_ty(&args[0].ty);
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                let vt = values::ty_to_valtype(&inner_ty).unwrap_or(ValType::I32);
                match vt {
                    ValType::I64 => {
                        wasm!(self.func, { local_set(s); local_get(s); i32_load(0); i32_eqz; if_i64; local_get(s); i64_load(4); else_; });
                        self.emit_expr(&args[1]);
                        wasm!(self.func, { end; });
                    }
                    ValType::F64 => {
                        wasm!(self.func, { local_set(s); local_get(s); i32_load(0); i32_eqz; if_f64; local_get(s); f64_load(4); else_; });
                        self.emit_expr(&args[1]);
                        wasm!(self.func, { end; });
                    }
                    _ => {
                        wasm!(self.func, { local_set(s); local_get(s); i32_load(0); i32_eqz; if_i32; local_get(s); i32_load(4); else_; });
                        self.emit_expr(&args[1]);
                        wasm!(self.func, { end; });
                    }
                }
            }
            "map" => {
                // map(result, f) → Result[B, E]
                // Use mem[4]=result_ptr, mem[0]=closure to avoid scratch local conflicts
                let ok_ty = self.result_ok_ty(&args[0].ty);
                let s = self.match_i32_base + self.match_depth;
                // Store result → mem[4]
                wasm!(self.func, { i32_const(4); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                // Store closure → mem[0]
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    // Check tag
                    i32_const(4); i32_load(0); // result_ptr
                    i32_load(0); i32_const(0); i32_ne; // tag != 0 (err?)
                    if_i32;
                      i32_const(4); i32_load(0); // return err as-is
                    else_;
                });
                let out_ty = self.fn_ret_ty(&args[1].ty);
                let out_size = values::byte_size(&out_ty);
                let result_size = 4 + out_size;
                wasm!(self.func, {
                    i32_const(result_size as i32); call(self.emitter.rt.alloc); local_set(s);
                    local_get(s); i32_const(0); i32_store(0); // tag=0
                    local_get(s);
                    // env
                    i32_const(0); i32_load(0); i32_load(4);
                    // Load ok value from result
                    i32_const(4); i32_load(0);
                });
                self.emit_load_at(&ok_ty, 4);
                // table_idx
                wasm!(self.func, { i32_const(0); i32_load(0); i32_load(0); });
                self.emit_closure_call(&ok_ty, &out_ty);
                self.emit_store_at(&out_ty, 4);
                wasm!(self.func, { local_get(s); end; });
            }
            "map_err" => {
                // map_err(result, f) → Result[T, F]
                // Use mem[4]=result_ptr, mem[0]=closure to avoid scratch local conflicts
                let err_ty = self.result_err_ty(&args[0].ty);
                let s = self.match_i32_base + self.match_depth;
                // Store result → mem[4]
                wasm!(self.func, { i32_const(4); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                // Store closure → mem[0]
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(4); i32_load(0); // result_ptr
                    i32_load(0); i32_eqz; // tag==0 (ok?)
                    if_i32;
                      i32_const(4); i32_load(0); // return ok as-is
                    else_;
                });
                let out_ty = self.fn_ret_ty(&args[1].ty);
                let out_size = values::byte_size(&out_ty);
                wasm!(self.func, {
                    i32_const((4 + out_size) as i32); call(self.emitter.rt.alloc); local_set(s);
                    local_get(s); i32_const(1); i32_store(0); // tag=1 (err)
                    local_get(s);
                    i32_const(0); i32_load(0); i32_load(4); // env
                });
                wasm!(self.func, { i32_const(4); i32_load(0); });
                self.emit_load_at(&err_ty, 4);
                wasm!(self.func, { i32_const(0); i32_load(0); i32_load(0); });
                self.emit_closure_call(&err_ty, &out_ty);
                self.emit_store_at(&out_ty, 4);
                wasm!(self.func, { local_get(s); end; });
            }
            "flat_map" => {
                // flat_map(result, f) → Result[B, E]
                // Use mem[4]=result_ptr, mem[0]=closure to avoid scratch local conflicts
                let ok_ty = self.result_ok_ty(&args[0].ty);
                // Store result → mem[4]
                wasm!(self.func, { i32_const(4); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                // Store closure → mem[0]
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(4); i32_load(0); // result_ptr
                    i32_load(0); i32_const(0); i32_ne; // tag != 0 (err?)
                    if_i32;
                      i32_const(4); i32_load(0); // return err as-is
                    else_;
                      i32_const(0); i32_load(0); i32_load(4); // env
                });
                wasm!(self.func, { i32_const(4); i32_load(0); });
                self.emit_load_at(&ok_ty, 4);
                wasm!(self.func, { i32_const(0); i32_load(0); i32_load(0); });
                // Returns Result ptr (i32)
                self.emit_closure_call(&ok_ty, &Ty::Unknown);
                wasm!(self.func, { end; });
            }
            "to_option" => {
                // to_option(result) → Option[T]: ok(x) → some(x), err(_) → none
                let ok_ty = self.result_ok_ty(&args[0].ty);
                let ok_size = values::byte_size(&ok_ty);
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_load(0); i32_const(0); i32_ne;
                    if_i32;
                      i32_const(0); // none
                    else_;
                      // some(ok_value)
                      i32_const(ok_size as i32); call(self.emitter.rt.alloc); local_set(s + 1);
                      local_get(s + 1);
                      local_get(s);
                });
                self.emit_load_at(&ok_ty, 4);
                self.emit_store_at(&ok_ty, 0);
                wasm!(self.func, { local_get(s + 1); end; });
            }
            "to_err_option" => {
                let err_ty = self.result_err_ty(&args[0].ty);
                let err_size = values::byte_size(&err_ty);
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_load(0); i32_eqz;
                    if_i32;
                      i32_const(0); // none (was ok)
                    else_;
                      i32_const(err_size as i32); call(self.emitter.rt.alloc); local_set(s + 1);
                      local_get(s + 1);
                      local_get(s);
                });
                self.emit_load_at(&err_ty, 4);
                self.emit_store_at(&err_ty, 0);
                wasm!(self.func, { local_get(s + 1); end; });
            }
            "unwrap_or_else" => {
                // Use mem[4]=result_ptr, mem[0]=closure to avoid scratch local conflicts
                let ok_ty = self.result_ok_ty(&args[0].ty);
                let vt = values::ty_to_valtype(&ok_ty).unwrap_or(ValType::I32);
                // Store result → mem[4]
                wasm!(self.func, { i32_const(4); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                // Store closure → mem[0]
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_store(0); });
                match vt {
                    ValType::I64 => {
                        wasm!(self.func, { i32_const(4); i32_load(0); i32_load(0); i32_eqz; if_i64; i32_const(4); i32_load(0); i64_load(4); else_; });
                        // f(err)
                        wasm!(self.func, { i32_const(0); i32_load(0); i32_load(4); i32_const(4); i32_load(0); i32_load(4); i32_const(0); i32_load(0); i32_load(0); });
                        let err_ty = self.result_err_ty(&args[0].ty);
                        self.emit_closure_call(&err_ty, &ok_ty);
                        wasm!(self.func, { end; });
                    }
                    _ => {
                        wasm!(self.func, { i32_const(4); i32_load(0); i32_load(0); i32_eqz; if_i32; i32_const(4); i32_load(0); i32_load(4); else_; });
                        wasm!(self.func, { i32_const(0); i32_load(0); i32_load(4); i32_const(4); i32_load(0); i32_load(4); i32_const(0); i32_load(0); i32_load(0); });
                        let err_ty = self.result_err_ty(&args[0].ty);
                        self.emit_closure_call(&err_ty, &ok_ty);
                        wasm!(self.func, { end; });
                    }
                }
            }
            _ => return false,
        }
        true
    }

    // ── Helpers ──

    fn option_inner_ty(&self, ty: &Ty) -> Ty {
        if let Ty::Applied(_, args) = ty {
            args.first().cloned().unwrap_or(Ty::Int)
        } else { Ty::Int }
    }

    fn result_ok_ty(&self, ty: &Ty) -> Ty {
        if let Ty::Applied(_, args) = ty {
            args.first().cloned().unwrap_or(Ty::Int)
        } else { Ty::Int }
    }

    fn result_err_ty(&self, ty: &Ty) -> Ty {
        if let Ty::Applied(_, args) = ty {
            args.get(1).cloned().unwrap_or(Ty::String)
        } else { Ty::String }
    }

    fn fn_ret_ty(&self, ty: &Ty) -> Ty {
        if let Ty::Fn { ret, .. } = ty {
            *ret.clone()
        } else { Ty::Int }
    }

    fn fn_ret_inner_ty(&self, ty: &Ty) -> Ty {
        // For flat_map: f returns Option[T], extract T
        let ret = self.fn_ret_ty(ty);
        self.option_inner_ty(&ret)
    }

    fn emit_closure_call(&mut self, param_ty: &Ty, ret_ty: &Ty) {
        let mut ct = vec![ValType::I32]; // env
        if let Some(vt) = values::ty_to_valtype(param_ty) {
            ct.push(vt);
        }
        let rt = if ret_ty == &Ty::Unknown || ret_ty == &Ty::Bool {
            // Unknown: return i32 (ptr). Bool: i32.
            vec![ValType::I32]
        } else {
            values::ret_type(ret_ty)
        };
        let ti = self.emitter.register_type(ct, rt);
        wasm!(self.func, { call_indirect(ti, 0); });
    }
}
