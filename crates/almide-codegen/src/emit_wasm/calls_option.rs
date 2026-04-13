//! Option and Result stdlib call dispatch for WASM codegen.

use super::FuncCompiler;
use super::values;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
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
                let s = self.scratch.alloc_i32();
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
                self.scratch.free_i32(s);
            }
            "map" => {
                // map(opt, f) → Option[B]
                let inner_ty = self.option_inner_ty(&args[0].ty);
                let opt = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let s = self.scratch.alloc_i32();
                // Store opt
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(opt); });
                // Store closure
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(opt); i32_eqz; // opt == 0?
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
                    local_get(closure); i32_load(4);
                    // Load inner value
                    local_get(opt);
                });
                self.emit_load_at(&inner_ty, 0);
                // table_idx
                wasm!(self.func, {
                    local_get(closure); i32_load(0);
                });
                self.emit_closure_call(&inner_ty, &out_ty);
                self.emit_store_at(&out_ty, 0);
                wasm!(self.func, {
                      local_get(s);
                    end;
                });
                self.scratch.free_i32(s);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(opt);
            }
            "flat_map" => {
                // flat_map(opt, f) → Option[B]
                // if opt == 0 → 0; else → f(*opt) (f returns Option)
                let inner_ty = self.option_inner_ty(&args[0].ty);
                let opt = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                // Store opt
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(opt); });
                // Store closure
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(opt); i32_eqz; // opt == 0?
                    if_i32;
                      i32_const(0);
                    else_;
                      local_get(closure); i32_load(4); // env
                });
                wasm!(self.func, { local_get(opt); });
                self.emit_load_at(&inner_ty, 0);
                wasm!(self.func, {
                    local_get(closure); i32_load(0); // table_idx
                });
                // Result is i32 (Option ptr)
                self.emit_closure_call(&inner_ty, &Ty::Unknown);
                wasm!(self.func, { end; });
                self.scratch.free_i32(closure);
                self.scratch.free_i32(opt);
            }
            "flatten" => {
                // flatten(opt) → Option[T]: if opt != 0 then *opt (which is also an Option ptr) else 0
                let s = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_eqz;
                    if_i32;
                      i32_const(0);
                    else_;
                      local_get(s);
                      i32_load(0); // inner Option ptr
                    end;
                });
                self.scratch.free_i32(s);
            }
            "filter" => {
                // filter(opt, pred) → Option[T]
                // if opt == 0 → 0; else if pred(*opt) → opt; else → 0
                let inner_ty = self.option_inner_ty(&args[0].ty);
                let opt = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                // Store opt
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(opt); });
                // Store closure
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(opt); i32_eqz; // opt == 0?
                    if_i32;
                      i32_const(0);
                    else_;
                      // Call pred(*opt)
                      local_get(closure); i32_load(4); // env
                });
                wasm!(self.func, { local_get(opt); });
                self.emit_load_at(&inner_ty, 0);
                wasm!(self.func, {
                    local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&inner_ty, &Ty::Bool);
                // Result is Bool(i32): if true return opt, else 0
                wasm!(self.func, {
                      if_i32;
                        local_get(opt);
                      else_;
                        i32_const(0);
                      end;
                    end;
                });
                self.scratch.free_i32(closure);
                self.scratch.free_i32(opt);
            }
            "to_result" => {
                // to_result(opt, err_msg) → Result[T, String]
                // if opt != 0 → ok(*opt) = [tag:0, *opt]
                // else → err(err_msg) = [tag:1, err_msg]
                let inner_ty = self.option_inner_ty(&args[0].ty);
                let inner_size = values::byte_size(&inner_ty);
                let err_size = values::byte_size(&Ty::String);
                let alloc_size = 4 + inner_size.max(err_size);
                let s = self.scratch.alloc_i32();
                let s2 = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_eqz;
                    if_i32;
                      // err(err_msg)
                      i32_const((4 + err_size) as i32);
                      call(self.emitter.rt.alloc);
                      local_set(s2);
                      local_get(s2); i32_const(1); i32_store(0); // tag=1
                      local_get(s2);
                });
                self.emit_expr(&args[1]); // err_msg
                wasm!(self.func, {
                      i32_store(4);
                      local_get(s2);
                    else_;
                      // ok(*opt)
                      i32_const(alloc_size as i32);
                      call(self.emitter.rt.alloc);
                      local_set(s2);
                      local_get(s2); i32_const(0); i32_store(0); // tag=0
                      local_get(s2);
                      local_get(s);
                });
                self.emit_load_at(&inner_ty, 0);
                self.emit_store_at(&inner_ty, 4);
                wasm!(self.func, {
                      local_get(s2);
                    end;
                });
                self.scratch.free_i32(s2);
                self.scratch.free_i32(s);
            }
            "or_else" => {
                // or_else(opt, f) → Option[T]: if opt != 0 then opt else f()
                let opt = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                // Store opt
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(opt); });
                // Store closure
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(opt); i32_eqz; // opt == 0?
                    if_i32;
                });
                // Call f() — thunk returning Option
                wasm!(self.func, {
                    local_get(closure); i32_load(4); // env
                    local_get(closure); i32_load(0); // table_idx
                });
                // call_indirect () → i32
                let ti = self.emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
                wasm!(self.func, {
                    call_indirect(ti, 0);
                    else_;
                      local_get(opt);
                    end;
                });
                self.scratch.free_i32(closure);
                self.scratch.free_i32(opt);
            }
            "unwrap_or_else" => {
                // unwrap_or_else(opt, f) → T: if opt != 0 then *opt else f()
                let inner_ty = self.option_inner_ty(&args[0].ty);
                let vt = values::ty_to_valtype(&inner_ty).unwrap_or(ValType::I32);
                let opt = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                // Store opt
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(opt); });
                // Store closure
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(closure); });
                match vt {
                    ValType::I64 => {
                        wasm!(self.func, { local_get(opt); i32_eqz; if_i64; });
                        // f()
                        wasm!(self.func, {
                            local_get(closure); i32_load(4);
                            local_get(closure); i32_load(0);
                        });
                        let ti = self.emitter.register_type(vec![ValType::I32], vec![ValType::I64]);
                        wasm!(self.func, { call_indirect(ti, 0); else_; local_get(opt); i64_load(0); end; });
                    }
                    ValType::F64 => {
                        wasm!(self.func, { local_get(opt); i32_eqz; if_f64; });
                        wasm!(self.func, {
                            local_get(closure); i32_load(4);
                            local_get(closure); i32_load(0);
                        });
                        let ti = self.emitter.register_type(vec![ValType::I32], vec![ValType::F64]);
                        wasm!(self.func, { call_indirect(ti, 0); else_; local_get(opt); f64_load(0); end; });
                    }
                    _ => {
                        wasm!(self.func, { local_get(opt); i32_eqz; if_i32; });
                        wasm!(self.func, {
                            local_get(closure); i32_load(4);
                            local_get(closure); i32_load(0);
                        });
                        let ti = self.emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
                        wasm!(self.func, { call_indirect(ti, 0); else_; local_get(opt); i32_load(0); end; });
                    }
                }
                self.scratch.free_i32(closure);
                self.scratch.free_i32(opt);
            }
            "to_list" => {
                // to_list(opt) → List[T]: some(x) → [x], none → []
                let inner_ty = self.option_inner_ty(&args[0].ty);
                let elem_size = values::byte_size(&inner_ty);
                let s = self.scratch.alloc_i32();
                let s2 = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_eqz;
                    if_i32;
                      // empty list: [len=0]
                      i32_const(4); call(self.emitter.rt.alloc); local_set(s2);
                      local_get(s2); i32_const(0); i32_store(0);
                      local_get(s2);
                    else_;
                      // [len=1, elem]
                      i32_const((4 + elem_size) as i32); call(self.emitter.rt.alloc); local_set(s2);
                      local_get(s2); i32_const(1); i32_store(0);
                      local_get(s2);
                      local_get(s);
                });
                self.emit_load_at(&inner_ty, 0);
                self.emit_store_at(&inner_ty, 4);
                wasm!(self.func, {
                      local_get(s2);
                    end;
                });
                self.scratch.free_i32(s2);
                self.scratch.free_i32(s);
            }
            "zip" => {
                // zip(a, b) → Option[(A,B)]
                // if a == 0 || b == 0 → 0; else → some((*a, *b))
                let sa = self.scratch.alloc_i32();
                let sb = self.scratch.alloc_i32();
                let s_tuple = self.scratch.alloc_i32();
                let s_some = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(sa); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(sb);
                    local_get(sa); i32_eqz;
                    local_get(sb); i32_eqz;
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
                    local_set(s_tuple);
                    local_get(s_tuple);
                    local_get(sa);
                });
                self.emit_load_at(&a_ty, 0);
                self.emit_store_at(&a_ty, 0);
                wasm!(self.func, { local_get(s_tuple); local_get(sb); });
                self.emit_load_at(&b_ty, 0);
                self.emit_store_at(&b_ty, a_size as u32);
                // Wrap tuple in Some: alloc ptr-size, store tuple ptr
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc);
                    local_set(s_some);
                    local_get(s_some);
                    local_get(s_tuple);
                    i32_store(0);
                    local_get(s_some);
                    end;
                });
                self.scratch.free_i32(s_some);
                self.scratch.free_i32(s_tuple);
                self.scratch.free_i32(sb);
                self.scratch.free_i32(sa);
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
                let s = self.scratch.alloc_i32();
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
                self.scratch.free_i32(s);
            }
            "map" => {
                // map(result, f) → Result[B, E]
                let ok_ty = self.result_ok_ty(&args[0].ty);
                let result = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                // Store result
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(result); });
                // Store closure
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(closure); });
                self.emit_result_branch_err(result);
                wasm!(self.func, {
                    if_i32;
                      local_get(result); // return err as-is
                    else_;
                });
                let out_ty = self.fn_ret_ty(&args[1].ty);
                let s = self.emit_result_alloc_ok(&out_ty);
                wasm!(self.func, {
                    local_get(s);
                    // env
                    local_get(closure); i32_load(4);
                    // Load ok value from result
                    local_get(result);
                });
                self.emit_load_at(&ok_ty, 4);
                // table_idx
                wasm!(self.func, { local_get(closure); i32_load(0); });
                self.emit_closure_call(&ok_ty, &out_ty);
                self.emit_store_at(&out_ty, 4);
                wasm!(self.func, { local_get(s); end; });
                self.scratch.free_i32(s);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(result);
            }
            "map_err" => {
                // map_err(result, f) → Result[T, F]
                let err_ty = self.result_err_ty(&args[0].ty);
                let result = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                // Store result
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(result); });
                // Store closure
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(closure); });
                self.emit_result_branch_ok(result);
                wasm!(self.func, {
                    if_i32;
                      local_get(result); // return ok as-is
                    else_;
                });
                let out_ty = self.fn_ret_ty(&args[1].ty);
                let s = self.emit_result_alloc_err(&out_ty);
                wasm!(self.func, {
                    local_get(s);
                    local_get(closure); i32_load(4); // env
                });
                wasm!(self.func, { local_get(result); });
                self.emit_load_at(&err_ty, 4);
                wasm!(self.func, { local_get(closure); i32_load(0); });
                self.emit_closure_call(&err_ty, &out_ty);
                self.emit_store_at(&out_ty, 4);
                wasm!(self.func, { local_get(s); end; });
                self.scratch.free_i32(s);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(result);
            }
            "flat_map" => {
                // flat_map(result, f) → Result[B, E]
                let ok_ty = self.result_ok_ty(&args[0].ty);
                let result = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                // Store result
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(result); });
                // Store closure
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(closure); });
                self.emit_result_branch_err(result);
                wasm!(self.func, {
                    if_i32;
                      local_get(result); // return err as-is
                    else_;
                      local_get(closure); i32_load(4); // env
                });
                wasm!(self.func, { local_get(result); });
                self.emit_load_at(&ok_ty, 4);
                wasm!(self.func, { local_get(closure); i32_load(0); });
                // Returns Result ptr (i32)
                self.emit_closure_call(&ok_ty, &Ty::Unknown);
                wasm!(self.func, { end; });
                self.scratch.free_i32(closure);
                self.scratch.free_i32(result);
            }
            "to_option" => {
                // to_option(result) → Option[T]: ok(x) → some(x), err(_) → none
                let ok_ty = self.result_ok_ty(&args[0].ty);
                let ok_size = values::byte_size(&ok_ty);
                let s = self.scratch.alloc_i32();
                let s2 = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_load(0); i32_const(0); i32_ne;
                    if_i32;
                      i32_const(0); // none
                    else_;
                      // some(ok_value)
                      i32_const(ok_size as i32); call(self.emitter.rt.alloc); local_set(s2);
                      local_get(s2);
                      local_get(s);
                });
                self.emit_load_at(&ok_ty, 4);
                self.emit_store_at(&ok_ty, 0);
                wasm!(self.func, { local_get(s2); end; });
                self.scratch.free_i32(s2);
                self.scratch.free_i32(s);
            }
            "to_err_option" => {
                let err_ty = self.result_err_ty(&args[0].ty);
                let err_size = values::byte_size(&err_ty);
                let s = self.scratch.alloc_i32();
                let s2 = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_load(0); i32_eqz;
                    if_i32;
                      i32_const(0); // none (was ok)
                    else_;
                      i32_const(err_size as i32); call(self.emitter.rt.alloc); local_set(s2);
                      local_get(s2);
                      local_get(s);
                });
                self.emit_load_at(&err_ty, 4);
                self.emit_store_at(&err_ty, 0);
                wasm!(self.func, { local_get(s2); end; });
                self.scratch.free_i32(s2);
                self.scratch.free_i32(s);
            }
            "unwrap_or_else" => {
                let ok_ty = self.result_ok_ty(&args[0].ty);
                let vt = values::ty_to_valtype(&ok_ty).unwrap_or(ValType::I32);
                let result = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                // Store result
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(result); });
                // Store closure
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(closure); });
                match vt {
                    ValType::I64 => {
                        wasm!(self.func, { local_get(result); i32_load(0); i32_eqz; if_i64; local_get(result); i64_load(4); else_; });
                        // f(err)
                        wasm!(self.func, { local_get(closure); i32_load(4); local_get(result); i32_load(4); local_get(closure); i32_load(0); });
                        let err_ty = self.result_err_ty(&args[0].ty);
                        self.emit_closure_call(&err_ty, &ok_ty);
                        wasm!(self.func, { end; });
                    }
                    _ => {
                        wasm!(self.func, { local_get(result); i32_load(0); i32_eqz; if_i32; local_get(result); i32_load(4); else_; });
                        wasm!(self.func, { local_get(closure); i32_load(4); local_get(result); i32_load(4); local_get(closure); i32_load(0); });
                        let err_ty = self.result_err_ty(&args[0].ty);
                        self.emit_closure_call(&err_ty, &ok_ty);
                        wasm!(self.func, { end; });
                    }
                }
                self.scratch.free_i32(closure);
                self.scratch.free_i32(result);
            }
            "collect" => {
                // collect(rs: List[Result[T, E]]) -> Result[List[T], List[E]]
                let inner_result_ty = self.resolve_list_elem(&args[0], None);
                let ok_ty = self.result_ok_ty(&inner_result_ty);
                let err_ty = self.result_err_ty(&inner_result_ty);
                let ok_size = values::byte_size(&ok_ty) as i32;
                let err_size = values::byte_size(&err_ty) as i32;

                let rs = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let ok_list = self.scratch.alloc_i32();
                let err_list = self.scratch.alloc_i32();
                let ok_cnt = self.scratch.alloc_i32();
                let err_cnt = self.scratch.alloc_i32();
                let elem = self.scratch.alloc_i32();
                let out = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(rs);
                    local_get(rs); i32_load(0); local_set(len);
                    i32_const(4); local_get(len); i32_const(ok_size); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(ok_list);
                    i32_const(4); local_get(len); i32_const(err_size); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(err_list);
                    i32_const(0); local_set(ok_cnt);
                    i32_const(0); local_set(err_cnt);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(rs); i32_const(4); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(elem);
                });
                self.emit_result_sort_into_lists(
                    &ok_ty, &err_ty, ok_size, err_size,
                    ok_list, err_list, ok_cnt, err_cnt, elem,
                );
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(ok_list); local_get(ok_cnt); i32_store(0);
                    local_get(err_list); local_get(err_cnt); i32_store(0);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(out);
                    local_get(err_cnt); i32_eqz;
                    if_empty;
                      local_get(out); i32_const(0); i32_store(0);
                      local_get(out); local_get(ok_list); i32_store(4);
                    else_;
                      local_get(out); i32_const(1); i32_store(0);
                      local_get(out); local_get(err_list); i32_store(4);
                    end;
                    local_get(out);
                });

                self.scratch.free_i32(out);
                self.scratch.free_i32(elem);
                self.scratch.free_i32(err_cnt);
                self.scratch.free_i32(ok_cnt);
                self.scratch.free_i32(err_list);
                self.scratch.free_i32(ok_list);
                self.scratch.free_i32(i);
                self.scratch.free_i32(len);
                self.scratch.free_i32(rs);
            }
            "partition" => {
                // partition(rs: List[Result[T, E]]) -> (List[T], List[E])
                let inner_result_ty = self.resolve_list_elem(&args[0], None);
                let ok_ty = self.result_ok_ty(&inner_result_ty);
                let err_ty = self.result_err_ty(&inner_result_ty);
                let ok_size = values::byte_size(&ok_ty) as i32;
                let err_size = values::byte_size(&err_ty) as i32;

                let rs = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let ok_list = self.scratch.alloc_i32();
                let err_list = self.scratch.alloc_i32();
                let ok_cnt = self.scratch.alloc_i32();
                let err_cnt = self.scratch.alloc_i32();
                let elem = self.scratch.alloc_i32();
                let tuple_ptr = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(rs);
                    local_get(rs); i32_load(0); local_set(len);
                    i32_const(4); local_get(len); i32_const(ok_size); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(ok_list);
                    i32_const(4); local_get(len); i32_const(err_size); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(err_list);
                    i32_const(0); local_set(ok_cnt);
                    i32_const(0); local_set(err_cnt);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(rs); i32_const(4); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(elem);
                });
                self.emit_result_sort_into_lists(
                    &ok_ty, &err_ty, ok_size, err_size,
                    ok_list, err_list, ok_cnt, err_cnt, elem,
                );
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(ok_list); local_get(ok_cnt); i32_store(0);
                    local_get(err_list); local_get(err_cnt); i32_store(0);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(tuple_ptr);
                    local_get(tuple_ptr); local_get(ok_list); i32_store(0);
                    local_get(tuple_ptr); local_get(err_list); i32_store(4);
                    local_get(tuple_ptr);
                });

                self.scratch.free_i32(tuple_ptr);
                self.scratch.free_i32(elem);
                self.scratch.free_i32(err_cnt);
                self.scratch.free_i32(ok_cnt);
                self.scratch.free_i32(err_list);
                self.scratch.free_i32(ok_list);
                self.scratch.free_i32(i);
                self.scratch.free_i32(len);
                self.scratch.free_i32(rs);
            }
            "collect_map" => {
                // collect_map(xs: List[T], f: Fn[T] -> Result[U, E]) -> Result[List[U], List[E]]
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let ret_ty = self.fn_ret_ty(&args[1].ty);
                let ok_ty = self.result_ok_ty(&ret_ty);
                let err_ty = self.result_err_ty(&ret_ty);
                let ok_size = values::byte_size(&ok_ty) as i32;
                let err_size = values::byte_size(&err_ty) as i32;
                let _ret_size = values::byte_size(&ret_ty) as i32;

                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let ok_list = self.scratch.alloc_i32();
                let err_list = self.scratch.alloc_i32();
                let ok_cnt = self.scratch.alloc_i32();
                let err_cnt = self.scratch.alloc_i32();
                let res_ptr = self.scratch.alloc_i32();
                let out = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); local_set(len);
                    i32_const(4); local_get(len); i32_const(ok_size); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(ok_list);
                    i32_const(4); local_get(len); i32_const(err_size); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(err_list);
                    i32_const(0); local_set(ok_cnt);
                    i32_const(0); local_set(err_cnt);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // Call closure with xs[i]
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                // call_indirect: (env, elem) -> Result ptr (i32)
                {
                    let mut ct = vec![ValType::I32]; // env
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      local_set(res_ptr);
                });
                self.emit_result_sort_into_lists(
                    &ok_ty, &err_ty, ok_size, err_size,
                    ok_list, err_list, ok_cnt, err_cnt, res_ptr,
                );
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(ok_list); local_get(ok_cnt); i32_store(0);
                    local_get(err_list); local_get(err_cnt); i32_store(0);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(out);
                    local_get(err_cnt); i32_eqz;
                    if_empty;
                      local_get(out); i32_const(0); i32_store(0);
                      local_get(out); local_get(ok_list); i32_store(4);
                    else_;
                      local_get(out); i32_const(1); i32_store(0);
                      local_get(out); local_get(err_list); i32_store(4);
                    end;
                    local_get(out);
                });

                self.scratch.free_i32(out);
                self.scratch.free_i32(res_ptr);
                self.scratch.free_i32(err_cnt);
                self.scratch.free_i32(ok_cnt);
                self.scratch.free_i32(err_list);
                self.scratch.free_i32(ok_list);
                self.scratch.free_i32(i);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            _ => return false,
        }
        true
    }

    // ── Result codegen helpers ──

    /// Allocate a new Result with tag=0 (ok). Stores tag, leaves value slot empty.
    /// Returns the scratch local holding the result pointer.
    /// Caller must store the ok value at offset 4 and free the local when done.
    fn emit_result_alloc_ok(&mut self, ok_ty: &Ty) -> u32 {
        let size = 4 + values::byte_size(ok_ty);
        let s = self.scratch.alloc_i32();
        wasm!(self.func, {
            i32_const(size as i32); call(self.emitter.rt.alloc); local_set(s);
            local_get(s); i32_const(0); i32_store(0); // tag=0
        });
        s
    }

    /// Allocate a new Result with tag=1 (err). Stores tag, leaves value slot empty.
    /// Returns the scratch local holding the result pointer.
    /// Caller must store the err value at offset 4 and free the local when done.
    fn emit_result_alloc_err(&mut self, err_ty: &Ty) -> u32 {
        let size = 4 + values::byte_size(err_ty);
        let s = self.scratch.alloc_i32();
        wasm!(self.func, {
            i32_const(size as i32); call(self.emitter.rt.alloc); local_set(s);
            local_get(s); i32_const(1); i32_store(0); // tag=1
        });
        s
    }

    /// Load the tag from a Result pointer in `result_local` and push `tag == 0` (i.e. is_ok).
    /// After this, the caller can emit `if_i32` / `if_empty` to branch on ok vs err.
    /// The "then" branch is the ok path; the "else" branch is the err path.
    fn emit_result_branch_ok(&mut self, result_local: u32) {
        wasm!(self.func, {
            local_get(result_local); i32_load(0); i32_eqz;
        });
    }

    /// Like `emit_result_branch_ok` but pushes `tag != 0` (i.e. is_err).
    /// The "then" branch is the err path; the "else" branch is the ok path.
    fn emit_result_branch_err(&mut self, result_local: u32) {
        wasm!(self.func, {
            local_get(result_local); i32_load(0); i32_const(0); i32_ne;
        });
    }

    /// Emit the inner loop body shared by `collect`, `partition`, and `collect_map`.
    ///
    /// Expects `elem` local to hold a Result ptr. Sorts the value at offset 4 into
    /// either ok_list or err_list, incrementing the corresponding counter.
    ///
    /// Locals: ok_list, err_list, ok_cnt, err_cnt, elem.
    /// ok_size/err_size are the byte sizes of the ok/err value types.
    fn emit_result_sort_into_lists(
        &mut self,
        ok_ty: &Ty,
        err_ty: &Ty,
        ok_size: i32,
        err_size: i32,
        ok_list: u32,
        err_list: u32,
        ok_cnt: u32,
        err_cnt: u32,
        elem: u32,
    ) {
        self.emit_result_branch_ok(elem);
        wasm!(self.func, {
            if_empty; // tag==0 → ok
              local_get(ok_list); i32_const(4); i32_add;
              local_get(ok_cnt); i32_const(ok_size); i32_mul; i32_add;
              local_get(elem); i32_const(4); i32_add;
        });
        self.emit_elem_copy(ok_ty);
        wasm!(self.func, {
              local_get(ok_cnt); i32_const(1); i32_add; local_set(ok_cnt);
            else_;
              local_get(err_list); i32_const(4); i32_add;
              local_get(err_cnt); i32_const(err_size); i32_mul; i32_add;
              local_get(elem); i32_const(4); i32_add;
        });
        self.emit_elem_copy(err_ty);
        wasm!(self.func, {
              local_get(err_cnt); i32_const(1); i32_add; local_set(err_cnt);
            end;
        });
    }

    // ── Type extraction helpers ──

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

    #[allow(dead_code)] // Will be used for option.flat_map WASM codegen
    fn fn_ret_inner_ty(&self, ty: &Ty) -> Ty {
        // For flat_map: f returns Option[T], extract T
        let ret = self.fn_ret_ty(ty);
        self.option_inner_ty(&ret)
    }

}
