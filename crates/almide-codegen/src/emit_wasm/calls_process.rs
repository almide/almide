//! process module: exit, stdin_lines — WASM codegen dispatch.

use super::FuncCompiler;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use super::values;
use wasm_encoder::Instruction;

impl FuncCompiler<'_> {
    /// process module: exit, stdin_lines
    pub(super) fn emit_process_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "exit" => {
                // process.exit(code: Int) -> Unit
                // Emit code arg (i64), wrap to i32, call proc_exit
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i32_wrap_i64;
                    call(self.emitter.rt.proc_exit);
                });
            }
            "stdin_lines" => {
                // process.stdin_lines() -> List[String]
                // Strategy: read all stdin, then split by '\n'.
                // 1. Read all stdin into a raw buffer (same logic as io.read_all)
                // 2. Split by '\n', building a list of Almide strings
                let buf = self.scratch.alloc_i32();
                let capacity = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let iov_ptr = self.scratch.alloc_i32();
                let nread_ptr = self.scratch.alloc_i32();
                let nread_val = self.scratch.alloc_i32();
                let new_buf = self.scratch.alloc_i32();
                let copy_i = self.scratch.alloc_i32();
                let scan_i = self.scratch.alloc_i32();
                let line_start = self.scratch.alloc_i32();
                let line_len = self.scratch.alloc_i32();
                let line_ptr = self.scratch.alloc_i32();
                let list_ptr = self.scratch.alloc_i32();
                let list_cap = self.scratch.alloc_i32();
                let list_count = self.scratch.alloc_i32();
                let new_list = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();

                // --- Phase 1: read all stdin ---
                wasm!(self.func, {
                    i32_const(4096); call(self.emitter.rt.alloc); local_set(buf);
                    i32_const(4096); local_set(capacity);
                    i32_const(0); local_set(len);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(iov_ptr);
                    i32_const(4); call(self.emitter.rt.alloc); local_set(nread_ptr);
                });

                wasm!(self.func, {
                    block_empty; loop_empty;
                });

                // Grow if needed
                wasm!(self.func, {
                    local_get(capacity); local_get(len); i32_sub;
                    i32_const(4096); i32_lt_u;
                    if_empty;
                      local_get(capacity); i32_const(2); i32_mul; local_set(capacity);
                      local_get(capacity); call(self.emitter.rt.alloc); local_set(new_buf);
                      i32_const(0); local_set(copy_i);
                      block_empty; loop_empty;
                        local_get(copy_i); local_get(len); i32_ge_u; br_if(1);
                        local_get(new_buf); local_get(copy_i); i32_add;
                        local_get(buf); local_get(copy_i); i32_add; i32_load8_u(0);
                        i32_store8(0);
                        local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                        br(0);
                      end; end;
                      local_get(new_buf); local_set(buf);
                    end;
                });

                // Read chunk
                wasm!(self.func, {
                    local_get(iov_ptr); local_get(buf); local_get(len); i32_add; i32_store(0);
                    local_get(iov_ptr); local_get(capacity); local_get(len); i32_sub; i32_store(4);
                    i32_const(0);
                    local_get(iov_ptr);
                    i32_const(1);
                    local_get(nread_ptr);
                    call(self.emitter.rt.fd_read);
                    drop;
                });

                wasm!(self.func, {
                    local_get(nread_ptr); i32_load(0); local_set(nread_val);
                    local_get(nread_val); i32_eqz;
                    br_if(1);
                    local_get(len); local_get(nread_val); i32_add; local_set(len);
                    br(0);
                    end; end;
                });

                // --- Phase 2: split buf[0..len] by '\n' into List[String] ---
                // List layout: [count:i32][elem0:i32][elem1:i32]...
                // Each elem is a ptr to Almide String [len:i32][data:u8...]
                // We'll build with a growable array of i32 pointers.
                wasm!(self.func, {
                    // Initial list capacity: 64 elements (i32 ptrs)
                    i32_const(64); local_set(list_cap);
                    local_get(list_cap); i32_const(4); i32_mul;
                    call(self.emitter.rt.alloc); local_set(list_ptr);
                    i32_const(0); local_set(list_count);
                    i32_const(0); local_set(scan_i);
                    i32_const(0); local_set(line_start);
                });

                // Scan loop: iterate through buf looking for '\n'
                wasm!(self.func, {
                    block_empty; loop_empty;
                      local_get(scan_i); local_get(len); i32_ge_u;
                      br_if(1);
                });

                // Check if buf[scan_i] == '\n'
                wasm!(self.func, {
                      local_get(buf); local_get(scan_i); i32_add; i32_load8_u(0);
                      i32_const(10); i32_eq;
                      if_empty;
                });

                // Found '\n': build string from line_start..scan_i
                wasm!(self.func, {
                        local_get(scan_i); local_get(line_start); i32_sub; local_set(line_len);
                        // Allocate Almide string
                        local_get(line_len); i32_const(4); i32_add;
                        call(self.emitter.rt.alloc); local_set(line_ptr);
                        local_get(line_ptr); local_get(line_len); i32_store(0);
                        // Copy line data
                        i32_const(0); local_set(copy_i);
                        block_empty; loop_empty;
                          local_get(copy_i); local_get(line_len); i32_ge_u; br_if(1);
                          local_get(line_ptr); i32_const(4); i32_add; local_get(copy_i); i32_add;
                          local_get(buf); local_get(line_start); i32_add; local_get(copy_i); i32_add;
                          i32_load8_u(0);
                          i32_store8(0);
                          local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                          br(0);
                        end; end;
                });

                // Grow list if needed
                wasm!(self.func, {
                        local_get(list_count); local_get(list_cap); i32_ge_u;
                        if_empty;
                          local_get(list_cap); i32_const(2); i32_mul; local_set(list_cap);
                          local_get(list_cap); i32_const(4); i32_mul;
                          call(self.emitter.rt.alloc); local_set(new_list);
                          // Copy old list ptrs
                          i32_const(0); local_set(copy_i);
                          block_empty; loop_empty;
                            local_get(copy_i); local_get(list_count); i32_ge_u; br_if(1);
                            local_get(new_list); local_get(copy_i); i32_const(4); i32_mul; i32_add;
                            local_get(list_ptr); local_get(copy_i); i32_const(4); i32_mul; i32_add;
                            i32_load(0);
                            i32_store(0);
                            local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                            br(0);
                          end; end;
                          local_get(new_list); local_set(list_ptr);
                        end;
                });

                // Append line_ptr to list
                wasm!(self.func, {
                        local_get(list_ptr); local_get(list_count); i32_const(4); i32_mul; i32_add;
                        local_get(line_ptr); i32_store(0);
                        local_get(list_count); i32_const(1); i32_add; local_set(list_count);
                        // line_start = scan_i + 1
                        local_get(scan_i); i32_const(1); i32_add; local_set(line_start);
                      end; // end if '\n'
                });

                // Advance scan_i
                wasm!(self.func, {
                      local_get(scan_i); i32_const(1); i32_add; local_set(scan_i);
                      br(0);
                    end; end; // end loop, end block
                });

                // Handle last line (if no trailing '\n')
                wasm!(self.func, {
                    local_get(line_start); local_get(len); i32_lt_u;
                    if_empty;
                      local_get(len); local_get(line_start); i32_sub; local_set(line_len);
                      local_get(line_len); i32_const(4); i32_add;
                      call(self.emitter.rt.alloc); local_set(line_ptr);
                      local_get(line_ptr); local_get(line_len); i32_store(0);
                      i32_const(0); local_set(copy_i);
                      block_empty; loop_empty;
                        local_get(copy_i); local_get(line_len); i32_ge_u; br_if(1);
                        local_get(line_ptr); i32_const(4); i32_add; local_get(copy_i); i32_add;
                        local_get(buf); local_get(line_start); i32_add; local_get(copy_i); i32_add;
                        i32_load8_u(0);
                        i32_store8(0);
                        local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                        br(0);
                      end; end;
                });

                // Grow list if needed for last line
                wasm!(self.func, {
                      local_get(list_count); local_get(list_cap); i32_ge_u;
                      if_empty;
                        local_get(list_cap); i32_const(2); i32_mul; local_set(list_cap);
                        local_get(list_cap); i32_const(4); i32_mul;
                        call(self.emitter.rt.alloc); local_set(new_list);
                        i32_const(0); local_set(copy_i);
                        block_empty; loop_empty;
                          local_get(copy_i); local_get(list_count); i32_ge_u; br_if(1);
                          local_get(new_list); local_get(copy_i); i32_const(4); i32_mul; i32_add;
                          local_get(list_ptr); local_get(copy_i); i32_const(4); i32_mul; i32_add;
                          i32_load(0);
                          i32_store(0);
                          local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                          br(0);
                        end; end;
                        local_get(new_list); local_set(list_ptr);
                      end;
                      // Append last line
                      local_get(list_ptr); local_get(list_count); i32_const(4); i32_mul; i32_add;
                      local_get(line_ptr); i32_store(0);
                      local_get(list_count); i32_const(1); i32_add; local_set(list_count);
                    end; // end if line_start < len
                });

                // Build final Almide List: [count:i32][elem0:i32][elem1:i32]...
                // elem_size = 4 (i32 pointer)
                wasm!(self.func, {
                    local_get(list_count); i32_const(4); i32_mul; i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(list_count); i32_store(0);
                    // Copy list_ptr[0..list_count] to result+4
                    i32_const(0); local_set(copy_i);
                    block_empty; loop_empty;
                      local_get(copy_i); local_get(list_count); i32_ge_u; br_if(1);
                      local_get(result); i32_const(4); i32_add;
                      local_get(copy_i); i32_const(4); i32_mul; i32_add;
                      local_get(list_ptr); local_get(copy_i); i32_const(4); i32_mul; i32_add;
                      i32_load(0);
                      i32_store(0);
                      local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                      br(0);
                    end; end;
                    local_get(result);
                });

                self.scratch.free_i32(result);
                self.scratch.free_i32(new_list);
                self.scratch.free_i32(list_count);
                self.scratch.free_i32(list_cap);
                self.scratch.free_i32(list_ptr);
                self.scratch.free_i32(line_ptr);
                self.scratch.free_i32(line_len);
                self.scratch.free_i32(line_start);
                self.scratch.free_i32(scan_i);
                self.scratch.free_i32(copy_i);
                self.scratch.free_i32(new_buf);
                self.scratch.free_i32(nread_val);
                self.scratch.free_i32(nread_ptr);
                self.scratch.free_i32(iov_ptr);
                self.scratch.free_i32(len);
                self.scratch.free_i32(capacity);
                self.scratch.free_i32(buf);
            }
            _ => panic!(
                "[ICE] emit_wasm: no WASM dispatch for `process.{}` — \
                 add an arm in emit_process_call or resolve upstream",
                func
            ),
        }
}
}
